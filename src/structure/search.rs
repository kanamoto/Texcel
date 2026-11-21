//!  Searches materialized source snapshots and prepares search results.

use super::*;

// Search the requested tables and collect matching cell addresses.
pub(crate) fn prepare_search(
    request: &SearchRequest,
    cancelled: &AtomicBool,
) -> Result<PreparedSearch, Message> {
    if request.query.is_empty() {
        return Err(Message::new("status-search-query-empty"));
    }
    if request.ranges.is_empty() {
        return Err(Message::new("status-search-range-empty"));
    }
    let source_before = request
        .structure
        .source
        .as_deref()
        .map(|source| materialize_current_source(&request.structure, source))
        .transpose()?;
    let table_ids = request
        .structure
        .tables
        .iter()
        .map(|table| table.id)
        .collect::<Vec<_>>();
    let source = source_before
        .as_ref()
        .ok_or_else(|| Message::new("status-search-document-missing"))?;
    let mut document = { source_to_document(source, request.structure.document_id, &table_ids)? };
    apply_table_metadata(&mut document, &request.structure.tables)?;
    let overlay_before = apply_requested_text_overlay(
        &mut document,
        request.structure.text_overlay_revision,
        request.structure.revision,
        &request.structure.text_overlay_cells,
        &request.structure.changes,
        true,
    )?;
    check_cancelled(cancelled)?;

    let pattern = search_pattern(request)?;
    let formula_results = request.formula_results.as_deref().filter(|_| {
        request.formula_results_revision == Some(request.structure.calculation_revision)
    });
    let mut hits = Vec::new();
    let mut hits_truncated = false;
    let mut matched_cells = 0_usize;
    let mut total_matches = 0_usize;
    let mut skipped_non_fresh = 0_usize;
    let mut changes = Vec::<(TableId, CellAddress, Option<Cell>)>::new();
    let mut visited_cells = BTreeSet::<(TableId, CellAddress)>::new();
    for (table_id, range) in &request.ranges {
        check_cancelled(cancelled)?;
        let Some(table) = document.tables.iter().find(|table| table.id == *table_id) else {
            return Err(Message::new("status-table-id-unavailable")
                .with_number("table_id", *table_id as i64));
        };
        let (start, end) = range.normalized();
        for (address, cell) in table.stored_cells_in_range(CellRange {
            anchor: start,
            head: end,
        }) {
            check_cancelled(cancelled)?;
            if !visited_cells.insert((*table_id, address)) {
                continue;
            }
            let source_text = cell.input.source_text();
            let (searched_text, result_state) = match request.representation {
                SearchRepresentation::Source => (source_text.clone(), None),
                SearchRepresentation::Display => {
                    let is_formula = is_formula_cell(table, address, cell);
                    if is_formula {
                        match formula_results.and_then(|results| results.get(*table_id, address)) {
                            Some((CalcStatus::Fresh, Some(value))) => {
                                let mut display_cell = cell.clone();
                                display_cell.calculation = Some(crate::document::CalcResult {
                                    revision: request.structure.calculation_revision,
                                    status: CalcStatus::Fresh,
                                    value: Some(value.clone()),
                                });
                                (
                                    display_cell
                                        .display_text(request.structure.calculation_revision),
                                    Some(SearchResultState::Calculation(CalcStatus::Fresh)),
                                )
                            }
                            Some((status, _)) => {
                                skipped_non_fresh = skipped_non_fresh.saturating_add(1);
                                continue_search_skip(
                                    &mut hits,
                                    *table_id,
                                    address,
                                    source_text,
                                    SearchResultState::Calculation(status),
                                    &mut hits_truncated,
                                );
                                continue;
                            }
                            None => {
                                skipped_non_fresh = skipped_non_fresh.saturating_add(1);
                                continue_search_skip(
                                    &mut hits,
                                    *table_id,
                                    address,
                                    source_text,
                                    SearchResultState::Missing,
                                    &mut hits_truncated,
                                );
                                continue;
                            }
                        }
                    } else if let Some(SourceDocument::Markdown(markdown)) = source_before.as_ref()
                        && matches!(cell.input, CellInput::Text(_))
                    {
                        let table_index = request
                            .structure
                            .tables
                            .iter()
                            .position(|table| table.id == *table_id)
                            .ok_or_else(|| {
                                Message::new("status-markdown-table-id-missing")
                                    .with_number("table_id", *table_id as i64)
                            })?;
                        let display_text = markdown
                            .cell_display_text_for_search(
                                table_index,
                                address.row,
                                address.column,
                                &source_text,
                                request.language,
                            )
                            .map_err(|error| {
                                Message::new("status-markdown-search-display-failed").with_message(
                                    "detail",
                                    crate::i18n::markdown_error_message(error),
                                )
                            })?;
                        (display_text, Some(SearchResultState::Display))
                    } else {
                        (
                            cell.display_text(request.structure.calculation_revision),
                            Some(SearchResultState::Display),
                        )
                    }
                }
            };
            let matches = pattern.find_iter(&searched_text).count();
            if matches == 0 {
                continue;
            }
            matched_cells = matched_cells.saturating_add(1);
            total_matches = total_matches.saturating_add(matches);
            let replacement_text = request.replacement.as_ref().map(|replacement| {
                if request.regular_expression {
                    pattern
                        .replace_all(&searched_text, replacement.as_str())
                        .into_owned()
                } else {
                    pattern
                        .replace_all(&searched_text, regex::NoExpand(replacement.as_str()))
                        .into_owned()
                }
            });
            if let Some(replacement_text) = replacement_text.as_ref()
                && replacement_text != &searched_text
            {
                let mut updated = cell.clone();
                updated.input = match request.representation {
                    SearchRepresentation::Source => {
                        cell.input.edited_source(replacement_text.clone())
                    }
                    SearchRepresentation::Display => CellInput::Text(replacement_text.clone()),
                };
                updated.imported_result = None;
                updated.calculation = None;
                changes.push((*table_id, address, Some(updated)));
            }
            if hits.len() < 10_000 {
                hits.push(SearchHit {
                    table_id: *table_id,
                    address,
                    source_text,
                    searched_text,
                    replacement_text,
                    result_state,
                });
            } else {
                hits_truncated = true;
            }
        }
    }
    let prepared = if changes.is_empty() {
        None
    } else {
        Some(prepare_cell_changes(
            &request.structure,
            document,
            source_before,
            overlay_before,
            changes,
            cancelled,
        )?)
    };
    Ok(PreparedSearch {
        document_revision: request.structure.revision,
        calculation_revision: request.structure.calculation_revision,
        hits,
        matched_cells,
        total_matches,
        hits_truncated,
        skipped_non_fresh,
        prepared,
    })
}
