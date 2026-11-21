use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

impl MarkdownDocument {
    /// Returns the first reader limit reached by the parsed source.
    pub(crate) fn reached_resource_limit(&self) -> Option<crate::i18n::ResourceLimit> {
        if self.bytes.len() == crate::markdown_adapter::MAX_MARKDOWN_BYTES {
            Some(crate::i18n::ResourceLimit::FileSize)
        } else {
            self.reached_shape_limit()
        }
    }

    /// Returns exact row, column, or cell limits without treating the old byte length as current.
    pub(crate) fn reached_shape_limit(&self) -> Option<crate::i18n::ResourceLimit> {
        if self.parsed_cell_count == crate::markdown_adapter::MAX_MARKDOWN_CELLS {
            Some(crate::i18n::ResourceLimit::CellCount)
        } else if self.parsed_row_count == crate::markdown_adapter::MAX_MARKDOWN_ROWS {
            Some(crate::i18n::ResourceLimit::RowCount)
        } else if self.parsed_max_column_count == crate::markdown_adapter::MAX_MARKDOWN_COLUMNS {
            Some(crate::i18n::ResourceLimit::ColumnCount)
        } else {
            None
        }
    }

    /// Measures one saved-cell replacement without rebuilding the surrounding document.
    pub(crate) fn source_cell_byte_delta(
        &self,
        table: usize,
        row: usize,
        column: usize,
        value: Option<&str>,
    ) -> Option<i128> {
        let cell = self.tables.get(table)?.rows.get(row)?.get(column)?;
        if !cell.has_source {
            return None;
        }
        let replacement = value.map_or(0, |value| {
            crate::markdown_adapter::syntax::escape_table_cell(value).len()
        });
        Some(replacement as i128 - cell.source_range.len() as i128)
    }

    /// Returns the source range floor that saves must keep until the table changes.
    pub(crate) fn source_extent(&self, table: usize) -> Option<(usize, usize)> {
        self.source_extents.get(table).copied()
    }

    /// Lets a blank new-document body row disappear after the last cell is cleared.
    pub(crate) fn make_blank_new_document_body_trimmable(&mut self) {
        for (table, (floor_rows, _)) in self.tables.iter().zip(&mut self.source_extents) {
            let has_only_blank_body = table.rows.len() == 2
                && table.rows[1..]
                    .iter()
                    .flatten()
                    .all(|cell| cell.value.is_empty());
            if has_only_blank_body {
                *floor_rows = 1;
            }
        }
    }

    // Imports sparse cells for each table and builds an initial model without document history.
    pub fn to_document(&self, id: DocumentId, name: impl Into<String>) -> Document {
        self.to_document_with_language(id, name, crate::i18n::Language::English)
    }

    /// Creates table defaults in the selected language and keeps authored headings unchanged.
    pub fn to_document_with_language(
        &self,
        id: DocumentId,
        name: impl Into<String>,
        language: crate::i18n::Language,
    ) -> Document {
        self.to_document_checked(id, name, language, || true)
            .expect("unconditional Markdown conversion succeeds")
    }

    /// Builds a source model while allowing a worker to stop between cells.
    pub(crate) fn to_document_with_cancel(
        &self,
        id: DocumentId,
        name: impl Into<String>,
        language: crate::i18n::Language,
        cancelled: &AtomicBool,
    ) -> Result<Document, crate::i18n::Message> {
        self.to_document_checked(id, name, language, || !cancelled.load(Ordering::Acquire))
    }

    // Converts parsed Markdown tables while allowing long imports to stop.
    fn to_document_checked(
        &self,
        id: DocumentId,
        name: impl Into<String>,
        language: crate::i18n::Language,
        mut keep_going: impl FnMut() -> bool,
    ) -> Result<Document, crate::i18n::Message> {
        if !keep_going() {
            return Err(crate::i18n::Message::new("status-operation-cancelled"));
        }
        let table_names = self
            .tables
            .iter()
            .enumerate()
            .map(|(index, table)| {
                if table.name_generated {
                    crate::i18n::Message::new("table-default-name")
                        .with_number("number", index as i64 + 1)
                        .resolve_generated_name(language)
                } else {
                    table.name.clone()
                }
            })
            .collect::<Vec<_>>();
        let table_name_refs = table_names.iter().map(String::as_str).collect::<Vec<_>>();
        let mut document = Document::new(id, name, &table_name_refs);
        for (table_index, source_table) in self.tables.iter().enumerate() {
            if !keep_going() {
                return Err(crate::i18n::Message::new("status-operation-cancelled"));
            }
            let Some(table) = document.tables.get_mut(table_index) else {
                continue;
            };
            let row_count = source_table.rows.len().max(1);
            let column_count = source_table
                .rows
                .iter()
                .map(Vec::len)
                .max()
                .unwrap_or(0)
                .max(1);
            table.set_dimensions(row_count, column_count);
            for (row, cells) in source_table.rows.iter().enumerate() {
                if !keep_going() {
                    return Err(crate::i18n::Message::new("status-operation-cancelled"));
                }
                for (column, cell) in cells.iter().enumerate() {
                    if !keep_going() {
                        return Err(crate::i18n::Message::new("status-operation-cancelled"));
                    }
                    table.set_loaded_cell(
                        CellAddress { row, column },
                        Cell {
                            input: CellInput::Text(cell.value.clone()),
                            imported_result: None,
                            calculation: None,
                        },
                    );
                }
            }
        }
        Ok(document)
    }
}
