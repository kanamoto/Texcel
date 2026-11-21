use super::*;

impl CsvDocument {
    // Capture source CSV shape so undo can restore the original serialization details.
    pub(crate) fn history_metadata(&self) -> CsvHistoryMetadata {
        CsvHistoryMetadata {
            origin_record_count: self.origin_record_count,
            origin_field_counts: self.origin_field_counts.clone(),
            origin_ends_with_newline: self.origin_ends_with_newline,
            introduced_fields: self
                .introduced_fields
                .iter()
                .map(|(&row, columns)| (row, columns.iter().copied().collect()))
                .collect(),
        }
    }

    // Restore saved CSV shape data after checking it matches the current records.
    pub(crate) fn restore_history_metadata(
        &mut self,
        metadata: CsvHistoryMetadata,
    ) -> Result<(), crate::i18n::Message> {
        if metadata.origin_record_count > self.records.len()
            || metadata.origin_record_count != metadata.origin_field_counts.len()
            || metadata.origin_record_count > MAX_CSV_ROWS
            || metadata
                .origin_field_counts
                .iter()
                .any(|count| *count > MAX_CSV_COLUMNS)
        {
            return Err(crate::i18n::Message::new(
                "status-history-csv-shape-invalid",
            ));
        }
        let mut introduced_fields = std::collections::BTreeMap::new();
        for (row, columns) in metadata.introduced_fields {
            if row >= self.records.len()
                || columns.is_empty()
                || columns.windows(2).any(|pair| pair[0] >= pair[1])
                || columns.iter().any(|column| {
                    *column >= MAX_CSV_COLUMNS
                        || self
                            .records
                            .get(row)
                            .is_none_or(|record| *column >= record.fields.len())
                })
            {
                return Err(crate::i18n::Message::new(
                    "status-history-csv-fields-invalid",
                ));
            }
            let fields = columns
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>();
            if introduced_fields.insert(row, fields).is_some() {
                return Err(crate::i18n::Message::new(
                    "status-history-csv-rows-duplicate",
                ));
            }
        }
        self.origin_record_count = metadata.origin_record_count;
        self.origin_field_counts = metadata.origin_field_counts;
        self.origin_ends_with_newline = metadata.origin_ends_with_newline;
        self.introduced_fields = introduced_fields;
        Ok(())
    }

    // Parses input with confirmed encoding and delimiter, then links values to source text.
    pub fn parse(
        bytes: Vec<u8>,
        encoding_choice: EncodingChoice,
        delimiter: char,
    ) -> Result<Self, CsvError> {
        validate_delimiter(delimiter)?;
        let (encoding, content_start) = resolve_encoding(&bytes, encoding_choice)?;
        Self::parse_resolved(bytes, encoding, content_start, delimiter)
    }

    /// Parses records while sending complete rows to a read-only preview.
    pub(crate) fn parse_with_progress(
        bytes: Vec<u8>,
        encoding_choice: EncodingChoice,
        delimiter: char,
        mut progress: impl FnMut(usize, &[CsvField]) -> bool,
    ) -> Result<Option<Self>, CsvError> {
        validate_delimiter(delimiter)?;
        let (encoding, content_start) = resolve_encoding(&bytes, encoding_choice)?;
        Self::parse_resolved_with_progress(bytes, encoding, content_start, delimiter, &mut progress)
    }

    // Parse CSV bytes with the encoding and delimiter already chosen by the caller.
    pub(super) fn parse_resolved(
        bytes: Vec<u8>,
        encoding: CsvEncoding,
        content_start: usize,
        delimiter: char,
    ) -> Result<Self, CsvError> {
        Self::parse_resolved_with_progress(
            bytes,
            encoding,
            content_start,
            delimiter,
            &mut |_, _| true,
        )?
        .ok_or(CsvError::ParserMismatch)
    }

    /// Parses each source record once and stops when the preview owner cancels.
    fn parse_resolved_with_progress(
        bytes: Vec<u8>,
        encoding: CsvEncoding,
        content_start: usize,
        delimiter: char,
        progress: &mut impl FnMut(usize, &[CsvField]) -> bool,
    ) -> Result<Option<Self>, CsvError> {
        if bytes.len() > MAX_CSV_SOURCE_BYTES {
            return Err(CsvError::ResourceLimit(
                crate::i18n::ResourceLimit::FileSize,
            ));
        }
        validate_delimiter(delimiter)?;
        if content_start > bytes.len()
            || (encoding == CsvEncoding::Utf8Bom && !bytes.starts_with(&[0xEF, 0xBB, 0xBF]))
        {
            return Err(CsvError::InvalidEncoding);
        }
        if encoding == CsvEncoding::Utf8 && std::str::from_utf8(&bytes).is_err()
            || encoding == CsvEncoding::Utf8Bom
                && std::str::from_utf8(&bytes[content_start..]).is_err()
            || encoding == CsvEncoding::ShiftJis && decode_text(&bytes, encoding).is_err()
        {
            return Err(CsvError::InvalidEncoding);
        }
        let raw_content = &bytes[content_start..];
        let delimiter_bytes = encode_text(&delimiter.to_string(), encoding)?;
        let spans = scan_records(raw_content, content_start, &delimiter_bytes)?;
        let newline = combine_newlines(spans.iter().filter_map(|span| span.newline));
        let mut records = Vec::with_capacity(spans.len());

        for (row, span) in spans.into_iter().enumerate() {
            let raw_record = &bytes[span.content.clone()];
            let decoded = decode_text(raw_record, encoding)?;
            let fields = if span.fields.is_empty() {
                Vec::new()
            } else {
                let parsed = parse_record(&decoded, delimiter)?;
                if parsed.len() != span.fields.len() {
                    return Err(CsvError::ParserMismatch);
                }
                span.fields
                    .into_iter()
                    .zip(parsed)
                    .map(|(field_span, value)| CsvField {
                        value,
                        raw_range: field_span.range,
                        was_quoted: field_span.was_quoted,
                    })
                    .collect()
            };
            if !progress(row, &fields) {
                return Ok(None);
            }
            records.push(CsvRecord {
                raw_range: span.range,
                fields,
            });
        }

        let origin_field_counts = records
            .iter()
            .map(|record| record.fields.len())
            .collect::<Vec<_>>();
        let parsed_cell_count = origin_field_counts.iter().sum();
        let parsed_max_column_count = origin_field_counts.iter().copied().max().unwrap_or(0);
        Ok(Some(Self {
            origin_record_count: records.len(),
            origin_field_counts,
            origin_ends_with_newline: bytes.ends_with(b"\n"),
            introduced_fields: std::collections::BTreeMap::new(),
            bytes,
            encoding,
            delimiter,
            newline,
            records,
            parsed_cell_count,
            parsed_max_column_count,
        }))
    }

    // Returns every source byte. An unchanged save uses these bytes as-is.
    pub fn source_bytes(&self) -> &[u8] {
        &self.bytes
    }
}
