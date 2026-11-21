//!  Applies prepared structure operations to IronCalc formula models.

use ironcalc_base::UserModel;
use ironcalc_base::expressions::types::Area;
use std::collections::BTreeSet;

use crate::calc::engine::formula::{engine_error_message, one_based_engine_index};
use crate::calc::snapshot::CalculationTable;
use crate::document::{CellAddress, StructureAxis, StructureOperation, StructurePlan};

// Apply a supported row or column operation to the calculation engine.
pub(super) fn apply_engine_structure(
    model: &mut UserModel<'static>,
    sheet: u32,
    operation: &StructureOperation,
    table: &CalculationTable,
) -> Result<(), crate::i18n::Message> {
    if matches!(
        operation,
        StructureOperation::SetTextCell { .. } | StructureOperation::SetTextCells { .. }
    ) {
        return Ok(());
    }
    if let StructureOperation::ReorderRows {
        row_sources,
        source_row_count,
        ..
    } = operation
    {
        let retained = row_sources.iter().copied().collect::<BTreeSet<_>>();
        let mut current = (0..*source_row_count).collect::<Vec<_>>();
        let mut cursor = current.len();
        while cursor > 0 {
            if retained.contains(&(cursor - 1)) {
                cursor -= 1;
                continue;
            }
            let mut start = cursor - 1;
            while start > 0 && !retained.contains(&(start - 1)) {
                start -= 1;
            }
            let count = cursor - start;
            model
                .delete_rows(
                    sheet,
                    one_based_engine_index(start)?,
                    engine_positive_count(count)?,
                )
                .map_err(engine_error_message)?;
            current.drain(start..cursor);
            cursor = start;
        }
        let mut target = 0;
        while target < row_sources.len() {
            if current[target] == row_sources[target] {
                target += 1;
                continue;
            }
            let source = row_sources[target];
            let source_position = current[target..]
                .iter()
                .position(|row| *row == source)
                .map(|offset| target + offset)
                .ok_or_else(|| {
                    crate::i18n::Message::new("status-structure-row-reorder-formula-target-missing")
                })?;
            let mut count = 1;
            while target + count < row_sources.len()
                && source_position + count < current.len()
                && current[source_position + count] == row_sources[target + count]
            {
                count += 1;
            }
            let delta = isize::try_from(target).map_err(|_| {
                crate::i18n::Message::new("status-calculation-index-too-large")
                    .with_number("index", target as i64)
            })? - isize::try_from(source_position).map_err(|_| {
                crate::i18n::Message::new("status-calculation-index-too-large")
                    .with_number("index", source_position as i64)
            })?;
            model
                .move_rows_action(
                    sheet,
                    one_based_engine_index(source_position)?,
                    engine_positive_count(count)?,
                    i32::try_from(delta).map_err(|_| {
                        crate::i18n::Message::new("status-calculation-index-too-large")
                            .with_number("index", delta as i64)
                    })?,
                )
                .map_err(engine_error_message)?;
            let moved = current
                .drain(source_position..source_position + count)
                .collect::<Vec<_>>();
            current.splice(target..target, moved);
            target += count;
        }
        return Ok(());
    }
    let (axis, index, count) = match operation {
        StructureOperation::Insert {
            axis, index, count, ..
        }
        | StructureOperation::Delete {
            axis, index, count, ..
        }
        | StructureOperation::Duplicate {
            axis, index, count, ..
        } => (*axis, *index, *count),
        StructureOperation::Move {
            axis,
            index,
            count,
            destination,
            ..
        } => {
            let delta = isize::try_from(*destination).map_err(|_| {
                crate::i18n::Message::new("status-calculation-index-too-large")
                    .with_number("index", *destination as i64)
            })? - isize::try_from(*index).map_err(|_| {
                crate::i18n::Message::new("status-calculation-index-too-large")
                    .with_number("index", *index as i64)
            })?;
            let delta = i32::try_from(delta).map_err(|_| {
                crate::i18n::Message::new("status-calculation-index-too-large")
                    .with_number("index", delta as i64)
            })?;
            let index = one_based_engine_index(*index)?;
            let count = engine_positive_count(*count)?;
            // This formula-only model is rebuilt from sparse cell inputs and has no hidden
            // row/column metadata, so IronCalc's move helper cannot adjust delta for hidden bands.
            return match axis {
                StructureAxis::Rows => model
                    .move_rows_action(sheet, index, count, delta)
                    .map_err(engine_error_message),
                StructureAxis::Columns => model
                    .move_columns_action(sheet, index, count, delta)
                    .map_err(engine_error_message),
            };
        }
        StructureOperation::ReorderRows { .. } => unreachable!("handled above"),
        StructureOperation::SetTextCell { .. } | StructureOperation::SetTextCells { .. } => {
            unreachable!("handled above")
        }
        StructureOperation::Fill { source, target, .. } => {
            let (source_start, source_end) = source.normalized();
            let (target_start, target_end) = target.normalized();
            let area = Area {
                sheet,
                row: one_based_engine_index(source_start.row)?,
                column: one_based_engine_index(source_start.column)?,
                width: engine_positive_count(source_end.column - source_start.column + 1)?,
                height: engine_positive_count(source_end.row - source_start.row + 1)?,
            };
            if target_start.column == source_start.column
                && target_end.column == source_end.column
                && (target_start.row < source_start.row || target_end.row > source_end.row)
            {
                if target_start.row < source_start.row {
                    model
                        .auto_fill_rows(&area, one_based_engine_index(target_start.row)?)
                        .map_err(engine_error_message)?;
                }
                if target_end.row > source_end.row {
                    model
                        .auto_fill_rows(&area, one_based_engine_index(target_end.row)?)
                        .map_err(engine_error_message)?;
                }
            } else if target_start.row == source_start.row
                && target_end.row == source_end.row
                && (target_start.column < source_start.column
                    || target_end.column > source_end.column)
            {
                if target_start.column < source_start.column {
                    model
                        .auto_fill_columns(&area, one_based_engine_index(target_start.column)?)
                        .map_err(engine_error_message)?;
                }
                if target_end.column > source_end.column {
                    model
                        .auto_fill_columns(&area, one_based_engine_index(target_end.column)?)
                        .map_err(engine_error_message)?;
                }
            } else {
                return Err(crate::i18n::Message::new(
                    "status-fill-direction-unsupported",
                ));
            }
            return Ok(());
        }
    };
    let index = one_based_engine_index(index)?;
    let count = engine_positive_count(count)?;
    match (operation, axis) {
        (StructureOperation::Insert { .. }, StructureAxis::Rows) => model
            .insert_rows(sheet, index, count)
            .map_err(engine_error_message),
        (StructureOperation::Insert { .. }, StructureAxis::Columns) => model
            .insert_columns(sheet, index, count)
            .map_err(engine_error_message),
        (StructureOperation::Delete { .. }, StructureAxis::Rows) => model
            .delete_rows(sheet, index, count)
            .map_err(engine_error_message),
        (StructureOperation::Delete { .. }, StructureAxis::Columns) => model
            .delete_columns(sheet, index, count)
            .map_err(engine_error_message),
        (StructureOperation::Duplicate { .. }, StructureAxis::Rows) => {
            model
                .insert_rows(sheet, index + count, count)
                .map_err(engine_error_message)?;
            let width = table
                .cells
                .iter()
                .map(|(address, _)| address.column)
                .max()
                .unwrap_or(0)
                + 1;
            let area = Area {
                sheet,
                row: index,
                column: 1,
                width: engine_positive_count(width)?,
                height: count,
            };
            model
                .auto_fill_rows(&area, index + 2 * count - 1)
                .map_err(engine_error_message)
        }
        (StructureOperation::Duplicate { .. }, StructureAxis::Columns) => {
            model
                .insert_columns(sheet, index + count, count)
                .map_err(engine_error_message)?;
            let height = table
                .cells
                .iter()
                .map(|(address, _)| address.row)
                .max()
                .unwrap_or(0)
                + 1;
            let area = Area {
                sheet,
                row: 1,
                column: index,
                width: count,
                height: engine_positive_count(height)?,
            };
            model
                .auto_fill_columns(&area, index + 2 * count - 1)
                .map_err(engine_error_message)
        }
        _ => Ok(()),
    }
}

