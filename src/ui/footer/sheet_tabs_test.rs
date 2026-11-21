use super::*;
use crate::document::{Cell, Table};

/// Builds a table whose listed cells hold the given text.
fn table_with(cells: &[((usize, usize), &str)]) -> Table {
    let mut table = Table::new(1, "Summary");
    for ((row, column), text) in cells {
        table.set_loaded_cell(
            CellAddress {
                row: *row,
                column: *column,
            },
            Cell::text(*text),
        );
    }
    table
}

#[test]
/// Uses the value nearest to A1 when A1 itself is blank.
fn summary_uses_the_nearest_value_when_a1_is_blank() {
    // C1 is 2 away and B3 is about 2.24 away, so C1 wins although it is in a later column.
    let table = table_with(&[((0, 0), ""), ((2, 1), "below"), ((0, 2), "right")]);
    assert_eq!(table_summary(&table).as_deref(), Some("right"));
}

#[test]
/// Prefers the upper cell when two values are equally far from A1.
fn summary_breaks_distance_ties_by_row() {
    let table = table_with(&[((0, 1), "top"), ((1, 0), "left")]);
    assert_eq!(table_summary(&table).as_deref(), Some("top"));
}

#[test]
/// Reports no summary for a table without values, and shortens long values.
fn summary_is_empty_or_shortened() {
    assert_eq!(table_summary(&table_with(&[((3, 3), "")])), None);
    let long = "x".repeat(40);
    let table = table_with(&[((5, 5), &long)]);
    assert_eq!(table_summary(&table), Some(format!("{}…", "x".repeat(28))));
}
