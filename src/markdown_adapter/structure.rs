use super::*;

impl MarkdownDocument {
    /// Appends a blank GFM table without rewriting the existing source bytes.
    pub(crate) fn append_table(&self) -> Result<Self, MarkdownError> {
        let prefix = if self.bytes.is_empty() || self.bytes.ends_with(b"\n\n") {
            ""
        } else if self.bytes.ends_with(b"\n") {
            "\n"
        } else {
            "\n\n"
        };
        let mut bytes = self.bytes.clone();
        bytes.extend_from_slice(prefix.as_bytes());
        bytes.extend_from_slice(b"|  |\n| --- |\n");
        let updated = Self::parse(bytes)?;
        if updated.tables.len() != self.tables.len().saturating_add(1) {
            return Err(MarkdownError::OutputMismatch);
        }
        Ok(updated)
    }

    /// Duplicates one complete table block at its current source position.
    pub(crate) fn duplicate_table(&self, table_index: usize) -> Result<Self, MarkdownError> {
        let table = self
            .tables
            .get(table_index)
            .ok_or(MarkdownError::CellNotFound {
                table: table_index,
                row: 0,
                column: 0,
            })?;
        let text = std::str::from_utf8(&self.bytes).map_err(|_| MarkdownError::InvalidUtf8)?;
        let block = text
            .get(table.source_range.clone())
            .ok_or(MarkdownError::OutputMismatch)?;
        let mut bytes = Vec::with_capacity(self.bytes.len().saturating_add(block.len() + 4));
        bytes.extend_from_slice(&self.bytes[..table.source_range.end]);
        bytes.extend_from_slice(b"\n\n");
        bytes.extend_from_slice(block.as_bytes());
        bytes.extend_from_slice(b"\n");
        bytes.extend_from_slice(&self.bytes[table.source_range.end..]);
        let updated = Self::parse(bytes)?;
        if updated.tables.len() != self.tables.len().saturating_add(1) {
            return Err(MarkdownError::OutputMismatch);
        }
        Ok(updated)
    }

    /// Removes the table and its name heading, leaving one blank line between the remaining text.
    pub(crate) fn delete_table(&self, table_index: usize) -> Result<Self, MarkdownError> {
        if self.tables.len() <= 1 {
            return Err(MarkdownError::LastTable);
        }
        let table = self
            .tables
            .get(table_index)
            .ok_or(MarkdownError::CellNotFound {
                table: table_index,
                row: 0,
                column: 0,
            })?;
        let text = std::str::from_utf8(&self.bytes).map_err(|_| MarkdownError::InvalidUtf8)?;
        // The name heading goes with the table; other text outside the table stays unchanged.
        let start = table
            .heading_range
            .as_ref()
            .map_or(table.source_range.start, |range| range.start);
        let before = without_trailing_blank_lines(&text[..start]);
        let after = without_leading_blank_lines(&text[table.source_range.end..]);
        let mut bytes = Vec::with_capacity(self.bytes.len());
        bytes.extend_from_slice(before.as_bytes());
        if !before.is_empty() && !after.is_empty() {
            bytes.extend_from_slice(table.newline.as_bytes());
        }
        bytes.extend_from_slice(after.as_bytes());
        let updated = Self::parse(bytes)?;
        if updated.tables.len() + 1 != self.tables.len() {
            return Err(MarkdownError::OutputMismatch);
        }
        Ok(updated)
    }

