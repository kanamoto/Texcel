use super::*;

#[path = "markdown_format/selection.rs"]
mod selection;

impl TexcelApp {
    // Apply bold, italic, strike, or code formatting to the selected text.
    pub(super) fn apply_markdown_modifier(
        &mut self,
        action: CellFormatAction,
        ctx: &egui::Context,
    ) {
        if !self.edit.is_editing() {
            self.apply_markdown_modifier_to_selection(action);
            return;
        }
        let marker = match action {
            CellFormatAction::Bold => "**",
            CellFormatAction::Italic => "*",
            CellFormatAction::Strike => "~~",
            CellFormatAction::Code => "`",
        };
        let Some(document_id) = self.edit_document_id else {
            return;
        };
        if self.edit.target != Some(crate::document::EditTarget::Cell)
            && self.edit.target != Some(crate::document::EditTarget::FormulaBar)
            && self.edit.target != Some(crate::document::EditTarget::DetailPanel)
        {
            return;
        }
        let source_is_csv = self
            .workspace
            .documents
            .get(&document_id)
            .is_some_and(|document| {
                matches!(document.source.as_deref(), Some(SourceDocument::Csv(_)))
            });
        if !source_is_csv
            && !matches!(
                self.workspace
                    .documents
                    .get(&document_id)
                    .and_then(|document| document.source.as_deref()),
                Some(SourceDocument::Markdown(_))
            )
        {
            return;
        }
        let Some((table_id, address)) = self.edit.table_id.zip(self.edit.address) else {
            return;
        };
        if self
            .workspace
            .documents
            .get(&document_id)
            .and_then(|document| document.tables.iter().find(|table| table.id == table_id))
            .is_some_and(|table| {
                crate::structure::is_formula_cell(
                    table,
                    address,
                    &Cell::text(self.edit.buffer.clone()),
                )
            })
        {
            self.status = Some(Message::new("status-formula-markdown-format-unsupported"));
            return;
        }
        let selected = self.text_selection.filter(|selection| {
            selection.document_id == document_id
                && selection.table_id == table_id
                && selection.address == address
        });
        let char_count = self.edit.buffer.chars().count();
        let (start_char, end_char) = selected
            .map(|selection| {
                (
                    selection.start.min(char_count),
                    selection.end.min(char_count),
                )
            })
            .unwrap_or((0, char_count));
        if matches!(action, CellFormatAction::Code) {
            let Some((formatted, selection_start, selection_end)) =
                markdown_code_candidate(&self.edit.buffer, start_char, end_char, source_is_csv)
            else {
                self.status = Some(Message::new("status-markdown-code-unrepresentable"));
                return;
            };
            if source_is_csv {
                self.pending_csv_promotion_cell_edits
                    .insert(document_id, (table_id, address, formatted));
                self.request_table_management(StructureAction::PromoteCsv, Some(table_id), None);
                if !self.pending_table_management.contains(&document_id) {
                    self.pending_csv_promotion_cell_edits.remove(&document_id);
                }
            } else {
                self.edit.buffer = formatted;
                self.edit.mark_buffer_changed();
                self.text_selection = Some(TextSelection {
                    document_id,
                    table_id,
                    address,
                    session_id: self.edit.session_id,
                    buffer_revision: self.edit.buffer_revision,
                    start: selection_start,
                    end: selection_end,
                });
                if let Some(mut state) = egui::widgets::text_edit::TextEditState::load(
                    ctx,
                    editor_id(document_id, self.edit.session_id),
                ) {
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::two(
                            egui::text::CCursor::new(selection_start),
                            egui::text::CCursor::new(selection_end),
                        )));
                    state.store(ctx, editor_id(document_id, self.edit.session_id));
                }
            }
            self.request_editor_focus = true;
            self.status = None;
            return;
        }
        let original_buffer = self.edit.buffer.clone();
        let semantic_buffer = crate::document::literal_text_value(&original_buffer);
        let literal_marker_removed = semantic_buffer.len() != original_buffer.len();
        let visible_text = if source_is_csv {
            semantic_buffer.to_owned()
        } else {
            crate::markdown_adapter::MarkdownCellDisplay::parse_fragment(&original_buffer)
                .plain_text
        };
        let (start_char, end_char) = if literal_marker_removed {
            (start_char.saturating_sub(1), end_char.saturating_sub(1))
        } else {
            (start_char, end_char)
        };
        if source_is_csv {
            let raw = semantic_buffer;
            let raw_start = byte_index_for_char(raw, start_char);
            let raw_end = byte_index_for_char(raw, end_char);
            let before = crate::export::escape_plain_markdown_cell(&raw[..raw_start]);
            let selected_value = &raw[raw_start..raw_end];
            let selected = crate::export::escape_plain_markdown_cell(selected_value);
            let after = crate::export::escape_plain_markdown_cell(&raw[raw_end..]);
            let formatted = if start_char == end_char {
                format!("{before}{marker}{marker}{after}")
            } else {
                format!("{before}{marker}{selected}{marker}{after}")
            };
            let Some((formatted, _)) =
                crate::markdown_adapter::MarkdownCellDisplay::persist_literal_marker(
                    &formatted, raw,
                )
            else {
                self.status = Some(Message::new("status-markdown-format-unrepresentable"));
                return;
            };
            self.pending_csv_promotion_cell_edits
                .insert(document_id, (table_id, address, formatted));
            self.request_table_management(StructureAction::PromoteCsv, Some(table_id), None);
            if !self.pending_table_management.contains(&document_id) {
                self.pending_csv_promotion_cell_edits.remove(&document_id);
            }
            self.request_editor_focus = true;
            return;
        }
        if literal_marker_removed {
            self.edit.buffer = semantic_buffer.to_owned();
        }
        let mut start = byte_index_for_char(&self.edit.buffer, start_char);
        let mut end = byte_index_for_char(&self.edit.buffer, end_char);
        let starts_inside = self.edit.buffer[start..end].starts_with(marker);
        let ends_inside = self.edit.buffer[start..end].ends_with(marker)
            && end.saturating_sub(start) >= marker.len().saturating_mul(2);
        let left_wrapped = start >= marker.len()
            && self.edit.buffer.get(start - marker.len()..start) == Some(marker);
        let right_wrapped = self.edit.buffer.get(end..end + marker.len()) == Some(marker);
        if start == end {
            self.edit.buffer.insert_str(start, marker);
            start += marker.len();
            self.edit.buffer.insert_str(start, marker);
            end = start;
        } else if (starts_inside && ends_inside) || (left_wrapped && right_wrapped) {
            if starts_inside && ends_inside {
                self.edit.buffer.replace_range(end - marker.len()..end, "");
                self.edit
                    .buffer
                    .replace_range(start..start + marker.len(), "");
                end -= marker.len() * 2;
            } else {
                self.edit.buffer.replace_range(end..end + marker.len(), "");
                self.edit
                    .buffer
                    .replace_range(start - marker.len()..start, "");
                start -= marker.len();
                end -= marker.len();
            }
        } else if !left_wrapped && right_wrapped {
            self.edit.buffer.insert_str(start, marker);
            start += marker.len();
            end += marker.len();
        } else if left_wrapped && !right_wrapped {
            self.edit.buffer.insert_str(end, marker);
        } else {
            self.edit.buffer.insert_str(start, marker);
            start += marker.len();
            end += marker.len();
            self.edit.buffer.insert_str(end, marker);
        }
        let Some((protected, marker_inserted_at)) =
            crate::markdown_adapter::MarkdownCellDisplay::persist_literal_marker(
                &self.edit.buffer,
                &visible_text,
            )
        else {
            self.edit.buffer = original_buffer;
            self.status = Some(Message::new("status-markdown-format-unrepresentable"));
            return;
        };
        if let Some(insertion) = marker_inserted_at {
            if insertion < start {
                start += 1;
                end += 1;
            } else if insertion <= end {
                end += 1;
            }
        }
        self.edit.buffer = protected;
        self.edit.mark_buffer_changed();
        let new_start = self.edit.buffer[..start].chars().count();
        let new_end = self.edit.buffer[..end].chars().count();
        self.text_selection = (new_start < new_end).then_some(TextSelection {
            document_id,
            table_id,
            address,
            session_id: self.edit.session_id,
            buffer_revision: self.edit.buffer_revision,
            start: new_start,
            end: new_end,
        });
        if let Some(mut state) = egui::widgets::text_edit::TextEditState::load(
            ctx,
            editor_id(document_id, self.edit.session_id),
        ) {
            let cursor_range = if new_start == new_end {
                egui::text::CCursorRange::one(egui::text::CCursor::new(new_start))
            } else {
                egui::text::CCursorRange::two(
                    egui::text::CCursor::new(new_start),
                    egui::text::CCursor::new(new_end),
                )
            };
            state.cursor.set_char_range(Some(cursor_range));
            state.store(ctx, editor_id(document_id, self.edit.session_id));
        }
        self.request_editor_focus = true;
        self.status = None;
    }
}

