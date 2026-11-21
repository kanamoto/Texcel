use super::*;

/// Returns the selected row indices that fall inside the table extent.
pub(super) fn selected_rows(range: CellRange, row_count: usize) -> Vec<usize> {
    let (start, end) = range.normalized();
    if row_count == 0 || start.row >= row_count {
        return Vec::new();
    }
    let end_row = end.row.min(row_count.saturating_sub(1));
    (start.row..=end_row).collect()
}

/// Counts selected rows inside the table extent without building a list.
pub(super) fn selected_row_count(range: CellRange, row_count: usize) -> usize {
    let (start, end) = range.normalized();
    if row_count == 0 || start.row >= row_count {
        return 0;
    }
    let end_row = end.row.min(row_count.saturating_sub(1));
    end_row.saturating_sub(start.row).saturating_add(1)
}
