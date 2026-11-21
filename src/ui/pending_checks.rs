use super::*;

impl TexcelApp {
    /// Checks whether a structure operation is still pending.
    pub(super) fn active_structure_pending(&self) -> bool {
        self.workspace
            .active_document()
            .is_some_and(|document| self.pending_structures.contains_key(&document.id))
    }

    /// Tells whether cell editing is paused while a row, column, undo, or export job runs.
    pub(super) fn cell_edit_locked(&self, document_id: DocumentId) -> bool {
        self.document_bulk_edit_pending(document_id)
    }

    /// Locks content edits while the target document has a pending result-based operation.
    pub(super) fn document_bulk_edit_pending(&self, document_id: DocumentId) -> bool {
        self.pending_structures.contains_key(&document_id)
            || self.pending_history_restores.contains_key(&document_id)
            || self.pending_exports.contains_key(&document_id)
            || self.pending_export_writes.contains_key(&document_id)
    }

    /// Checks whether undo is restoring a structure snapshot.
    pub(super) fn active_history_restore_pending(&self) -> bool {
        self.workspace
            .active_document()
            .is_some_and(|document| self.pending_history_restores.contains_key(&document.id))
    }
}