// Builds code markup only when the full cell keeps the same visible text.
fn markdown_code_candidate(
    raw: &str,
    start_char: usize,
    end_char: usize,
    source_is_csv: bool,
) -> Option<(String, usize, usize)> {
    let semantic_source = crate::document::literal_text_value(raw);
    let marker_removed = semantic_source.len() != raw.len();
    let mut start_char = start_char;
    let mut end_char = end_char;
    if marker_removed {
        start_char = start_char.saturating_sub(1);
        end_char = end_char.saturating_sub(1);
    }
    let char_count = semantic_source.chars().count();
    start_char = start_char.min(char_count);
    end_char = end_char.min(char_count).max(start_char);
    let start = byte_index_for_char(semantic_source, start_char);
    let end = byte_index_for_char(semantic_source, end_char);
    let before = &semantic_source[..start];
    let selected = &semantic_source[start..end];
    let after = &semantic_source[end..];

    let (visible_text, selected_text, mut formatted, selection_start, selection_end, expect_code) =
        if source_is_csv {
            let selected_markup = crate::export::markdown_code_markup(selected)?;
            let formatted = format!(
                "{}{}{}",
                crate::export::escape_plain_markdown_cell(before),
                selected_markup,
                crate::export::escape_plain_markdown_cell(after)
            );
            let selection_start = crate::export::escape_plain_markdown_cell(before)
                .chars()
                .count();
            let selection_end = selection_start + selected_markup.chars().count();
            (
                crate::document::literal_text_value(raw).to_owned(),
                selected.to_owned(),
                formatted,
                selection_start,
                selection_end,
                true,
            )
        } else {
            let whole_display = crate::markdown_adapter::MarkdownCellDisplay::parse_fragment(raw);
            let whole = whole_display.plain_text;
            let selection_contains_cell_marker = !marker_removed
                && whole_display
                    .literal_marker_range
                    .as_ref()
                    .is_some_and(|range| range.start >= start && range.start < end);
            let selected_display = if selection_contains_cell_marker {
                crate::markdown_adapter::MarkdownCellDisplay::parse_fragment(selected)
            } else {
                markdown_fragment_display(selected)
            };
            let selected_text = selected_display.plain_text;
            let selected_is_code_content = before.ends_with('`')
                && after.starts_with('`')
                && whole_display.runs.iter().any(|run| {
                    run.code && !run.link && run.text.as_deref() == Some(selected_text.as_str())
                });
            let (prefix, suffix, selected_markup, remove_code) = if selected_is_code_content {
                let prefix = before.get(..before.len().saturating_sub(1))?;
                let suffix = after.get(1..)?;
                if format!(
                    "{}{}{}",
                    markdown_fragment_plain_text(prefix),
                    selected_text,
                    markdown_fragment_plain_text(suffix)
                ) != whole
                {
                    return None;
                }
                (
                    prefix,
                    suffix,
                    crate::export::escape_plain_markdown_cell(&selected_text),
                    true,
                )
            } else {
                let before_text = markdown_fragment_plain_text(before);
                let after_text = markdown_fragment_plain_text(after);
                if format!("{before_text}{selected_text}{after_text}") != whole {
                    return None;
                }
                let remove_code = !selected_display.runs.is_empty()
                    && selected_display
                        .runs
                        .iter()
                        .all(|run| run.code && !run.link);
                let selected_markup = if remove_code {
                    crate::export::escape_plain_markdown_cell(&selected_text)
                } else {
                    crate::export::markdown_code_markup(&selected_text)?
                };
                (before, after, selected_markup, remove_code)
            };
            let selection_start = prefix.chars().count();
            let selection_end = selection_start + selected_markup.chars().count();
            (
                whole,
                selected_text,
                format!("{prefix}{selected_markup}{suffix}"),
                selection_start,
                selection_end,
                !remove_code,
            )
        };

    let protected = crate::document::encode_literal_text(&formatted);
    let protection_added = protected.len() != formatted.len();
    formatted = protected;
    let reparsed = crate::markdown_adapter::MarkdownCellDisplay::parse_fragment(&formatted);
    let valid = reparsed.plain_text == visible_text
        && (!expect_code
            || selected_text.is_empty()
            || reparsed
                .runs
                .iter()
                .any(|run| run.code && run.text.as_deref() == Some(selected_text.as_str())));
    valid.then_some((
        formatted,
        selection_start + usize::from(protection_added),
        selection_end + usize::from(protection_added),
    ))
}

