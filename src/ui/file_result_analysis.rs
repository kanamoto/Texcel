use super::*;

impl TexcelApp {
    // Apply a completed background analysis to the matching document.
    pub(super) fn handle_analysis_result(
        &mut self,
        document_id: DocumentId,
        token: JobToken,
        kind: FileResultKind,
    ) -> bool {
        match kind {
            FileResultKind::SearchPrepared(prepared) => {
                if !self
                    .pending_searches
                    .get(&document_id)
                    .is_some_and(|pending| pending.token == token)
                {
                    return true;
                }
                let pending = self
                    .pending_searches
                    .remove(&document_id)
                    .expect("matching search token remains pending");
                if prepared.document_revision != pending.key.document_revision
                    || prepared.calculation_revision != pending.key.calculation_revision
                    || !self.search_job_key_is_current(&pending.key)
                {
                    if let Some(dialog) = self.search_dialog.as_mut()
                        && dialog.document_id == document_id
                    {
                        dialog.preview = None;
                        dialog.preview_key = None;
                        dialog.message = Some(Message::new("dialog-search-stale"));
                    }
                    return true;
                }
                let summary = search_summary_message(
                    prepared.total_matches,
                    prepared.matched_cells,
                    prepared.hits_truncated,
                    prepared.skipped_non_fresh,
                );
                if let Some(dialog) = self.search_dialog.as_mut()
                    && dialog.document_id == document_id
                {
                    dialog.preview_key = Some(pending.key.clone());
                    dialog.preview = Some(prepared);
                    dialog.message = Some(summary.clone());
                }
                self.status = Some(summary);
            }
            FileResultKind::DataEditPrepared(prepared) => {
                if !self
                    .pending_data_edits
                    .get(&document_id)
                    .is_some_and(|pending| pending.token == token)
                {
                    return true;
                }
                let pending = self
                    .pending_data_edits
                    .remove(&document_id)
                    .expect("matching data-edit token remains pending");
                if prepared.document_revision != pending.key.document_revision
                    || prepared.calculation_revision != pending.key.calculation_revision
                    || !self.data_edit_job_key_is_current(&pending.key)
                {
                    if let Some(dialog) = self.data_edit_dialog.as_mut()
                        && dialog.document_id == document_id
                    {
                        dialog.preview = None;
                        dialog.preview_key = None;
                        dialog.message = Some(Message::new("dialog-data-edit-stale"));
                    }
                    return true;
                }
                let duplicate_action = self
                    .data_edit_dialog
                    .as_ref()
                    .filter(|dialog| dialog.document_id == document_id)
                    .map(|dialog| dialog.kind);
                let summary = if prepared.duplicate_count > 0 {
                    if duplicate_action == Some(DataEditKind::DetectDuplicates) {
                        Message::new("dialog-data-edit-summary-duplicates")
                            .with_number("rows", prepared.duplicate_count as i64)
                            .with_number("groups", prepared.duplicate_groups as i64)
                    } else {
                        Message::new("dialog-data-edit-summary-removal")
                            .with_number("rows", prepared.duplicate_count as i64)
                            .with_number("cells", prepared.changed_cells as i64)
                    }
                } else if matches!(
                    duplicate_action,
                    Some(DataEditKind::DetectDuplicates | DataEditKind::RemoveDuplicates)
                ) {
                    Message::new("dialog-data-edit-summary-none")
                } else {
                    Message::new("dialog-data-edit-summary-changed")
                        .with_number("count", prepared.changed_cells as i64)
                        .with_message(
                            "tail",
                            Message::new(if prepared.preview_truncated {
                                "dialog-data-edit-summary-limit"
                            } else {
                                "dialog-empty-tail"
                            }),
                        )
                };
                if let Some(dialog) = self.data_edit_dialog.as_mut()
                    && dialog.document_id == document_id
                {
                    dialog.preview_key = Some(pending.key.clone());
                    dialog.preview = Some(prepared);
                    dialog.message = Some(summary.clone());
                }
                self.status = Some(summary);
            }
            _ => unreachable!("file result was routed to the wrong handler"),
        }
        false
    }
}

/// Builds a search summary that can be translated after the worker finishes.
fn search_summary_message(
    matches: usize,
    cells: usize,
    truncated: bool,
    skipped: usize,
) -> Message {
    let tail = match (truncated, skipped > 0) {
        (true, true) => {
            Message::new("dialog-search-summary-tail-both").with_number("count", skipped as i64)
        }
        (true, false) => Message::new("dialog-search-summary-truncated"),
        (false, true) => {
            Message::new("dialog-search-summary-skipped").with_number("count", skipped as i64)
        }
        (false, false) => Message::new("dialog-empty-tail"),
    };
    Message::new("dialog-search-summary")
        .with_number("matches", matches as i64)
        .with_number("cells", cells as i64)
        .with_message("tail", tail)
}
