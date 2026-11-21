use crate::document::{CellAddress, StructureAxis, StructureOperation};

use super::{StructurePlan, map_source_index};

impl StructurePlan {
    /// Lists output positions for a source address.
    pub fn output_positions(&self, source: CellAddress) -> Vec<CellAddress> {
        if matches!(&self.operation, StructureOperation::ReorderRows { .. }) {
            if let StructureOperation::ReorderRows {
                source_row_count, ..
            } = &self.operation
                && source.row >= *source_row_count
            {
                return vec![source];
            }
            return self
                .row_sources
                .iter()
                .enumerate()
                .filter_map(|(output, input)| {
                    (*input == Some(source.row)).then_some(CellAddress {
                        row: output,
                        column: source.column,
                    })
                })
                .collect();
        }
        let (axis, kind) = match &self.operation {
            StructureOperation::Insert {
                axis, index, count, ..
            } => (*axis, Some((0, *index, *count, 0))),
            StructureOperation::Delete {
                axis, index, count, ..
            } => (*axis, Some((1, *index, *count, 0))),
            StructureOperation::Duplicate {
                axis, index, count, ..
            } => (*axis, Some((2, *index, *count, 0))),
            StructureOperation::Move {
                axis,
                index,
                count,
                destination,
                ..
            } => (*axis, Some((3, *index, *count, *destination))),
            StructureOperation::ReorderRows { .. } => unreachable!("handled above"),
            StructureOperation::Fill { .. }
            | StructureOperation::SetTextCell { .. }
            | StructureOperation::SetTextCells { .. } => return vec![source],
        };
        let (rows, columns) = if axis == StructureAxis::Rows {
            (map_source_index(source.row, kind), vec![source.column])
        } else {
            (vec![source.row], map_source_index(source.column, kind))
        };
        rows.into_iter()
            .flat_map(|row| {
                columns
                    .iter()
                    .copied()
                    .map(move |column| CellAddress { row, column })
            })
            .collect()
    }
}
