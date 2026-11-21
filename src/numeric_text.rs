//! Shared rules for reading numeric values from source text.

/// Parses a finite, canonical ASCII decimal with at most 15 significant digits.
pub(crate) fn parse_ascii_decimal(value: &str) -> Option<f64> {
    let bytes = value.as_bytes();
    if bytes.is_empty() || !value.is_ascii() {
        return None;
    }

    let mut index = usize::from(bytes[0] == b'-');
    if index == bytes.len() {
        return None;
    }

    let integer_start = index;
    if bytes[index] == b'0' {
        index += 1;
        if bytes.get(index).is_some_and(u8::is_ascii_digit) {
            return None;
        }
    } else if (b'1'..=b'9').contains(&bytes[index]) {
        index += 1;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
    } else {
        return None;
    }

    let integer_end = index;
    let mut fraction = None;
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == start {
            return None;
        }
        fraction = Some(start..index);
    }

    let mut significant_digits = bytes[integer_start..integer_end].iter().chain(
        fraction
            .as_ref()
            .into_iter()
            .flat_map(|range| &bytes[range.clone()]),
    );
    let first_nonzero = significant_digits.position(|byte| *byte != b'0');
    let digit_count =
        integer_end - integer_start + fraction.as_ref().map_or(0, |range| range.end - range.start);
    let significant_count = first_nonzero.map_or(1, |first| digit_count - first);
    if significant_count > 15 {
        return None;
    }

    if bytes
        .get(index)
        .is_some_and(|byte| matches!(byte, b'e' | b'E'))
    {
        index += 1;
        if bytes
            .get(index)
            .is_some_and(|byte| matches!(byte, b'+' | b'-'))
        {
            index += 1;
        }
        let exponent_start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == exponent_start {
            return None;
        }
    }
    if index != bytes.len() {
        return None;
    }

    value
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite())
}

#[cfg(test)]
#[path = "numeric_text_test.rs"]
mod tests;
