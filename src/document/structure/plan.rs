//!  Builds zero-based source maps for prepared table structure changes.

use crate::document::{Cell, CellAddress, CellRange, MAX_COLUMNS, MAX_ROWS, Table, TableId};
use crate::i18n::Message;

mod positions;

// One cell value before and after a command.
#[derive(Clone, Debug)]
pub struct CellChange {
    pub address: CellAddress,
    pub(crate) before: Option<Cell>,
    pub(crate) after: Option<Cell>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StructureAxis {
    Rows,
    Columns,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FillMode {
    Copy,
    Series,
    /// Continues a readable progression in a one-line source and copies anything else.
    Auto,
}

/// One worker-prepared row, column, or fill action in zero-based coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum StructureOperation {
    Insert {
        table_id: TableId,
        axis: StructureAxis,
        index: usize,
        count: usize,
    },
    Delete {
        table_id: TableId,
        axis: StructureAxis,
        index: usize,
        count: usize,
    },
    Duplicate {
        table_id: TableId,
        axis: StructureAxis,
        index: usize,
        count: usize,
    },
    Move {
        table_id: TableId,
        axis: StructureAxis,
        index: usize,
        count: usize,
        destination: usize,
    },
    /// Reorders every row according to output-to-input positions.
    ReorderRows {
        table_id: TableId,
        source_row_count: usize,
        row_sources: Vec<usize>,
    },
    Fill {
        table_id: TableId,
        source: CellRange,
        target: CellRange,
        mode: FillMode,
    },
    /// Sets one sparse cell without expanding the source rectangle.
    SetTextCell {
        table_id: TableId,
        address: CellAddress,
        /// Boxed because a cell is much larger than the other operations.
        cell: Box<Cell>,
    },
    /// Sets several sparse cells as one worker-prepared undoable edit.
    SetTextCells {
        table_id: TableId,
        changes: Vec<(CellAddress, Option<Cell>)>,
        string_overrides: Vec<crate::document::StringOverride>,
        completion_status: Option<Message>,
    },
}

/// Sparse-table coordinate plan prepared on the file worker.
#[derive(Clone, Debug, PartialEq)]
pub struct StructurePlan {
    pub operation: StructureOperation,
    pub row_sources: Vec<Option<usize>>,
    pub column_sources: Vec<Option<usize>>,
}

impl StructurePlan {
    /// Builds the source-to-output mapping for a structural operation.
    pub fn build(table: &Table, operation: StructureOperation) -> Result<Self, Message> {
        if let StructureOperation::SetTextCell {
            table_id, address, ..
        } = &operation
        {
            if *table_id != table.id || address.row >= MAX_ROWS || address.column >= MAX_COLUMNS {
                return Err(Message::new("status-cell-coordinate-out-of-bounds"));
            }
            return Ok(Self {
                operation,
                row_sources: Vec::new(),
                column_sources: Vec::new(),
            });
        }
        if let StructureOperation::SetTextCells {
            table_id, changes, ..
        } = &operation
        {
            if *table_id != table.id
                || changes
                    .iter()
                    .any(|(address, _)| address.row >= MAX_ROWS || address.column >= MAX_COLUMNS)
            {
                return Err(Message::new("status-cell-coordinate-out-of-bounds"));
            }
            return Ok(Self {
                operation,
                row_sources: Vec::new(),
                column_sources: Vec::new(),
            });
        }
        let (row_count, column_count) = (table.row_count(), table.column_count());
        let mut row_sources = (0..row_count).map(Some).collect::<Vec<_>>();
        let mut column_sources = (0..column_count).map(Some).collect::<Vec<_>>();
        let (axis, kind) = match &operation {
            StructureOperation::Insert {
                table_id,
                axis,
                index,
                count,
            } => {
                if *table_id != table.id || *count == 0 {
                    return Err(Message::new("status-structure-insert-count-required"));
                }
                let size = axis_size(*axis, row_count, column_count);
                if *index > size || size.saturating_add(*count) > axis_limit(*axis) {
                    return Err(Message::new("status-structure-insert-out-of-bounds"));
                }
                (
                    Some(*axis),
                    PlanKind::Insert {
                        index: *index,
                        count: *count,
                    },
                )
            }
            StructureOperation::Delete {
                table_id,
                axis,
                index,
                count,
            } => {
                if *table_id != table.id || *count == 0 {
                    return Err(Message::new("status-structure-delete-count-required"));
                }
                let size = axis_size(*axis, row_count, column_count);
                if index.checked_add(*count).is_none_or(|end| end > size) || *count >= size {
                    return Err(Message::new("status-structure-delete-must-keep-one"));
                }
                (
                    Some(*axis),
                    PlanKind::Delete {
                        index: *index,
                        count: *count,
                    },
                )
            }
            StructureOperation::Duplicate {
                table_id,
                axis,
                index,
                count,
            } => {
                if *table_id != table.id || *count == 0 {
                    return Err(Message::new("status-structure-duplicate-count-required"));
                }
                let size = axis_size(*axis, row_count, column_count);
                if index.checked_add(*count).is_none_or(|end| end > size)
                    || size.saturating_add(*count) > axis_limit(*axis)
                {
                    return Err(Message::new("status-structure-duplicate-out-of-bounds"));
                }
                (
                    Some(*axis),
                    PlanKind::Duplicate {
                        index: *index,
                        count: *count,
                    },
                )
            }
            StructureOperation::Move {
                table_id,
                axis,
                index,
                count,
                destination,
            } => {
                if *table_id != table.id || *count == 0 {
                    return Err(Message::new("status-structure-move-count-required"));
                }
                let size = axis_size(*axis, row_count, column_count);
                if index.checked_add(*count).is_none_or(|end| end > size)
                    || *destination > size.saturating_sub(*count)
                {
                    return Err(Message::new("status-structure-move-out-of-bounds"));
                }
                (
                    Some(*axis),
                    PlanKind::Move {
                        index: *index,
                        count: *count,
                        destination: *destination,
                    },
                )
            }
            StructureOperation::ReorderRows {
                table_id,
                source_row_count,
                row_sources: output_to_source,
            } => {
                if *table_id != table.id
                    || *source_row_count != row_count
                    || output_to_source.is_empty()
                    || output_to_source.len() > row_count
                    || output_to_source.iter().any(|row| *row >= row_count)
                {
                    return Err(Message::new("status-structure-reorder-range-invalid"));
                }
                let mut seen = vec![false; row_count];
                for row in output_to_source {
                    if std::mem::replace(&mut seen[*row], true) {
                        return Err(Message::new("status-structure-reorder-duplicate-row"));
                    }
                }
                (None, PlanKind::Identity)
            }
            StructureOperation::Fill {
                table_id,
                source,
                target,
                ..
            } => {
                if *table_id != table.id {
                    return Err(Message::new("status-structure-fill-table-mismatch"));
                }
                let (source_start, source_end) = source.normalized();
                let (target_start, target_end) = target.normalized();
                if source_end.row >= row_count
                    || source_end.column >= column_count
                    || target_end.row >= row_count
                    || target_end.column >= column_count
                    || target_start.row > source_start.row
                    || target_start.column > source_start.column
                    || target_end.row < source_end.row
                    || target_end.column < source_end.column
                {
                    return Err(Message::new("status-structure-fill-range-invalid"));
                }
                (None, PlanKind::Identity)
            }
            StructureOperation::SetTextCell { .. } | StructureOperation::SetTextCells { .. } => {
                unreachable!("handled above")
            }
        };
        if let Some(axis) = axis {
            let mappings = if axis == StructureAxis::Rows {
                &mut row_sources
            } else {
                &mut column_sources
            };
            *mappings = match kind {
                PlanKind::Insert { index, count } => (0..index)
                    .map(Some)
                    .chain(std::iter::repeat_n(None, count))
                    .chain((index..mappings.len()).map(Some))
                    .collect(),
                PlanKind::Delete { index, count } => (0..index)
                    .map(Some)
                    .chain((index + count..mappings.len()).map(Some))
                    .collect(),
                PlanKind::Duplicate { index, count } => (0..index + count)
                    .map(Some)
                    .chain((index..index + count).map(Some))
                    .chain((index + count..mappings.len()).map(Some))
                    .collect(),
                PlanKind::Move {
                    index,
                    count,
                    destination,
                } => {
                    let mut moved = mappings.drain(index..index + count).collect::<Vec<_>>();
                    mappings.splice(destination..destination, moved.drain(..));
                    std::mem::take(mappings)
                }
                PlanKind::Identity => mappings.clone(),
            };
        }
        if let StructureOperation::ReorderRows {
            row_sources: output_to_source,
            ..
        } = &operation
        {
            row_sources = output_to_source.iter().copied().map(Some).collect();
        }
        Ok(Self {
            operation,
            row_sources,
            column_sources,
        })
    }