// Keeps a real leading apostrophe when parsing a selected inline fragment.
fn markdown_fragment_plain_text(source: &str) -> String {
    markdown_fragment_display(source).plain_text
}

// Parses a fragment without mistaking a real apostrophe for a cell marker.
fn markdown_fragment_display(source: &str) -> crate::markdown_adapter::MarkdownCellDisplay {
    let mut display = crate::markdown_adapter::MarkdownCellDisplay::parse_fragment(source);
    if let Some(range) = &display.literal_marker_range {
        let mut protected = String::with_capacity(source.len().saturating_add(1));
        if let Some(prefix) = source.get(..range.start) {
            protected.push_str(prefix);
            protected.push('\\');
            protected.push_str(source.get(range.start..).unwrap_or_default());
            display = crate::markdown_adapter::MarkdownCellDisplay::parse_fragment(&protected);
        }
    }
    let visible = display.plain_text.chars().collect::<Vec<_>>();
    let leading = source
        .chars()
        .take_while(|character| character.is_whitespace())
        .collect::<Vec<_>>();
    let leading_overlap = (0..=leading.len().min(visible.len()))
        .rev()
        .find(|count| visible.starts_with(&leading[..*count]))
        .unwrap_or(0);
    let trailing = source
        .chars()
        .rev()
        .take_while(|character| character.is_whitespace())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    let trailing_overlap = (0..=trailing.len().min(visible.len()))
        .rev()
        .find(|count| visible.ends_with(&trailing[trailing.len() - *count..]))
        .unwrap_or(0);
    if leading_overlap < leading.len() {
        display
            .plain_text
            .insert_str(0, &leading[leading_overlap..].iter().collect::<String>());
    }
    if trailing_overlap < trailing.len() {
        display.plain_text.push_str(
            &trailing[..trailing.len() - trailing_overlap]
                .iter()
                .collect::<String>(),
        );
    }
    display
}
