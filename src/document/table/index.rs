//!  Keeps the occupied edge of each row and column in sparse runs.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::document::{Cell, CellAddress, CellInput};

/// Inclusive occupied runs on one grid axis, updated alongside sparse cell edits.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct AxisRuns {
    runs: BTreeMap<usize, usize>,
}

impl AxisRuns {
    /// Visits occupied positions without scanning empty parts of a sparse axis.
    pub(super) fn positions(&self) -> impl Iterator<Item = usize> + '_ {
        self.runs.iter().flat_map(|(start, end)| *start..=(*end))
    }

    /// Updates a row or column entry in the sparse index.
    fn set(&mut self, position: usize, occupied: bool) {
        let previous = self
            .runs
            .range(..=position)
            .next_back()
            .map(|(start, end)| (*start, *end));
        if occupied {
            if previous.is_some_and(|(_, end)| end >= position) {
                return;
            }
            let left = previous.filter(|(_, end)| end.checked_add(1) == Some(position));
            let right = self
                .runs
                .range(position.saturating_add(1)..)
                .next()
                .map(|(start, end)| (*start, *end))
                .filter(|(start, _)| *start == position.saturating_add(1));
            match (left, right) {
                (Some((left_start, _)), Some((right_start, right_end))) => {
                    self.runs.remove(&left_start);
                    self.runs.remove(&right_start);
                    self.runs.insert(left_start, right_end);
                }
                (Some((left_start, _)), None) => {
                    self.runs.insert(left_start, position);
                }
                (None, Some((right_start, right_end))) => {
                    self.runs.remove(&right_start);
                    self.runs.insert(position, right_end);
                }
                (None, None) => {
                    self.runs.insert(position, position);
                }
            }
        } else if let Some((start, end)) = previous.filter(|(_, end)| position <= *end) {
            self.runs.remove(&start);
            if start < position {
                self.runs.insert(start, position - 1);
            }
            if position < end {
                self.runs.insert(position + 1, end);
            }
        }
    }

    /// Finds the edge of stored data in a requested region.
    pub(super) fn region_edge(&self, position: usize, delta: isize, axis_max: usize) -> usize {
        let containing = self
            .runs
            .range(..=position)
            .next_back()
            .filter(|(_, end)| position <= **end)
            .map(|(start, end)| (*start, *end));
        if delta > 0 {
            if let Some((_, end)) = containing {
                if position < end {
                    return end;
                }
                return self
                    .runs
                    .range((std::ops::Bound::Excluded(end), std::ops::Bound::Unbounded))
                    .next()
                    .map_or(axis_max, |(start, _)| *start);
            }
            self.runs
                .range((
                    std::ops::Bound::Excluded(position),
                    std::ops::Bound::Unbounded,
                ))
                .next()
                .map_or(axis_max, |(start, _)| *start)
        } else {
            if let Some((start, _)) = containing {
                if position > start {
                    return start;
                }
                return self
                    .runs
                    .range(..start)
                    .next_back()
                    .map_or(0, |(_, end)| *end);
            }
            self.runs
                .range(..position)
                .next_back()
                .map_or(0, |(_, end)| *end)
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct AxisIndex {
    pub(super) rows: BTreeMap<usize, AxisRuns>,
    pub(super) columns: BTreeMap<usize, AxisRuns>,
}

impl AxisIndex {
    /// Builds row and column indexes from stored cells.
    pub(super) fn from_cells(cells: &BTreeMap<CellAddress, Cell>) -> Self {
        let mut index = Self::default();
        for (address, cell) in cells {
            if cell_counts_as_data(cell) {
                index.set(*address, true);
            }
        }
        index
    }

    /// Updates a row or column entry in the sparse index.
    pub(super) fn set(&mut self, address: CellAddress, occupied: bool) {
        for (lines, line, position) in [
            (&mut self.rows, address.row, address.column),
            (&mut self.columns, address.column, address.row),
        ] {
            let runs = lines.entry(line).or_default();
            runs.set(position, occupied);
            if runs.runs.is_empty() {
                lines.remove(&line);
            }
        }
    }
}

/// Checks whether a cell contributes to logical table dimensions.
pub(super) fn cell_counts_as_data(cell: &Cell) -> bool {
    !matches!(cell.input, CellInput::Empty)
}
