//!  Worker-side operations for history validation.

use super::*;

/// Checks that saved layout data stays within table limits.
pub(crate) fn validate_layout_history(layout: &TableLayout) -> Result<(), Message> {
    if layout
        .columns()
        .any(|(_, column)| column.width.is_some_and(|value| !value.is_finite()))
    {
        return Err(Message::new("status-history-column-width-invalid"));
    }
    Ok(())
}

/// Checks table data before restoring a structure snapshot.
pub(crate) fn validate_table_history(table: &crate::document::Table) -> Result<(), Message> {
    validate_layout_history(&table.layout)?;
    for (address, cell) in table.stored_cells() {
        let has_non_finite = matches!(cell.input, crate::document::CellInput::Number(value) if !value.is_finite())
            || matches!(cell.imported_result, Some(crate::document::CalcValue::Number(value)) if !value.is_finite())
            || cell.calculation.as_ref().and_then(|calc| calc.value.as_ref()).is_some_and(
                |value| matches!(value, crate::document::CalcValue::Number(number) if !number.is_finite()),
            )
;
        if has_non_finite {
            return Err(Message::new("status-history-cell-value-invalid")
                .with_text("table", &table.name)
                .with_text("cell", crate::xlsx_adapter::cell_reference(address)));
        }
    }
    Ok(())
}

/// Checks the stored history root and its referenced tables.
pub(crate) fn validate_recovery_root(
    tables: &[crate::document::Table],
    view_layout: &TableLayout,
    scroll: ScrollPosition,
) -> Result<(), Message> {
    if !scroll.x.is_finite() || !scroll.y.is_finite() {
        return Err(Message::new("status-history-scroll-invalid"));
    }
    validate_layout_history(view_layout)?;
    for table in tables {
        validate_table_history(table)?;
    }
    Ok(())
}
