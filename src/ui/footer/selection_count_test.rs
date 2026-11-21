use super::selection_count::{count_selected_cells, grouped_number};
use crate::document::{CellAddress, CellRange, MAX_COLUMNS, MAX_ROWS};

/// Counts cells shared by overlapping ranges only once.
#[test]
fn overlapping_ranges_count_shared_cells_once() {
    let ranges = [rectangle(0, 0, 2, 2), rectangle(1, 1, 3, 3)];

    assert_eq!(count_selected_cells(&ranges), 9 + 9 - 4);
}

/// Counts a whole sheet from its dimensions instead of visiting its cells.
#[test]
fn whole_sheet_count_uses_dimensions_and_groups_large_numbers() {
    let ranges = [rectangle(0, 0, MAX_ROWS - 1, MAX_COLUMNS - 1)];

    let count = count_selected_cells(&ranges);
    assert_eq!(count, (MAX_ROWS as u64) * (MAX_COLUMNS as u64));
    assert_eq!(grouped_number(count), "17,179,869,184");
}

fn rectangle(top: usize, left: usize, bottom: usize, right: usize) -> CellRange {
    CellRange {
        anchor: CellAddress {
            row: top,
            column: left,
        },
        head: CellAddress {
            row: bottom,
            column: right,
        },
    }
}
