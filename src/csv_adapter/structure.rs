use super::*;

impl CsvDocument {
    /// Applies a worker-prepared row/column mapping. Original field bytes are reused wherever
    /// the operation leaves a field unchanged; only structurally touched records are rebuilt.
    pub(crate) fn apply_structure(
        &self,
        plan: &StructurePlan,
        formula_updates: &std::collections::BTreeMap<(u64, CellAddress), String>,
        table_id: u64,
    ) -> Result<Self, CsvError> {
        let mut output = Vec::new();
        if self.encoding == CsvEncoding::Utf8Bom {
            output.extend_from_slice(&self.bytes[..3]);
        }
        let (axis, operation) = match &plan.operation {
            StructureOperation::Insert { axis, .. }
            | StructureOperation::Delete { axis, .. }
            | StructureOperation::Duplicate { axis, .. }
            | StructureOperation::Move { axis, .. } => (*axis, &plan.operation),
            StructureOperation::ReorderRows { .. } => (StructureAxis::Rows, &plan.operation),
            StructureOperation::Fill { .. }
            | StructureOperation::SetTextCell { .. }
            | StructureOperation::SetTextCells { .. } => (StructureAxis::Rows, &plan.operation),
        };
        let mut touched_rows = std::collections::BTreeSet::new();
        let maximum_width = self
            .records
            .iter()
            .map(|record| record.fields.len())
            .max()
            .unwrap_or(0);
        let formula_rows = formula_updates
            .keys()
            .filter_map(|(id, address)| (*id == table_id).then_some(address.row))
            .collect::<std::collections::BTreeSet<_>>();
        for (output_row, source_row) in plan.row_sources.iter().copied().enumerate() {
            let source_record = source_row.and_then(|row| self.records.get(row));
            let output_width = match source_record {
                Some(record) if axis == StructureAxis::Columns => {
                    mapped_record_width(record.fields.len(), operation, plan)
                }
                Some(record) => record.fields.len(),
                None if matches!(
                    operation,
                    StructureOperation::Insert {
                        axis: StructureAxis::Rows,
                        ..
                    }
                ) =>
                {
                    maximum_width
                }
                None => 0,
            };
            let source_width = source_record.map_or(0, |record| record.fields.len());
            let columns_unchanged = (0..output_width)
                .all(|column| plan.column_sources.get(column).copied().flatten() == Some(column))
                && output_width == source_width;
            let row_changed = source_row != Some(output_row)
                || !columns_unchanged
                || formula_rows.contains(&output_row);

            let record_bytes = if !row_changed {
                source_record.map(|record| self.bytes[record.raw_range.clone()].to_vec())
            } else {
                touched_rows.insert(output_row);
                Some(self.encode_structured_record(
                    output_row,
                    source_row,
                    output_width,
                    plan,
                    formula_updates,
                    table_id,
                )?)
            };
            if let Some(mut bytes) = record_bytes {
                let needs_newline =
                    output_row + 1 < plan.row_sources.len() || self.origin_ends_with_newline;
                if needs_newline {
                    if !bytes.ends_with(b"\n") {
                        bytes.extend_from_slice(self.preferred_newline());
                    }
                } else if bytes.ends_with(b"\r\n") {
                    bytes.truncate(bytes.len() - 2);
                } else if bytes.ends_with(b"\n") {
                    bytes.truncate(bytes.len() - 1);
                }
                output.extend_from_slice(&bytes);
            }
        }
        let content_start = if self.encoding == CsvEncoding::Utf8Bom {
            3
        } else {
            0
        };
        let mut updated =
            Self::parse_resolved(output, self.encoding, content_start, self.delimiter)?;
        updated.origin_record_count = updated.records.len();
        updated.origin_field_counts = updated
            .records
            .iter()
            .map(|record| record.fields.len())
            .collect();
        updated.origin_ends_with_newline = updated.bytes.ends_with(b"\n");
        updated.introduced_fields.clear();
        if updated.records.len() != plan.row_sources.len() {
            return Err(CsvError::OutputMismatch);
        }
        for row in touched_rows {
            if updated.records.get(row).is_none() {
                return Err(CsvError::OutputMismatch);
            }
        }
        Ok(updated)
    }
}
