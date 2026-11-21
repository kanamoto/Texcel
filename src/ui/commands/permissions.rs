use super::*;

impl TexcelApp {
    /// Finds the input owner before a menu click changes widget focus.
    pub(in crate::ui) fn command_input_owner(
        &self,
        ctx: &egui::Context,
        blocked: bool,
    ) -> (InputOwner, Option<Id>) {
        let actual_focus = ctx
            .text_edit_focused()
            .then(|| ctx.memory(|memory| memory.focused()))
            .flatten();
        let focused = self
            .last_text_input_focus
            .filter(|remembered| Some(*remembered) == actual_focus);
        if self.edit.composing {
            return (InputOwner::Ime, focused);
        }
        #[cfg(any(test, not(any(target_os = "macos", target_os = "windows"))))]
        if self.preserve_text_input_focus
            && egui::Popup::is_any_open(ctx)
            && ctx.memory(|memory| memory.focused().is_none())
            && !context_has_ime_event(ctx)
            && let Some(target) = self.shared_edit_menu_target
            && self.shared_edit_menu_target_is_current(target)
        {
            let owner = match target {
                SharedEditMenuTarget::CellEditor { .. } => InputOwner::CellEditor,
                SharedEditMenuTarget::NameBox { .. } => InputOwner::OtherText,
            };
            return (owner, Some(target.focus()));
        }
        if blocked
            && focused.is_none()
            && let Some(modal_focus) = self.modal_menu_text_focus
            && self.last_modal_text_input_focus == Some(modal_focus)
            && self.pending_csv_opens.iter().any(|(document_id, pending)| {
                pending.prompt.is_some()
                    && modal_focus
                        == Id::unique(egui::IdSalt::new(("csv_custom_delimiter", document_id)))
            })
        {
            return (InputOwner::OtherText, Some(modal_focus));
        }
        if let Some(focused) = focused {
            let own_editor = self
                .edit_document_id
                .is_some_and(|document_id| focused == editor_id(document_id, self.edit.session_id));
            let detail_modal_editor = blocked
                && own_editor
                && self.last_modal_text_input_focus == Some(focused)
                && self
                    .modal_text_session()
                    .is_some_and(|(kind, document_id, session)| {
                        kind == 9
                            && self.edit_document_id == Some(document_id)
                            && self.edit.session_id == session
                    });
            if blocked
                && !own_editor
                && (self.last_modal_text_input_focus != Some(focused)
                    || self.modal_text_session().is_none())
            {
                return (InputOwner::Modal, None);
            }
            return (
                if own_editor && !detail_modal_editor {
                    InputOwner::CellEditor
                } else {
                    InputOwner::OtherText
                },
                Some(focused),
            );
        }
        if blocked {
            return (InputOwner::Modal, focused);
        }
        if self.grid_focused {
            (InputOwner::Grid, None)
        } else {
            (InputOwner::Chrome, focused)
        }
    }

    /// Checks whether a document command may act for its current input owner.
    pub(in crate::ui) fn can_execute_command(
        &self,
        command: &AppCommand,
        owner: InputOwner,
        blocked: bool,
    ) -> bool {
        if blocked {
            return owner == InputOwner::OtherText && is_standard_text_command(command);
        }
        if matches!(owner, InputOwner::CellEditor | InputOwner::OtherText) {
            return matches!(
                command,
                AppCommand::Undo
                    | AppCommand::Redo
                    | AppCommand::CopyAs(CopyFormat::DisplayTsv)
                    | AppCommand::CutAs(CopyFormat::DisplayTsv)
                    | AppCommand::PasteText { .. }
                    | AppCommand::PasteSystem(_)
                    | AppCommand::ClearSelection
                    | AppCommand::SelectAll
            );
        }
        if !matches!(owner, InputOwner::Grid | InputOwner::Chrome) {
            return false;
        }
        if self.edit.is_editing() {
            return false;
        }
        let Some(document) = self.workspace.active_document() else {
            return false;
        };
        if self.pending_saves.contains_key(&document.id)
            && !matches!(
                command,
                AppCommand::CopyAs(_)
                    | AppCommand::CutAs(_)
                    | AppCommand::OpenCopyOptions
                    | AppCommand::ClearSelection
                    | AppCommand::SelectAll
            )
        {
            return false;
        }
        if self.document_bulk_edit_pending(document.id)
            && !matches!(command, AppCommand::CopyAs(_) | AppCommand::OpenCopyOptions)
        {
            return false;
        }
        match command {
            AppCommand::Undo => document.can_undo(),
            AppCommand::Redo => document.can_redo(),
            AppCommand::CopyAs(_)
            | AppCommand::CutAs(_)
            | AppCommand::PasteText { .. }
            | AppCommand::PasteSystem(_)
            | AppCommand::OpenPasteSpecial
            | AppCommand::OpenCopyOptions
            | AppCommand::ClearSelection
            | AppCommand::SelectAll => document.active_table().is_some(),
        }
    }

    /// Keeps cell-menu items aligned with the shared command dispatcher.
    pub(in crate::ui) fn grid_context_command_availability(&self, blocked: bool) -> [bool; 6] {
        [
            AppCommand::CutAs(CopyFormat::DisplayTsv),
            AppCommand::CopyAs(CopyFormat::DisplayTsv),
            AppCommand::PasteSystem(PasteOptions::default()),
            AppCommand::ClearSelection,
            AppCommand::SelectAll,
            AppCommand::OpenPasteSpecial,
        ]
        .map(|command| self.can_execute_command(&command, InputOwner::Grid, blocked))
    }
}
