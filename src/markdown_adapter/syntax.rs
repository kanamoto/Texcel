use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct MarkdownRowSyntax {
    pub(super) range: Range<usize>,
    pub(super) leading_space: String,
    pub(super) trailing_space: String,
    pub(super) has_leading_pipe: bool,
    pub(super) has_trailing_pipe: bool,
    pub(super) source_cell_count: usize,
    pub(super) cell_ranges: Vec<Range<usize>>,
}

/// Finds the cell text range without outer syntax padding.
pub(super) fn trim_cell_range(text: &str, range: Range<usize>) -> Range<usize> {
    let cell = &text[range.clone()];
    let leading = cell.len() - cell.trim_start_matches([' ', '\t']).len();
    let trimmed = cell.trim_matches([' ', '\t']);
    let start = range.start + leading;
    start..start + trimmed.len()
}

/// Reads the delimiter and padding form used by a table row.
pub(super) fn row_syntax(text: &str, parser_range: Range<usize>) -> MarkdownRowSyntax {
    let line_start = text[..parser_range.start]
        .rfind('\n')
        .map_or(0, |newline| newline + 1);
    let line_end = text[line_start..]
        .find('\n')
        .map_or(text.len(), |newline| line_start + newline);
    let mut line_end = line_end;
    if line_end > line_start && text.as_bytes()[line_end - 1] == b'\r' {
        line_end -= 1;
    }
    let bom_length = if line_start == 0 && text.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        0
    };
    let content_start = line_start + bom_length;
    let line = &text[content_start..line_end];
    let leading_length = line.len() - line.trim_start_matches([' ', '\t']).len();
    let trimmed = line.trim_matches([' ', '\t']);
    let trailing_length = line.len() - leading_length - trimmed.len();
    let mut pipe_positions = Vec::new();
    let mut preceding_backslashes = 0_usize;
    for (index, character) in trimmed.char_indices() {
        if character == '|' && preceding_backslashes == 0 {
            pipe_positions.push(index);
        }
        if character == '\\' {
            preceding_backslashes += 1;
        } else {
            preceding_backslashes = 0;
        }
    }
    let has_leading_pipe = pipe_positions.first() == Some(&0);
    let has_trailing_pipe = pipe_positions.last() == trimmed.len().checked_sub(1).as_ref();
    let mut cell_ranges = Vec::new();
    let mut cell_start = usize::from(has_leading_pipe);
    for (index, pipe) in pipe_positions.iter().copied().enumerate() {
        if has_leading_pipe && index == 0 {
            continue;
        }
        if has_trailing_pipe && index + 1 == pipe_positions.len() {
            cell_ranges.push(
                content_start + leading_length + cell_start..content_start + leading_length + pipe,
            );
        } else {
            cell_ranges.push(
                content_start + leading_length + cell_start..content_start + leading_length + pipe,
            );
            cell_start = pipe + 1;
        }
    }
    if pipe_positions.is_empty() || !has_trailing_pipe {
        cell_ranges.push(
            content_start + leading_length + cell_start
                ..content_start + leading_length + trimmed.len(),
        );
    }
    if cell_ranges.is_empty() {
        cell_ranges
            .push(content_start + leading_length..content_start + leading_length + trimmed.len());
    }
    let source_cell_count = cell_ranges.len();

    MarkdownRowSyntax {
        range: content_start..line_end,
        leading_space: line[..leading_length].to_owned(),
        trailing_space: line[line.len() - trailing_length..].to_owned(),
        has_leading_pipe,
        has_trailing_pipe,
        source_cell_count,
        cell_ranges,
    }
}

