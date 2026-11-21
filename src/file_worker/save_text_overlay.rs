use super::*;
use std::collections::BTreeMap;

/// Adds sparse source cells to the source patch only when their value is not already saved.
pub(super) fn csv_changes(
    source: &CsvDocument,
    table_id: u64,
    changes: Vec<CsvCellChange>,
    overlay: &TextOverlaySaveRequest,
) -> Vec<CsvCellChange> {
    let parsed = source.to_document(0, "CSV");
    let table = parsed.tables.first();
    let mut values = BTreeMap::new();
    for (saved_table, address, cell) in overlay.base.cells.iter() {
        if *saved_table != table_id {
            continue;
        }
        let string_cell = overlay.string_cells.contains(&(*saved_table, *address));
        let value = cell.source_text_for_text_file(string_cell);
        let current = table.and_then(|table| table.cell(*address));
        let current_string = table.is_some_and(|table| table.is_string_cell(*address));
        if current.is_some_and(|current| current.input != cell.input)
            || current.is_some() && current_string != string_cell
            || current.is_none() && !matches!(&cell.input, crate::document::CellInput::Empty)
        {
            values.insert((address.row, address.column), Some(value));
        }
    }
    for change in changes {
        values.insert((change.row, change.column), change.value);
    }
    values
        .into_iter()
        .map(|((row, column), value)| CsvCellChange { row, column, value })
        .collect()
}

/// Adds sparse source cells to Markdown patches without changing untouched syntax.
pub(super) fn markdown_changes(
    source: &MarkdownDocument,
    document_id: u64,
    changes: Vec<MarkdownCellChange>,
    overlay: &TextOverlaySaveRequest,
) -> Vec<MarkdownCellChange> {
    let parsed = source.to_document(document_id, "Markdown");
    let mut values = BTreeMap::new();
    for (saved_table, address, cell) in overlay.base.cells.iter() {
        let Some(table_index) = overlay.table_ids.iter().position(|id| id == saved_table) else {
            continue;
        };
        let string_cell = overlay.string_cells.contains(&(*saved_table, *address));
        let value = cell.source_text_for_text_file(string_cell);
        let table = parsed.tables.get(table_index);
        let current = table.and_then(|table| table.cell(*address));
        let current_string = table.is_some_and(|table| table.is_string_cell(*address));
        if current.is_some_and(|current| current.input != cell.input)
            || current.is_some() && current_string != string_cell
            || current.is_none() && !matches!(&cell.input, crate::document::CellInput::Empty)
        {
            values.insert((table_index, address.row, address.column), value);
        }
    }
    for change in changes {
        values.insert((change.table, change.row, change.column), change.value);
    }
    values
        .into_iter()
        .map(|((table, row, column), value)| MarkdownCellChange {
            table,
            row,
            column,
            value,
        })
        .collect()
}
