use super::*;

#[path = "recovery_ui/helpers.rs"]
mod helpers;

#[cfg(test)]
#[path = "recovery_ui_test.rs"]
mod tests;

impl TexcelApp {
    /// Reads recovery events, installs restored documents, and schedules new snapshots.
    pub(super) fn process_recovery(&mut self, ctx: &egui::Context) {
        let mut events = Vec::new();
        if let Some(worker) = self.recovery_worker.as_ref() {
            loop {
                match worker.try_recv() {
                    Ok(event) => events.push(event),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                }
            }
        }
        for event in events {
            match event {
                crate::recovery::RecoveryEvent::Loaded(Ok((items, skipped))) => {
                    self.recovery_items.extend(items);
                    if skipped > 0 {
                        self.status = Some(
                            Message::new("status-recovery-skipped")
                                .with_number("count", skipped as i64),
                        );
                    }
                }
                crate::recovery::RecoveryEvent::Loaded(Err(detail)) => {
                    self.status = Some(
                        Message::new("status-recovery-load-failed").with_message("detail", detail),
                    );
                }
                crate::recovery::RecoveryEvent::Saved {
                    key,
                    revision,
                    generation,
                    result,
                } => {
                    let document_id = self
                        .recovery_keys
                        .iter()
                        .find_map(|(id, saved_key)| (saved_key == &key).then_some(*id));
                    let matches_in_flight = document_id
                        .and_then(|document_id| self.recovery_schedules.get(&document_id))
                        .is_some_and(|schedule| {
                            schedule
                                .in_flight
                                .is_some_and(|(sent_revision, sent_generation)| {
                                    sent_revision == revision && sent_generation == generation
                                })
                        });
                    if matches_in_flight
                        && let Some(document_id) = document_id
                        && let Some(schedule) = self.recovery_schedules.get_mut(&document_id)
                    {
                        schedule.in_flight = None;
                        match result {
                            Ok(()) => {
                                let now = Instant::now();
                                if schedule.generation == generation {
                                    schedule.last_success = Some(now);
                                    schedule.interval_started = now;
                                }
                            }
                            Err(detail) => {
                                schedule.changed_at = Instant::now();
                                self.status = Some(
                                    Message::new("status-recovery-save-failed")
                                        .with_message("detail", detail),
                                );
                            }
                        }
                    }
                }
                crate::recovery::RecoveryEvent::Removed { key, result } => {
                    self.handle_recovery_maintenance_result(key, result);
                }
                crate::recovery::RecoveryEvent::Restored { key, result } => {
                    self.recovery_restore_pending.remove(&key);
                    match result {
                        Ok(recovered) => self.install_recovered_document(*recovered),
                        // The record stays on disk, so the next start tries it again.
                        Err(message) => {
                            self.status = Some(
                                Message::new("status-recovery-restore-failed")
                                    .with_message("detail", message),
                            )
                        }
                    }
                }
            }
        }
        self.restore_recovery_items();
        self.schedule_recovery();
        self.submit_pending_recovery_flushes();
        self.submit_pending_recovery_maintenance();
        self.finish_recovery_maintenance(ctx);
    }

