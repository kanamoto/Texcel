use super::*;

// Format a cell address with spreadsheet column letters and row numbers.
pub(crate) fn cell_reference(address: CellAddress) -> String {
    let mut column = address.column + 1;
    let mut letters = Vec::new();
    while column > 0 {
        let digit = (column - 1) % 26;
        letters.push((b'A' + digit as u8) as char);
        column = (column - 1) / 26;
    }
    letters.reverse();
    format!(
        "{}{}",
        letters.into_iter().collect::<String>(),
        address.row + 1
    )
}

// Parse a spreadsheet range reference into zero-based bounds.
pub(super) fn parse_merged_range(value: &str) -> Option<MergedRange> {
    let (start, end) = value.split_once(':')?;
    let start = parse_cell_reference(start)?;
    let end = parse_cell_reference(end)?;
    (start.row <= end.row && start.column <= end.column).then_some(MergedRange { start, end })
}

// Parse a spreadsheet cell reference into a zero-based address.
pub(crate) fn parse_cell_reference(value: &str) -> Option<CellAddress> {
    let value = value.trim().strip_prefix('$').unwrap_or(value.trim());
    let bytes = value.as_bytes();
    let mut split = 0;
    while split < bytes.len() && bytes[split].is_ascii_alphabetic() {
        split += 1;
    }
    if split == 0 || split == bytes.len() {
        return None;
    }
    let mut column = 0_usize;
    for byte in &bytes[..split] {
        let upper = byte.to_ascii_uppercase();
        column = column
            .checked_mul(26)?
            .checked_add(usize::from(upper - b'A' + 1))?;
    }
    let row_text = value[split..].strip_prefix('$').unwrap_or(&value[split..]);
    let row = row_text.parse::<usize>().ok()?;
    if row == 0 || column == 0 {
        return None;
    }
    let address = CellAddress {
        row: row - 1,
        column: column - 1,
    };
    (address.row < crate::document::MAX_ROWS && address.column < crate::document::MAX_COLUMNS)
        .then_some(address)
}
