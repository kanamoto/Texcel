use super::*;

impl TexcelApp {
    /// Publishes only complete roots from the latest calculation request.
    pub(super) fn process_calculation_results(&mut self, ctx: &egui::Context) {
        let mut completed = Vec::new();
        for result in self.calculation_worker.poll_limit(2) {
            let document_id = result.token.document_id;
            if self.pending_calculations.get(&document_id) != Some(&result.token) {
                self.calculation_worker.forget_cancellation(result.token);
                continue;
            }
            let Some(document) = self.workspace.documents.get_mut(&document_id) else {
                self.forget_calculation_job(document_id, result.token);
                continue;
            };

            if !result.failed_changes.is_empty() {
                document.requeue_calculation_changes(&result.failed_changes);
            }
            if !result.done {
                continue;
            }

            self.pending_calculations.remove(&document_id);
            if self
                .submitted_calculation_changes
                .get(&document_id)
                .is_some_and(|(submitted, _)| *submitted == result.token)
            {
                self.submitted_calculation_changes.remove(&document_id);
            }
            let worker_error = result.error.is_some();
            if worker_error {
                self.calculation_retry_available.insert(document_id);
            } else {
                if self
                    .calculation_initial_snapshots
                    .get(&document_id)
                    .is_some_and(|(token, _)| *token == result.token)
                {
                    self.calculation_initial_snapshots.remove(&document_id);
                }
                if document.calculation_revision == result.token.revision
                    && !document.has_pending_calculation_changes()
                    && let Some(snapshot) = result.full_snapshot
                    && document.install_calculation_results(Arc::clone(&snapshot))
                {
                    self.calculation_snapshots
                        .insert(document_id, (result.token.revision, snapshot));
                    self.calculation_retry_available.remove(&document_id);
                }
            }
            if let Some(error) = result.error {
                self.status = Some(error);
            }

            let has_coalesced_changes = document.has_pending_calculation_changes();
            completed.push((
                result.token,
                worker_error,
                document.calculation_revision,
                has_coalesced_changes,
            ));
        }

        for (token, failed, revision, has_changes) in completed {
            let document_id = token.document_id;
            if self.pending_initial_calculations.contains_key(&document_id) && !failed {
                self.try_initialize_calculation(document_id);
            } else if !self.pending_calculations.contains_key(&document_id)
                && should_schedule_after_calculation_ack(
                    revision,
                    token.revision,
                    has_changes,
                    failed,
                )
            {
                self.schedule_calculation(document_id);
            } else if !failed {
            }
        }

        let retry_ids = std::mem::take(&mut self.calculation_retries);
        for document_id in retry_ids {
            if self.pending_initial_calculations.contains_key(&document_id) {
                self.try_initialize_calculation(document_id);
            } else {
                self.schedule_calculation(document_id);
            }
        }
        self.start_ready_auto_fits(ctx);
        if !self.pending_calculations.is_empty()
            || !self.pending_initial_calculations.is_empty()
            || !self.calculation_retries.is_empty()
        {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    /// Removes bookkeeping only when it still belongs to the completed job.
    fn forget_calculation_job(&mut self, document_id: DocumentId, token: JobToken) {
        if self.pending_calculations.get(&document_id) == Some(&token) {
            self.pending_calculations.remove(&document_id);
        }
        self.forget_submitted_changes(document_id, token);
    }

    /// Keeps a newer edit batch when an older worker result arrives.
    fn forget_submitted_changes(&mut self, document_id: DocumentId, token: JobToken) {
        if self
            .submitted_calculation_changes
            .get(&document_id)
            .is_some_and(|(submitted, _)| *submitted == token)
        {
            self.submitted_calculation_changes.remove(&document_id);
        }
    }
}
