//!  Worker-side operations for data helpers.

use super::*;
use crate::i18n::Message;

// Find a table by its stable ID or return a clear error.
pub(crate) fn find_table(
    document: &Document,
    table_id: TableId,
) -> Result<&crate::document::Table, Message> {
    document
        .tables
        .iter()
        .find(|table| table.id == table_id)
        .ok_or_else(|| {
            Message::new("status-table-id-unavailable").with_number("table_id", table_id as i64)
        })
}

// Check whether a cell contains a supported formula source.
pub(crate) fn is_formula_cell(
    table: &crate::document::Table,
    address: CellAddress,
    cell: &Cell,
) -> bool {
    crate::calc::formula_source(&cell.input, table.is_string_cell(address)).is_some()
}

// Check that a custom delimiter is nonempty and safe to use.
pub(crate) fn validate_delimiter(delimiter: &str) -> Result<(), Message> {
    if delimiter.is_empty() {
        return Err(Message::new("status-delimiter-empty"));
    }
    if delimiter.len() > 32 || delimiter.contains('\r') || delimiter.contains('\n') {
        return Err(Message::new("status-delimiter-invalid"));
    }
    Ok(())
}

// Check whether a cell carries visible or saved content.
pub(crate) fn cell_has_content(cell: &Cell) -> bool {
    !matches!(cell.input, CellInput::Empty)
}