// Find the destination cells whose formulas must be copied by a structure operation.
pub(super) fn formula_copy_destinations(
    address: CellAddress,
    plan: &StructurePlan,
) -> Vec<CellAddress> {
    match &plan.operation {
        StructureOperation::Duplicate {
            axis, index, count, ..
        } => {
            let destination = match axis {
                StructureAxis::Rows if address.row >= *index && address.row < index + count => {
                    Some(CellAddress {
                        row: address.row + count,
                        ..address
                    })
                }
                StructureAxis::Columns
                    if address.column >= *index && address.column < index + count =>
                {
                    Some(CellAddress {
                        column: address.column + count,
                        ..address
                    })
                }
                _ => None,
            };
            destination.into_iter().collect()
        }
        StructureOperation::Fill { source, target, .. } => {
            let (source_start, source_end) = source.normalized();
            let (target_start, target_end) = target.normalized();
            if address.row < source_start.row
                || address.row > source_end.row
                || address.column < source_start.column
                || address.column > source_end.column
            {
                return Vec::new();
            }
            (target_start.row..=target_end.row)
                .flat_map(|row| {
                    (target_start.column..=target_end.column)
                        .map(move |column| CellAddress { row, column })
                })
                .filter(|destination| {
                    destination.row < source_start.row
                        || destination.row > source_end.row
                        || destination.column < source_start.column
                        || destination.column > source_end.column
                })
                .filter(|destination| source.repeated_source(*destination) == address)
                .collect()
        }
        _ => Vec::new(),
    }
}

// Keep row and column counts within the spreadsheet engine limits.
fn engine_positive_count(count: usize) -> Result<i32, crate::i18n::Message> {
    i32::try_from(count)
        .ok()
        .filter(|count| *count > 0)
        .ok_or_else(|| {
            crate::i18n::Message::new("status-calculation-index-too-large")
                .with_number("index", count as i64)
        })
}
