use super::*;

/// Describes whether value-bearing source cells agree on one Markdown alignment.
pub(super) enum MarkdownAlignmentResolution {
    Uniform(Option<Alignment>),
    Mixed,
}

/// Resolves one imported column while ignoring its header and empty body cells.
pub(super) fn imported_markdown_alignment(
    table: &ExportTable,
    column: usize,
) -> MarkdownAlignmentResolution {
    let mut found_value = false;
    let mut resolved = None;
    for (row_index, row) in table.rows.iter().enumerate().skip(1) {
        if row.get(column).is_none_or(|cell| !has_cell_value(cell)) {
            continue;
        }
        found_value = true;
        let address = CellAddress {
            row: row_index,
            column,
        };
        let style = table.style(address);
        let alignment = match style.horizontal_alignment.as_deref() {
            None | Some("general" | "General") => None,
            Some("left" | "Left") => Some(Alignment::Left),
            Some("center" | "Center") => Some(Alignment::Center),
            Some("right" | "Right") => Some(Alignment::Right),
            Some(_) => return MarkdownAlignmentResolution::Mixed,
        };
        if let Some(previous) = resolved {
            if previous != alignment {
                return MarkdownAlignmentResolution::Mixed;
            }
        } else {
            resolved = Some(alignment);
        }
    }
    MarkdownAlignmentResolution::Uniform(if found_value {
        resolved.flatten()
    } else {
        None
    })
}

/// Ignores blank source cells when deciding whether alignment is consistent.
fn has_cell_value(cell: &Cell) -> bool {
    match &cell.input {
        CellInput::Empty => false,
        CellInput::Text(value) | CellInput::Error(value) => !value.is_empty(),
        CellInput::Number(_) | CellInput::Boolean(_) | CellInput::Formula(_) => true,
    }
}

/// Returns the Markdown separator for a column alignment.
pub(super) fn markdown_alignment_separator(alignment: Option<&Alignment>) -> &'static str {
    match alignment {
        Some(Alignment::Left) => ":---",
        Some(Alignment::Center) => ":---:",
        Some(Alignment::Right) => "---:",
        None | Some(Alignment::None) => "---",
    }
}