    /// Rebuilds only the target GFM table for a prepared row/column operation.
    pub(crate) fn apply_structure(
        &self,
        table_index: usize,
        table_id: u64,
        plan: &StructurePlan,
        formula_updates: &std::collections::BTreeMap<(u64, CellAddress), String>,
    ) -> Result<Self, MarkdownError> {
        let table = self
            .tables
            .get(table_index)
            .ok_or(MarkdownError::CellNotFound {
                table: table_index,
                row: 0,
                column: 0,
            })?;
        let text = std::str::from_utf8(&self.bytes).map_err(|_| MarkdownError::InvalidUtf8)?;
        let width = plan.column_sources.len();
        let mut rendered_rows = Vec::with_capacity(plan.row_sources.len());
        for (output_row, source_row) in plan.row_sources.iter().copied().enumerate() {
            let source_cells = source_row.and_then(|row| table.rows.get(row));
            let source_syntax = source_row.and_then(|row| table.row_syntax.get(row));
            let mut row_cells = Vec::with_capacity(width);
            let mut changes = std::collections::BTreeMap::new();
            let mut formula_changed = false;
            for output_column in 0..width {
                let source_column = plan.column_sources.get(output_column).copied().flatten();
                let mut cell = source_column
                    .and_then(|column| source_cells.and_then(|cells| cells.get(column)))
                    .cloned()
                    .unwrap_or(MarkdownCell {
                        value: String::new(),
                        source_range: 0..0,
                        has_source: false,
                    });
                if let Some(formula) = formula_updates.get(&(
                    table_id,
                    CellAddress {
                        row: output_row,
                        column: output_column,
                    },
                )) {
                    cell.value.clone_from(formula);
                    formula_changed = true;
                }
                changes.insert(output_column, cell.value.clone());
                row_cells.push(cell);
            }
            let source_width = source_cells.map_or(0, Vec::len);
            let columns_unchanged = width == source_width
                && (0..width).all(|column| {
                    plan.column_sources.get(column).copied().flatten() == Some(column)
                });
            if columns_unchanged && !formula_changed {
                let syntax = source_syntax.ok_or(MarkdownError::OutputMismatch)?;
                rendered_rows.push(
                    text.get(syntax.range.clone())
                        .ok_or(MarkdownError::OutputMismatch)?
                        .to_owned(),
                );
                continue;
            }

            if columns_unchanged {
                let syntax = source_syntax.ok_or(MarkdownError::OutputMismatch)?;
                rendered_rows.push(rebuild_row_text(text, &row_cells, syntax, &changes)?);
                continue;
            }
            let template = table
                .row_syntax
                .get(source_row.unwrap_or(0))
                .or_else(|| table.row_syntax.first())
                .ok_or(MarkdownError::OutputMismatch)?;
            let mut syntax = template.clone();
            let surplus_ranges = if let Some(source_syntax) = source_syntax {
                if source_syntax.source_cell_count > source_width
                    && source_syntax.cell_ranges.len() < source_syntax.source_cell_count
                {
                    return Err(MarkdownError::OutputMismatch);
                }
                source_syntax
                    .cell_ranges
                    .iter()
                    .skip(source_width)
                    .cloned()
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            syntax.source_cell_count = width + surplus_ranges.len();
            syntax.cell_ranges = (0..width)
                .map(|output_column| {
                    plan.column_sources
                        .get(output_column)
                        .copied()
                        .flatten()
                        .and_then(|column| {
                            source_syntax.and_then(|source| source.cell_ranges.get(column))
                        })
                        .cloned()
                        .unwrap_or(0..0)
                })
                .collect::<Vec<_>>();
            syntax.cell_ranges.extend(surplus_ranges);
            if source_row.is_none() {
                syntax.cell_ranges.clear();
                syntax.source_cell_count = width;
            }
            rendered_rows.push(rebuild_row_text(text, &row_cells, &syntax, &changes)?);
        }

        let separator = transform_separator(&table.separator_line, &plan.column_sources);
        let mut replacement = String::new();
        if let Some((header, body)) = rendered_rows.split_first() {
            replacement.push_str(header);
            replacement.push_str(&table.newline);
            replacement.push_str(&separator);
            for row in body {
                replacement.push_str(&table.newline);
                replacement.push_str(row);
            }
        }
        let range = table.replacement_range();
        let mut bytes = self.bytes.clone();
        bytes.splice(range, replacement.into_bytes());
        let mut updated = Self::parse(bytes)?;
        let updated_table = updated
            .tables
            .get(table_index)
            .ok_or(MarkdownError::OutputMismatch)?;
        if updated_table.rows.len() != plan.row_sources.len()
            || updated_table.rows.iter().any(|row| row.len() != width)
        {
            return Err(MarkdownError::OutputMismatch);
        }
        self.carry_structure_alignments(
            table_index,
            &mut updated,
            &plan.operation,
            &plan.column_sources,
        )?;
        Ok(updated)
    }
}

/// Drops blank lines at the end of the text but keeps the last line's own line break.
fn without_trailing_blank_lines(text: &str) -> &str {
    let mut end = text.len();
    while end > 0 && text[..end].ends_with('\n') {
        let line_start = text[..end - 1].rfind('\n').map_or(0, |index| index + 1);
        if !text[line_start..end].trim().is_empty() {
            break;
        }
        end = line_start;
    }
    &text[..end]
}

/// Drops blank lines, and the rest of the table's last line, at the start of the text.
fn without_leading_blank_lines(text: &str) -> &str {
    let mut start = 0;
    while start < text.len() {
        let line_end = text[start..]
            .find('\n')
            .map_or(text.len(), |offset| start + offset + 1);
        if !text[start..line_end].trim().is_empty() {
            break;
        }
        start = line_end;
    }
    &text[start..]
}
