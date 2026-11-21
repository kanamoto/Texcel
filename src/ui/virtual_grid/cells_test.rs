use super::semantic_numeric_input as input_is_semantic_number;
use crate::document::Cell as TestCell;

#[test]
// Checks that canonical numbers right-align and other numeric-looking text stays left.
fn default_alignment_follows_the_calculation_number_rule() {
    for text in ["123", "-0.5", "1.25e3", "0"] {
        assert!(input_is_semantic_number(&TestCell::text(text)), "{text}");
    }
    for text in [
        "00123",
        "+5",
        "12345678901234567890",
        "1,000",
        "2026-01-01",
        "**123**",
    ] {
        assert!(!input_is_semantic_number(&TestCell::text(text)), "{text}");
    }
    assert!(input_is_semantic_number(&TestCell::number(123.0)));
}
