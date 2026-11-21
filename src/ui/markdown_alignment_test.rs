use super::*;

/// Keeps an explicit source alignment ahead of the numeric default.
#[test]
fn explicit_left_alignment_wins_for_numeric_text() {
    let mut style = CellTextStyle::default();
    apply_markdown_alignment(
        &mut style,
        Some(crate::markdown_adapter::MarkdownTableAlignment::Left),
        true,
        false,
        None,
    );
    assert_eq!(style.alignment, Some(HorizontalAlignment::Left));
}

/// Keeps untyped text left-aligned while typed numbers use the numeric default.
#[test]
fn typed_values_and_text_use_distinct_default_alignment() {
    let mut text_style = CellTextStyle::default();
    apply_markdown_alignment(&mut text_style, None, false, false, None);
    assert_eq!(text_style.alignment, None);

    let mut number_style = CellTextStyle::default();
    apply_markdown_alignment(&mut number_style, None, true, false, None);
    assert_eq!(number_style.alignment, Some(HorizontalAlignment::Right));
}

/// Uses only fresh numeric formula results for the formula default.
#[test]
fn formula_numeric_default_requires_a_fresh_number() {
    let mut style = CellTextStyle::default();
    let fresh = crate::document::CalcResult {
        revision: 4,
        status: CalcStatus::Fresh,
        value: Some(crate::document::CalcValue::Number(12.0)),
    };
    apply_markdown_alignment(&mut style, None, false, true, Some(&fresh));
    assert_eq!(style.alignment, Some(HorizontalAlignment::Right));

    let mut style = CellTextStyle::default();
    let stale = crate::document::CalcResult {
        status: CalcStatus::Stale,
        ..fresh
    };
    apply_markdown_alignment(&mut style, None, false, true, Some(&stale));
    assert_eq!(style.alignment, None);
}
