use super::*;

impl TexcelApp {
    /// Rebuilds current editor state with a fresh egui session after worker restoration.
    pub(crate) fn install_recovered_document(
        &mut self,
        mut recovered: crate::recovery::RecoveredDocument,
    ) {
        let document_id = recovered.document.id;
        let requires_initial_save = recovered.requires_initial_save || recovered.original_changed;
        let calculation = recovered.calculation;
        let revision = recovered.document.calculation_revision;
        let source = recovered.document.source.as_deref().cloned();
        if !recovered.original_changed
            && let (Some(path), Some(identity)) = (
                recovered.original_path.clone(),
                recovered.original_identity.clone(),
            )
        {
            self.file_backings.insert(
                document_id,
                FileBacking {
                    path,
                    identity,
                    source: source.clone(),
                    overwrite_confirmed: false,
                },
            );
        }
        if requires_initial_save {
            self.recovered_documents_needing_save.insert(document_id);
        }
        self.recovery_keys
            .insert(document_id, recovered.key.clone());
        self.recovery_schedules.insert(
            document_id,
            RecoverySchedule {
                signature: None,
                generation: recovered.generation,
                changed_at: Instant::now(),
                interval_started: Instant::now(),
                last_success: None,
                in_flight: None,
            },
        );
        let recovered_cell_edit = recovered.cell_edit.take().filter(|edit| {
            recovered
                .document
                .tables
                .iter()
                .any(|table| table.id == edit.table_id)
        });
        let has_cell_edit = recovered_cell_edit.is_some();
        if self.edit.is_editing()
            && let Some(active_id) = self.edit_document_id
            && let Some(previous) = self.recovery_cell_edit(active_id)
        {
            self.pending_recovered_cell_edits
                .insert(active_id, previous);
        }
        self.edit.reset();
        self.edit_document_id = None;
        self.workspace.insert_loaded_document(recovered.document);
        if let Some(report) = recovered.import_report.take() {
            let source_path = recovered.original_path.clone().unwrap_or_default();
            let mut state = crate::conversion_report::ImportReportState::new(report, source_path);
            if let Some(automatic) = state.take_automatic_dialog() {
                self.queued_import_reports
                    .push_back((document_id, automatic));
            }
            self.import_reports.insert(document_id, state);
            self.activate_next_import_report();
        }
        if let Some(edit) = recovered_cell_edit {
            self.pending_recovered_cell_edits.insert(document_id, edit);
        }
        let token = self.next_job_token(document_id, revision);
        self.initialize_calculation(token, calculation);
        if self.workspace.active_document == Some(document_id) {
            self.restore_recovered_cell_edit(document_id);
        }
        self.grid_focused = false;
        self.request_grid_focus = !has_cell_edit;
        self.status = Some(Message::new(if recovered.original_changed {
            "status-recovery-restored-separately"
        } else {
            "status-recovery-restored"
        }));
    }

    /// Restores one document's saved cell buffer after its tab becomes active.
    pub(crate) fn restore_recovered_cell_edit(&mut self, document_id: DocumentId) {
        if self.edit_document_id == Some(document_id) || self.edit.is_editing() {
            return;
        }
        let Some(edit) = self.pending_recovered_cell_edits.remove(&document_id) else {
            return;
        };
        let Some(document) = self.workspace.documents.get_mut(&document_id) else {
            return;
        };
        if !document
            .tables
            .iter()
            .any(|table| table.id == edit.table_id)
        {
            return;
        }
        let address = CellAddress {
            row: edit.address.row.min(crate::document::MAX_ROWS - 1),
            column: edit.address.column.min(crate::document::MAX_COLUMNS - 1),
        };
        document.active_table = edit.table_id;
        if let Some(view) = document.views.get_mut(&edit.table_id) {
            view.set_selection(CellRange::single(address));
        }
        self.edit.begin(
            edit.table_id,
            address,
            edit.original,
            crate::document::EditTarget::Cell,
        );
        self.edit.buffer = edit.buffer;
        self.edit.mark_buffer_changed();
        self.edit.cursor_byte = clamp_utf8_offset(&self.edit.buffer, edit.cursor_byte);
        self.edit.anchor_byte = clamp_utf8_offset(&self.edit.buffer, edit.anchor_byte);
        self.edit.preserve_recovered_anchor = true;
        self.edit_document_id = Some(document_id);
        self.request_editor_focus = true;
        self.request_scroll_to_address = Some((document_id, edit.table_id, address));
        self.request_grid_focus = false;
    }