/// Returns the padding that this row style puts around cell text, so new cells match it.
///
/// A row that pads its cells gets one space on that side; column-aligning runs of spaces
/// are not copied, since Texcel never aligns column widths.
pub(super) fn row_cell_padding(text: &str, syntax: &MarkdownRowSyntax) -> (String, String) {
    let single = |space: &str| if space.is_empty() { "" } else { " " }.to_owned();
    for range in &syntax.cell_ranges {
        let trimmed = trim_cell_range(text, range.clone());
        if trimmed.is_empty() {
            continue;
        }
        if let (Some(left), Some(right)) = (
            text.get(range.start..trimmed.start),
            text.get(trimmed.end..range.end),
        ) {
            return (single(left), single(right));
        }
    }
    if syntax.has_leading_pipe || syntax.has_trailing_pipe {
        (" ".to_owned(), " ".to_owned())
    } else {
        (String::new(), String::new())
    }
}

/// Builds a new cell with the same padding as the rest of its row.
fn padded_new_cell(padding: &(String, String), value: Option<&String>) -> String {
    let content = value
        .map(|value| escape_table_cell(value))
        .unwrap_or_default();
    format!("{}{}{}", padding.0, content, padding.1)
}

/// Writes a changed value into an existing cell by the one output rule for written cells.
///
/// The value gets the row's padding (one space per padded side, see `row_cell_padding`)
/// instead of the old cell's spaces, so an empty `|  |` or a column-aligned cell does not
/// leave uneven spaces. A cell whose text does not change keeps its source bytes.
fn replaced_field(
    text: &str,
    source_range: &Range<usize>,
    cell: &MarkdownCell,
    value: &str,
    padding: &(String, String),
) -> Result<String, MarkdownError> {
    let replacement = escape_table_cell(value);
    let current = text
        .get(cell.source_range.clone())
        .ok_or(MarkdownError::OutputMismatch)?;
    if current == replacement {
        return Ok(text
            .get(source_range.clone())
            .ok_or(MarkdownError::OutputMismatch)?
            .to_owned());
    }
    Ok(format!("{}{}{}", padding.0, replacement, padding.1))
}

/// Rebuilds a changed row without changing its surrounding source.
pub(super) fn rebuild_row_text(
    text: &str,
    row: &[MarkdownCell],
    syntax: &MarkdownRowSyntax,
    changes: &std::collections::BTreeMap<usize, String>,
) -> Result<String, MarkdownError> {
    let width = changes
        .keys()
        .next_back()
        .map_or(syntax.source_cell_count, |column| {
            syntax.source_cell_count.max(column + 1)
        });
    let padding = row_cell_padding(text, syntax);
    let mut values = Vec::with_capacity(width);
    for column in 0..width {
        match (syntax.cell_ranges.get(column), changes.get(&column)) {
            (Some(source_range), Some(value)) => {
                let cell = row.get(column).ok_or(MarkdownError::OutputMismatch)?;
                values.push(replaced_field(text, source_range, cell, value, &padding)?);
            }
            (Some(source_range), None) => {
                values.push(
                    text.get(source_range.clone())
                        .ok_or(MarkdownError::OutputMismatch)?
                        .to_owned(),
                );
            }
            (None, value) => values.push(padded_new_cell(&padding, value)),
        }
    }
    let has_leading_pipe =
        syntax.has_leading_pipe || changes.get(&0).is_some_and(|value| value.trim().is_empty());
    let has_trailing_pipe = syntax.has_trailing_pipe
        || changes
            .get(&(width.saturating_sub(1)))
            .is_some_and(|value| value.trim().is_empty());
    let mut line = String::with_capacity(values.iter().map(String::len).sum::<usize>() + width + 2);
    line.push_str(&syntax.leading_space);
    if has_leading_pipe {
        line.push('|');
    }
    line.push_str(&values.join("|"));
    if has_trailing_pipe {
        line.push('|');
    }
    line.push_str(&syntax.trailing_space);
    Ok(line)
}

