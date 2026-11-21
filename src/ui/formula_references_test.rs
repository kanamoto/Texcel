use super::*;

// Builds a formula edit with its cursor at the requested UTF-8 byte offset.
fn formula_edit(formula: &str, cursor: usize) -> EditSession {
    let mut edit = EditSession::default();
    edit.begin(
        1,
        CellAddress::default(),
        CellInput::Formula(formula.to_owned()),
        crate::document::EditTarget::Cell,
    );
    edit.cursor_byte = cursor;
    edit.anchor_byte = cursor;
    edit
}

// Verifies that lexer character positions map to correct UTF-8 token bytes.
#[test]
fn formula_tokens_keep_utf8_positions_and_hidden_out_of_extent_references() {
    let formula = "=SUM(\"日本語\",A1:B2)";
    let tokens = parse_formula_references(formula, 2, 2);
    let range_start = formula.find("A1:B2").expect("the range is in the formula");
    let range = tokens
        .iter()
        .find(|token| &formula[token.bytes.clone()] == "A1:B2")
        .expect("IronCalc identifies the range");
    assert_eq!(range.bytes, range_start..range_start + "A1:B2".len());
    assert_eq!(range.cells.normalized().0, CellAddress::default());
    assert!(range.overlay.is_some());

    let far_formula = "=B100";
    let far_edit = formula_edit(far_formula, far_formula.len());
    let mut state = FormulaReferenceState::default();
    state.refresh(
        Some(1),
        Some(1),
        Some(CellAddress::default()),
        &far_edit,
        2,
        2,
    );
    assert_eq!(state.tokens.len(), 1);
    assert_eq!(
        state.tokens[0].cells.anchor,
        CellAddress { row: 99, column: 1 }
    );
    assert!(state.overlays().is_empty());
}

// Verifies IronCalc's complete F4 cycle after text that uses multibyte characters.
#[test]
fn formula_reference_cycle_uses_ironcalc_cursor_and_absolute_rules() {
    let formula = "=SUM(\"日本語\",A1)";
    let mut edit = formula_edit(formula, formula.find("A1").unwrap() + 2);
    let mut state = FormulaReferenceState::default();
    let target = CellAddress::default();
    state.refresh(Some(1), Some(1), Some(target), &edit, 4, 4);
    for expected in [
        "=SUM(\"日本語\",$A$1)",
        "=SUM(\"日本語\",A$1)",
        "=SUM(\"日本語\",$A1)",
        "=SUM(\"日本語\",A1)",
    ] {
        assert_eq!(
            state.cycle_reference(&mut edit),
            Some(CellRange::single(target))
        );
        assert_eq!(edit.buffer, expected);
        assert!(edit.buffer.is_char_boundary(edit.cursor_byte));
        assert!(edit.buffer.is_char_boundary(edit.anchor_byte));
        state.refresh(Some(1), Some(1), Some(target), &edit, 4, 4);
    }
}

// Verifies incomplete formula text remains valid input while completed refs outline.
#[test]
fn incomplete_formula_has_no_reference_and_completed_formula_has_overlay() {
    let incomplete = formula_edit("=SUM(", "=SUM(".len());
    let mut state = FormulaReferenceState::default();
    state.refresh(
        Some(1),
        Some(1),
        Some(CellAddress::default()),
        &incomplete,
        4,
        4,
    );
    assert!(state.tokens.is_empty());
    assert!(incomplete.buffer.starts_with("=SUM("));

    let formula = "=SUM(A1:B2)";
    let mut edit = incomplete.clone();
    edit.buffer = formula.to_owned();
    edit.cursor_byte = formula.len();
    edit.anchor_byte = formula.len();
    edit.mark_buffer_changed();
    state.refresh(Some(1), Some(1), Some(CellAddress::default()), &edit, 4, 4);
    assert_eq!(state.overlays().len(), 1);
    assert_eq!(
        state.overlays()[0].cells.normalized().1,
        CellAddress { row: 1, column: 1 }
    );
}

// Verifies that cursor movement selects the token under the cursor without reparsing text.
#[test]
fn cursor_movement_refreshes_the_current_reference() {
    let formula = "=A1+B2";
    let mut edit = formula_edit(formula, formula.len());
    let mut state = FormulaReferenceState::default();
    let target = CellAddress::default();
    state.refresh(Some(1), Some(1), Some(target), &edit, 8, 8);
    assert_eq!(
        state.current_range,
        Some(CellRange::single(CellAddress { row: 1, column: 1 }))
    );

    edit.cursor_byte = 3;
    edit.anchor_byte = 3;
    state.refresh(Some(1), Some(1), Some(target), &edit, 8, 8);
    assert_eq!(state.current_range, Some(CellRange::single(target)));

    edit.anchor_byte = 1;
    edit.cursor_byte = formula.len();
    state.refresh(Some(1), Some(1), Some(target), &edit, 8, 8);
    assert_eq!(state.insertion_range(&edit, None), 1..formula.len());
}
