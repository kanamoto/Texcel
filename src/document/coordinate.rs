//!  Zero-based cell addresses, selections, and merged-range geometry.

use serde::{Deserialize, Serialize};

// Identifies a cell by zero-based row and column numbers.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct CellAddress {
    pub row: usize,
    pub column: usize,
}

// Stores the start and current position of a range selection.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct CellRange {
    pub anchor: CellAddress,
    pub head: CellAddress,
}

/// Inclusive bounds of a merged range.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct MergedRange {
    pub start: CellAddress,
    pub end: CellAddress,
}

impl MergedRange {
    // Check whether a cell address lies inside this merged range.
    pub fn contains(self, address: CellAddress) -> bool {
        address.row >= self.start.row
            && address.row <= self.end.row
            && address.column >= self.start.column
            && address.column <= self.end.column
    }

    // Check whether two merged ranges share at least one cell.
    pub fn intersects(self, other: Self) -> bool {
        self.start.row <= other.end.row
            && other.start.row <= self.end.row
            && self.start.column <= other.end.column
            && other.start.column <= self.end.column
    }
}

/// Helpers for a selected rectangle.
impl CellRange {
    // Selects one cell.
    pub fn single(address: CellAddress) -> Self {
        Self {
            anchor: address,
            head: address,
        }
    }

    // Returns the range from top-left to bottom-right regardless of drag direction.
    pub fn normalized(self) -> (CellAddress, CellAddress) {
        (
            CellAddress {
                row: self.anchor.row.min(self.head.row),
                column: self.anchor.column.min(self.head.column),
            },
            CellAddress {
                row: self.anchor.row.max(self.head.row),
                column: self.anchor.column.max(self.head.column),
            },
        )
    }

    /// Returns the source cell repeated by a fill operation, including fills before the seed.
    pub(crate) fn repeated_source(self, destination: CellAddress) -> CellAddress {
        let (start, end) = self.normalized();
        let height = end.row - start.row + 1;
        let width = end.column - start.column + 1;
        let row_offset = (destination.row as i128 - start.row as i128).rem_euclid(height as i128);
        let column_offset =
            (destination.column as i128 - start.column as i128).rem_euclid(width as i128);
        CellAddress {
            row: start.row + row_offset as usize,
            column: start.column + column_offset as usize,
        }
    }
}
