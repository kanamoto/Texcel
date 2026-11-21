use super::*;
mod fields;
use fields::data_edit_text_row;
mod footer;
mod sort_keys;
use footer::{DataEditFooterAction, draw_data_edit_footer};

impl TexcelApp {
    // Draw and handle the table sort or data edit dialog.
    pub(super) fn draw_data_edit_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.data_edit_dialog.take() else {
            return;
        };
        let pending = self.pending_data_edits.contains_key(&dialog.document_id);
        let initial = (
            dialog.range,
            dialog.range_text.clone(),
            dialog.delimiter.clone(),
            dialog.destination_column.clone(),
            dialog.overwrite,
            dialog.sort_keys.clone(),
        );
        let mut submit = false;
        let mut apply = false;
        let mut header_choice = None;
        let is_csv = self
            .workspace
            .documents
            .get(&dialog.document_id)
            .is_some_and(|document| {
                matches!(document.source.as_deref(), Some(SourceDocument::Csv(_)))
            });
        let mut close = false;
        let mut modal_text_focus = None;
        let modal_id = Id::unique(("data_edit_dialog", dialog.document_id));
        let enter_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Enter, self.edit.composing);
        let cancel_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Escape, self.edit.composing);
        let title_key = match dialog.kind {
            DataEditKind::TrimWhitespace => "dialog-data-trim-title",
            DataEditKind::SplitColumn => "dialog-data-split-title",
            DataEditKind::JoinColumns => "dialog-data-join-title",
            DataEditKind::Transpose => "dialog-data-transpose-title",
            DataEditKind::SortRows => "dialog-data-sort-title",
            DataEditKind::DetectDuplicates => "dialog-data-detect-duplicates-title",
            DataEditKind::RemoveDuplicates => "dialog-data-remove-duplicates-title",
        };
        egui::Modal::new(modal_id).show(ctx, |ui| {
            let width = (ctx.content_rect().width() - 40.0).clamp(360.0, 760.0);
            ui.set_min_width(width.min(520.0));
            ui.set_max_width(width);
            ui.set_max_height((ctx.content_rect().height() - 40.0).max(260.0));
            visual::dialog_surface(ui, |ui| {
                visual::prepare_dialog_body(ui);
                visual::dialog_title(ui, &text(self.language, title_key));
                visual::group_gap(ui);
                let body_height =
                    (ui.available_height() - visual::BUTTON_HEIGHT - 3.0 * visual::SPACE_8)
                        .max(120.0);
                egui::ScrollArea::vertical()
                    .max_height(body_height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.label(
                            Message::new("dialog-data-target-range")
                                .with_text(
                                    "range",
                                    crate::xlsx_adapter::cell_reference(
                                        dialog.range.normalized().0,
                                    ),
                                )
                                .resolve(self.language),
                        );
                        if dialog.kind == DataEditKind::TrimWhitespace {
                            ui.label(
                                Message::new("dialog-data-trim-help")
                                    .with_number("count", dialog.selected_ranges.len() as i64)
                                    .resolve(self.language),
                            );
                        } else if dialog.kind == DataEditKind::SortRows {
                            ui.label(text(self.language, "dialog-data-sort-help"));
                            let header_row = self
                                .workspace
                                .documents
                                .get(&dialog.document_id)
                                .and_then(|document| document.header_row(dialog.table_id));
                            // CSV has no header row by default; the user chooses it here.
                            if is_csv {
                                let mut first_row_header = header_row == Some(0);
                                if ui
                                    .checkbox(
                                        &mut first_row_header,
                                        text(self.language, "dialog-sort-first-row-header"),
                                    )
                                    .changed()
                                {
                                    header_choice = Some(first_row_header);
                                }
                            }
                            egui::CollapsingHeader::new(text(self.language, "dialog-sort-details"))
                                .id_salt(("sort_details", dialog.document_id))
                                .default_open(false)
                                .show(ui, |ui| {
                                    ui.label(text(self.language, "dialog-sort-note"));
                                    ui.label(text(self.language, "dialog-sort-moved-formulas"));
                                    ui.label(text(self.language, "dialog-sort-formula-note"));
                                    if let Some(header_row) = header_row {
                                        ui.label(
                                            Message::new("dialog-data-header-kept")
                                                .with_number("row", (header_row + 1) as i64)
                                                .resolve(self.language),
                                        );
                                    } else {
                                        ui.label(text(self.language, "dialog-data-no-header"));
                                    }
                                });
                        } else if matches!(
                            dialog.kind,
                            DataEditKind::DetectDuplicates | DataEditKind::RemoveDuplicates
                        ) {
                            ui.label(text(self.language, "dialog-data-duplicates-help"));
                            if let Some(header_row) = self
                                .workspace
                                .documents
                                .get(&dialog.document_id)
                                .and_then(|document| document.header_row(dialog.table_id))
                            {
                                ui.label(
                                    Message::new("dialog-data-header-excluded")
                                        .with_number("row", (header_row + 1) as i64)
                                        .resolve(self.language),
                                );
                            }
                        } else {
                            let field_width =
                                (ui.available_width() - 144.0 - ui.spacing().item_spacing.x)
                                    .max(80.0);
                            let range = data_edit_text_row(
                                ui,
                                text(self.language, "dialog-data-range-label"),
                                &mut dialog.range_text,
                                field_width,
                            );
                            if range.has_focus() {
                                modal_text_focus = Some(range.id);
                            }
                            if range.changed()
                                && let Some(range) = parse_cell_range_text(&dialog.range_text)
                            {
                                dialog.range = range;
                            }
                        }
                        match dialog.kind {
                            DataEditKind::SplitColumn => {
                                ui.label(text(self.language, "dialog-data-split-help"));
                                let delimiter = data_edit_text_row(
                                    ui,
                                    text(self.language, "dialog-data-delimiter"),
                                    &mut dialog.delimiter,
                                    88.0,
                                );
                                let destination = data_edit_text_row(
                                    ui,
                                    text(self.language, "dialog-data-destination-column"),
                                    &mut dialog.destination_column,
                                    96.0,
                                );
                                modal_text_focus = delimiter
                                    .has_focus()
                                    .then_some(delimiter.id)
                                    .or(modal_text_focus);
                                modal_text_focus = destination
                                    .has_focus()
                                    .then_some(destination.id)
                                    .or(modal_text_focus);
                            }
                            DataEditKind::JoinColumns => {
                                ui.label(text(self.language, "dialog-data-join-help"));
                                let delimiter = data_edit_text_row(
                                    ui,
                                    text(self.language, "dialog-data-delimiter"),
                                    &mut dialog.delimiter,
                                    88.0,
                                );
                                let destination = data_edit_text_row(
                                    ui,
                                    text(self.language, "dialog-data-destination-column"),
                                    &mut dialog.destination_column,
                                    96.0,
                                );
                                modal_text_focus = delimiter
                                    .has_focus()
                                    .then_some(delimiter.id)
                                    .or(modal_text_focus);
                                modal_text_focus = destination
                                    .has_focus()
                                    .then_some(destination.id)
                                    .or(modal_text_focus);
                            }
                            DataEditKind::Transpose => {
                                ui.label(text(self.language, "dialog-data-transpose-help"));
                            }
                            DataEditKind::TrimWhitespace => (),
                            DataEditKind::DetectDuplicates | DataEditKind::RemoveDuplicates => (),
                            DataEditKind::SortRows => {
                                if let Some(id) = sort_keys::draw_sort_keys(
                                    ui,
                                    self.language,
                                    &mut dialog.sort_keys,
                                ) {
                                    modal_text_focus = Some(id);
                                }
                            }
                        }
                        if matches!(
                            dialog.kind,
                            DataEditKind::SplitColumn
                                | DataEditKind::JoinColumns
                                | DataEditKind::Transpose
                        ) {
                            ui.checkbox(
                                &mut dialog.overwrite,
                                text(self.language, "dialog-data-overwrite"),
                            );
                        }
                        if let Some(preview) = dialog.preview.as_ref() {
                            ui.separator();
                            if matches!(
                                dialog.kind,
                                DataEditKind::DetectDuplicates | DataEditKind::RemoveDuplicates
                            ) {
                                if preview.duplicate_count == 0 {
                                    ui.label(text(self.language, "dialog-data-no-duplicates"));
                                } else {
                                    let tail = if dialog.kind == DataEditKind::RemoveDuplicates {
                                        Message::new("dialog-data-remove-tail")
                                    } else {
                                        Message::new("dialog-empty-tail")
                                    };
                                    ui.label(
                                        Message::new("dialog-data-remove-summary")
                                            .with_number("rows", preview.duplicate_count as i64)
                                            .with_number("groups", preview.duplicate_groups as i64)
                                            .with_message("tail", tail)
                                            .resolve(self.language),
                                    );
                                    egui::ScrollArea::vertical()
                                        .max_height(160.0)
                                        .show(ui, |ui| {
                                            for duplicate in preview.duplicate_rows.iter().take(100)
                                            {
                                                let values = duplicate
                                                    .key_values
                                                    .iter()
                                                    .map(|(column, value)| {
                                                        format!(
                                                            "{}={}",
                                                            column_name(*column),
                                                            text_preview(value, 60)
                                                        )
                                                    })
                                                    .collect::<Vec<_>>()
                                                    .join(", ");
                                                ui.label(
                                                    Message::new("dialog-data-duplicate-row")
                                                        .with_number(
                                                            "row",
                                                            (duplicate.row + 1) as i64,
                                                        )
                                                        .with_text("values", values)
                                                        .resolve(self.language),
                                                );
                                            }
                                        });
                                    if preview.duplicate_count > 100 {
                                        ui.label(text(
                                            self.language,
                                            "dialog-data-duplicate-limit",
                                        ));
                                    }
                                }
                            } else {
                                let summary = preview.changed_rows.map_or_else(
                                    || {
                                        Message::new("dialog-data-changes-count")
                                            .with_number("count", preview.changed_cells as i64)
                                    },
                                    |rows| {
                                        Message::new("dialog-data-changes-rows")
                                            .with_number("rows", rows as i64)
                                            .with_number("cells", preview.changed_cells as i64)
                                    },
                                );
                                let tail = if preview.preview_truncated {
                                    Message::new("dialog-data-change-limit")
                                } else {
                                    Message::new("dialog-empty-tail")
                                };
                                ui.label(
                                    Message::new("dialog-data-summary-with-tail")
                                        .with_message("summary", summary)
                                        .with_message("tail", tail)
                                        .resolve(self.language),
                                );
                            }
                            egui::ScrollArea::vertical()
                                .max_height(240.0)
                                .show(ui, |ui| {
                                    for diff in &preview.preview {
                                        let table_name = self
                                            .workspace
                                            .documents
                                            .get(&dialog.document_id)
                                            .and_then(|document| {
                                                document
                                                    .tables
                                                    .iter()
                                                    .find(|table| table.id == diff.table_id)
                                            })
                                            .map_or("?", |table| table.name.as_str());
                                        ui.label(
                                            Message::new("dialog-data-cell-diff")
                                                .with_text("table", table_name)
                                                .with_text("cell", cell_coordinate(diff.address))
                                                .with_text(
                                                    "before",
                                                    text_preview(&diff.before, 100),
                                                )
                                                .with_text("after", text_preview(&diff.after, 100))
                                                .resolve(self.language),
                                        );
                                    }
                                });
                            if preview.preview_truncated {
                                ui.label(text(self.language, "dialog-data-hidden-changes-ready"));
                            }
                        }
                        if let Some(message) = dialog.message.as_ref() {
                            ui.label(message.resolve(self.language));
                        }
                    });
                ui.separator();
                visual::dialog_footer(ui, |ui| {
                    let options_unchanged = dialog.range == initial.0
                        && dialog.range_text == initial.1
                        && dialog.delimiter == initial.2
                        && dialog.destination_column == initial.3
                        && dialog.overwrite == initial.4
                        && dialog.sort_keys == initial.5;
                    let preview_ready = dialog
                        .preview
                        .as_ref()
                        .is_some_and(|preview| preview.prepared.is_some());
                    match draw_data_edit_footer(
                        ui,
                        self.language,
                        pending,
                        options_unchanged,
                        preview_ready,
                    ) {
                        DataEditFooterAction::Apply => apply = true,
                        DataEditFooterAction::Preview => submit = true,
                        DataEditFooterAction::Close => close = true,
                        DataEditFooterAction::None => (),
                    }
                });
            });
        });
        if modal_text_focus.is_some() {
            self.active_modal_text_input_focus = modal_text_focus;
        }
        let current = (
            dialog.range,
            dialog.range_text.clone(),
            dialog.delimiter.clone(),
            dialog.destination_column.clone(),
            dialog.overwrite,
            dialog.sort_keys.clone(),
        );
        if enter_key && !pending {
            if current == initial
                && dialog
                    .preview
                    .as_ref()
                    .is_some_and(|preview| preview.prepared.is_some())
            {
                apply = true;
            } else {
                submit = true;
            }
        }
        // A new header choice changes which rows move, so an old preview no longer applies.
        let mut header_changed = false;
        if let Some(enabled) = header_choice
            && let Some(view) = self
                .workspace
                .documents
                .get_mut(&dialog.document_id)
                .and_then(|document| document.view_mut(dialog.table_id))
        {
            view.header_row = enabled.then_some(0);
            header_changed = true;
        }
        if (current != initial || header_changed) && !submit {
            dialog.preview = None;
            dialog.preview_key = None;
            dialog.message = Some(Message::new("dialog-data-edit-stale"));
            if let Some(pending) = self.pending_data_edits.remove(&dialog.document_id) {
                self.file_worker.cancel(pending.token);
            }
        }
        if submit {
            let state = dialog.submission_snapshot();
            self.data_edit_dialog = Some(dialog);
            self.submit_data_edit(&state);
            return;
        }
        if apply {
            let preview = dialog.preview.take();
            self.data_edit_dialog = Some(dialog);
            if let Some(preview) = preview {
                self.apply_data_edit_prepared(preview);
            }
            return;
        }
        if close || cancel_key {
            if let Some(pending) = self.pending_data_edits.remove(&dialog.document_id) {
                self.file_worker.cancel(pending.token);
            }
            return;
        }
        self.data_edit_dialog = Some(dialog);
    }
}
