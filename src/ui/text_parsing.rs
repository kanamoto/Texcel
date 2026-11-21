use super::*;

// Parses a cell range such as "A1:C5" typed into a data dialog.
pub(super) fn parse_cell_range_text(value: &str) -> Option<CellRange> {
    let value = value.trim();
    let (start, end) = value
        .split_once(':')
        .map_or((value, value), |(start, end)| (start, end));
    let start = crate::xlsx_adapter::parse_cell_reference(start)?;
    let end = crate::xlsx_adapter::parse_cell_reference(end)?;
    (start.row <= end.row && start.column <= end.column).then_some(CellRange {
        anchor: start,
        head: end,
    })
}
