use super::*;

impl MarkdownDocument {
    // Rebuilds edited rows when needed, then reparses them to check table coordinates and values.
    pub fn apply_changes(&self, changes: &[MarkdownCellChange]) -> Result<Self, MarkdownError> {
        std::str::from_utf8(&self.bytes).map_err(|_| MarkdownError::InvalidUtf8)?;
        let mut by_row = std::collections::BTreeMap::<
            (usize, usize),
            std::collections::BTreeMap<usize, String>,
        >::new();
        for change in changes {
            if change.row >= MAX_MARKDOWN_ROWS {
                return Err(MarkdownError::ResourceLimit(
                    crate::i18n::ResourceLimit::RowCount,
                ));
            }
            if change.column >= MAX_MARKDOWN_COLUMNS {
                return Err(MarkdownError::ResourceLimit(
                    crate::i18n::ResourceLimit::ColumnCount,
                ));
            }
            if self.tables.get(change.table).is_none() {
                return Err(MarkdownError::CellNotFound {
                    table: change.table,
                    row: change.row,
                    column: change.column,
                });
            }
            let exists = self
                .tables
                .get(change.table)
                .and_then(|table| table.rows.get(change.row))
                .and_then(|row| row.get(change.column))
                .is_some();
            if !exists && change.value.is_empty() {
                continue;
            }
            by_row
                .entry((change.table, change.row))
                .or_default()
                .insert(change.column, change.value.clone());
        }

        if by_row.is_empty() {
            return Ok(self.clone());
        }

        let extents = self.projected_extents(&by_row)?;
        let shape_changed = self.tables.iter().enumerate().any(|(index, table)| {
            let (rows, columns) = extents[index];
            let current_columns = table
                .row_syntax
                .iter()
                .map(|row| row.source_cell_count)
                .max()
                .unwrap_or(1);
            rows != table.rows.len() || columns != current_columns
        });
        if shape_changed {
            return self.apply_extent_changes(&by_row, &extents);
        }

        let rebuilt = self.apply_markdown_rows(&by_row)?;
        if self.changes_match(&rebuilt, &by_row) && self.same_table_shape(&rebuilt, &by_row) {
            self.keep_open_time_extents(rebuilt)
        } else {
            Err(MarkdownError::OutputMismatch)
        }
    }

    /// Finds the rectangular range that keeps source cells and the open-time floor.
    fn projected_extents(
        &self,
        changes: &std::collections::BTreeMap<
            (usize, usize),
            std::collections::BTreeMap<usize, String>,
        >,
    ) -> Result<Vec<(usize, usize)>, MarkdownError> {
        self.tables
            .iter()
            .enumerate()
            .map(|(table_index, table)| {
                let (floor_rows, floor_columns) = self
                    .source_extents
                    .get(table_index)
                    .copied()
                    .unwrap_or((1, 1));
                let changed_table = changes.keys().any(|(index, _)| *index == table_index);
                if !changed_table {
                    return Ok((
                        table.rows.len().max(1),
                        table
                            .row_syntax
                            .iter()
                            .map(|row| row.source_cell_count)
                            .max()
                            .unwrap_or(1)
                            .max(1),
                    ));
                }
                let mut last_row = None::<usize>;
                let mut last_column = None::<usize>;
                for (row_index, row) in table.rows.iter().enumerate() {
                    for (column, cell) in row.iter().enumerate() {
                        let value = changes
                            .get(&(table_index, row_index))
                            .and_then(|row| row.get(&column))
                            .unwrap_or(&cell.value);
                        if !value.is_empty() {
                            last_row = Some(last_row.map_or(row_index, |last| last.max(row_index)));
                            last_column = Some(last_column.map_or(column, |last| last.max(column)));
                        }
                    }
                }
                for (&(changed_table, row), row_changes) in changes {
                    if changed_table != table_index {
                        continue;
                    }
                    for (&column, value) in row_changes {
                        if !value.is_empty() {
                            last_row = Some(last_row.map_or(row, |last| last.max(row)));
                            last_column = Some(last_column.map_or(column, |last| last.max(column)));
                        }
                    }
                }
                let rows = floor_rows.max(last_row.map_or(1, |row| row.saturating_add(1)));
                let columns =
                    floor_columns.max(last_column.map_or(1, |column| column.saturating_add(1)));
                Ok((rows, columns))
            })
            .collect()
    }

