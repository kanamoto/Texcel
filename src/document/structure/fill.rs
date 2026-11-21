//!  Builds copied values and numeric, date, text, or formula fill series.

use crate::document::{CellAddress, CellInput, Table};
use crate::i18n::Message;

/// Updates row or column references in a copied formula.
pub(crate) fn replace_formula_input(input: &mut CellInput, formula: String) {
    match input {
        CellInput::Formula(value) => *value = formula,
        CellInput::Text(value) if value.starts_with('=') => *value = formula,
        _ => (),
    }
}

#[derive(Clone, Debug)]
pub(crate) enum SeriesKind {
    Formula,
    Numeric {
        first: f64,
        step: f64,
        start_index: usize,
        vertical: bool,
    },
    NumericText {
        first: f64,
        step: f64,
        decimals: usize,
        start_index: usize,
        vertical: bool,
    },
    CalendarDate {
        first_days: i64,
        step_days: i64,
        start_index: usize,
        vertical: bool,
    },
    TrailingNumber {
        prefix: String,
        first: i64,
        step: i64,
        width: usize,
        padded: bool,
        start_index: usize,
        vertical: bool,
    },
}

pub(crate) enum SeriesValue {
    Formula,
    Number(f64),
    Text(String),
}

impl SeriesKind {
    /// Returns a series value at the requested position.
    pub(crate) fn value_for(&self, destination: CellAddress) -> Result<SeriesValue, Message> {
        match self {
            Self::Formula => Ok(SeriesValue::Formula),
            Self::Numeric {
                first,
                step,
                start_index,
                vertical,
            } => {
                let position = if *vertical {
                    destination.row
                } else {
                    destination.column
                };
                let offset = position as isize - *start_index as isize;
                let value = *first + *step * offset as f64;
                if value.is_finite() {
                    Ok(SeriesValue::Number(value))
                } else {
                    Err(Message::new("status-fill-number-out-of-range"))
                }
            }
            Self::NumericText {
                first,
                step,
                decimals,
                start_index,
                vertical,
            } => {
                let position = if *vertical {
                    destination.row
                } else {
                    destination.column
                };
                let offset = position as i128 - *start_index as i128;
                let value = *first + *step * offset as f64;
                if value.is_finite() {
                    Ok(SeriesValue::Text(format!("{value:.decimals$}")))
                } else {
                    Err(Message::new("status-fill-number-out-of-range"))
                }
            }
            Self::CalendarDate {
                first_days,
                step_days,
                start_index,
                vertical,
            } => {
                let position = if *vertical {
                    destination.row
                } else {
                    destination.column
                };
                let offset = position as i128 - *start_index as i128;
                let days = (*step_days as i128)
                    .checked_mul(offset)
                    .and_then(|delta| (*first_days as i128).checked_add(delta))
                    .and_then(|days| i64::try_from(days).ok())
                    .ok_or_else(|| Message::new("status-fill-date-out-of-range"))?;
                Ok(SeriesValue::Text(format_iso_date(days)?))
            }
            Self::TrailingNumber {
                prefix,
                first,
                step,
                width,
                padded,
                start_index,
                vertical,
            } => {
                let position = if *vertical {
                    destination.row
                } else {
                    destination.column
                };
                let offset = position as i64 - *start_index as i64;
                let value = step
                    .checked_mul(offset)
                    .and_then(|increment| first.checked_add(increment))
                    .filter(|value| *value >= 0)
                    .ok_or_else(|| Message::new("status-fill-trailing-number-out-of-range"))?;
                let suffix = if *padded {
                    format!("{value:0width$}")
                } else {
                    value.to_string()
                };
                Ok(SeriesValue::Text(format!("{prefix}{suffix}")))
            }
        }
    }
}

