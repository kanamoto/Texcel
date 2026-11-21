use super::*;

impl CsvDocument {
    /// Measures one existing-field replacement using the source encoding and quote style.
    pub(crate) fn source_cell_byte_delta(
        &self,
        row: usize,
        column: usize,
        value: Option<&str>,
    ) -> Option<i128> {
        let record = self.records.get(row)?;
        let field = record.fields.get(column)?;
        let text = value.unwrap_or("");
        let was_quoted = match value {
            Some(text) => field.was_quoted || (record.fields.len() == 1 && text.is_empty()),
            None => record.fields.len() == 1,
        };
        let replacement = match encode_replacement(text, self.delimiter, was_quoted, self.encoding)
        {
            Ok(bytes) => bytes.len(),
            Err(CsvError::UnrepresentableCharacter) => {
                let needs_quotes = was_quoted
                    || text.contains(self.delimiter)
                    || text.contains('"')
                    || text.contains('\r')
                    || text.contains('\n');
                let escaped = text.len().saturating_add(text.matches('"').count());
                escaped.saturating_add(usize::from(needs_quotes) * 2)
            }
            Err(_) => return None,
        };
        Some(replacement as i128 - field.raw_range.len() as i128)
    }

    // Replaces changed cell ranges, adds only needed fields or rows, and reparses.
    pub fn apply_changes(&self, changes: &[CsvCellChange]) -> Result<Self, CsvError> {
        if changes.is_empty() {
            return Ok(self.clone());
        }
        // An empty value in a row or field that Texcel added means "no field", so the
        // saved range shrinks back instead of keeping empty records or quoted "" cells.
        let normalized = changes
            .iter()
            .map(|change| {
                let original_width = self
                    .origin_field_counts
                    .get(change.row)
                    .copied()
                    .unwrap_or(0);
                let introduced =
                    change.row >= self.origin_record_count || change.column >= original_width;
                let value = if introduced && change.value.as_deref() == Some("") {
                    None
                } else {
                    change.value.clone()
                };
                CsvCellChange {
                    row: change.row,
                    column: change.column,
                    value,
                }
            })
            .collect::<Vec<_>>();
        let changes = normalized.as_slice();
        let mut by_row = std::collections::BTreeMap::<
            usize,
            std::collections::BTreeMap<usize, Option<&str>>,
        >::new();
        let mut introduced_fields = self.introduced_fields.clone();
        for change in changes {
            by_row
                .entry(change.row)
                .or_default()
                .insert(change.column, change.value.as_deref());
            let original_width = self
                .origin_field_counts
                .get(change.row)
                .copied()
                .unwrap_or(0);
            if change.row >= self.origin_record_count || change.column >= original_width {
                let columns = introduced_fields.entry(change.row).or_default();
                if change.value.is_some() {
                    columns.insert(change.column);
                } else {
                    columns.remove(&change.column);
                }
                if columns.is_empty() {
                    introduced_fields.remove(&change.row);
                }
            }
        }

        let row_widths = self
            .records
            .iter()
            .enumerate()
            .map(|(row, _record)| {
                desired_record_width(
                    self.origin_field_counts.get(row).copied().unwrap_or(0),
                    introduced_fields.get(&row),
                )
            })
            .collect::<Vec<_>>();
        let mut target_record_count = self.origin_record_count;
        for (row, width) in row_widths
            .iter()
            .copied()
            .enumerate()
            .skip(self.origin_record_count)
        {
            if width > 0 {
                target_record_count = target_record_count.max(row + 1);
            }
        }
        for (&row, fields) in by_row.range(self.records.len()..) {
            if fields.values().any(Option::is_some) {
                target_record_count = target_record_count.max(row + 1);
            }
        }
        let existing_rows = target_record_count.min(self.records.len());
        let mut target_column_count = row_widths
            .iter()
            .take(existing_rows)
            .copied()
            .max()
            .unwrap_or(0);
        let mut target_field_count = row_widths
            .iter()
            .take(existing_rows)
            .copied()
            .sum::<usize>();
        if target_record_count > self.records.len() {
            for (_, fields) in by_row.range(self.records.len()..target_record_count) {
                let width = desired_new_width(Some(fields));
                target_column_count = target_column_count.max(width);
                target_field_count = target_field_count.saturating_add(width);
            }
        }
        if target_record_count > MAX_CSV_ROWS
            || target_column_count > MAX_CSV_COLUMNS
            || target_field_count > MAX_CSV_CELLS
        {
            return Err(CsvError::ResourceLimit(
                crate::i18n::ResourceLimit::CsvOutputCellCount,
            ));
        }

        let mut replacements = Vec::<(Range<usize>, Vec<u8>)>::new();
        for (row, record) in self.records.iter().enumerate().take(target_record_count) {
            let width = row_widths.get(row).copied().unwrap_or(0);
            let Some(fields) = by_row.get(&row) else {
                if width == record.fields.len() {
                    continue;
                }
                replacements.push((
                    record.raw_range.start..self.record_content_end(record),
                    self.encode_record_content(row, record, width, None)?,
                ));
                continue;
            };
            replacements.push((
                record.raw_range.start..self.record_content_end(record),
                self.encode_record_content(row, record, width, Some(fields))?,
            ));
        }

        let first_new_row = self.records.len();
        let mut appended_bytes = Vec::new();
        if target_record_count > first_new_row {
            let line_ending = self.preferred_newline();
            let has_final_newline = self.bytes.ends_with(b"\n");
            if !self.records.is_empty() && !has_final_newline {
                appended_bytes.extend_from_slice(line_ending);
            }
            for row in first_new_row..target_record_count {
                let row_changes = by_row.get(&row);
                let width = desired_new_width(row_changes);
                appended_bytes.extend_from_slice(&encode_new_record(
                    row_changes,
                    width,
                    self.delimiter,
                    self.encoding,
                )?);
                let is_last = row + 1 == target_record_count;
                if !is_last || has_final_newline || self.records.is_empty() {
                    appended_bytes.extend_from_slice(line_ending);
                }
            }
        }

        if target_record_count < self.records.len() {
            let content_start = if self.encoding == CsvEncoding::Utf8Bom {
                3
            } else {
                0
            };
            let remove_start = if target_record_count == 0 {
                content_start
            } else if self.origin_ends_with_newline {
                self.records[target_record_count - 1].raw_range.end
            } else {
                self.record_content_end(&self.records[target_record_count - 1])
            };
            replacements.push((remove_start..self.bytes.len(), Vec::new()));
        }

        if !appended_bytes.is_empty() {
            let has_open_last_record = self
                .records
                .last()
                .is_some_and(|record| self.record_content_end(record) == self.bytes.len());
            if has_open_last_record
                && target_record_count > self.records.len()
                && let Some((_, row_replacement)) = replacements.iter_mut().find(|(range, _)| {
                    range.start == self.records[self.records.len() - 1].raw_range.start
                        && range.end
                            == self.record_content_end(&self.records[self.records.len() - 1])
                })
            {
                row_replacement.extend_from_slice(&appended_bytes);
            } else {
                replacements.push((self.bytes.len()..self.bytes.len(), appended_bytes));
            }
        }

        replacements.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
        let mut bytes = self.bytes.clone();
        for (range, replacement) in replacements {
            bytes.splice(range, replacement);
        }
        let content_start = if self.encoding == CsvEncoding::Utf8Bom {
            3
        } else {
            0
        };
        let mut updated =
            Self::parse_resolved(bytes, self.encoding, content_start, self.delimiter)?;
        updated.origin_record_count = self.origin_record_count;
        updated
            .origin_field_counts
            .clone_from(&self.origin_field_counts);
        updated.origin_ends_with_newline = self.origin_ends_with_newline;
        updated.introduced_fields = introduced_fields;
        if updated.records.len() != target_record_count {
            return Err(CsvError::OutputMismatch);
        }
        for (row, fields) in &by_row {
            let width = if *row < self.records.len() {
                row_widths.get(*row).copied().unwrap_or(0)
            } else {
                desired_new_width(Some(fields))
            };
            if *row < target_record_count
                && updated
                    .records
                    .get(*row)
                    .map_or(0, |record| record.fields.len())
                    != width
            {
                return Err(CsvError::OutputMismatch);
            }
            for (&column, value) in fields {
                let actual = updated
                    .records
                    .get(*row)
                    .and_then(|record| record.fields.get(column));
                match value {
                    Some(expected) if actual.is_some_and(|field| field.value == *expected) => {}
                    None if column >= width
                        || actual.is_some_and(|field| field.value.is_empty()) => {}
                    _ => return Err(CsvError::OutputMismatch),
                }
            }
        }
        Ok(updated)
    }