    /// Returns table dimensions after the planned operation.
    pub fn after_dimensions(&self) -> (usize, usize) {
        (
            self.row_sources.len().max(1),
            self.column_sources.len().max(1),
        )
    }
}

/// Maps one source row or column index to its new position.
fn map_source_index(value: usize, kind: Option<(u8, usize, usize, usize)>) -> Vec<usize> {
    let Some((operation, index, count, destination)) = kind else {
        return vec![value];
    };
    match operation {
        0 => vec![if value < index { value } else { value + count }],
        1 if (index..index + count).contains(&value) => Vec::new(),
        1 => vec![if value < index { value } else { value - count }],
        2 if (index..index + count).contains(&value) => vec![value, value + count],
        2 => vec![if value < index + count {
            value
        } else {
            value + count
        }],
        3 => move_output_indices(value, index, count, destination),
        _ => vec![value],
    }
}

/// Moves a range of output indices in one direction.
pub(crate) fn move_output_indices(
    source: usize,
    index: usize,
    count: usize,
    destination: usize,
) -> Vec<usize> {
    if (index..index + count).contains(&source) {
        return vec![destination + source - index];
    }
    let after_remove = if source >= index + count {
        source - count
    } else {
        source
    };
    vec![if after_remove >= destination {
        after_remove + count
    } else {
        after_remove
    }]
}

#[derive(Clone, Copy)]
enum PlanKind {
    Insert {
        index: usize,
        count: usize,
    },
    Delete {
        index: usize,
        count: usize,
    },
    Duplicate {
        index: usize,
        count: usize,
    },
    Move {
        index: usize,
        count: usize,
        destination: usize,
    },
    Identity,
}

/// Returns the size of the selected axis.
fn axis_size(axis: StructureAxis, rows: usize, columns: usize) -> usize {
    if axis == StructureAxis::Rows {
        rows
    } else {
        columns
    }
}

/// Returns the maximum allowed size for the selected axis.
fn axis_limit(axis: StructureAxis) -> usize {
    if axis == StructureAxis::Rows {
        MAX_ROWS
    } else {
        MAX_COLUMNS
    }
}

/// Returns the table ID affected by this plan.
pub(crate) fn structure_table_id(operation: &StructureOperation) -> TableId {
    match operation {
        StructureOperation::Insert { table_id, .. }
        | StructureOperation::Delete { table_id, .. }
        | StructureOperation::Duplicate { table_id, .. }
        | StructureOperation::Move { table_id, .. }
        | StructureOperation::ReorderRows { table_id, .. }
        | StructureOperation::Fill { table_id, .. }
        | StructureOperation::SetTextCell { table_id, .. }
        | StructureOperation::SetTextCells { table_id, .. } => *table_id,
    }
}

/// Updates frozen row or column counts after a structural operation.
pub(crate) fn frozen_prefix(
    output_to_source: &[Option<usize>],
    old_count: usize,
    operation: &StructureOperation,
    axis: StructureAxis,
) -> usize {
    output_to_source
        .iter()
        .enumerate()
        .take_while(|(output, source)| {
            source.is_some_and(|source| source < old_count)
                || matches!(operation,
                    StructureOperation::Insert { axis: op_axis, index, .. }
                        if *op_axis == axis && *output >= *index && *index < old_count)
        })
        .count()
}
