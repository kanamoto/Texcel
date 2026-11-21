use super::grid::expand_grid;
use super::parser::TableBuilder;
use super::*;

// Reads positive spans and preserves the HTML meaning of rowspan=0.
pub(super) fn parse_span(value: Option<&String>) -> Option<usize> {
    let raw = value?.trim();
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let parsed = raw.parse::<usize>().ok()?;
    (parsed <= MAX_HTML_ROWS.max(MAX_HTML_COLUMNS)).then_some(parsed)
}

// Omits hidden element text from the normal table display.
pub(super) fn is_hidden_element(name: &str) -> bool {
    matches!(
        name,
        "script" | "style" | "iframe" | "xmp" | "noembed" | "noframe" | "noframes" | "plaintext"
    )
}

// Keeps empty physical rows because later cells retain their source coordinates.
pub(super) fn close_row(
    table: &mut TableBuilder,
    total_rows: &mut usize,
    element_end: usize,
    content_end: usize,
) -> Result<(), HtmlError> {
    let Some(mut row) = table.row.take() else {
        return Ok(());
    };
    *total_rows = total_rows.saturating_add(1);
    if *total_rows > MAX_HTML_ROWS || row.cells.len() > MAX_HTML_COLUMNS {
        return Err(HtmlError::ResourceLimit);
    }
    row.range.end = element_end;
    row.content_range.end = content_end;
    table.rows.push(row);
    Ok(())
}

// Expands spans into logical coordinates and marks unsafe tables read-only.
pub(super) fn finish_table(
    mut table: TableBuilder,
    tables: &mut Vec<HtmlTable>,
    diagnostics: &mut Vec<HtmlDiagnostic>,
    total_grid_cells: &mut usize,
    cell_limit: usize,
) -> Result<(), HtmlError> {
    let remaining = cell_limit.saturating_sub(*total_grid_cells);
    if table.caption_active {
        table.malformed = true;
        if let Some(start) = table.caption_start.take() {
            table.caption_range = Some(start..table.end);
        }
    }
    let source_rows = table
        .rows
        .iter()
        .map(|row| HtmlSourceRow {
            range: row.range.clone(),
            content_range: row.content_range.clone(),
            group: row.group,
            has_id: row.has_id,
            cells: row
                .cells
                .iter()
                .map(|cell| HtmlSourceCell {
                    element_range: cell.element_range.clone(),
                    tag_name: cell.tag_name.clone(),
                    has_id: cell.has_id,
                    row_span: cell.row_span,
                    column_span: cell.column_span,
                })
                .collect(),
        })
        .collect();
    let super::grid::ExpandedGrid {
        rows,
        merges,
        malformed: grid_error,
        cells: grid_cells,
    } = expand_grid(&table.rows, remaining)?;
    *total_grid_cells = total_grid_cells
        .checked_add(grid_cells)
        .ok_or(HtmlError::ResourceLimit)?;
    table.malformed |= grid_error;
    let index = tables.len();
    if table.nested {
        diagnostics.push(HtmlDiagnostic::NestedTable { table: index });
    }
    if table.malformed {
        diagnostics.push(HtmlDiagnostic::MalformedTable { table: index });
    }
    let name_generated = table.caption_range.is_none();
    tables.push(HtmlTable {
        name: if name_generated {
            format!("Table {}", index + 1)
        } else {
            table.caption_text.trim().to_owned()
        },
        name_generated,
        rows,
        merges,
        source_range: table.start..table.end,
        opening_tag_range: table.opening_tag_range,
        caption_range: table.caption_range,
        has_id: table.has_id,
        source_rows,
        nested: table.nested,
        malformed: table.malformed,
    });
    Ok(())
}
