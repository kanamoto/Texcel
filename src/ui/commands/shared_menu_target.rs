use super::*;

impl TexcelApp {
    /// Checks whether a delayed text command still belongs to its live widget.
    pub(in crate::ui) fn deferred_text_target_is_live(
        &self,
        target: DeferredTextTarget,
        ctx: &egui::Context,
    ) -> bool {
        if context_has_ime_event(ctx) {
            return false;
        }
        let focused = ctx.memory(|memory| memory.focused());
        let focus_is_live = ctx.text_edit_focused() && focused == Some(target.focus);
        #[cfg(any(test, not(any(target_os = "macos", target_os = "windows"))))]
        let restored_menu_target = focused.is_none()
            && self.preserve_text_input_focus
            && self.shared_edit_menu_target_matches(target);
        #[cfg(not(any(test, not(any(target_os = "macos", target_os = "windows")))))]
        let restored_menu_target = false;
        if !focus_is_live && !restored_menu_target {
            return false;
        }

        let document = self.workspace.active_document();
        if document.map(|document| document.id) != target.document_id
            || document.and_then(|document| document.active_table().map(|table| table.id))
                != target.table_id
        {
            return false;
        }
        if let Some((document_id, session_id, address)) = target.edit_session {
            return self.edit.is_editing()
                && self.edit_document_id == Some(document_id)
                && self.edit.session_id == session_id
                && self.edit.address == address
                && target.document_id == Some(document_id)
                && target.modal_session == self.modal_text_session();
        }
        if target.modal {
            self.last_modal_text_input_focus == Some(target.focus)
                && target.modal_session.is_some()
                && target.modal_session == self.modal_text_session()
        } else {
            self.last_text_input_focus == Some(target.focus)
                && target.modal_session == self.modal_text_session()
        }
    }

    /// Confirms the captured editor still names the active document and session.
    #[cfg(any(test, not(any(target_os = "macos", target_os = "windows"))))]
    pub(in crate::ui) fn shared_edit_menu_target_is_current(
        &self,
        target: SharedEditMenuTarget,
    ) -> bool {
        match target {
            SharedEditMenuTarget::CellEditor {
                document_id,
                table_id,
                session_id,
                address,
                focus,
                ..
            } => {
                self.active_menu_document_table_matches(document_id, table_id)
                    && self.edit.is_editing()
                    && !self.edit.composing
                    && self.edit_document_id == Some(document_id)
                    && self.edit.table_id == Some(table_id)
                    && self.edit.session_id == session_id
                    && self.edit.address == address
                    && focus == editor_id(document_id, session_id)
                    && self.modal_ime_composition.is_none()
            }
            SharedEditMenuTarget::NameBox {
                document_id,
                table_id,
                focus,
                ..
            } => {
                self.active_menu_document_table_matches(document_id, table_id)
                    && self.modal_text_session().is_none()
                    && self.modal_ime_composition.is_none()
                    && !self.edit.composing
                    && focus == name_box_id(document_id)
            }
        }
    }

    /// Matches a deferred text command only to the editor that opened Edit.
    #[cfg(any(test, not(any(target_os = "macos", target_os = "windows"))))]
    pub(in crate::ui) fn shared_edit_menu_target_matches(
        &self,
        deferred: DeferredTextTarget,
    ) -> bool {
        let Some(target) = self.shared_edit_menu_target else {
            return false;
        };
        let belongs_to_target = match target {
            SharedEditMenuTarget::CellEditor {
                document_id,
                table_id,
                session_id,
                address,
                focus,
                ..
            } => {
                deferred.focus == focus
                    && deferred.document_id == Some(document_id)
                    && deferred.table_id == Some(table_id)
                    && deferred.edit_session == Some((document_id, session_id, address))
                    && !deferred.modal
                    && deferred.modal_session == self.modal_text_session()
            }
            SharedEditMenuTarget::NameBox {
                document_id,
                table_id,
                focus,
                ..
            } => {
                deferred.focus == focus
                    && deferred.document_id == Some(document_id)
                    && deferred.table_id == Some(table_id)
                    && deferred.edit_session.is_none()
                    && !deferred.modal
                    && deferred.modal_session.is_none()
            }
        };
        belongs_to_target && self.shared_edit_menu_target_is_current(target)
    }

    /// Keeps restored text commands tied to the document and table at capture time.
    #[cfg(any(test, not(any(target_os = "macos", target_os = "windows"))))]
    fn active_menu_document_table_matches(
        &self,
        document_id: DocumentId,
        table_id: TableId,
    ) -> bool {
        self.workspace.active_document == Some(document_id)
            && self
                .workspace
                .active_document()
                .and_then(|document| document.active_table().map(|table| table.id))
                == Some(table_id)
    }
}
