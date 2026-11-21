use super::*;

use crate::document::{LayoutTarget, LayoutValue, TableLayout};

impl HtmlDocument {
    /// Builds a grid document from supported table cells.
    pub fn to_document(&self, id: DocumentId, name: impl Into<String>) -> Document {
        self.to_document_with_language(id, name, crate::i18n::Language::English)
    }

    /// Creates localized defaults while keeping authored captions unchanged.
    pub fn to_document_with_language(
        &self,
        id: DocumentId,
        name: impl Into<String>,
        language: crate::i18n::Language,
    ) -> Document {
        self.to_document_checked(id, name, language, || true)
            .expect("unconditional HTML conversion succeeds")
    }

    // Converts HTML tables while allowing the caller to stop long imports.
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
            .map(|(index, table)| table.name_with_language(index, language))
            .collect::<Vec<_>>();
        let table_names = table_names.iter().map(String::as_str).collect::<Vec<_>>();
        let mut document = Document::new(id, name, &table_names);
        for (table_index, source_table) in self.tables.iter().enumerate() {
            if !keep_going() {
                return Err(crate::i18n::Message::new("status-operation-cancelled"));
            }
            let Some(table) = document.tables.get_mut(table_index) else {
                continue;
            };
            for (row, cells) in source_table.rows.iter().enumerate() {
                if !keep_going() {
                    return Err(crate::i18n::Message::new("status-operation-cancelled"));
                }
                for (column, cell) in cells.iter().enumerate() {
                    if !keep_going() {
                        return Err(crate::i18n::Message::new("status-operation-cancelled"));
                    }
                    if cell.placeholder {
                        continue;
                    }
                    let address = CellAddress { row, column };
                    table.set_loaded_cell(
                        address,
                        Cell {
                            input: CellInput::Text(cell.value.clone()),
                            imported_result: None,
                            calculation: None,
                        },
                    );
                    table.set_loaded_format(
                        address,
                        crate::document::CellFormat {
                            style: crate::document::CellStyle {
                                horizontal_alignment: cell.alignment.clone(),
                                ..crate::document::CellStyle::default()
                            },
                            hyperlink: cell.hyperlink.clone(),
                            rich_text: cell.rich_text.clone(),
                        },
                    );
                }
            }
            let mut layout = TableLayout::default();
            for range in &source_table.merges {
                layout.apply_value(LayoutTarget::Merge(*range), LayoutValue::Merge(true));
            }
            table.set_loaded_layout(layout);
            let column_count = source_table.rows.iter().map(Vec::len).max().unwrap_or(0);
            table.set_dimensions(source_table.rows.len().max(1), column_count.max(1));
        }
        Ok(document)
    }
}
