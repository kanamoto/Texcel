use super::*;

impl TexcelApp {
    /// Starts the shared save flow for each changed document before app exit.
    pub(super) fn start_app_close_save_batch(&mut self) {
        if !self.app_close_save_tokens.is_empty()
            || !self.pending_saves.is_empty()
            || !self.pending_save_intents.is_empty()
        {
            return;
        }
        self.app_close_save_failed = false;
        self.app_close_save_collecting_targets = false;
        self.pending_app_close_save_queue.clear();
        self.pending_app_close_save_targets.clear();
        self.app_close_save_generation = self.app_close_save_generation.wrapping_add(1);
        if self.app_close_has_unresolved_drafts()
            || self.app_close_has_active_composition()
            || self.app_close_has_pending_operations()
        {
            return;
        }
        let active_edit_document = self.edit_document_id.or(self.workspace.active_document);
        let mut ordered = self
            .workspace
            .order
            .iter()
            .copied()
            .filter(|document_id| self.document_has_unsaved_changes(*document_id))
            .collect::<Vec<_>>();
        ordered.sort_by_key(|document_id| Some(*document_id) != active_edit_document);
        self.pending_app_close_save_queue = ordered.into();
        self.pending_app_close_save_targets.clear();
        self.app_close_save_collecting_targets = true;
        self.app_close_save_batch_active = true;
        self.advance_app_close_save_queue();
    }

    /// Saves known paths directly and opens the shared Save As dialog for new paths.
    pub(in crate::ui) fn advance_app_close_save_queue(&mut self) {
        if !self.app_close_save_batch_active || self.export_dialog.is_some() {
            return;
        }
        let generation = self.app_close_save_generation;
        if self.app_close_save_collecting_targets {
            while let Some(document_id) = self.pending_app_close_save_queue.pop_front() {
                let track_recent = !self.backing_matches_document(document_id);
                if let Some(path) = self
                    .backing_matches_document(document_id)
                    .then(|| {
                        self.file_backings
                            .get(&document_id)
                            .map(|backing| backing.path.clone())
                    })
                    .flatten()
                {
                    if let Some(options) = self.default_save_options(document_id, &path) {
                        self.pending_app_close_save_targets
                            .insert(document_id, (path, options, track_recent));
                    } else {
                        self.app_close_save_failed = true;
                        self.pending_app_close_save_queue.clear();
                        break;
                    }
                } else {
                    self.open_save_as_dialog(
                        document_id,
                        SaveDialogIntent {
                            track_recent,
                            app_close_generation: Some(generation),
                            ..SaveDialogIntent::default()
                        },
                    );
                    if self.export_dialog.is_none() {
                        self.app_close_save_failed = true;
                        self.pending_app_close_save_queue.clear();
                    }
                    return;
                }
            }
            self.app_close_save_collecting_targets = false;
            if self.app_close_save_failed {
                self.finish_app_close_save_batch();
                return;
            }
            if !self.commit_app_close_window_edits() {
                self.status = Some(Message::new("dialog-app-close-wait-operations"));
                self.app_close_save_failed = true;
                self.pending_app_close_save_targets.clear();
                self.finish_app_close_save_batch();
                return;
            }
            let active_edit_document = self.edit_document_id.or(self.workspace.active_document);
            let mut targets = std::mem::take(&mut self.pending_app_close_save_targets)
                .into_iter()
                .collect::<Vec<_>>();
            targets.sort_by_key(|(document_id, _)| Some(*document_id) != active_edit_document);
            for (document_id, (path, options, track_recent)) in targets {
                self.save_as_to_path_with_options(
                    document_id,
                    PendingSaveIntent {
                        path,
                        options,
                        track_recent,
                        close_after_save: false,
                        close_edit: None,
                        app_close_generation: Some(generation),
                    },
                );
                let waiting = self.pending_save_intents.contains_key(&document_id)
                    || self.app_close_save_tokens.contains_key(&document_id)
                    || self.pending_saves.contains_key(&document_id);
                if !waiting {
                    self.app_close_save_failed = true;
                }
            }
        }
        self.finish_app_close_save_batch();
    }
}
