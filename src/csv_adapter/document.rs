use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

impl CsvDocument {
    /// Returns the first reader limit reached by the parsed source.
    pub(crate) fn reached_resource_limit(&self) -> Option<crate::i18n::ResourceLimit> {
        if self.bytes.len() == crate::csv_adapter::MAX_CSV_SOURCE_BYTES {
            Some(crate::i18n::ResourceLimit::FileSize)
        } else {
            self.reached_shape_limit()
        }
    }

    /// Returns exact row, column, or cell limits without treating the old byte length as current.
    pub(crate) fn reached_shape_limit(&self) -> Option<crate::i18n::ResourceLimit> {
        if self.parsed_cell_count == crate::csv_adapter::MAX_CSV_CELLS {
            Some(crate::i18n::ResourceLimit::CellCount)
        } else if self.records.len() == crate::csv_adapter::MAX_CSV_ROWS_READER {
            Some(crate::i18n::ResourceLimit::RowCount)
        } else if self.parsed_max_column_count == crate::csv_adapter::MAX_CSV_COLUMNS_READER {
            Some(crate::i18n::ResourceLimit::ColumnCount)
        } else {
            None
        }
    }

    // Expands each actual CSV field into a text cell in the initial document.
    pub fn to_document(&self, id: DocumentId, name: impl Into<String>) -> Document {
        self.to_document_checked(id, name, || true)
            .expect("unconditional CSV conversion succeeds")
    }

    /// Builds a source model while allowing a worker to stop between fields.
    pub(crate) fn to_document_with_cancel(
        &self,
        id: DocumentId,
        name: impl Into<String>,
        cancelled: &AtomicBool,
    ) -> Result<Document, crate::i18n::Message> {
        self.to_document_checked(id, name, || !cancelled.load(Ordering::Acquire))
    }

    // Converts CSV rows into a document while allowing the caller to stop.
    fn to_document_checked(
        &self,
        id: DocumentId,
        name: impl Into<String>,
        mut keep_going: impl FnMut() -> bool,
    ) -> Result<Document, crate::i18n::Message> {
        if !keep_going() {
            return Err(crate::i18n::Message::new("status-operation-cancelled"));
        }
        // The only table is named after the file without its extension, such as "data".
        let name = name.into();
        let table_name = std::path::Path::new(&name)
            .file_stem()
            .map_or_else(|| name.clone(), |stem| stem.to_string_lossy().into_owned());
        let mut document = Document::new(id, name, &[table_name.as_str()]);
        if let Some(table) = document.tables.first_mut() {
            let row_count = self.records.len().max(1);
            let column_count = self
                .records
                .iter()
                .map(|record| record.fields.len())
                .max()
                .unwrap_or(0)
                .max(1);
            table.set_dimensions(row_count, column_count);
            for (row, record) in self.records.iter().enumerate() {
                if !keep_going() {
                    return Err(crate::i18n::Message::new("status-operation-cancelled"));
                }
                for (column, field) in record.fields.iter().enumerate() {
                    if !keep_going() {
                        return Err(crate::i18n::Message::new("status-operation-cancelled"));
                    }
                    table.set_loaded_cell(
                        CellAddress { row, column },
                        Cell {
                            input: CellInput::Text(field.value.clone()),
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
