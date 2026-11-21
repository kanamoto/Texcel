use super::*;

/// Remembers which document hit a save conflict and whether it was closing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SaveConflictPrompt {
    pub(super) document_id: DocumentId,
    pub(super) close_after_save: bool,
}

/// The choices offered when the file changed on disk before saving.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SaveConflictChoice {
    Overwrite,
    SaveAs,
    Cancel,
}

impl TexcelApp {
    /// Stops a save whose file changed on disk and asks the user what to do.
    pub(super) fn handle_save_conflict(
        &mut self,
        document_id: DocumentId,
        token: JobToken,
    ) -> bool {
        if self.pending_saves.get(&document_id) != Some(&token) {
            return false;
        }
        // Remember a close request first, because the failure path clears it.
        let close_after_save = self.close_after_save.contains(&document_id);
        self.handle_failure_result(
            document_id,
            token,
            FileOperation::Save,
            FileResultKind::Failed(Message::new("status-save-conflict")),
        );
        if close_after_save && self.pending_close_document == Some(document_id) {
            self.pending_close_document = None;
        }
        self.save_conflict_dialog = Some(SaveConflictPrompt {
            document_id,
            close_after_save,
        });
        false
    }

    /// Shows the save-conflict choices: overwrite, save as, or cancel.
    pub(super) fn draw_save_conflict_dialog(&mut self, ctx: &egui::Context) {
        let Some(prompt) = self.save_conflict_dialog else {
            return;
        };
        let document_id = prompt.document_id;
        let Some(name) = self
            .workspace
            .documents
            .get(&document_id)
            .map(|document| document.name.clone())
        else {
            self.save_conflict_dialog = None;
            return;
        };
        let modal_id = Id::unique(("save_conflict", document_id));
        let enter_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Enter, self.edit.composing);
        let escape_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Escape, self.edit.composing);
        let mut choice = None;
        egui::Modal::new(modal_id).show(ctx, |ui| {
            let width = (ctx.content_rect().width() - 40.0).clamp(320.0, 620.0);
            ui.set_min_width(width.min(480.0));
            ui.set_max_width(width);
            visual::dialog_surface(ui, |ui| {
                visual::prepare_dialog_body(ui);
                visual::dialog_title(ui, &text(self.language, "dialog-save-conflict-title"));
                visual::group_gap(ui);
                ui.label(
                    Message::new("dialog-save-conflict-help")
                        .with_text("name", &name)
                        .resolve(self.language),
                );
                ui.separator();
                // The footer lays out right to left; each OS keeps its usual button order.
                visual::dialog_footer(ui, |ui| {
                    let buttons = [
                        (SaveConflictChoice::Overwrite, "action-overwrite", true),
                        (SaveConflictChoice::SaveAs, "action-save-as", false),
                        (SaveConflictChoice::Cancel, "action-cancel", false),
                    ];
                    let ordered: Vec<_> = if cfg!(target_os = "macos") {
                        buttons.to_vec()
                    } else {
                        buttons.iter().rev().copied().collect()
                    };
                    for (value, key, primary) in ordered {
                        if visual::dialog_button(ui, text(self.language, key), 144.0, primary) {
                            choice = Some(value);
                        }
                    }
                });
            });
        });
        if escape_key {
            choice = Some(SaveConflictChoice::Cancel);
        } else if enter_key && choice.is_none() {
            choice = Some(SaveConflictChoice::Overwrite);
        }
        let Some(choice) = choice else {
            return;
        };
        self.save_conflict_dialog = None;
        if choice == SaveConflictChoice::Cancel {
            return;
        }
        // Save the document the dialog belongs to, even if another one became active.
        self.workspace.select_document(document_id);
        self.restore_recovered_cell_edit(document_id);
        if choice == SaveConflictChoice::Overwrite {
            if let Some(backing) = self.file_backings.get_mut(&document_id) {
                backing.overwrite_confirmed = true;
            }
            if prompt.close_after_save {
                self.save_and_close_document(document_id);
            } else {
                self.save_active(false);
            }
        } else {
            self.save_active(true);
        }
    }
}
