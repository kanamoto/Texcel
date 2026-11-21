use super::*;

impl TexcelApp {
    // Save a document and close it only after the save succeeds.
    pub(super) fn save_and_close_document(&mut self, document_id: DocumentId) {
        let edit_before_save = (self.edit.is_editing()
            && self.edit_document_id == Some(document_id))
        .then(|| self.edit.clone());
        let track_recent = !self.backing_matches_document(document_id);
        if track_recent {
            self.pending_close_document = None;
            self.open_save_as_dialog(
                document_id,
                SaveDialogIntent {
                    track_recent: true,
                    allow_formula_results: false,
                    close_after_save: true,
                    close_edit: edit_before_save,
                    app_close_generation: None,
                },
            );
        } else if let Some(path) = self
            .file_backings
            .get(&document_id)
            .map(|backing| backing.path.clone())
        {
            self.pending_close_document = None;
            self.save_as_to_path_with_intent(
                document_id,
                path,
                false,
                true,
                edit_before_save,
                None,
            );
        }
    }

    // Restore an uncommitted edit after a close or save failure.
    pub(super) fn restore_close_edit(&mut self, document_id: DocumentId, edit: EditSession) {
        if self.workspace.documents.contains_key(&document_id)
            && self.workspace.active_document == Some(document_id)
            && !self.edit.is_editing()
        {
            self.edit = edit;
            self.edit_document_id = Some(document_id);
            self.request_editor_focus = true;
        }
    }
}
