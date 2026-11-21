use super::*;

impl TexcelApp {
    /// Reopens each unsaved work item as its own document, retrying while the worker is busy.
    pub(super) fn restore_recovery_items(&mut self) {
        let Some(worker) = self.recovery_worker.as_ref() else {
            return;
        };
        while let Some(item) = self.recovery_items.pop() {
            let key = item.record.key.clone();
            if self.recovery_restore_pending.contains_key(&key) {
                continue;
            }
            let document_id = self.workspace.reserve_document_id();
            match worker.restore(item, document_id, self.language) {
                Ok(()) => {
                    self.recovery_restore_pending.insert(key, document_id);
                }
                Err(item) => {
                    // The queue is full; try the rest on a later frame.
                    self.recovery_items.push(*item);
                    break;
                }
            }
        }
    }

    /// Reports whether cleanup or removal work still owns recovery files.
    pub(super) fn recovery_maintenance_pending(&self) -> bool {
        !self.pending_recovery_cleanups.is_empty()
            || !self.pending_recovery_removals.is_empty()
            || !self.in_flight_recovery_maintenance.is_empty()
            || !self.in_flight_recovery_cleanups.is_empty()
            || !self.in_flight_recovery_removals.is_empty()
    }

    /// Applies one cleanup or removal result without dropping newer queued work.
    pub(super) fn handle_recovery_maintenance_result(
        &mut self,
        key: String,
        result: Result<(), Message>,
    ) {
        self.in_flight_recovery_maintenance.remove(&key);
        let was_remove = self.in_flight_recovery_removals.remove(&key);
        let cleanup_cutoff = self.in_flight_recovery_cleanups.remove(&key);
        match result {
            Ok(()) => {
                if was_remove {
                    self.pending_recovery_removals.remove(&key);
                }
                if let Some(cutoff) = cleanup_cutoff
                    && self.pending_recovery_cleanups.get(&key) == Some(&cutoff)
                {
                    self.pending_recovery_cleanups.remove(&key);
                }
                self.failed_recovery_maintenance.remove(&key);
            }
            Err(detail) => {
                if was_remove {
                    self.pending_recovery_removals.insert(key.clone());
                }
                if !self.pending_recovery_removals.contains(&key)
                    && let Some((revision, generation)) = cleanup_cutoff
                {
                    self.pending_recovery_cleanups
                        .entry(key.clone())
                        .and_modify(|cutoff| {
                            cutoff.0 = cutoff.0.max(revision);
                            cutoff.1 = cutoff.1.max(generation);
                        })
                        .or_insert((revision, generation));
                }
                self.failed_recovery_maintenance.insert(key);
                if self.app_close_waiting_recovery_maintenance {
                    self.app_close_recovery_maintenance_failed = true;
                }
                if was_remove && self.app_close_waiting_recovery_removal {
                    self.app_close_recovery_removal_failed = true;
                }
                self.status = Some(
                    Message::new("status-recovery-remove-failed").with_message("detail", detail),
                );
            }
        }
    }

