use super::super::*;

impl TexcelApp {
    /// Finds a changed cell buffer in the active or a parked document window.
    pub(in crate::ui) fn app_close_window_has_changed_edit(&self, document_id: DocumentId) -> bool {
        let current_matches =
            self.edit_document_id == Some(document_id) && edit_requires_attention(&self.edit);
        current_matches
            || self.window_states.values().any(|state| {
                state.edit_document_id == Some(document_id) && edit_requires_attention(&state.edit)
            })
    }

    /// Commits every parked editor before Save All collects document revisions.
    pub(super) fn commit_app_close_window_edits(&mut self) -> bool {
        let mut committed = true;
        if edit_requires_attention(&self.edit) {
            self.commit_edit(None);
            committed &= !self.edit.is_editing();
        }
        let viewports = self.window_states.keys().copied().collect::<Vec<_>>();
        for viewport in viewports {
            let Some(mut state) = self.window_states.remove(&viewport) else {
                continue;
            };
            let previous_document = self.workspace.active_document;
            let previous_viewport = self.current_viewport;
            state.swap_with(self);
            self.workspace.active_document = state
                .document_id
                .filter(|id| self.workspace.documents.contains_key(id));
            self.current_viewport = Some(viewport);
            if edit_requires_attention(&self.edit) {
                self.commit_edit(None);
                committed &= !self.edit.is_editing();
            }
            state.swap_with(self);
            self.workspace.active_document = previous_document;
            self.current_viewport = previous_viewport;
            self.window_states.insert(viewport, state);
        }
        committed
    }
}

/// Compares the editor's completed input with the original cell value.
fn edit_buffer_changed(edit: &EditSession) -> bool {
    edit.is_editing()
        && edit
            .completed_input()
            .is_some_and(|input| Some(&input) != edit.original.as_ref())
}

/// Treats an active composition as unfinished input even before its text commits.
fn edit_requires_attention(edit: &EditSession) -> bool {
    edit.is_editing() && (edit.composing || edit_buffer_changed(edit))
}
