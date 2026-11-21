use csv::{ReaderBuilder, Terminator};
use encoding_rs::SHIFT_JIS;

use super::*;

mod scan;
pub(super) use scan::{choose_marker, combine_newlines, map_delimiter, scan_records};

/// Returns the width of a record after delimiter mapping.
pub(super) fn mapped_record_width(
    source_width: usize,
    operation: &StructureOperation,
    plan: &StructurePlan,
) -> usize {
    let include_inserted = matches!(
        operation,
        StructureOperation::Insert { axis: StructureAxis::Columns, index, .. } if *index <= source_width
    );
    plan.column_sources
        .iter()
        .enumerate()
        .filter_map(|(output, source)| {
            (source.is_some_and(|source| source < source_width)
                || (include_inserted && source.is_none()))
            .then_some(output + 1)
        })
        .max()
        .unwrap_or(source_width)
}

/// Finds the width required by all changed cells.
pub(super) fn desired_record_width(
    origin_width: usize,
    introduced_fields: Option<&std::collections::BTreeSet<usize>>,
) -> usize {
    introduced_fields
        .and_then(|columns| columns.last().copied())
        .map_or(origin_width, |column| origin_width.max(column + 1))
}

/// Finds the width required for newly added cells.
pub(super) fn desired_new_width(
    values: Option<&std::collections::BTreeMap<usize, Option<&str>>>,
) -> usize {
    values
        .and_then(|values| values.iter().rev().find(|(_, value)| value.is_some()))
        .map_or(0, |(&column, _)| column + 1)
}

/// Encodes a new record using the source delimiter and newline style.
pub(super) fn encode_new_record(
    values: Option<&std::collections::BTreeMap<usize, Option<&str>>>,
    width: usize,
    delimiter: char,
    encoding: CsvEncoding,
) -> Result<Vec<u8>, CsvError> {
    if width == 0 {
        return Ok(Vec::new());
    }
    let mut output = Vec::new();
    for column in 0..width {
        if column > 0 {
            output.extend_from_slice(&encode_text(&delimiter.to_string(), encoding)?);
        }
        let value = values
            .and_then(|values| values.get(&column))
            .and_then(|value| *value)
            .unwrap_or("");
        output.extend_from_slice(&encode_replacement(
            value,
            delimiter,
            width == 1 && value.is_empty(),
            encoding,
        )?);
    }
    Ok(output)
}

