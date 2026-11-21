//!  Compares edited cell payloads and applies recorded cell changes.

use crate::document::{Cell, CellChange, CellInput, Document, DocumentError, TableId};

impl super::EditCommand {
    /// Returns cell writes that can change a text source without a worker structure plan.
    pub(crate) fn text_source_changes(
        &self,
    ) -> Option<(TableId, &[CellChange], &[crate::document::StringOverride])> {
        match self {
            Self::SetCells { table_id, changes } => Some((*table_id, changes, &[])),
            Self::SetFormulaResults {
                table_id,
                changes,
                string_overrides,
            } => Some((*table_id, changes, string_overrides)),
            Self::ReplaceStructure { .. } => None,
        }
    }
}

// Apply cell changes in the requested direction and refresh affected calculations.
pub(crate) fn apply_cell_changes(
    document: &mut Document,
    table_id: TableId,
    changes: &[CellChange],
    forward: bool,
    preserve_current_calculation: bool,
) -> Result<(), DocumentError> {
    let table = document
        .tables
        .iter_mut()
        .find(|table| table.id == table_id)
        .ok_or(DocumentError::MissingTable(table_id))?;
    for change in changes {
        let mut value = if forward {
            change.after.clone()
        } else {
            change.before.clone()
        };
        // Undoing a formatting-only edit must not roll back a result that arrived
        // after the command was recorded for the same cell input.
        if preserve_current_calculation
            && let (Some(current), Some(restored)) = (table.cell(change.address), value.as_mut())
            && !cell_inputs_differ(Some(current), Some(restored))
        {
            restored.calculation = current.calculation.clone();
            restored.imported_result = current.imported_result.clone();
        }
        table.set_cell(change.address, value);
    }
    Ok(())
}

// Check whether a cell change alters formula or entered input.
pub(crate) fn cell_inputs_differ(before: Option<&Cell>, after: Option<&Cell>) -> bool {
    match (
        before.map(|cell| &cell.input),
        after.map(|cell| &cell.input),
    ) {
        (None, None) => false,
        (Some(before), Some(after)) => before != after,
        (None, Some(after)) => *after != CellInput::Empty,
        (Some(before), None) => *before != CellInput::Empty,
    }
}