    /// Rebuilds only tables whose rectangular extent grew or shrank.
    fn apply_extent_changes(
        &self,
        changes: &std::collections::BTreeMap<
            (usize, usize),
            std::collections::BTreeMap<usize, String>,
        >,
        extents: &[(usize, usize)],
    ) -> Result<Self, MarkdownError> {
        let text = std::str::from_utf8(&self.bytes).map_err(|_| MarkdownError::InvalidUtf8)?;
        let mut replacements = Vec::<(Range<usize>, Vec<u8>)>::new();
        for (table_index, table) in self.tables.iter().enumerate() {
            let Some((rows, columns)) = extents.get(table_index).copied() else {
                return Err(MarkdownError::OutputMismatch);
            };
            let changed_table = changes.keys().any(|(index, _)| *index == table_index);
            if !changed_table {
                continue;
            }
            if rows.saturating_mul(columns) > MAX_MARKDOWN_CELLS {
                return Err(MarkdownError::ResourceLimit(
                    crate::i18n::ResourceLimit::CellCount,
                ));
            }
            let Some(template) = table.row_syntax.get(1).or_else(|| table.row_syntax.first())
            else {
                return Err(MarkdownError::OutputMismatch);
            };
            let mut rendered_rows = Vec::with_capacity(rows);
            for row_index in 0..rows {
                let row = table.rows.get(row_index).map(Vec::as_slice).unwrap_or(&[]);
                let syntax = table.row_syntax.get(row_index).unwrap_or(template);
                let row_changes = changes.get(&(table_index, row_index));
                if row_changes.is_none()
                    && row.len() == columns
                    && syntax.source_cell_count == columns
                {
                    rendered_rows.push(
                        text.get(syntax.range.clone())
                            .ok_or(MarkdownError::OutputMismatch)?
                            .to_owned(),
                    );
                } else {
                    rendered_rows.push(rebuild_row_to_width(
                        text,
                        row,
                        syntax,
                        row_changes,
                        columns,
                    )?);
                }
            }
            let column_sources = (0..columns).map(Some).collect::<Vec<_>>();
            let old_width = table
                .row_syntax
                .iter()
                .map(|row| row.source_cell_count)
                .max()
                .unwrap_or(0);
            let separator = if old_width == columns {
                table.separator_line.clone()
            } else {
                transform_separator(&table.separator_line, &column_sources)
            };
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
            replacements.push((table.replacement_range(), replacement.into_bytes()));
        }
        replacements.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
        let mut bytes = self.bytes.clone();
        for (range, replacement) in replacements {
            bytes.splice(range, replacement);
        }
        let mut updated = Self::parse(bytes)?;
        if updated.tables.len() != self.tables.len() {
            return Err(MarkdownError::OutputMismatch);
        }
        for (table_index, (rows, columns)) in extents.iter().copied().enumerate() {
            if !changes.keys().any(|(index, _)| *index == table_index) {
                continue;
            }
            let table = updated
                .tables
                .get(table_index)
                .ok_or(MarkdownError::OutputMismatch)?;
            if table.rows.len() != rows || table.rows.iter().any(|row| row.len() != columns) {
                return Err(MarkdownError::OutputMismatch);
            }
        }
        updated.source_extents.clone_from(&self.source_extents);
        for (table_index, table) in self.tables.iter().enumerate() {
            let width = table.alignments.len().max(
                updated
                    .tables
                    .get(table_index)
                    .map_or(0, |table| table.parser_width),
            );
            super::alignment::carry_pending_alignments(
                self,
                table_index,
                &mut updated,
                table_index,
                &(0..width).map(Some).collect::<Vec<_>>(),
            )?;
        }
        if !self.changes_match(&updated, changes) {
            return Err(MarkdownError::OutputMismatch);
        }
        Ok(updated)
    }

    /// Keeps source-shape floors while adopting freshly parsed byte offsets.
    fn keep_open_time_extents(&self, mut updated: Self) -> Result<Self, MarkdownError> {
        if updated.tables.len() != self.source_extents.len() {
            return Err(MarkdownError::OutputMismatch);
        }
        updated.source_extents.clone_from(&self.source_extents);
        for (table_index, table) in self.tables.iter().enumerate() {
            let width = table.alignments.len().max(
                updated
                    .tables
                    .get(table_index)
                    .map_or(0, |table| table.parser_width),
            );
            super::alignment::carry_pending_alignments(
                self,
                table_index,
                &mut updated,
                table_index,
                &(0..width).map(Some).collect::<Vec<_>>(),
            )?;
        }
        Ok(updated)
    }

