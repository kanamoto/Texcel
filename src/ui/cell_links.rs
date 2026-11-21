use super::*;

#[path = "cell_links/start.rs"]
mod start;

impl TexcelApp {
    // Open the selected cell link with the system handler.
    pub(super) fn open_selected_link(&mut self, ctx: &egui::Context) {
        let target = {
            self.workspace
                .active_document()
                .and_then(|document| document.active_table().map(|table| (document, table)))
                .and_then(|(document, table)| {
                    let address = document.view(table.id)?.active_cell();
                    // Links live in the Markdown text of the cell.
                    table.cell(address)?;
                    {
                        let Some(SourceDocument::Markdown(markdown)) = document.source.as_deref()
                        else {
                            return None;
                        };
                        let table_index = document
                            .tables
                            .iter()
                            .position(|candidate| candidate.id == table.id)?;
                        let target = if document.cell_input_is_dirty(table.id, address) {
                            self.markdown_cell_displays
                                .get(&(document.id, table.id, address))
                                .filter(|cached| {
                                    cached.cell_input_revision
                                        == Some(document.cell_input_revision(table.id, address))
                                        && cached.edit_session_id.is_none()
                                })
                                .and_then(|cached| cached.display.first_link_target.as_deref())
                        } else {
                            markdown
                                .cell_display(table_index, address.row, address.column)
                                .and_then(|display| display.first_link_target.as_deref())
                        }?;
                        Some((target.to_owned(), false))
                    }
                })
        };
        let Some((target, location)) = target else {
            self.status = Some(Message::new("status-selected-cell-link-unavailable"));
            return;
        };
        if location {
            self.status = Some(Message::new("status-sheet-link-navigation-unsupported"));
            return;
        }
        if !(target.starts_with("https://")
            || target.starts_with("http://")
            || target.starts_with("mailto:"))
        {
            self.status = Some(Message::new("status-link-type-unavailable"));
            return;
        }
        ctx.open_url(egui::OpenUrl::new_tab(target));
        self.status = None;
    }

    // Draw and handle the active cell link dialog.
    pub(super) fn draw_cell_link_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog_info) = self.cell_link_dialog.as_ref() else {
            return;
        };
        #[derive(Clone, Copy)]
        enum LinkAction {
            Save,
            Remove,
            Cancel,
        }
        let mut action = None;
        let mut modal_text_focus = None;
        let modal_id = Id::unique(egui::IdSalt::new((
            "texcel_cell_link_dialog",
            dialog_info.document_id,
            dialog_info.table_id,
        )));
        let save_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Enter, self.edit.composing);
        let cancel_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Escape, self.edit.composing);
        let Some(dialog) = self.cell_link_dialog.as_mut() else {
            return;
        };
        egui::Modal::new(modal_id).show(ctx, |ui| {
            let width = (ctx.content_rect().width() - 40.0).clamp(300.0, 560.0);
            ui.set_min_width(width.min(380.0));
            ui.set_max_width(width);
            visual::prepare_dialog_body(ui);
            visual::dialog_surface(ui, |ui| {
                visual::dialog_title(ui, &context_text(ui.ctx(), "dialog-link-title"));
                visual::group_gap(ui);
                ui.label(context_text(ui.ctx(), "dialog-link-address"));
                let target_id = Id::unique(egui::IdSalt::new((
                    "cell_link_target",
                    dialog.document_id,
                    dialog.table_id,
                )));
                if dialog.focus_input {
                    ui.memory_mut(|memory| memory.request_focus(target_id));
                }
                let target = ui.add_sized(
                    [ui.available_width(), visual::BUTTON_HEIGHT],
                    egui::TextEdit::singleline(&mut dialog.target).id(target_id),
                );
                if target.has_focus() {
                    modal_text_focus = Some(target.id);
                }
                if dialog.focus_input {
                    target.request_focus();
                    dialog.focus_input = false;
                }
                visual::group_gap(ui);
                ui.label(context_text(ui.ctx(), "dialog-link-tooltip"));
                let tooltip = ui.add_sized(
                    [ui.available_width(), visual::BUTTON_HEIGHT],
                    egui::TextEdit::singleline(&mut dialog.tooltip),
                );
                if tooltip.has_focus() {
                    modal_text_focus = Some(tooltip.id);
                }
                ui.add_space(visual::SPACE_8);
                ui.checkbox(
                    &mut dialog.location,
                    context_text(ui.ctx(), "dialog-link-sheet-location"),
                );
                visual::group_gap(ui);
                ui.separator();
                visual::dialog_footer(ui, |ui| {
                    #[cfg(target_os = "macos")]
                    {
                        if visual::dialog_button(
                            ui,
                            context_text(ui.ctx(), "action-apply"),
                            112.0,
                            true,
                        ) {
                            action = Some(LinkAction::Save);
                        }
                        if visual::dialog_button(
                            ui,
                            context_text(ui.ctx(), "dialog-link-remove"),
                            112.0,
                            false,
                        ) {
                            action = Some(LinkAction::Remove);
                        }
                        if visual::dialog_button(
                            ui,
                            context_text(ui.ctx(), "action-cancel"),
                            112.0,
                            false,
                        ) {
                            action = Some(LinkAction::Cancel);
                        }
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        if visual::dialog_button(
                            ui,
                            context_text(ui.ctx(), "action-cancel"),
                            112.0,
                            false,
                        ) {
                            action = Some(LinkAction::Cancel);
                        }
                        if visual::dialog_button(
                            ui,
                            context_text(ui.ctx(), "dialog-link-remove"),
                            112.0,
                            false,
                        ) {
                            action = Some(LinkAction::Remove);
                        }
                        if visual::dialog_button(
                            ui,
                            context_text(ui.ctx(), "action-apply"),
                            112.0,
                            true,
                        ) {
                            action = Some(LinkAction::Save);
                        }
                    }
                });
            });
        });
        if modal_text_focus.is_some() {
            self.active_modal_text_input_focus = modal_text_focus;
        }
        if save_key {
            action = Some(LinkAction::Save);
        } else if cancel_key {
            action = Some(LinkAction::Cancel);
        }
        let Some(action) = action else {
            return;
        };
        let Some(dialog) = self.cell_link_dialog.take() else {
            return;
        };
        if matches!(action, LinkAction::Cancel) {
            return;
        }
        let hyperlink = if matches!(action, LinkAction::Remove) || dialog.target.trim().is_empty() {
            None
        } else {
            Some(CellHyperlink {
                target: dialog.target.trim().to_owned(),
                tooltip: (!dialog.tooltip.trim().is_empty())
                    .then(|| dialog.tooltip.trim().to_owned()),
                location: dialog.location,
            })
        };
        // Markdown cannot link to a location inside a workbook.
        if hyperlink.as_ref().is_some_and(|link| link.location) {
            self.status = Some(Message::new("status-markdown-link-unrepresentable"));
            return;
        }
        // Links become Markdown link text; a CSV table is promoted to Markdown first.
        self.pending_markdown_link_edits.insert(
            dialog.document_id,
            (dialog.table_id, dialog.range, hyperlink),
        );
        self.request_table_management_for_document(
            dialog.document_id,
            StructureAction::PromoteCsv,
            Some(dialog.table_id),
            None,
        );
        if !self.pending_structures.contains_key(&dialog.document_id) {
            self.pending_markdown_link_edits.remove(&dialog.document_id);
        }
    }
}
