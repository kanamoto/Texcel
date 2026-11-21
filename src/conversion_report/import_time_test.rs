use super::*;
use chrono::TimeZone;

#[test]
// Uses the active system's regional date and time format for one known instant.
fn local_formatter_returns_a_human_readable_date_and_time() {
    let instant = Utc.with_ymd_and_hms(2026, 11, 18, 3, 4, 5).unwrap();
    let formatted = format_local(instant);

    assert!(
        !formatted.is_empty(),
        "date and time are shown: {formatted}"
    );
    assert!(
        !has_iso_date_time_separator(&formatted),
        "ISO separator is not shown: {formatted}"
    );
}

// Detect only the ISO date-time separator, not a timezone name such as UTC.
fn has_iso_date_time_separator(value: &str) -> bool {
    value
        .as_bytes()
        .windows(3)
        .any(|part| part[0].is_ascii_digit() && part[1] == b'T' && part[2].is_ascii_digit())
}

#[test]
// Allows timezone text to contain T while still rejecting an ISO separator.
fn iso_separator_check_allows_local_timezone_names() {
    assert!(!has_iso_date_time_separator("18.11.2026 8:30:00 UTC"));
    assert!(has_iso_date_time_separator("2026-11-18T08:30:00Z"));
}
