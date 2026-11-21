//!  Worker-side operations for bulk.

use super::*;

// Prepare a batch of cell edits and their updated source document.
pub(crate) fn prepare_cell_changes(
    request: &StructureRequest,
    mut document: Document,
    source_before: Option<SourceDocument>,
    overlay_before: Arc<Vec<TextOverlayCell>>,
    changes: Vec<(TableId, CellAddress, Option<Cell>)>,
    cancelled: &AtomicBool,
) -> Result<crate::document::PreparedStructure, Message> {
    check_cancelled(cancelled)?;
    let table_ids = request
        .tables
        .iter()
        .map(|table| table.id)
        .collect::<Vec<_>>();
    let source = source_before
        .as_ref()
        .ok_or_else(|| Message::new("status-document-not-found"))?;
    let mut before_document = { source_to_document(source, request.document_id, &table_ids)? };
    apply_table_metadata(&mut before_document, &request.tables)?;
    apply_requested_text_overlay(
        &mut before_document,
        request.text_overlay_revision,
        request.revision,
        &request.text_overlay_cells,
        &request.changes,
        true,
    )?;
    let before_calculation = Arc::new(CalculationSnapshot::from_document(
        &before_document,
        request.cross_sheet_references,
    ));
    let before_root = history_root_ref(
        &before_document.tables,
        source_before.as_ref(),
        &request.view_layout,
        StructureHistoryMetadata {
            document_id: request.document_id,
            view_table_id: request.view_table_id,
            selection: request.selection,
            scroll: request.scroll,
            target: (request.view_table_id, request.selection.anchor),
            cross_sheet_references: request.cross_sheet_references,
        },
    );
    let before_size = serialized_size(&before_root)?;
    let history_file = if before_size.saturating_mul(2) >= STRUCTURE_HISTORY_SPILL_THRESHOLD_BYTES {
        let file = create_history_file()?;
        write_history_root(&file, "before", &before_root)?;
        Some(file)
    } else {
        None
    };
    for (table_id, address, cell) in &changes {
        check_cancelled(cancelled)?;
        let table = document
            .tables
            .iter_mut()
            .find(|table| table.id == *table_id)
            .ok_or_else(|| {
                Message::new("status-table-id-unavailable")
                    .with_number("table_id", *table_id as i64)
            })?;
        table.apply_snapshot_cell(*address, cell.clone());
    }
    let source_after = source_before
        .as_ref()
        .map(|source| apply_bulk_cell_changes_multi(source, &request.tables, &changes, &document))
        .transpose()?;
    if let Some(source) = source_after.as_ref() {
        document.set_source(Some(Arc::new(source.clone())));
    }
    let text_overlay_after = collect_preserved_text_overlay(
        &document.tables,
        source_after.as_ref().is_none_or(|source| {
            matches!(source, SourceDocument::Csv(_) | SourceDocument::Markdown(_))
        }),
        &overlay_before,
    );
    let after_calculation = Arc::new(CalculationSnapshot::from_document(
        &document,
        request.cross_sheet_references,
    ));
    validate_recovery_root(
        &before_document.tables,
        &request.view_layout,
        request.scroll,
    )?;
    validate_recovery_root(&document.tables, &request.view_layout, request.scroll)?;
    let first = changes
        .first()
        .map(|(table, address, _)| (*table, *address))
        .unwrap_or((request.view_table_id, request.selection.head));
    if let Some(file) = &history_file {
        let after_root = history_root_ref(
            &document.tables,
            source_after.as_ref(),
            &request.view_layout,
            StructureHistoryMetadata {
                document_id: request.document_id,
                view_table_id: request.view_table_id,
                selection: request.selection,
                scroll: request.scroll,
                target: first,
                cross_sheet_references: request.cross_sheet_references,
            },
        );
        write_history_root(file, "after", &after_root)?;
    }
    Ok(crate::document::PreparedStructure {
        tables: document.tables,
        calculation_revision: request.calculation_revision,
        replace_table_set: false,
        text_overlay_before: overlay_before,
        text_overlay_after,
        source_before: history_file
            .is_none()
            .then(|| source_before.map(Arc::new))
            .flatten(),
        source_after: source_after.map(Arc::new),
        view_table_id: request.view_table_id,
        view_layout_revision: request.view_layout.revision(),
        view_layout_after: Arc::clone(&request.view_layout),
        selection_after: CellRange::single(first.1),
        scroll_after: request.scroll,
        target: first,
        target_before: None,
        before_calculation: history_file.is_none().then_some(before_calculation),
        after_calculation,
        history_file,
        conversion_report: None,
        status: None,
    })
}

// Apply cell edits across several source tables while preserving source text.
pub(crate) fn apply_bulk_cell_changes_multi(
    source: &SourceDocument,
    tables: &[StructureTableMeta],
    cells: &[(TableId, CellAddress, Option<Cell>)],
    document: &Document,
) -> Result<SourceDocument, Message> {
    match source {
        SourceDocument::Csv(source) => {
            let source_table_id = tables
                .first()
                .filter(|_| tables.len() == 1)
                .map(|table| table.id)
                .ok_or_else(|| Message::new("status-table-count-changed-after-parse"))?;
            if document
                .tables
                .iter()
                .all(|table| table.id != source_table_id)
                || cells
                    .iter()
                    .any(|(table_id, _, _)| *table_id != source_table_id)
            {
                return Err(Message::new("status-table-id-unavailable")
                    .with_number("table_id", source_table_id as i64));
            }
            let changes = cells
                .iter()
                .filter(|(table_id, _, _)| *table_id == source_table_id)
                .map(|(_, address, cell)| CsvCellChange {
                    row: address.row,
                    column: address.column,
                    value: cell.as_ref().map(|cell| {
                        let is_string = document
                            .tables
                            .iter()
                            .find(|table| table.id == source_table_id)
                            .is_some_and(|table| table.is_string_cell(*address));
                        cell.source_text_for_text_file(is_string)
                    }),
                })
                .collect::<Vec<_>>();
            source
                .apply_changes(&changes)
                .map(|source| SourceDocument::Csv(Arc::new(source)))
                .map_err(crate::i18n::csv_error_message)
        }
        SourceDocument::Markdown(source) => {
            let changes = cells
                .iter()
                .filter_map(|(table_id, address, cell)| {
                    tables
                        .iter()
                        .position(|table| table.id == *table_id)
                        .map(|table| MarkdownCellChange {
                            table,
                            row: address.row,
                            column: address.column,
                            value: cell.as_ref().map_or_else(String::new, |cell| {
                                let is_string = document
                                    .tables
                                    .iter()
                                    .find(|source_table| source_table.id == *table_id)
                                    .is_some_and(|source_table| {
                                        source_table.is_string_cell(*address)
                                    });
                                cell.source_text_for_text_file(is_string)
                            }),
                        })
                })
                .collect::<Vec<_>>();
            source
                .apply_changes(&changes)
                .map(|source| SourceDocument::Markdown(Arc::new(source)))
                .map_err(crate::i18n::markdown_error_message)
        }
    }
}
