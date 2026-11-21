//!  Worker-side operations for selection.

use super::*;
use crate::i18n::Message;

// Choose the affected selection bounds for a structure operation.
pub(crate) fn selection_for_operation(
    operation: &StructureOperation,
    plan: &StructurePlan,
    text_format: bool,
) -> CellRange {
    match operation {
        StructureOperation::Insert {
            axis, index, count, ..
        }
        | StructureOperation::Duplicate {
            axis, index, count, ..
        } => match axis {
            StructureAxis::Rows => CellRange {
                anchor: CellAddress {
                    row: *index,
                    column: 0,
                },
                head: CellAddress {
                    row: index.saturating_add(*count).saturating_sub(1),
                    column: if text_format {
                        crate::document::MAX_COLUMNS - 1
                    } else {
                        plan.after_dimensions().1.saturating_sub(1)
                    },
                },
            },
            StructureAxis::Columns => CellRange {
                anchor: CellAddress {
                    row: 0,
                    column: *index,
                },
                head: CellAddress {
                    row: if text_format {
                        crate::document::MAX_ROWS - 1
                    } else {
                        plan.after_dimensions().0.saturating_sub(1)
                    },
                    column: index.saturating_add(*count).saturating_sub(1),
                },
            },
        },
        StructureOperation::Delete { axis, index, .. } => match axis {
            StructureAxis::Rows => CellRange {
                anchor: CellAddress {
                    row: *index,
                    column: 0,
                },
                head: CellAddress {
                    row: *index,
                    column: if text_format {
                        crate::document::MAX_COLUMNS - 1
                    } else {
                        plan.after_dimensions().1.saturating_sub(1)
                    },
                },
            },
            StructureAxis::Columns => CellRange {
                anchor: CellAddress {
                    row: 0,
                    column: *index,
                },
                head: CellAddress {
                    row: if text_format {
                        crate::document::MAX_ROWS - 1
                    } else {
                        plan.after_dimensions().0.saturating_sub(1)
                    },
                    column: *index,
                },
            },
        },
        StructureOperation::Move {
            axis,
            count,
            destination,
            ..
        } => match axis {
            StructureAxis::Rows => CellRange {
                anchor: CellAddress {
                    row: *destination,
                    column: 0,
                },
                head: CellAddress {
                    row: destination.saturating_add(*count).saturating_sub(1),
                    column: if text_format {
                        crate::document::MAX_COLUMNS - 1
                    } else {
                        plan.after_dimensions().1.saturating_sub(1)
                    },
                },
            },
            StructureAxis::Columns => CellRange {
                anchor: CellAddress {
                    row: 0,
                    column: *destination,
                },
                head: CellAddress {
                    row: if text_format {
                        crate::document::MAX_ROWS - 1
                    } else {
                        plan.after_dimensions().0.saturating_sub(1)
                    },
                    column: destination.saturating_add(*count).saturating_sub(1),
                },
            },
        },
        StructureOperation::ReorderRows { row_sources, .. } => CellRange {
            anchor: CellAddress::default(),
            head: CellAddress {
                row: row_sources.len().saturating_sub(1),
                column: plan.after_dimensions().1.saturating_sub(1),
            },
        },
        StructureOperation::Fill { target, .. } => *target,
        StructureOperation::SetTextCell { address, .. } => CellRange {
            anchor: *address,
            head: *address,
        },
        StructureOperation::SetTextCells { changes, .. } => {
            let (start, end) = changes.iter().fold(
                (CellAddress::default(), CellAddress::default()),
                |(start, end), (address, _)| {
                    (
                        CellAddress {
                            row: start.row.min(address.row),
                            column: start.column.min(address.column),
                        },
                        CellAddress {
                            row: end.row.max(address.row),
                            column: end.column.max(address.column),
                        },
                    )
                },
            );
            CellRange {
                anchor: start,
                head: end,
            }
        }
    }
}

// Stop preparation when the worker has received a cancellation request.
pub(crate) fn check_cancelled(cancelled: &AtomicBool) -> Result<(), Message> {
    if cancelled.load(Ordering::Acquire) {
        Err(Message::new("status-structure-cancelled"))
    } else {
        Ok(())
    }
}