    // Encode one edited CSV record while preserving its surrounding source bytes.
    pub(super) fn encode_structured_record(
        &self,
        output_row: usize,
        source_row: Option<usize>,
        width: usize,
        plan: &StructurePlan,
        formula_updates: &std::collections::BTreeMap<(u64, CellAddress), String>,
        table_id: u64,
    ) -> Result<Vec<u8>, CsvError> {
        let axis = match &plan.operation {
            StructureOperation::Insert { axis, .. }
            | StructureOperation::Delete { axis, .. }
            | StructureOperation::Duplicate { axis, .. }
            | StructureOperation::Move { axis, .. } => *axis,
            StructureOperation::ReorderRows { .. } => StructureAxis::Rows,
            StructureOperation::Fill { .. }
            | StructureOperation::SetTextCell { .. }
            | StructureOperation::SetTextCells { .. } => StructureAxis::Rows,
        };
        let delimiter = encode_text(&self.delimiter.to_string(), self.encoding)?;
        let record = source_row.and_then(|row| self.records.get(row));
        let mut bytes = Vec::new();
        for output_column in 0..width {
            if output_column > 0 {
                bytes.extend_from_slice(&delimiter);
            }
            let source_column = if axis == StructureAxis::Columns {
                plan.column_sources.get(output_column).copied().flatten()
            } else {
                Some(output_column)
            };
            let original = source_column
                .and_then(|column| record.and_then(|record| record.fields.get(column)));
            let raw_source = original;
            let replacement = formula_updates.get(&(
                table_id,
                CellAddress {
                    row: output_row,
                    column: output_column,
                },
            ));
            if let Some(value) = replacement {
                bytes.extend_from_slice(&encode_replacement(
                    value,
                    self.delimiter,
                    raw_source.is_some_and(|field| field.was_quoted),
                    self.encoding,
                )?);
            } else if let Some(field) = raw_source {
                bytes.extend_from_slice(&self.bytes[field.raw_range.clone()]);
            } else {
                bytes.extend_from_slice(&encode_replacement(
                    "",
                    self.delimiter,
                    width == 1 && source_row.is_some(),
                    self.encoding,
                )?);
            }
        }
        Ok(bytes)
    }

