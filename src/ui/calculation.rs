use super::*;

impl TexcelApp {
    /// Drops request state when a document replaces its calculation model.
    pub(super) fn clear_calculation_jobs_for_reset(&mut self, document_id: DocumentId) {
        self.pending_calculations.remove(&document_id);
        self.pending_initial_calculations.remove(&document_id);
        self.calculation_initial_snapshots.remove(&document_id);
        self.submitted_calculation_changes.remove(&document_id);
        self.calculation_retries.remove(&document_id);
        self.calculation_retry_available.remove(&document_id);
        self.calculation_snapshots.remove(&document_id);
    }

    // Registers the sparse initial model built by the worker and schedules a redraw when calculation finishes.
    pub(super) fn initialize_calculation(
        &mut self,
        token: JobToken,
        snapshot: CalculationSnapshot,
    ) {
        let snapshot = Arc::new(snapshot);
        self.calculation_initial_snapshots
            .insert(token.document_id, (token, Arc::clone(&snapshot)));
        self.pending_initial_calculations
            .insert(token.document_id, (token, snapshot));
        self.try_initialize_calculation(token.document_id);
    }

    // Create calculation worker state from the current document settings.
    pub(super) fn initialize_calculation_shared(
        &mut self,
        token: JobToken,
        snapshot: Arc<CalculationSnapshot>,
    ) {
        self.calculation_initial_snapshots
            .insert(token.document_id, (token, Arc::clone(&snapshot)));
        self.pending_initial_calculations
            .insert(token.document_id, (token, snapshot));
        self.try_initialize_calculation(token.document_id);
    }

    /// Retains an initial snapshot across queue-full responses and submits it once a slot opens.
    pub(super) fn try_initialize_calculation(&mut self, document_id: DocumentId) {
        if self.pending_calculations.contains_key(&document_id) {
            return;
        }
        let Some((token, snapshot)) = self.pending_initial_calculations.remove(&document_id) else {
            return;
        };
        match self
            .calculation_worker
            .initialize_shared_retaining(token, Arc::clone(&snapshot))
        {
            Ok(()) => {
                self.calculation_retries.remove(&document_id);
                self.calculation_retry_available.remove(&document_id);
                self.pending_calculations.insert(document_id, token);
            }
            Err((error, snapshot)) => {
                self.pending_initial_calculations
                    .insert(document_id, (token, snapshot));
                if error == crate::calc::CalculationSubmitError::QueueFull {
                    self.calculation_retries.insert(document_id);
                }
                self.status = Some(crate::i18n::calculation_submit_message(error));
            }
        }
    }

    // Sends only cells not yet sent to the calculation worker. Tracks them separately from save changes.
    pub(super) fn schedule_calculation(&mut self, document_id: DocumentId) {
        if self.pending_initial_calculations.contains_key(&document_id) {
            self.try_initialize_calculation(document_id);
            return;
        }
        if self
            .calculation_initial_snapshots
            .contains_key(&document_id)
            && !self.pending_calculations.contains_key(&document_id)
        {
            self.retry_calculation(document_id);
            return;
        }
        // Keep one job in flight per document; edits remain coalesced in Document's key set.
        if self.pending_calculations.contains_key(&document_id) {
            return;
        }
        let Some((revision, queued_cells, markdown_source)) =
            self.workspace.documents.get(&document_id).map(|document| {
                let queued_cells = document.calculation_cell_snapshot();
                (
                    document.calculation_revision,
                    queued_cells,
                    crate::calc::MarkdownCalculationSource::from_document(document),
                )
            })
        else {
            self.calculation_retries.remove(&document_id);
            return;
        };
        if queued_cells.is_empty() {
            self.calculation_retries.remove(&document_id);
            return;
        }
        let keys = queued_cells
            .iter()
            .map(|(table_id, address, _)| (*table_id, *address))
            .collect::<Vec<_>>();
        let changes = queued_cells
            .into_iter()
            .map(|(table_id, address, cell)| {
                let table = self
                    .workspace
                    .documents
                    .get(&document_id)
                    .and_then(|document| document.tables.iter().find(|table| table.id == table_id));
                CalculationChange {
                    table_id,
                    address,
                    cell,
                    string_cell: table.is_some_and(|table| table.is_string_cell(address)),
                }
            })
            .collect::<Vec<_>>();
        let token = self.next_job_token(document_id, revision);
        let submitted = if let Some(source) = markdown_source {
            self.calculation_worker
                .update_markdown(token, changes, source)
        } else {
            self.calculation_worker.update(token, changes)
        };
        match submitted {
            Ok(()) => {
                if let Some(document) = self.workspace.documents.get_mut(&document_id) {
                    document.mark_calculation_submitted(&keys);
                }
                self.submitted_calculation_changes
                    .insert(document_id, (token, keys));
                self.calculation_retries.remove(&document_id);
                self.calculation_retry_available.remove(&document_id);
                self.pending_calculations.insert(document_id, token);
            }
            Err(error) => {
                self.calculation_retries.insert(document_id);
                self.status = Some(crate::i18n::calculation_submit_message(error));
            }
        }
    }

    /// Cancels publication and restores submitted keys for a later explicit retry.
    pub(super) fn cancel_calculation(&mut self, document_id: DocumentId) {
        self.cancel_auto_fit_for(document_id);
        let active_token = self.pending_calculations.remove(&document_id);
        if let Some(token) = active_token {
            self.calculation_worker.cancel(token);
        }
        let initial_token = self
            .pending_initial_calculations
            .remove(&document_id)
            .map(|(token, _)| token);
        let retry_waiting = self.calculation_retries.remove(&document_id);
        let token = active_token.or(initial_token);
        if token.is_none() && !retry_waiting {
            return;
        }
        if let Some((submitted_token, keys)) =
            self.submitted_calculation_changes.remove(&document_id)
            && Some(submitted_token) == token
            && let Some(document) = self.workspace.documents.get_mut(&document_id)
        {
            document.requeue_calculation_changes(&keys);
        }
        if let Some((snapshot_token, snapshot)) = self
            .calculation_initial_snapshots
            .get(&document_id)
            .cloned()
            && let Some((revision, updated)) =
                self.workspace.documents.get(&document_id).map(|document| {
                    let mut updated = (*snapshot).clone();
                    updated.preserve_string_cells(&document.tables);
                    (document.calculation_revision, Arc::new(updated))
                })
        {
            let snapshot_token = if snapshot_token.revision == revision {
                snapshot_token
            } else {
                self.next_job_token(document_id, revision)
            };
            self.calculation_initial_snapshots
                .insert(document_id, (snapshot_token, updated));
        }
        self.calculation_retry_available.insert(document_id);
        self.status = Some(crate::i18n::Message::new("status-calculation-cancelled"));
    }

    /// Retries retained work with a new token so a late result cannot replace it.
    pub(super) fn retry_calculation(&mut self, document_id: DocumentId) {
        if self.pending_calculations.contains_key(&document_id) {
            return;
        }
        self.calculation_retry_available.remove(&document_id);
        if self.status.as_ref() == Some(&crate::i18n::Message::new("status-calculation-cancelled"))
        {
            self.status = None;
        }
        if let Some((old_token, snapshot)) = self
            .calculation_initial_snapshots
            .get(&document_id)
            .cloned()
        {
            let token = self.next_job_token(document_id, old_token.revision);
            self.calculation_initial_snapshots
                .insert(document_id, (token, Arc::clone(&snapshot)));
            self.pending_initial_calculations
                .insert(document_id, (token, snapshot));
            self.try_initialize_calculation(document_id);
            return;
        }
        self.schedule_calculation(document_id);
    }
}