    /// Hashes only document versions and active draft buffers, never the whole document.
    pub(super) fn recovery_signature(
        &self,
        document_id: DocumentId,
        revision: u64,
        draft_revision: Option<(u64, bool)>,
    ) -> u64 {
        let mut hasher = DefaultHasher::new();
        document_id.hash(&mut hasher);
        revision.hash(&mut hasher);
        draft_revision.hash(&mut hasher);
        if let Some(edit) = self.recovery_cell_edit(document_id) {
            edit.table_id.hash(&mut hasher);
            edit.address.row.hash(&mut hasher);
            edit.address.column.hash(&mut hasher);
            edit.buffer.hash(&mut hasher);
            edit.cursor_byte.hash(&mut hasher);
            edit.anchor_byte.hash(&mut hasher);
        }
        if let Some(edit) = self.pending_recovered_cell_edits.get(&document_id) {
            edit.table_id.hash(&mut hasher);
            edit.address.row.hash(&mut hasher);
            edit.address.column.hash(&mut hasher);
            edit.buffer.hash(&mut hasher);
            edit.cursor_byte.hash(&mut hasher);
            edit.anchor_byte.hash(&mut hasher);
        }
        if let Some(pending) = self.pending_text_growth_sessions.get(&document_id) {
            pending.session.table_id.hash(&mut hasher);
            pending.session.address.hash(&mut hasher);
            pending.session.buffer.hash(&mut hasher);
            pending.session.cursor_byte.hash(&mut hasher);
            pending.session.anchor_byte.hash(&mut hasher);
        }
        hasher.finish()
    }

    /// Gives each live document a private recovery file name for this app session.
    pub(super) fn recovery_key(&mut self, document_id: DocumentId) -> String {
        if let Some(key) = self.recovery_keys.get(&document_id) {
            return key.clone();
        }
        let key = self.fresh_recovery_key(document_id);
        self.recovery_keys.insert(document_id, key.clone());
        key
    }

    /// Builds a short stable key without exposing a source path in the recovery directory.
    fn fresh_recovery_key(&self, document_id: DocumentId) -> String {
        let mut hasher = DefaultHasher::new();
        self.recovery_session_id.hash(&mut hasher);
        document_id.hash(&mut hasher);
        if let Some(backing) = self.file_backings.get(&document_id) {
            normalize_settings_path(&backing.path).hash(&mut hasher);
        }
        format!("{:016x}", hasher.finish())
    }

    /// Copies the target window's uncommitted cell text into the recovery request.
    pub(super) fn recovery_cell_edit(
        &self,
        document_id: DocumentId,
    ) -> Option<crate::recovery::RecoveryCellEdit> {
        if self.edit_document_id.or(self.workspace.active_document) == Some(document_id)
            && let Some(edit) = recovery_edit_from_session(&self.edit)
        {
            return Some(edit);
        }
        self.window_states.values().find_map(|state| {
            (state.edit_document_id == Some(document_id))
                .then(|| recovery_edit_from_session(&state.edit))
                .flatten()
        })
    }
}

/// Copies one live cell buffer without borrowing its egui widget state.
fn recovery_edit_from_session(edit: &EditSession) -> Option<crate::recovery::RecoveryCellEdit> {
    Some(crate::recovery::RecoveryCellEdit {
        table_id: edit.table_id?,
        address: edit.address?,
        original: edit.original.clone()?,
        buffer: edit.buffer.clone(),
        cursor_byte: edit.cursor_byte,
        anchor_byte: edit.anchor_byte,
    })
}

/// Keeps recovered selection offsets on UTF-8 character boundaries.
fn clamp_utf8_offset(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset = offset.saturating_sub(1);
    }
    offset
}

impl TexcelApp {
    // Sends queued recovery writes unless the app is discarding its documents.
    pub(super) fn submit_pending_recovery_flushes(&mut self) {
        if self.app_close_discard_pending || self.app_close_discard_exit_ready {
            return;
        }
        let Some(worker) = self.recovery_worker.as_ref() else {
            return;
        };
        let document_ids = self
            .pending_recovery_flushes
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for document_id in document_ids {
            if self.recovery_key_has_pending_removal(document_id) {
                continue;
            }
            let Some((snapshot, saved_revision, saved_generation)) =
                self.pending_recovery_flushes.remove(&document_id)
            else {
                continue;
            };
            let revision = snapshot.revision;
            let generation = snapshot.generation;
            match worker.save_and_cleanup(snapshot, saved_revision, saved_generation) {
                Ok(()) => {
                    if let Some(schedule) = self.recovery_schedules.get_mut(&document_id) {
                        schedule.in_flight = Some((revision, generation));
                    }
                }
                Err(snapshot) => {
                    self.pending_recovery_flushes
                        .insert(document_id, (*snapshot, saved_revision, saved_generation));
                    break;
                }
            }
        }
    }
}
