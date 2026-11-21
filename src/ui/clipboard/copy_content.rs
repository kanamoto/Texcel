use super::*;

/// Resolves the text shown for one cell before it is copied.
pub(super) fn displayed_text(
    cell: &Cell,
    calculation: Option<&crate::document::CalcResult>,
    markdown_text: Option<&str>,
) -> String {
    if let Some(markdown_text) = markdown_text {
        return markdown_text.to_owned();
    }
    let value = cell.display_text_with_calculation(calculation);
    if let CellInput::Text(raw) = &cell.input
        && crate::document::literal_text_value(raw) != raw
    {
        return crate::document::literal_text_value(raw).to_owned();
    }
    value
}

/// Replaces an explicit result copy with the value that the display represents.
pub(super) fn replace_with_displayed_result(
    cell: &mut crate::clipboard::ClipboardCell,
    display_text: &str,
) -> bool {
    if cell.formula_source {
        let Some(value) = cell.fresh_formula_value.take() else {
            return false;
        };
        cell.input = value;
    } else {
        cell.input = CellInput::Text(crate::document::encode_literal_text(display_text));
    }
    cell.formula_source = false;
    cell.raw_markdown_source = false;
    cell.markdown_plain_text = None;
    cell.markdown_has_features = false;
    true
}

#[cfg(test)]
#[path = "copy_content_test.rs"]
mod tests;
