use super::*;

impl TexcelApp {
    // Queue parsing for rich display of visible source cells.
    pub(super) fn request_cell_display_previews(
        &mut self,
        markdown_display_requests: Vec<(u64, CellAddress)>,
    ) {
        let large_display_sources = self.workspace.active_document().and_then(|document| {
            let SourceDocument::Markdown(markdown) = document.source.as_deref()? else {
                return None;
            };
            Some({
                let requests = markdown_display_requests
                    .iter()
                    .copied()
                    .collect::<BTreeSet<_>>();
                let markdown_source = Arc::clone(markdown);
                (
                    document.id,
                    document.revision,
                    requests
                        .into_iter()
                        .filter_map(|(table_id, address)| {
                            let key = (document.id, table_id, address);
                            let cell_input_revision =
                                document.cell_input_revision(table_id, address);
                            if self
                                .pending_markdown_displays
                                .get(&key)
                                .is_some_and(|(_, version)| *version == cell_input_revision)
                            {
                                return None;
                            }
                            let cell_value = document
                                .tables
                                .iter()
                                .find(|table| table.id == table_id)
                                .and_then(|table| table.cell(address))
                                .and_then(|cell| match &cell.input {
                                    CellInput::Text(text) => Some(text.clone()),
                                    _ => None,
                                })?;
                            let markdown_table = document
                                .tables
                                .iter()
                                .position(|table| table.id == table_id)?;
                            Some((
                                (table_id, address),
                                cell_input_revision,
                                Arc::clone(&markdown_source),
                                crate::markdown_adapter::MarkdownCellChange {
                                    table: markdown_table,
                                    row: address.row,
                                    column: address.column,
                                    value: cell_value,
                                },
                            ))
                        })
                        .collect::<Vec<_>>(),
                )
            })
        });
        if let Some((document_id, revision, sources)) = large_display_sources {
            for ((table_id, address), cell_input_revision, source, change) in sources {
                let key = (document_id, table_id, address);
                let token = self.next_job_token(document_id, revision);
                if self
                    .file_worker
                    .prepare_markdown_cell_display(
                        token,
                        table_id,
                        address,
                        cell_input_revision,
                        source,
                        change,
                    )
                    .is_ok()
                {
                    self.pending_markdown_displays
                        .insert(key, (token, cell_input_revision));
                }
            }
        }
    }
}