    /// Rebuilds one changed cell's display through pulldown's full-document reference context.
    pub(crate) fn display_for_cell_change(
        &self,
        change: &MarkdownCellChange,
    ) -> MarkdownCellDisplay {
        let parsed = self
            .apply_changes(std::slice::from_ref(change))
            .ok()
            .and_then(|updated| {
                let mut display = updated
                    .cell_display(change.table, change.row, change.column)?
                    .clone();
                display.limit_and_own_runs(Some(&updated), 4 * 1024);
                Some(display)
            });
        parsed.unwrap_or_else(|| MarkdownCellDisplay::parse_fragment(&change.value))
    }

    // Apply edited table cells while reusing unchanged parsed Markdown data.
    fn apply_markdown_rows(
        &self,
        changes: &std::collections::BTreeMap<
            (usize, usize),
            std::collections::BTreeMap<usize, String>,
        >,
    ) -> Result<Self, MarkdownError> {
        let mut replacements = Vec::<(Range<usize>, Vec<u8>)>::new();
        for (&(table_index, row_index), row_changes) in changes {
            let table = self
                .tables
                .get(table_index)
                .ok_or(MarkdownError::CellNotFound {
                    table: table_index,
                    row: row_index,
                    column: 0,
                })?;
            let row = table
                .rows
                .get(row_index)
                .ok_or(MarkdownError::CellNotFound {
                    table: table_index,
                    row: row_index,
                    column: 0,
                })?;
            let syntax = table
                .row_syntax
                .get(row_index)
                .ok_or(MarkdownError::OutputMismatch)?;
            // Every changed row is rebuilt so written cells follow one padding rule;
            // the rebuild keeps the bytes of the row's unchanged cells.
            replacements.push((
                syntax.range.clone(),
                rebuild_row_text(
                    std::str::from_utf8(&self.bytes).map_err(|_| MarkdownError::InvalidUtf8)?,
                    row,
                    syntax,
                    row_changes,
                )?
                .into_bytes(),
            ));
        }
        replacements.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
        let mut bytes = self.bytes.clone();
        for (range, replacement) in replacements {
            bytes.splice(range, replacement);
        }
        Self::parse(bytes)
    }

    // Check whether the parsed update contains exactly the requested cell changes.
    fn changes_match(
        &self,
        updated: &Self,
        changes: &std::collections::BTreeMap<
            (usize, usize),
            std::collections::BTreeMap<usize, String>,
        >,
    ) -> bool {
        changes.iter().all(|(&(table, row), row_changes)| {
            row_changes.iter().all(|(&column, value)| {
                updated
                    .tables
                    .get(table)
                    .and_then(|table| table.rows.get(row))
                    .and_then(|row| row.get(column))
                    .map_or(value.is_empty(), |cell| {
                        cell.value == escape_table_cell(value).trim_matches([' ', '\t'])
                    })
            })
        })
    }

    // Check whether the update keeps the same table and row layout.
    fn same_table_shape(
        &self,
        updated: &Self,
        changes: &std::collections::BTreeMap<
            (usize, usize),
            std::collections::BTreeMap<usize, String>,
        >,
    ) -> bool {
        self.tables.len() == updated.tables.len()
            && self.tables.iter().zip(&updated.tables).enumerate().all(
                |(table_index, (before, after))| {
                    before.rows.len() == after.rows.len()
                        && before.rows.iter().zip(&after.rows).enumerate().all(
                            |(row_index, (before_row, after_row))| {
                                if before_row.len() != after_row.len() {
                                    return false;
                                }
                                let Some(original_count) = before
                                    .row_syntax
                                    .get(row_index)
                                    .map(|syntax| syntax.source_cell_count)
                                else {
                                    return false;
                                };
                                let expected_count = changes.get(&(table_index, row_index)).map_or(
                                    original_count,
                                    |row_changes| {
                                        row_changes.iter().fold(
                                            original_count,
                                            |count, (&column, value)| {
                                                if !value.is_empty()
                                                    && before_row
                                                        .get(column)
                                                        .is_some_and(|cell| !cell.has_source)
                                                {
                                                    count.max(column + 1)
                                                } else {
                                                    count
                                                }
                                            },
                                        )
                                    },
                                );
                                after.row_syntax.get(row_index).is_some_and(|syntax| {
                                    syntax.source_cell_count == expected_count
                                })
                            },
                        )
                },
            )
    }
}
