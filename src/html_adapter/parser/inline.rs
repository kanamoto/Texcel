use super::*;

// Recognizes the basic inline tags that map to Texcel's shared rich-text model.
pub(super) fn is_inline_format(name: &str) -> bool {
    matches!(
        name,
        "b" | "strong" | "i" | "em" | "s" | "del" | "strike" | "code"
    )
}

// Allows harmless inline containers while flagging elements that need conversion review.
pub(super) fn is_basic_inline_element(name: &str) -> bool {
    is_inline_format(name) || matches!(name, "a" | "br" | "span")
}

// Keeps table structure tags available to the outer parser state machine.
pub(super) fn is_table_structure_element(name: &str) -> bool {
    matches!(
        name,
        "table" | "caption" | "thead" | "tbody" | "tfoot" | "tr" | "td" | "th"
    )
}

// Reads the formatting represented by one supported HTML tag.
pub(super) fn format_for_tag(name: &str) -> InlineFormat {
    InlineFormat {
        tag: name.to_owned(),
        bold: matches!(name, "b" | "strong"),
        italic: matches!(name, "i" | "em"),
        strike: matches!(name, "s" | "del" | "strike"),
        code: name == "code",
        href: None,
    }
}

// Adds one display-text run with the formatting active at the source position.
pub(super) fn push_inline_text(cell: &mut CellBuilder, text: &str) {
    if cell.format_stack.iter().any(|format| format.href.is_some()) {
        cell.linked_characters = cell.linked_characters.saturating_add(text.chars().count());
    }
    let mut run = crate::document::CellTextRun {
        text: text.to_owned(),
        bold: cell.format_stack.iter().any(|format| format.bold),
        italic: cell.format_stack.iter().any(|format| format.italic),
        strike: cell.format_stack.iter().any(|format| format.strike),
        code: cell.format_stack.iter().any(|format| format.code),
        hyperlink: cell
            .format_stack
            .iter()
            .rev()
            .find_map(|format| format.href.as_ref())
            .map(|target| crate::document::CellHyperlink {
                target: target.clone(),
                tooltip: cell.link_tooltip.clone(),
                location: false,
            }),
        ..crate::document::CellTextRun::default()
    };
    if let Some(previous) = cell.rich_text.last_mut()
        && previous.bold == run.bold
        && previous.italic == run.italic
        && previous.strike == run.strike
        && previous.code == run.code
        && previous.hyperlink == run.hyperlink
    {
        previous.text.push_str(&run.text);
    } else {
        cell.rich_text.push(std::mem::take(&mut run));
    }
}

// Avoids creating rich text when the cell contains no supported formatting.
pub(super) fn has_rich_style(runs: &[crate::document::CellTextRun]) -> bool {
    runs.iter()
        .any(|run| run.bold || run.italic || run.strike || run.code || run.hyperlink.is_some())
}
