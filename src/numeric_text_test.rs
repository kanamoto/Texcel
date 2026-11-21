use super::parse_ascii_decimal;

#[test]
// Keep source identifiers as text while accepting canonical decimal values.
fn parses_only_canonical_decimal_text_with_fifteen_significant_digits() {
    for (source, expected) in [
        ("0", 0.0),
        ("-0", -0.0),
        ("1200", 1200.0),
        ("-1.25e2", -125.0),
        ("0.000000000000001", 0.000000000000001),
        ("123456789012345", 123456789012345.0),
    ] {
        assert_eq!(parse_ascii_decimal(source), Some(expected), "{source}");
    }

    for source in [
        "",
        "00123",
        "+5",
        "1234567890123456",
        "12345678901234567890",
        "1.000000000000000",
        ".5",
        "1.",
        "1e",
        "1,200",
        " 5",
        "5 ",
        "NaN",
        "１２３",
        "1e9999",
    ] {
        assert_eq!(parse_ascii_decimal(source), None, "{source}");
    }
}
