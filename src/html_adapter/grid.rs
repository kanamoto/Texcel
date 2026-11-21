use super::{CellAddress, HtmlCell, HtmlError, MAX_HTML_COLUMNS, MergedRange};

// One physical source row before spans expand into logical grid positions.
#[derive(Default)]
pub(super) struct RowBuilder {
    pub(super) cells: Vec<HtmlCell>,
    pub(super) group: usize,
    pub(super) range: std::ops::Range<usize>,
    pub(super) content_range: std::ops::Range<usize>,
    pub(super) has_id: bool,
}

/// The logical grid of one HTML table after row and column spans are expanded.
pub(super) struct ExpandedGrid {
    pub(super) rows: Vec<Vec<HtmlCell>>,
    pub(super) merges: Vec<MergedRange>,
    /// True when spans overlapped or ran outside their row group.
    pub(super) malformed: bool,
    /// Number of grid positions the spans filled, counted toward the import limit.
    pub(super) cells: usize,
}

// Places source cells into the table model, including row-group span boundaries.
pub(super) fn expand_grid(
    rows: &[RowBuilder],
    remaining: usize,
) -> Result<ExpandedGrid, HtmlError> {
    let mut grid: Vec<Vec<Option<HtmlCell>>> = vec![Vec::new(); rows.len()];
    let mut merges = Vec::new();
    let mut malformed = false;
    let mut expanded_slots = 0_usize;
    let mut group_ends = vec![rows.len(); rows.len()];
    let mut group_start = 0;
    while group_start < rows.len() {
        let mut group_end = group_start + 1;
        while group_end < rows.len() && rows[group_end].group == rows[group_start].group {
            group_end += 1;
        }
        group_ends[group_start..group_end].fill(group_end);
        group_start = group_end;
    }
    for (row_index, row) in rows.iter().enumerate() {
        let group_end = group_ends[row_index];
        let mut column = 0_usize;
        for source_cell in &row.cells {
            while grid[row_index].get(column).is_some_and(Option::is_some) {
                column += 1;
            }
            let row_span = if source_cell.row_span == 0 {
                group_end.saturating_sub(row_index).max(1)
            } else {
                source_cell.row_span
            };
            let requested_row_end = row_index.saturating_add(row_span);
            let row_end = requested_row_end.min(rows.len());
            let column_end = column.saturating_add(source_cell.column_span);
            if source_cell.column_span == 0 || column_end > MAX_HTML_COLUMNS {
                return Err(HtmlError::ResourceLimit);
            }
            if requested_row_end > group_end {
                malformed = true;
                ensure_grid_width(
                    &mut grid,
                    row_index,
                    column + 1,
                    &mut expanded_slots,
                    remaining,
                )?;
                grid[row_index][column] = Some(unspanned_cell(source_cell));
                column += 1;
                continue;
            }
            let span_area = row_span
                .checked_mul(source_cell.column_span)
                .ok_or(HtmlError::ResourceLimit)?;
            if span_area > remaining.saturating_sub(expanded_slots) {
                return Err(HtmlError::ResourceLimit);
            }
            let collision = (row_index..row_end).any(|covered_row| {
                (column..column_end).any(|covered_column| {
                    grid[covered_row]
                        .get(covered_column)
                        .is_some_and(Option::is_some)
                })
            });
            if collision {
                malformed = true;
                ensure_grid_width(
                    &mut grid,
                    row_index,
                    column + 1,
                    &mut expanded_slots,
                    remaining,
                )?;
                grid[row_index][column] = Some(unspanned_cell(source_cell));
                column += 1;
                continue;
            }
            ensure_grid_width(
                &mut grid,
                row_index,
                column_end,
                &mut expanded_slots,
                remaining,
            )?;
            if row_span > 1 || source_cell.column_span > 1 {
                merges.push(MergedRange {
                    start: CellAddress {
                        row: row_index,
                        column,
                    },
                    end: CellAddress {
                        row: row_end - 1,
                        column: column_end - 1,
                    },
                });
            }
            grid[row_index][column] = Some(source_cell.clone());
            for covered_row in row_index..row_end {
                ensure_grid_width(
                    &mut grid,
                    covered_row,
                    column_end,
                    &mut expanded_slots,
                    remaining,
                )?;
                for (covered_column, slot) in grid[covered_row]
                    .iter_mut()
                    .enumerate()
                    .take(column_end)
                    .skip(column)
                {
                    if covered_row == row_index && covered_column == column {
                        continue;
                    }
                    if slot.is_some() {
                        malformed = true;
                    } else {
                        *slot = Some(HtmlCell::placeholder());
                    }
                }
            }
            column = column_end;
        }
    }
    let rows = grid
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|cell| cell.unwrap_or_else(HtmlCell::placeholder))
                .collect()
        })
        .collect();
    Ok(ExpandedGrid {
        rows,
        merges,
        malformed,
        cells: expanded_slots,
    })
}

// Grows one logical row without exceeding the configured grid width.
fn ensure_grid_width(
    grid: &mut [Vec<Option<HtmlCell>>],
    row: usize,
    width: usize,
    expanded_slots: &mut usize,
    remaining: usize,
) -> Result<(), HtmlError> {
    if width > MAX_HTML_COLUMNS {
        return Err(HtmlError::ResourceLimit);
    }
    if width <= grid[row].len() {
        return Ok(());
    }
    let added = width.saturating_sub(grid[row].len());
    *expanded_slots = expanded_slots
        .checked_add(added)
        .ok_or(HtmlError::ResourceLimit)?;
    if *expanded_slots > remaining {
        return Err(HtmlError::ResourceLimit);
    }
    grid[row].resize_with(width, || None);
    Ok(())
}

// Keeps invalid span content visible while the table remains read-only.
fn unspanned_cell(source: &HtmlCell) -> HtmlCell {
    let mut cell = source.clone();
    cell.row_span = 1;
    cell.column_span = 1;
    cell
}

impl HtmlCell {
    // Creates a non-editable placeholder covered by a merged source cell.
    pub(super) fn placeholder() -> Self {
        Self {
            value: String::new(),
            display_value: String::new(),
            alignment: None,
            source_range: 0..0,
            rich_text: None,
            hyperlink: None,
            has_partial_link: false,
            has_unsupported_markup: false,
            element_range: 0..0,
            tag_name: String::new(),
            has_id: false,
            row_span: 1,
            column_span: 1,
            placeholder: true,
        }
    }
}