/// Scores delimiter candidates using the parsed record widths.
pub fn analyze_delimiters(
    bytes: &[u8],
    encoding_choice: EncodingChoice,
) -> Result<DelimiterAnalysis, CsvError> {
    if bytes.len() > MAX_CSV_SOURCE_BYTES {
        return Err(CsvError::ResourceLimit(
            crate::i18n::ResourceLimit::FileSize,
        ));
    }
    let (encoding, content_start) = resolve_encoding(bytes, encoding_choice)?;
    let raw_content = &bytes[content_start..];
    let mut candidates = Vec::new();
    let mut previews = std::collections::BTreeMap::new();
    for delimiter in [',', '\t', ';'] {
        let delimiter_bytes = encode_text(&delimiter.to_string(), encoding)?;
        let Ok(spans) = scan_records(raw_content, content_start, &delimiter_bytes) else {
            continue;
        };
        let preview = spans
            .iter()
            .take(6)
            .filter_map(|span| {
                let record = decode_text(&bytes[span.content.clone()], encoding).ok()?;
                let fields = parse_record(&record, delimiter).ok()?;
                Some(
                    fields
                        .into_iter()
                        .take(8)
                        .map(|field| field.chars().take(160).collect::<String>())
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        previews.insert(delimiter, preview);
        let widths = spans
            .iter()
            .filter(|span| !span.fields.is_empty())
            .map(|span| span.fields.len())
            .collect::<Vec<_>>();
        let record_count = widths.len();
        let most_common_width = mode(&widths);
        let consistent_record_count = widths
            .iter()
            .filter(|width| **width == most_common_width)
            .count();
        candidates.push(DelimiterCandidate {
            delimiter,
            record_count,
            most_common_width,
            consistent_record_count,
            total_fields: widths.iter().sum(),
        });
    }
    if candidates.is_empty() {
        return Ok(DelimiterAnalysis {
            candidates,
            suggested: None,
            previews,
        });
    }

    let multi_column = candidates
        .iter()
        .filter(|candidate| candidate.most_common_width > 1)
        .collect::<Vec<_>>();
    let plausible = if multi_column.is_empty() {
        candidates.iter().collect::<Vec<_>>()
    } else {
        multi_column
    };
    let best = plausible
        .iter()
        .copied()
        .filter(|candidate| {
            plausible
                .iter()
                .all(|other| compare_consistency(candidate, other) != std::cmp::Ordering::Less)
        })
        .collect::<Vec<_>>();
    let suggested = if best.len() == 1 {
        Some(best[0].delimiter)
    } else {
        None
    };
    Ok(DelimiterAnalysis {
        candidates,
        suggested,
        previews,
    })
}

/// Compares the share of records that match each candidate's common width.
fn compare_consistency(
    left: &DelimiterCandidate,
    right: &DelimiterCandidate,
) -> std::cmp::Ordering {
    let left_score = (left.consistent_record_count as u128) * (right.record_count as u128);
    let right_score = (right.consistent_record_count as u128) * (left.record_count as u128);
    left_score.cmp(&right_score)
}

/// Finds the most common line ending or record width.
pub(super) fn mode(values: &[usize]) -> usize {
    let mut counts = std::collections::BTreeMap::new();
    for value in values {
        *counts.entry(*value).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(value, count)| (*count, *value))
        .map_or(0, |(value, _)| value)
}

/// Rejects delimiter choices that conflict with supported parsing.
pub(super) fn validate_delimiter(delimiter: char) -> Result<(), CsvError> {
    if matches!(delimiter, '"' | '\r' | '\n') {
        Err(CsvError::InvalidDelimiter)
    } else {
        Ok(())
    }
}

/// Chooses the input encoding or returns a user-facing choice.
pub(super) fn resolve_encoding(
    bytes: &[u8],
    choice: EncodingChoice,
) -> Result<(CsvEncoding, usize), CsvError> {
    let has_bom = bytes.starts_with(&[0xEF, 0xBB, 0xBF]);
    match choice {
        EncodingChoice::Auto => {
            if has_bom {
                return Ok((CsvEncoding::Utf8Bom, 3));
            }
            if std::str::from_utf8(bytes).is_ok() {
                return Ok((CsvEncoding::Utf8, 0));
            }
            Err(CsvError::EncodingChoiceRequired)
        }
        EncodingChoice::Utf8 => {
            if has_bom {
                return Ok((CsvEncoding::Utf8Bom, 3));
            }
            std::str::from_utf8(bytes)
                .map(|_| (CsvEncoding::Utf8, 0))
                .map_err(|_| CsvError::InvalidEncoding)
        }
        EncodingChoice::ShiftJis => {
            let (_, had_errors) = SHIFT_JIS.decode_without_bom_handling(bytes);
            if had_errors {
                Err(CsvError::InvalidEncoding)
            } else {
                Ok((CsvEncoding::ShiftJis, 0))
            }
        }
    }
}

/// Decodes source bytes with the selected encoding.
pub(super) fn decode_text(bytes: &[u8], encoding: CsvEncoding) -> Result<String, CsvError> {
    match encoding {
        CsvEncoding::Utf8 | CsvEncoding::Utf8Bom => std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| CsvError::InvalidEncoding),
        CsvEncoding::ShiftJis => {
            let (decoded, had_errors) = SHIFT_JIS.decode_without_bom_handling(bytes);
            if had_errors {
                Err(CsvError::InvalidEncoding)
            } else {
                Ok(decoded.into_owned())
            }
        }
    }
}

/// Encodes text with the selected source encoding.
pub(super) fn encode_text(text: &str, encoding: CsvEncoding) -> Result<Vec<u8>, CsvError> {
    match encoding {
        CsvEncoding::Utf8 | CsvEncoding::Utf8Bom => Ok(text.as_bytes().to_vec()),
        CsvEncoding::ShiftJis => {
            let (encoded, _, had_errors) = SHIFT_JIS.encode(text);
            if had_errors {
                Err(CsvError::UnrepresentableCharacter)
            } else {
                let encoded = encoded.into_owned();
                let (round_trip, had_decode_errors) =
                    SHIFT_JIS.decode_without_bom_handling(&encoded);
                if had_decode_errors || round_trip.as_ref() != text {
                    Err(CsvError::UnrepresentableCharacter)
                } else {
                    Ok(encoded)
                }
            }
        }
    }
}

/// Encodes a changed field while keeping its quoting rules.
pub(super) fn encode_replacement(
    value: &str,
    delimiter: char,
    was_quoted: bool,
    encoding: CsvEncoding,
) -> Result<Vec<u8>, CsvError> {
    let needs_quotes = was_quoted
        || value.contains(delimiter)
        || value.contains('"')
        || value.contains('\r')
        || value.contains('\n');
    let escaped = value.replace('"', "\"\"");
    let text = if needs_quotes {
        format!("\"{escaped}\"")
    } else {
        escaped
    };
    encode_text(&text, encoding)
}

/// Parses one CSV record and records source byte ranges.
pub(super) fn parse_record(record: &str, delimiter: char) -> Result<Vec<String>, CsvError> {
    let marker = choose_marker(record.as_bytes())?;
    let leading_bom = record.starts_with('\u{FEFF}');
    let mut mapped = map_delimiter(record, delimiter, marker);
    // rust-csv treats a BOM at the start of every standalone record as file metadata.
    // Prefix one sentinel byte so later-record U+FEFF remains ordinary cell data.
    if leading_bom {
        mapped.insert(0, b'x');
    }
    let mut reader = ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(marker)
        .terminator(Terminator::Any(b'\n'))
        .from_reader(mapped.as_slice());
    let mut records = reader.records();
    let Some(record) = records.next() else {
        return Err(CsvError::ParserMismatch);
    };
    let mut values = record
        .map_err(|error| CsvError::CsvParse(error.to_string()))?
        .iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if leading_bom {
        let Some(first) = values.first_mut() else {
            return Err(CsvError::ParserMismatch);
        };
        let Some(value) = first.strip_prefix('x') else {
            return Err(CsvError::ParserMismatch);
        };
        *first = value.to_owned();
    }
    if records.next().is_some() {
        return Err(CsvError::ParserMismatch);
    }
    Ok(values)
}

#[cfg(test)]
#[path = "codec_test.rs"]
mod tests;
