use super::*;

#[cfg(test)]
#[path = "markdown_alignment_test.rs"]
mod tests;

/// Applies source alignment before the grid's semantic numeric default.
pub(super) fn apply_markdown_alignment(
    style: &mut CellTextStyle,
    explicit: Option<crate::markdown_adapter::MarkdownTableAlignment>,
    semantic_numeric_input: bool,
    is_formula: bool,
    calculation: Option<&crate::document::CalcResult>,
) {
    if let Some(alignment) = explicit {
        style.alignment = Some(match alignment {
            crate::markdown_adapter::MarkdownTableAlignment::Left => HorizontalAlignment::Left,
            crate::markdown_adapter::MarkdownTableAlignment::Center => HorizontalAlignment::Center,
            crate::markdown_adapter::MarkdownTableAlignment::Right => HorizontalAlignment::Right,
        });
    } else if style.alignment.is_none()
        && (semantic_numeric_input
            || is_formula
                && calculation.is_some_and(|result| {
                    result.status == CalcStatus::Fresh
                        && result.value.as_ref().is_some_and(|value| {
                            matches!(value, crate::document::CalcValue::Number(_))
                        })
                }))
    {
        style.alignment = Some(HorizontalAlignment::Right);
    }
}
