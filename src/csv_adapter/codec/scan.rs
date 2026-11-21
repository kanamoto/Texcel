use super::*;

/// Chooses a byte marker not present in the input.
pub(in crate::csv_adapter) fn choose_marker(source: &[u8]) -> Result<u8, CsvError> {
    (1..=31)
        .filter(|byte| !matches!(*byte, b'\t' | b'\r' | b'\n'))
        .find(|byte| !source.contains(byte))
        .ok_or(CsvError::NoSafeDelimiterMarker)
}

/// Maps supported delimiter characters to the CSV parser.
pub(in crate::csv_adapter) fn map_delimiter(source: &str, delimiter: char, marker: u8) -> Vec<u8> {
    let mut mapped = Vec::with_capacity(source.len());
    let mut characters = source.chars().peekable();
    let mut in_quotes = false;
    let mut field_start = true;
    while let Some(character) = characters.next() {
        match character {
            '"' if in_quotes && characters.peek() == Some(&'"') => {
                mapped.extend_from_slice(b"\"\"");
                characters.next();
            }
            '"' if field_start => {
                mapped.push(b'"');
                in_quotes = true;
                field_start = false;
            }
            '"' if in_quotes => {
                mapped.push(b'"');
                in_quotes = false;
            }
            current if current == delimiter && !in_quotes => {
                mapped.push(marker);
                field_start = true;
            }
            current => {
                let mut buffer = [0; 4];
                mapped.extend_from_slice(current.encode_utf8(&mut buffer).as_bytes());
                if !in_quotes {
                    field_start = false;
                }
            }
        }
    }
    mapped
}

/// Finds record and field ranges without changing the source bytes.
pub(in crate::csv_adapter) fn scan_records(
    source: &[u8],
    base_offset: usize,
    delimiter: &[u8],
) -> Result<Vec<RecordSpan>, CsvError> {
    if delimiter.is_empty()
        || delimiter
            .iter()
            .any(|byte| matches!(*byte, b'"' | b'\r' | b'\n'))
    {
        return Err(CsvError::InvalidDelimiter);
    }
    let mut records = Vec::new();
    let mut record_start = 0;
    let mut field_start = 0;
    let mut in_quotes = false;
    let mut quoted_fields = Vec::new();
    let mut field_was_quoted = false;
    let mut cursor = 0;
    let mut closed_quote = false;
    let mut total_fields = 0_usize;

    while cursor < source.len() {
        let byte = source[cursor];
        if closed_quote
            && !source[cursor..].starts_with(delimiter)
            && byte != b'\r'
            && byte != b'\n'
        {
            return Err(CsvError::InvalidQuoteSyntax);
        }
        if in_quotes {
            if byte == b'"' && source.get(cursor + 1) == Some(&b'"') {
                cursor += 2;
            } else if byte == b'"' {
                in_quotes = false;
                closed_quote = true;
                cursor += 1;
            } else {
                cursor += 1;
            }
            continue;
        }
        if cursor == field_start && byte == b'"' {
            field_was_quoted = true;
            in_quotes = true;
            cursor += 1;
            continue;
        }
        if source[cursor..].starts_with(delimiter) {
            quoted_fields.push(FieldSpan {
                range: base_offset + field_start..base_offset + cursor,
                was_quoted: field_was_quoted,
            });
            total_fields = total_fields.saturating_add(1);
            if total_fields > MAX_CSV_CELLS {
                return Err(CsvError::ResourceLimit(
                    crate::i18n::ResourceLimit::CellCount,
                ));
            }
            cursor += delimiter.len();
            field_start = cursor;
            field_was_quoted = false;
            closed_quote = false;
            continue;
        }
        if byte == b'\n' || byte == b'\r' {
            let (terminator_len, newline) = if byte == b'\n' {
                (1, NewlineStyle::Lf)
            } else if source.get(cursor + 1) == Some(&b'\n') {
                (2, NewlineStyle::CrLf)
            } else {
                return Err(CsvError::InvalidNewline);
            };
            let blank = cursor == record_start;
            if !blank {
                quoted_fields.push(FieldSpan {
                    range: base_offset + field_start..base_offset + cursor,
                    was_quoted: field_was_quoted,
                });
                total_fields = total_fields.saturating_add(1);
                if total_fields > MAX_CSV_CELLS {
                    return Err(CsvError::ResourceLimit(
                        crate::i18n::ResourceLimit::CellCount,
                    ));
                }
            }
            if records.len() >= MAX_CSV_ROWS {
                return Err(CsvError::ResourceLimit(
                    crate::i18n::ResourceLimit::RowCount,
                ));
            }
            records.push(RecordSpan {
                range: base_offset + record_start..base_offset + cursor + terminator_len,
                content: base_offset + record_start..base_offset + cursor,
                fields: std::mem::take(&mut quoted_fields),
                newline: Some(newline),
            });
            cursor += terminator_len;
            record_start = cursor;
            field_start = cursor;
            field_was_quoted = false;
            closed_quote = false;
            continue;
        }
        cursor += 1;
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote);
    }
    if record_start < source.len() {
        quoted_fields.push(FieldSpan {
            range: base_offset + field_start..base_offset + source.len(),
            was_quoted: field_was_quoted,
        });
        total_fields = total_fields.saturating_add(1);
        if total_fields > MAX_CSV_CELLS {
            return Err(CsvError::ResourceLimit(
                crate::i18n::ResourceLimit::CellCount,
            ));
        }
        if quoted_fields.len() > MAX_CSV_COLUMNS {
            return Err(CsvError::ResourceLimit(
                crate::i18n::ResourceLimit::ColumnCount,
            ));
        }
        if records.len() >= MAX_CSV_ROWS {
            return Err(CsvError::ResourceLimit(
                crate::i18n::ResourceLimit::RowCount,
            ));
        }
        records.push(RecordSpan {
            range: base_offset + record_start..base_offset + source.len(),
            content: base_offset + record_start..base_offset + source.len(),
            fields: quoted_fields,
            newline: None,
        });
    }
    if records
        .iter()
        .any(|record| record.fields.len() > MAX_CSV_COLUMNS)
    {
        return Err(CsvError::ResourceLimit(
            crate::i18n::ResourceLimit::ColumnCount,
        ));
    }
    Ok(records)
}

/// Combines parsed records using the source newline style.
pub(in crate::csv_adapter) fn combine_newlines(
    styles: impl Iterator<Item = NewlineStyle>,
) -> NewlineStyle {
    let mut style = None;
    for next in styles {
        if let Some(previous) = style {
            if previous != next {
                return NewlineStyle::Mixed;
            }
        } else {
            style = Some(next);
        }
    }
    style.unwrap_or(NewlineStyle::None)
}