/// Checks that the selected source cells can define a fill series.
pub(crate) fn validate_series_source(
    table: &Table,
    source_start: CellAddress,
    source_end: CellAddress,
    target_start: CellAddress,
    target_end: CellAddress,
) -> Result<SeriesKind, Message> {
    let vertical = source_start.column == source_end.column;
    let horizontal = source_start.row == source_end.row;
    if vertical == horizontal {
        return Err(Message::new("status-fill-series-source-shape"));
    }
    if vertical && target_start.column != target_end.column
        || horizontal && target_start.row != target_end.row
    {
        return Err(Message::new("status-fill-series-direction-mismatch"));
    }
    let count = if vertical {
        source_end.row - source_start.row + 1
    } else {
        source_end.column - source_start.column + 1
    };
    if count < 2 {
        return Err(Message::new("status-fill-series-needs-two-values"));
    }
    let inputs = (0..count)
        .map(|offset| {
            let address = if vertical {
                CellAddress {
                    row: source_start.row + offset,
                    ..source_start
                }
            } else {
                CellAddress {
                    column: source_start.column + offset,
                    ..source_start
                }
            };
            table
                .cell(address)
                .map(|cell| (address, cell.input.clone()))
                .ok_or_else(|| Message::new("status-fill-source-cell-empty"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let all_formulas = inputs.iter().all(|(address, input)| match input {
        CellInput::Formula(_) => true,
        CellInput::Text(value) => !table.is_string_cell(*address) && value.starts_with('='),
        _ => false,
    });
    if all_formulas {
        return Ok(SeriesKind::Formula);
    }
    let start_index = if vertical {
        source_start.row
    } else {
        source_start.column
    };
    if inputs
        .iter()
        .all(|(_, input)| matches!(input, CellInput::Number(_)))
    {
        let values = inputs
            .iter()
            .map(|(_, input)| match input {
                CellInput::Number(value) if value.is_finite() => Ok(*value),
                _ => Err(Message::new("status-fill-source-number-invalid")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let step = values[1] - values[0];
        if values.windows(2).any(|pair| {
            let actual = pair[1] - pair[0];
            (actual - step).abs() > f64::EPSILON * actual.abs().max(step.abs()).max(1.0) * 8.0
        }) {
            return Err(Message::new("status-fill-number-step-mismatch"));
        }
        return Ok(SeriesKind::Numeric {
            first: values[0],
            step,
            start_index,
            vertical,
        });
    }
    if inputs
        .iter()
        .all(|(_, input)| matches!(input, CellInput::Text(_)))
    {
        let text_values = inputs
            .iter()
            .map(|(_, input)| match input {
                CellInput::Text(value) => value.as_str(),
                _ => unreachable!(),
            })
            .collect::<Vec<_>>();
        if text_values.iter().any(|value| has_iso_date_shape(value)) {
            let dates = text_values
                .iter()
                .map(|value| parse_iso_date(value))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| Message::new("status-fill-date-format-invalid"))?;
            let step_days = dates[1] - dates[0];
            if dates.windows(2).any(|pair| pair[1] - pair[0] != step_days) {
                return Err(Message::new("status-fill-date-step-mismatch"));
            }
            return Ok(SeriesKind::CalendarDate {
                first_days: dates[0],
                step_days,
                start_index,
                vertical,
            });
        }
        if let Some(numbers) = text_values
            .iter()
            .map(|value| parse_canonical_numeric_text(value))
            .collect::<Option<Vec<_>>>()
        {
            let step = numbers[1].0 - numbers[0].0;
            if numbers.windows(2).any(|pair| {
                let actual = pair[1].0 - pair[0].0;
                (actual - step).abs() > f64::EPSILON * actual.abs().max(step.abs()).max(1.0) * 8.0
            }) {
                return Err(Message::new("status-fill-text-number-step-mismatch"));
            }
            let decimals = numbers
                .iter()
                .map(|(_, decimals)| *decimals)
                .max()
                .unwrap_or(0);
            return Ok(SeriesKind::NumericText {
                first: numbers[0].0,
                step,
                decimals,
                start_index,
                vertical,
            });
        }
        let parts = inputs
            .iter()
            .map(|(_, input)| match input {
                CellInput::Text(value) => split_trailing_number(value)
                    .ok_or_else(|| Message::new("status-fill-trailing-number-required")),
                _ => unreachable!(),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let (prefix, first_text, first) = parts[0];
        let width = first_text.len();
        let padded = width > 1 && first_text.starts_with('0');
        if parts.iter().any(|(candidate_prefix, digits, _)| {
            *candidate_prefix != prefix
                || (padded && (digits.len() != width || !digits.starts_with('0')))
                || (!padded && digits.len() > 1 && digits.starts_with('0'))
        }) {
            return Err(Message::new("status-fill-trailing-number-format-mismatch"));
        }
        let step = parts[1].2 - first;
        if parts.windows(2).any(|pair| pair[1].2 - pair[0].2 != step) {
            return Err(Message::new("status-fill-trailing-number-step-mismatch"));
        }
        return Ok(SeriesKind::TrailingNumber {
            prefix: prefix.to_owned(),
            first,
            step,
            width,
            padded,
            start_index,
            vertical,
        });
    }
    Err(Message::new("status-fill-series-seed-required"))
}

/// Splits a text value into its prefix and final number.
fn split_trailing_number(value: &str) -> Option<(&str, &str, i64)> {
    let digit_start = value
        .char_indices()
        .rev()
        .find_map(|(index, character)| {
            (!character.is_ascii_digit()).then_some(index + character.len_utf8())
        })
        .unwrap_or(0);
    let digits = &value[digit_start..];
    if digits.is_empty() {
        return None;
    }
    Some((&value[..digit_start], digits, digits.parse().ok()?))
}

/// Parses a number only when its text form is canonical.
fn parse_canonical_numeric_text(value: &str) -> Option<(f64, usize)> {
    if !value.is_ascii() || value.is_empty() {
        return None;
    }
    let unsigned = value.strip_prefix('-').unwrap_or(value);
    let (integer, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if integer.is_empty()
        || !integer.bytes().all(|byte| byte.is_ascii_digit())
        || (integer.len() > 1 && integer.starts_with('0'))
        || (unsigned.contains('.')
            && (fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit())))
    {
        return None;
    }
    let decimals = fraction.len();
    let number = value.parse::<f64>().ok()?;
    number.is_finite().then_some((number, decimals))
}

/// Parses a date in the supported ISO format.
fn parse_iso_date(value: &str) -> Option<i64> {
    if !has_iso_date_shape(value) {
        return None;
    }
    let year = value.get(0..4)?.parse::<i32>().ok()?;
    let month = value.get(5..7)?.parse::<u32>().ok()?;
    let day = value.get(8..10)?.parse::<u32>().ok()?;
    if !(1..=9999).contains(&year) || !(1..=12).contains(&month) {
        return None;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_days = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if !(1..=month_days).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day))
}

/// Checks whether text has the supported ISO date shape.
fn has_iso_date_shape(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}

/// Formats a calendar date as ISO text.
fn format_iso_date(days: i64) -> Result<String, Message> {
    let (year, month, day) = civil_from_days(days);
    if !(1..=9999).contains(&year) {
        return Err(Message::new("status-fill-date-out-of-range"));
    }
    Ok(format!("{year:04}-{month:02}-{day:02}"))
}

/// Converts a calendar date to a day count.
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let mut year = i64::from(year);
    let month = i64::from(month);
    let day = i64::from(day);
    year -= i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let shifted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Converts a day count to a calendar date.
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year as i32, month as u32, day as u32)
}
