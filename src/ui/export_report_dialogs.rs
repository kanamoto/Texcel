use super::report_rows::{group_location, group_reason, group_summary};
use super::*;

/// The fixed height of the import report's item and reason panes.
const IMPORT_REPORT_PANE_HEIGHT: f32 = 300.0;

impl TexcelApp {
    // Show conversion notes and confirm the prepared export.
    pub(super) fn draw_export_confirmation(&mut self, ctx: &egui::Context) {
        let Some(mut export) = self.active_export_confirmation.take() else {
            return;
        };
        let report = Arc::clone(&export.prepared.report);
        let summary = report.summary();
        let groups = report.groups();
        let stale = self
            .workspace
            .documents
            .get(&export.document_id)
            .is_none_or(|document| document.revision != report.document_revision);
        let mut close = false;
        let mut continue_export = false;
        let mut refresh = false;
        let mut save_report = false;
        let language = self.language;
        let modal_id = Id::unique(egui::IdSalt::new((
            "texcel_export_report",
            export.document_id,
            report.document_revision,
        )));
        let cancel_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Escape, self.edit.composing);
        let continue_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Enter, self.edit.composing);
        egui::Modal::new(modal_id).show(ctx, |ui| {
            let width = (ctx.content_rect().width() - 40.0).clamp(400.0, 840.0);
            ui.set_min_width(width.min(600.0));
            ui.set_max_width(width);
            ui.set_max_height((ctx.content_rect().height() - 40.0).max(300.0));
            visual::dialog_surface(ui, |ui| {
                visual::prepare_dialog_body(ui);
                ui.spacing_mut().scroll.floating = false;
                ui.spacing_mut().scroll.bar_width = 6.0;
                visual::dialog_title(
                    ui,
                    &Message::new("report-dialog-export-title")
                        .with_text("format", &report.output_format)
                        .resolve(language),
                );
                visual::group_gap(ui);
                let body_height =
                    (ui.available_height() - 2.0 * visual::BUTTON_HEIGHT - 3.0 * visual::SPACE_8)
                        .max(140.0);
                egui::ScrollArea::vertical()
                    .max_height(body_height)
                    .auto_shrink([false, true])
                    .scroll_bar_visibility(
                        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                    )
                    .show(ui, |ui| {
                        ui.label(report.settings_summary.resolve(language));
                        ui.label(
                            Message::new("report-summary")
                                .with_number("retained", summary.retained as i64)
                                .with_number("converted", summary.converted as i64)
                                .with_number("omitted", summary.omitted as i64)
                                .with_number("unprocessable", summary.unprocessable as i64)
                                .resolve(language),
                        );
                        for artifact in &export.prepared.artifacts {
                            let name = artifact.path.file_name().map_or_else(
                                || context_text(ui.ctx(), "dialog-file"),
                                |name| name.to_string_lossy().into_owned(),
                            );
                            ui.label(
                                Message::new("report-output-path")
                                    .with_text("path", name)
                                    .resolve(language),
                            )
                            .on_hover_text(artifact.path.display().to_string());
                        }
                        ui.separator();
                        ui.label(context_text(ui.ctx(), "report-export-help"));
                        egui::ScrollArea::vertical()
                            .id_salt(("export_report_groups", export.document_id))
                            .max_height(220.0)
                            .scroll_bar_visibility(
                                egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                            )
                            .show_rows(ui, visual::BUTTON_HEIGHT, groups.len(), |ui, rows| {
                                for index in rows {
                                    let button = egui::Button::selectable(
                                        export.selected_group == index,
                                        group_summary(&groups[index], language, false),
                                    )
                                    .right_text("")
                                    .truncate();
                                    if ui
                                        .add_sized(
                                            [ui.available_width(), visual::BUTTON_HEIGHT],
                                            button,
                                        )
                                        .clicked()
                                    {
                                        export.selected_group = index;
                                    }
                                }
                            });
                        if let Some(group) = groups.get(export.selected_group) {
                            ui.separator();
                            ui.label(group_location(group, language, false));
                            ui.label(group_reason(group, language));
                            if !group.ranges.is_empty() {
                                ui.label(
                                    Message::new("report-range-count")
                                        .with_number("count", group.ranges.len() as i64)
                                        .resolve(language),
                                );
                                egui::ScrollArea::vertical()
                                    .id_salt((
                                        "export_report_ranges",
                                        export.document_id,
                                        export.selected_group,
                                    ))
                                    .max_height(110.0)
                                    .scroll_bar_visibility(
                                        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                                    )
                                    .show_rows(ui, 20.0, group.ranges.len(), |ui, rows| {
                                        for index in rows {
                                            ui.label(&group.ranges[index]);
                                        }
                                    });
                            }
                        }
                        if stale {
                            ui.colored_label(
                                ui.visuals().error_fg_color,
                                context_text(ui.ctx(), "report-export-stale"),
                            );
                        } else if !report.can_export() {
                            ui.colored_label(
                                ui.visuals().error_fg_color,
                                context_text(ui.ctx(), "dialog-export-unprocessed"),
                            );
                        } else if let Some(error) = &export.queue_error {
                            ui.colored_label(
                                ui.visuals().error_fg_color,
                                Message::new("dialog-export-queue-error")
                                    .with_message("detail", error.clone())
                                    .resolve(self.language),
                            );
                        }
                    });
                ui.separator();
                let can_continue =
                    !stale && report.can_export() && !export.prepared.artifacts.is_empty();
                let continue_label = if export.queue_error.is_some() {
                    context_text(ui.ctx(), "action-retry")
                } else {
                    context_text(ui.ctx(), "export-continue-save")
                };
                visual::dialog_footer(ui, |ui| {
                    #[cfg(target_os = "macos")]
                    {
                        if continue_report_button(
                            ui,
                            continue_label.clone(),
                            can_continue,
                            can_continue,
                        ) {
                            continue_export = true;
                        }
                        if stale
                            && ui
                                .button(context_text(ui.ctx(), "report-refresh"))
                                .clicked()
                        {
                            refresh = true;
                        }
                        if report_action_button(
                            ui,
                            context_text(ui.ctx(), "report-save"),
                            !self
                                .pending_export_report_saves
                                .contains_key(&export.document_id),
                            false,
                        ) {
                            save_report = true;
                        }
                        if ui.button(context_text(ui.ctx(), "action-cancel")).clicked() {
                            close = true;
                        }
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        if ui.button(context_text(ui.ctx(), "action-cancel")).clicked() {
                            close = true;
                        }
                        if report_action_button(
                            ui,
                            context_text(ui.ctx(), "report-save"),
                            !self
                                .pending_export_report_saves
                                .contains_key(&export.document_id),
                            false,
                        ) {
                            save_report = true;
                        }
                        if stale
                            && ui
                                .button(context_text(ui.ctx(), "report-refresh"))
                                .clicked()
                        {
                            refresh = true;
                        }
                        if continue_report_button(ui, continue_label, can_continue, can_continue) {
                            continue_export = true;
                        }
                    }
                });
            });
        });
        if cancel_key {
            close = true;
        } else if continue_key
            && !stale
            && report.can_export()
            && !export.prepared.artifacts.is_empty()
        {
            continue_export = true;
        }
        if save_report {
            self.active_export_confirmation = Some(export);
            self.save_active_export_report();
        } else if refresh {
            self.export_configs.insert(
                export.document_id,
                (export.options, export.destination.clone()),
            );
            self.start_export_preflight(export.document_id, export.options, export.destination);
        } else if continue_export {
            self.submit_prepared_export(export);
        } else if close {
            self.export_configs.remove(&export.document_id);
            self.save_as_export_documents.remove(&export.document_id);
            self.finish_save_intent_failed(export.document_id);
        } else {
            self.active_export_confirmation = Some(export);
        }
    }

    // Shows the XLSX or HTML load report in a modal with grouped rows and ranges for the selected row only.
    pub(super) fn draw_import_report(&mut self, ctx: &egui::Context) {
        self.activate_next_import_report();
        let Some((document_id, report)) = self.active_import_report.clone() else {
            return;
        };
        let summary = report.summary();
        let groups = report.groups();
        let mut close = false;
        let mut save = false;
        let language = self.language;
        let modal_id = Id::unique(egui::IdSalt::new(("xlsx_import_report", document_id)));
        let close_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Escape, self.edit.composing);
        let save_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Enter, self.edit.composing);
        egui::Modal::new(modal_id).show(ctx, |ui| {
            let width = (ctx.content_rect().width() - 40.0).clamp(400.0, 840.0);
            ui.set_min_width(width.min(600.0));
            ui.set_max_width(width);
            ui.set_max_height((ctx.content_rect().height() - 40.0).max(300.0));
            visual::dialog_surface(ui, |ui| {
                visual::prepare_dialog_body(ui);
                ui.spacing_mut().scroll.floating = false;
                ui.spacing_mut().scroll.bar_width = 6.0;
                visual::dialog_title(
                    ui,
                    &Message::new("dialog-report-title-import")
                        .with_text("format", &report.input_format)
                        .resolve(language),
                );
                visual::group_gap(ui);
                ui.label(
                    Message::new("report-file-line")
                        .with_text("file", &report.file_name)
                        .resolve(language),
                );
                ui.label(
                    Message::new("report-timestamp-line")
                        .with_text("timestamp", report.formatted_imported_at())
                        .resolve(language),
                );
                ui.label(
                    Message::new("report-summary")
                        .with_number("retained", summary.retained as i64)
                        .with_number("converted", summary.converted as i64)
                        .with_number("omitted", summary.omitted as i64)
                        .with_number("unprocessable", summary.unprocessable as i64)
                        .resolve(language),
                );
                ui.label(context_text(ui.ctx(), "report-import-help"));
                ui.separator();
                // Items on the left, the selected item's reason on the right, at a fixed height.
                let pane_height =
                    (ui.available_height() - visual::BUTTON_HEIGHT - 3.0 * visual::SPACE_8)
                        .clamp(140.0, IMPORT_REPORT_PANE_HEIGHT);
                report_rows::import_report_panes(
                    ui,
                    document_id,
                    groups,
                    &mut self.selected_import_group,
                    pane_height,
                    language,
                );
                ui.separator();
                visual::dialog_footer(ui, |ui| {
                    #[cfg(target_os = "macos")]
                    {
                        if report_action_button(
                            ui,
                            context_text(ui.ctx(), "report-save"),
                            !self.pending_report_saves.contains_key(&document_id),
                            false,
                        ) {
                            save = true;
                        }
                        if ui.button(context_text(ui.ctx(), "action-close")).clicked() {
                            close = true;
                        }
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        if ui.button(context_text(ui.ctx(), "action-close")).clicked() {
                            close = true;
                        }
                        if report_action_button(
                            ui,
                            context_text(ui.ctx(), "report-save"),
                            !self.pending_report_saves.contains_key(&document_id),
                            false,
                        ) {
                            save = true;
                        }
                    }
                });
            });
        });
        if save_key && !self.pending_report_saves.contains_key(&document_id) {
            save = true;
        } else if close_key {
            close = true;
        }
        if save {
            self.save_active_import_report();
        }
        if close {
            self.active_import_report = None;
            self.activate_next_import_report();
        }
    }
}

/// Highlights the ready report action and keeps its width consistent.
fn continue_report_button(ui: &mut egui::Ui, label: String, enabled: bool, primary: bool) -> bool {
    report_action_button(ui, label, enabled, primary)
}

/// Keeps report actions aligned with the common dialog footer.
fn report_action_button(ui: &mut egui::Ui, label: String, enabled: bool, primary: bool) -> bool {
    let button = egui::Button::new(label);
    let button = if primary {
        button.fill(ui.visuals().selection.bg_fill)
    } else {
        button
    };
    ui.add_enabled_ui(enabled, |ui| {
        ui.add_sized([144.0, visual::BUTTON_HEIGHT], button)
    })
    .inner
    .clicked()
}