    /// Saves a source root and sparse changes after a short idle period or a long edit interval.
    pub(super) fn schedule_recovery(&mut self) {
        if self.app_close_discard_pending || self.app_close_discard_exit_ready {
            return;
        }
        let Some(_) = &self.recovery_worker else {
            return;
        };
        let now = Instant::now();
        let document_ids = self.workspace.order.clone();
        for document_id in document_ids {
            if self.recovery_key_has_pending_removal(document_id) {
                continue;
            }
            let Some(document) = self.workspace.documents.get(&document_id) else {
                continue;
            };
            let saved_revision = document.saved_revision;
            let active_edit_changed = self.app_close_window_has_changed_edit(document_id);
            let edit_active = (active_edit_changed
                && self.edit_document_id.or(self.workspace.active_document) == Some(document_id))
                || self.pending_recovered_cell_edits.contains_key(&document_id)
                || self.pending_text_growth_sessions.contains_key(&document_id);
            let draft_revision: Option<(u64, bool)> = None;
            let draft_changed = draft_revision.is_some_and(|(_, changed)| changed);
            if !self.document_has_unsaved_changes(document_id) && !edit_active && !draft_changed {
                let pending_generation = self
                    .pending_recovery_flushes
                    .remove(&document_id)
                    .map(|(snapshot, _, _)| snapshot.generation);
                let (had_recovery, generation) = self
                    .recovery_schedules
                    .get_mut(&document_id)
                    .map_or((false, 0), |schedule| {
                        let had_recovery =
                            schedule.signature.take().is_some() || schedule.in_flight.is_some();
                        schedule.last_success = None;
                        (had_recovery, schedule.generation)
                    });
                if (had_recovery || pending_generation.is_some())
                    && let Some(key) = self.recovery_keys.get(&document_id).cloned()
                {
                    self.queue_recovery_cleanup(
                        key,
                        saved_revision,
                        pending_generation.unwrap_or(generation).max(generation),
                    );
                }
                continue;
            }
            let signature = self.recovery_signature(document_id, document.revision, draft_revision);
            let has_pending_flush = self.pending_recovery_flushes.contains_key(&document_id);
            let generation = {
                let schedule =
                    self.recovery_schedules
                        .entry(document_id)
                        .or_insert(RecoverySchedule {
                            signature: None,
                            generation: 0,
                            changed_at: now,
                            interval_started: now,
                            last_success: None,
                            in_flight: None,
                        });
                if schedule.signature != Some(signature) {
                    schedule.signature = Some(signature);
                    schedule.generation = schedule.generation.saturating_add(1);
                    schedule.changed_at = now;
                    schedule.last_success = None;
                }
                let periodic_due = now >= schedule.interval_started + Duration::from_secs(30);
                if schedule.in_flight.is_some()
                    || (!has_pending_flush
                        && !periodic_due
                        && (schedule.last_success.is_some()
                            || now < schedule.changed_at + Duration::from_secs(2)))
                {
                    continue;
                }
                schedule.generation
            };
            let pending_needs_refresh = self
                .pending_recovery_flushes
                .get(&document_id)
                .is_some_and(|(snapshot, _, _)| snapshot.generation != generation);
            if has_pending_flush && !pending_needs_refresh {
                continue;
            }
            let Some(snapshot) = self.recovery_snapshot(document_id, generation) else {
                continue;
            };
            let revision = snapshot.revision;
            let generation = snapshot.generation;
            if has_pending_flush {
                let cutoff = self
                    .pending_recovery_flushes
                    .get(&document_id)
                    .map(|(_, revision, generation)| (*revision, *generation));
                if let Some((saved_revision, saved_generation)) = cutoff {
                    self.pending_recovery_flushes
                        .insert(document_id, (snapshot, saved_revision, saved_generation));
                }
                continue;
            }
            match self
                .recovery_worker
                .as_ref()
                .expect("the recovery worker was checked before scheduling")
                .save(snapshot)
            {
                Ok(()) => {
                    if let Some(schedule) = self.recovery_schedules.get_mut(&document_id) {
                        schedule.in_flight = Some((revision, generation));
                    }
                }
                Err(snapshot) => {
                    self.pending_recovery_flushes.insert(
                        document_id,
                        (*snapshot, saved_revision, generation.saturating_sub(1)),
                    );
                }
            }
        }
    }

    /// Captures the current recovery signature when a document save starts.
    pub(super) fn recovery_generation_for_save(&mut self, document_id: DocumentId) -> u64 {
        self.recovery_key(document_id);
        let Some(document) = self.workspace.documents.get(&document_id) else {
            return 0;
        };
        let draft_revision: Option<(u64, bool)> = None;
        let signature = self.recovery_signature(document_id, document.revision, draft_revision);
        let now = Instant::now();
        let schedule = self
            .recovery_schedules
            .entry(document_id)
            .or_insert(RecoverySchedule {
                signature: None,
                generation: 0,
                changed_at: now,
                interval_started: now,
                last_success: None,
                in_flight: None,
            });
        if schedule.signature != Some(signature) {
            schedule.signature = Some(signature);
            schedule.generation = schedule.generation.saturating_add(1);
            schedule.changed_at = now;
        }
        schedule.generation
    }

    /// Shares immutable source data and sparse edits with the recovery worker.
    pub(super) fn recovery_snapshot(
        &mut self,
        document_id: DocumentId,
        generation: u64,
    ) -> Option<crate::recovery::RecoverySnapshot> {
        let key = self.recovery_key(document_id);
        let requires_initial_save = self.recovered_documents_needing_save.contains(&document_id);
        let document = self.workspace.documents.get(&document_id)?;
        let active_edit = self.recovery_cell_edit(document_id);
        let edit = self
            .pending_recovered_cell_edits
            .get(&document_id)
            .cloned()
            .or(active_edit)
            .or_else(|| {
                let session = &self.pending_text_growth_sessions.get(&document_id)?.session;
                Some(crate::recovery::RecoveryCellEdit {
                    table_id: session.table_id?,
                    address: session.address?,
                    original: session.original.clone()?,
                    buffer: session.buffer.clone(),
                    cursor_byte: session.cursor_byte,
                    anchor_byte: session.anchor_byte,
                })
            });
        let backing = self
            .file_backings
            .get(&document_id)
            .map(|backing| (backing.path.clone(), backing.identity.clone()));
        let mut snapshot = crate::recovery::RecoverySnapshot::from_document(
            document,
            key,
            backing.as_ref().map(|(path, _)| path.clone()),
            backing.map(|(_, identity)| identity),
            edit,
            generation,
        )?;
        snapshot.requires_initial_save = requires_initial_save;
        Some(snapshot)
    }
}
