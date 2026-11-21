use super::*;

#[test]
// Ensures an explicit result copy pastes the fresh typed value instead of its formula.
fn result_copy_clears_formula_source_and_keeps_the_fresh_value_type() {
    let mut cell = crate::clipboard::ClipboardCell::from(&Cell::text("=1+2"));
    cell.input = CellInput::Formula("=1+2".to_owned());
    cell.formula_source = true;
    cell.fresh_formula_value = Some(CellInput::Number(3.0));
    cell.raw_markdown_source = true;
    cell.markdown_plain_text = Some("3".to_owned());

    assert!(replace_with_displayed_result(&mut cell, "3"));

    assert_eq!(cell.input, CellInput::Number(3.0));
    assert!(!cell.formula_source);
    assert!(cell.fresh_formula_value.is_none());
    assert!(!cell.raw_markdown_source);
    assert!(cell.markdown_plain_text.is_none());
}

#[test]
// Refuses a result copy when the formula does not have a fresh value.
fn result_copy_requires_a_fresh_formula_value() {
    let mut cell = crate::clipboard::ClipboardCell::from(&Cell::text("=1+2"));
    cell.input = CellInput::Formula("=1+2".to_owned());
    cell.formula_source = true;

    assert!(!replace_with_displayed_result(&mut cell, "=1+2"));
    assert!(matches!(cell.input, CellInput::Formula(ref formula) if formula == "=1+2"));
    assert!(cell.formula_source);
}

#[test]
// Stores a formula-like text result as a marked literal in the internal payload.
fn result_copy_marks_formula_like_text_as_a_literal() {
    let mut cell = crate::clipboard::ClipboardCell::from(&Cell::text("=A1"));
    cell.input = CellInput::Formula("=A1".to_owned());
    cell.formula_source = true;
    cell.fresh_formula_value = Some(CellInput::Text(crate::document::encode_literal_text(
        "=literal result",
    )));

    assert!(replace_with_displayed_result(&mut cell, "=literal result"));
    assert_eq!(cell.input, CellInput::Text("'=literal result".to_owned()));
    assert!(!cell.formula_source);
}

#[test]
// Keeps formula-looking source text as a literal when the displayed result is copied.
fn result_copy_marks_non_formula_text_as_a_literal() {
    let mut cell = crate::clipboard::ClipboardCell::from(&Cell::text("=A1"));

    assert!(replace_with_displayed_result(&mut cell, "=A1"));

    assert_eq!(cell.input, CellInput::Text("'=A1".to_owned()));
    assert!(!cell.formula_source);
}