/// Rewrites a row to the requested width while retaining untouched source cell bytes.
pub(super) fn rebuild_row_to_width(
    text: &str,
    row: &[MarkdownCell],
    syntax: &MarkdownRowSyntax,
    changes: Option<&std::collections::BTreeMap<usize, String>>,
    width: usize,
) -> Result<String, MarkdownError> {
    let padding = row_cell_padding(text, syntax);
    let mut values = Vec::with_capacity(width);
    for column in 0..width {
        let change = changes.and_then(|changes| changes.get(&column));
        match (syntax.cell_ranges.get(column), row.get(column), change) {
            (Some(source_range), Some(cell), Some(value)) => {
                values.push(replaced_field(text, source_range, cell, value, &padding)?);
            }
            (Some(source_range), Some(_), None) => values.push(
                text.get(source_range.clone())
                    .ok_or(MarkdownError::OutputMismatch)?
                    .to_owned(),
            ),
            (_, _, value) => values.push(padded_new_cell(&padding, value)),
        }
    }
    let mut line = String::with_capacity(
        syntax.leading_space.len()
            + syntax.trailing_space.len()
            + values.iter().map(String::len).sum::<usize>()
            + width
            + 2,
    );
    line.push_str(&syntax.leading_space);
    let expanded_empty_edges = width > syntax.source_cell_count;
    let expanded_empty_head =
        expanded_empty_edges && values.first().is_some_and(|value| value.trim().is_empty());
    let expanded_empty_tail =
        expanded_empty_edges && values.last().is_some_and(|value| value.trim().is_empty());
    if syntax.has_leading_pipe || expanded_empty_head {
        line.push('|');
    }
    line.push_str(&values.join("|"));
    if syntax.has_trailing_pipe || expanded_empty_tail {
        line.push('|');
    }
    line.push_str(&syntax.trailing_space);
    Ok(line)
}

/// Updates a table separator row after a structure edit.
pub(super) fn transform_separator(line: &str, column_sources: &[Option<usize>]) -> String {
    let leading_length = line.len() - line.trim_start_matches([' ', '\t']).len();
    let trimmed = line.trim_matches([' ', '\t']);
    let trailing_length = line.len() - leading_length - trimmed.len();
    let has_leading_pipe = trimmed.starts_with('|');
    let has_trailing_pipe = trimmed.ends_with('|');
    let inner_start = usize::from(has_leading_pipe);
    let inner_end = trimmed.len().saturating_sub(usize::from(has_trailing_pipe));
    let cells = trimmed
        .get(inner_start..inner_end)
        .unwrap_or_default()
        .split('|')
        .collect::<Vec<_>>();
    let template = cells.first().copied().unwrap_or("---");
    let leading_padding = template.len() - template.trim_start_matches([' ', '\t']).len();
    let trailing_padding =
        template.len() - leading_padding - template.trim_matches([' ', '\t']).len();
    let default_cell = format!(
        "{}---{}",
        &template[..leading_padding],
        &template[template.len().saturating_sub(trailing_padding)..]
    );
    let mut output = String::new();
    output.push_str(&line[..leading_length]);
    if has_leading_pipe || line.is_empty() {
        output.push('|');
    }
    for (index, source) in column_sources.iter().enumerate() {
        if index > 0 {
            output.push('|');
        }
        output.push_str(
            source
                .and_then(|source| cells.get(source).copied())
                .unwrap_or(&default_cell),
        );
    }
    if has_trailing_pipe || line.is_empty() {
        output.push('|');
    }
    if trailing_length > 0 {
        output.push_str(&line[line.len() - trailing_length..]);
    }
    output
}

/// Escapes characters that would change Markdown table structure.
pub(super) fn escape_table_cell(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    let mut backslashes = 0;
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\r' => {
                if characters.peek() == Some(&'\n') {
                    characters.next();
                }
                escaped.push_str("<br>");
                backslashes = 0;
            }
            '\n' => {
                escaped.push_str("<br>");
                backslashes = 0;
            }
            '|' if backslashes == 0 => {
                escaped.push('\\');
                escaped.push('|');
                backslashes = 0;
            }
            '|' => {
                escaped.push('|');
                backslashes = 0;
            }
            character => {
                escaped.push(character);
                if character == '\\' {
                    backslashes += 1;
                } else {
                    backslashes = 0;
                }
            }
        }
    }
    // Place a space before a column delimiter to separate it from trailing backslashes.
    // Do not add extra backslashes, so the reloaded text still matches the original.
    if backslashes > 0 {
        escaped.push(' ');
    }
    escaped
}
