//!  Worker-side operations for history growth.

use super::*;

// Estimate the added history size for a structure operation.
pub(crate) fn estimate_structure_growth(
    table: &crate::document::Table,
    operation: &StructureOperation,
) -> Result<u64, Message> {
    let mut growth = 0_u64;
    let mut include_cell =
        |address: CellAddress, cell: &crate::document::Cell| -> Result<(), Message> {
            growth = growth.saturating_add(serialized_size(&(address, cell))?);
            Ok(())
        };
    match operation {
        StructureOperation::Duplicate {
            axis, index, count, ..
        } => {
            for (address, cell) in table.stored_cells() {
                let is_source = match axis {
                    StructureAxis::Rows => address.row >= *index && address.row < index + count,
                    StructureAxis::Columns => {
                        address.column >= *index && address.column < index + count
                    }
                };
                if !is_source {
                    continue;
                }
                let output = match axis {
                    StructureAxis::Rows => CellAddress {
                        row: address.row + count,
                        ..address
                    },
                    StructureAxis::Columns => CellAddress {
                        column: address.column + count,
                        ..address
                    },
                };
                include_cell(output, cell)?;
            }
            let layout_count = match axis {
                StructureAxis::Rows => 0,
                StructureAxis::Columns => table
                    .layout
                    .columns()
                    .filter(|(column, _)| *column >= *index && *column < index + count)
                    .count(),
            };
            growth = growth.saturating_add(layout_count as u64 * 128);
        }
        StructureOperation::Fill { source, target, .. } => {
            let (source_start, source_end) = source.normalized();
            let (target_start, target_end) = target.normalized();
            let area = (target_end.row - target_start.row + 1)
                .saturating_mul(target_end.column - target_start.column + 1);
            if area > 50_000 {
                return Err(Message::new("status-fill-range-limit").with_number("limit", 50_000));
            }
            for row in target_start.row..=target_end.row {
                for column in target_start.column..=target_end.column {
                    if row >= source_start.row
                        && row <= source_end.row
                        && column >= source_start.column
                        && column <= source_end.column
                    {
                        continue;
                    }
                    let source_address = source.repeated_source(CellAddress { row, column });
                    if let Some(cell) = table.cell(source_address) {
                        include_cell(CellAddress { row, column }, cell)?;
                    }
                }
            }
        }
        StructureOperation::SetTextCell { address, cell, .. } => {
            growth = growth.saturating_add(serialized_size(&(address, cell))?);
        }
        StructureOperation::SetTextCells { changes, .. } => {
            for (address, cell) in changes {
                growth = growth.saturating_add(serialized_size(&(address, cell))?);
            }
        }
        StructureOperation::Insert { .. }
        | StructureOperation::Delete { .. }
        | StructureOperation::Move { .. }
        | StructureOperation::ReorderRows { .. } => {}
    }
    Ok(growth)
}
