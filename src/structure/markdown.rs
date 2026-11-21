//!  Worker-side operations for markdown.

use super::*;

// Apply pending cell edits to the source representation.
pub(crate) fn materialize_current_source(
    request: &StructureRequest,
    source: &SourceDocument,
) -> Result<SourceDocument, Message> {
    match source {
        SourceDocument::Csv(source) => {
            let source_table_id = request
                .tables
                .first()
                .filter(|_| request.tables.len() == 1)
                .map(|table| table.id)
                .ok_or_else(|| Message::new("status-table-count-changed-after-parse"))?;
            let changes = request
                .changes
                .iter()
                .filter(|(table_id, _, _)| *table_id == source_table_id)
                .map(|(_, address, cell)| CsvCellChange {
                    row: address.row,
                    column: address.column,
                    value: cell.as_ref().map(|cell| {
                        let is_string = request
                            .tables
                            .iter()
                            .find(|table| table.id == source_table_id)
                            .is_some_and(|table| table.string_cells.contains(address));
                        cell.source_text_for_text_file(is_string)
                    }),
                })
                .collect::<Vec<_>>();
            let updated = source
                .apply_changes(&changes)
                .map_err(crate::i18n::csv_error_message)?;
            Ok(SourceDocument::Csv(Arc::new(updated)))
        }
        SourceDocument::Markdown(source) => {
            let changes = request
                .changes
                .iter()
                .filter_map(|(table_id, address, cell)| {
                    request
                        .tables
                        .iter()
                        .position(|table| table.id == *table_id)
                        .map(|table| MarkdownCellChange {
                            table,
                            row: address.row,
                            column: address.column,
                            value: cell.as_ref().map_or_else(String::new, |cell| {
                                let is_string = request
                                    .tables
                                    .iter()
                                    .find(|source_table| source_table.id == *table_id)
                                    .is_some_and(|source_table| {
                                        source_table.string_cells.contains(address)
                                    });
                                cell.source_text_for_text_file(is_string)
                            }),
                        })
                })
                .collect::<Vec<_>>();
            let updated = source
                .apply_changes(&changes)
                .map_err(crate::i18n::markdown_error_message)?;
            Ok(SourceDocument::Markdown(Arc::new(updated)))
        }
    }
}