    /// Holds app exit until every recovery cleanup and removal has a result.
    pub(super) fn finish_recovery_maintenance(&mut self, ctx: &egui::Context) {
        if !self.app_close_waiting_recovery_maintenance {
            return;
        }
        if self.app_close_recovery_maintenance_failed {
            self.app_close_waiting_recovery_maintenance = false;
            self.app_close_recovery_maintenance_failed = false;
            let native_request = self
                .app_close_after_recovery_maintenance
                .take()
                .unwrap_or(false);
            if native_request {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            return;
        }
        if self.recovery_maintenance_pending() {
            return;
        }
        self.app_close_waiting_recovery_maintenance = false;
        let native_request = self
            .app_close_after_recovery_maintenance
            .take()
            .unwrap_or(false);
        if native_request {
            #[cfg(target_os = "macos")]
            crate::macos_app::request_terminate_after_recovery();
            #[cfg(not(target_os = "macos"))]
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Waits for an older remove to finish before sending a newer snapshot.
    pub(super) fn recovery_key_has_pending_removal(&self, document_id: DocumentId) -> bool {
        self.recovery_keys.get(&document_id).is_some_and(|key| {
            self.pending_recovery_removals.contains(key)
                || self.in_flight_recovery_removals.contains(key)
        })
    }

    /// Preserves work typed after a saved version before cleaning its older recovery file.
    pub(super) fn finish_recovery_after_save(
        &mut self,
        document_id: DocumentId,
        saved_revision: u64,
        saved_generation: u64,
    ) {
        if self.app_close_discard_pending {
            return;
        }
        let Some(key) = self.recovery_keys.get(&document_id).cloned() else {
            return;
        };
        let current_generation = self.recovery_generation_for_save(document_id);
        let current_revision = self
            .workspace
            .documents
            .get(&document_id)
            .map_or(saved_revision, |document| document.revision);
        if current_revision > saved_revision || current_generation > saved_generation {
            let Some(snapshot) = self.recovery_snapshot(document_id, current_generation) else {
                return;
            };
            let revision = snapshot.revision;
            let generation = snapshot.generation;
            if self.recovery_key_has_pending_removal(document_id) {
                self.pending_recovery_flushes
                    .insert(document_id, (snapshot, saved_revision, saved_generation));
                return;
            }
            let Some(worker) = self.recovery_worker.as_ref() else {
                return;
            };
            match worker.save_and_cleanup(snapshot, saved_revision, saved_generation) {
                Ok(()) => {
                    if let Some(schedule) = self.recovery_schedules.get_mut(&document_id) {
                        schedule.in_flight = Some((revision, generation));
                    }
                }
                Err(snapshot) => {
                    // Keep the latest snapshot in memory until the worker can accept it.
                    self.pending_recovery_flushes
                        .insert(document_id, (*snapshot, saved_revision, saved_generation));
                }
            }
        } else {
            self.queue_recovery_cleanup(key, saved_revision, saved_generation);
        }
    }

    /// Keeps cleanup and remove requests until the bounded worker accepts them.
    pub(super) fn submit_pending_recovery_maintenance(&mut self) {
        let Some(worker) = self.recovery_worker.as_ref() else {
            let close_needs_maintenance =
                self.app_close_waiting_recovery_maintenance && self.recovery_maintenance_pending();
            let discard_needs_removal = self.app_close_waiting_recovery_removal
                && (!self.pending_recovery_removals.is_empty()
                    || !self.in_flight_recovery_removals.is_empty());
            self.app_close_recovery_removal_failed |= discard_needs_removal;
            self.app_close_recovery_maintenance_failed |= close_needs_maintenance;
            if (close_needs_maintenance || discard_needs_removal)
                && self.failed_recovery_maintenance.is_empty()
            {
                self.status = Some(Message::new("status-recovery-worker-stopped"));
            }
            return;
        };
        if worker.is_stopped() {
            let close_needs_maintenance =
                self.app_close_waiting_recovery_maintenance && self.recovery_maintenance_pending();
            let discard_needs_removal = self.app_close_waiting_recovery_removal
                && (!self.pending_recovery_removals.is_empty()
                    || !self.in_flight_recovery_removals.is_empty());
            self.app_close_recovery_removal_failed |= discard_needs_removal;
            self.app_close_recovery_maintenance_failed |= close_needs_maintenance;
            if (close_needs_maintenance || discard_needs_removal)
                && self.failed_recovery_maintenance.is_empty()
            {
                self.status = Some(Message::new("status-recovery-worker-stopped"));
            }
            return;
        }
        let removals = self
            .pending_recovery_removals
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        for key in removals {
            if self.in_flight_recovery_maintenance.contains(&key)
                || self.failed_recovery_maintenance.contains(&key)
            {
                continue;
            }
            if worker.remove(key.clone()) {
                self.in_flight_recovery_maintenance.insert(key.clone());
                self.in_flight_recovery_removals.insert(key.clone());
            }
        }
        let cleanups = self
            .pending_recovery_cleanups
            .iter()
            .map(|(key, cutoff)| (key.clone(), *cutoff))
            .collect::<Vec<_>>();
        for (key, (revision, generation)) in cleanups {
            if self.in_flight_recovery_maintenance.contains(&key)
                || self.pending_recovery_removals.contains(&key)
                || self.failed_recovery_maintenance.contains(&key)
            {
                continue;
            }
            if worker.cleanup(key.clone(), revision, generation) {
                self.in_flight_recovery_maintenance.insert(key.clone());
                self.in_flight_recovery_cleanups
                    .insert(key, (revision, generation));
            }
        }
    }

    /// Queues cleanup behind current writes and retries when the worker is full.
    pub(crate) fn queue_recovery_cleanup(&mut self, key: String, revision: u64, generation: u64) {
        if !self.pending_recovery_removals.contains(&key) {
            self.failed_recovery_maintenance.remove(&key);
            self.pending_recovery_cleanups
                .entry(key)
                .and_modify(|cutoff| {
                    cutoff.0 = cutoff.0.max(revision);
                    cutoff.1 = cutoff.1.max(generation);
                })
                .or_insert((revision, generation));
        }
    }

    /// Queues removal after explicit discard or a clean document transition.
    pub(crate) fn queue_recovery_remove(&mut self, key: String) {
        self.failed_recovery_maintenance.remove(&key);
        self.pending_recovery_cleanups.remove(&key);
        self.pending_recovery_removals.insert(key);
    }
}