    // Write field content with the existing delimiter and quote rules.
    pub(super) fn encode_record_content(
        &self,
        row: usize,
        record: &CsvRecord,
        width: usize,
        changes: Option<&std::collections::BTreeMap<usize, Option<&str>>>,
    ) -> Result<Vec<u8>, CsvError> {
        let delimiter = encode_text(&self.delimiter.to_string(), self.encoding)?;
        let mut output = Vec::new();
        for column in 0..width {
            if column > 0 {
                output.extend_from_slice(&delimiter);
            }
            let field = record.fields.get(column);
            let change = changes.and_then(|fields| fields.get(&column));
            let encoded = match change {
                Some(Some(value)) => encode_replacement(
                    value,
                    self.delimiter,
                    field.is_some_and(|field| field.was_quoted) || (width == 1 && value.is_empty()),
                    self.encoding,
                )?,
                Some(None) => encode_replacement("", self.delimiter, width == 1, self.encoding)?,
                None => match field {
                    Some(field) => self.bytes[field.raw_range.clone()].to_vec(),
                    None => encode_replacement(
                        "",
                        self.delimiter,
                        width == 1 && column == 0 && row < self.origin_record_count,
                        self.encoding,
                    )?,
                },
            };
            output.extend_from_slice(&encoded);
        }
        Ok(output)
    }

    // Find the byte offset where a record’s field content ends.
    pub(super) fn record_content_end(&self, record: &CsvRecord) -> usize {
        let range = &record.raw_range;
        if self.bytes.get(range.end.saturating_sub(2)..range.end) == Some(b"\r\n") {
            range.end - 2
        } else if self.bytes.get(range.end.saturating_sub(1)..range.end) == Some(b"\n") {
            range.end - 1
        } else {
            range.end
        }
    }

    // Return the newline style found in the source, or the default style.
    pub(super) fn preferred_newline(&self) -> &'static [u8] {
        self.records
            .first()
            .and_then(|record| {
                let range = &record.raw_range;
                if self.bytes.get(range.end.saturating_sub(2)..range.end) == Some(b"\r\n") {
                    Some(b"\r\n".as_slice())
                } else if self.bytes.get(range.end.saturating_sub(1)..range.end) == Some(b"\n") {
                    Some(b"\n".as_slice())
                } else {
                    None
                }
            })
            .unwrap_or(b"\n")
    }
}

#[cfg(test)]
impl CsvDocument {
    // Replaces existing fields, reparses, and leaves all other bytes unchanged.
    pub fn replace_field(&self, row: usize, column: usize, value: &str) -> Result<Self, CsvError> {
        self.apply_changes(&[CsvCellChange {
            row,
            column,
            value: Some(value.to_owned()),
        }])
    }
}
