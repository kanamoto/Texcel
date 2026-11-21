//!  Worker-side operations for prepare.

use super::*;

// Prepare a structure change and route it to the matching operation handler.
pub(crate) fn prepare_structure(
    request: &StructureRequest,
    cancelled: &AtomicBool,
) -> Result<crate::document::PreparedStructure, Message> {
    check_cancelled(cancelled)?;
    if request.clipboard_paste.is_some() {
        return prepare_bulk_cells(request, cancelled);
    }
    let source_before = materialize_current_source(
        request,
        request
            .source
            .as_deref()
            .ok_or_else(|| Message::new("status-structure-source-missing"))?,
    )?;
    let table_ids = request
        .tables
        .iter()
        .map(|table| table.id)
        .collect::<Vec<_>>();
    let mut document = source_to_document(&source_before, request.document_id, &table_ids)?;
    apply_table_metadata(&mut document, &request.tables)?;
    let overlay_before = apply_requested_text_overlay(
        &mut document,
        request.text_overlay_revision,
        request.revision,
        &request.text_overlay_cells,
        &request.changes,
        matches!(
            &source_before,
            SourceDocument::Csv(_) | SourceDocument::Markdown(_)
        ),
    )?;
    check_cancelled(cancelled)?;

    let operation = request
        .operation
        .as_ref()
        .ok_or_else(|| Message::new("status-structure-operation-missing"))?;
    let target_id = operation_table_id(operation);
    let target_index = document
        .tables
        .iter()
        .position(|table| table.id == target_id)
        .ok_or_else(|| {
            Message::new("status-table-id-unavailable").with_number("table_id", target_id as i64)
        })?;
    let source_extent_before = source_extent(&source_before, target_id, &request.tables);
    let target_before = &document.tables[target_index];
    let stored_extent = target_before.last_used_cell();
    let operation_outside_source_extent = structure_operation_outside_extent(
        operation,
        source_extent_before
            .0
            .max(stored_extent.map_or(0, |cell| cell.row.saturating_add(1)))
            .max(1),
        source_extent_before
            .1
            .max(stored_extent.map_or(0, |cell| cell.column.saturating_add(1)))
            .max(1),
    );
    let text_format = matches!(
        &source_before,
        SourceDocument::Csv(_) | SourceDocument::Markdown(_)
    );
    let mut text_history_dimensions = None;
    if text_format {
        let extent = (
            source_extent_before
                .0
                .max(stored_extent.map_or(0, |cell| cell.row.saturating_add(1))),
            source_extent_before
                .1
                .max(stored_extent.map_or(0, |cell| cell.column.saturating_add(1))),
        );
        let changed_extent = request
            .changes
            .iter()
            .filter(|(table_id, _, _)| *table_id == target_id)
            .fold((0, 0), |(rows, columns), (_, address, _)| {
                (
                    rows.max(address.row.saturating_add(1)),
                    columns.max(address.column.saturating_add(1)),
                )
            });
        let table = &mut document.tables[target_index];
        let operation_bounds = match operation {
            StructureOperation::SetTextCell { address, .. } => (
                address.row.saturating_add(1),
                address.column.saturating_add(1),
            ),
            _ => operation_extent(operation),
        };
        let rows = extent
            .0
            .max(changed_extent.0)
            .max(operation_bounds.0)
            .max(1);
        let columns = extent
            .1
            .max(changed_extent.1)
            .max(operation_bounds.1)
            .max(1);
        let history_rows = source_extent_before
            .0
            .max(stored_extent.map_or(0, |cell| cell.row.saturating_add(1)))
            .max(1);
        let history_columns = source_extent_before
            .1
            .max(stored_extent.map_or(0, |cell| cell.column.saturating_add(1)))
            .max(1);
        text_history_dimensions = Some((rows, columns, history_rows, history_columns));
        table.set_dimensions(rows, columns);
    }

    let before_calculation = Arc::new(CalculationSnapshot::from_document(
        &document,
        request.cross_sheet_references,
    ));
    let plan = StructurePlan::build(&document.tables[target_index], operation.clone())?;
    check_cancelled(cancelled)?;
    let formula_updates = if matches!(
        operation,
        StructureOperation::SetTextCell { .. } | StructureOperation::SetTextCells { .. }
    ) {
        BTreeMap::new()
    } else {
        rewrite_structure_formulas(&before_calculation, &plan)?
    };

    let selection_after = request.prepared_selection_after.unwrap_or_else(|| {
        if matches!(operation, StructureOperation::SetTextCells { .. }) {
            request.selection
        } else {
            selection_for_operation(operation, &plan, text_format)
        }
    });
    let (selection_after_anchor, _) = selection_after.normalized();
    let target_after = (request.view_table_id, selection_after_anchor);
    let view_layout_after = if matches!(
        operation,
        StructureOperation::SetTextCell { .. } | StructureOperation::SetTextCells { .. }
    ) {
        Arc::clone(&request.view_layout)
    } else {
        Arc::new(request.view_layout.transformed(&plan)?)
    };
    let before_target = (request.view_table_id, request.selection.anchor);
    let before_metadata = StructureHistoryMetadata {
        document_id: request.document_id,
        view_table_id: request.view_table_id,
        selection: request.selection,
        scroll: request.scroll,
        target: before_target,
        cross_sheet_references: request.cross_sheet_references,
    };
    validate_recovery_root(&document.tables, &request.view_layout, request.scroll)?;
    if let Some((_, _, history_rows, history_columns)) = text_history_dimensions {
        // The before root keeps source data and earlier sparse edits only.
        document.tables[target_index].set_dimensions(history_rows, history_columns);
    }
    let before_size = {
        let before_root = history_root_ref_with_text_overlay(
            &document.tables,
            Some(&source_before),
            &request.view_layout,
            before_metadata,
            &overlay_before,
        );
        serialized_size(&before_root)?
    };
    if let Some((rows, columns, _, _)) = text_history_dimensions {
        // Structure planning continues with only the actual text extent.
        document.tables[target_index].set_dimensions(rows, columns);
    }
    let target_table = &document.tables[target_index];
    let copied_formula_bytes = formula_updates
        .values()
        .map(|formula| formula.len() as u64)
        .fold(0_u64, u64::saturating_add);
    let growth = estimate_structure_growth(target_table, operation)?
        .saturating_add(copied_formula_bytes.saturating_mul(2));
    let estimated_history_bytes = before_size
        .saturating_mul(2)
        .saturating_add(growth.saturating_mul(2));
    let history_file = if estimated_history_bytes >= STRUCTURE_HISTORY_SPILL_THRESHOLD_BYTES {
        let history_file = create_history_file()?;
        if let Some((_, _, history_rows, history_columns)) = text_history_dimensions {
            document.tables[target_index].set_dimensions(history_rows, history_columns);
        }
        let before_write = {
            let before_root = history_root_ref_with_text_overlay(
                &document.tables,
                Some(&source_before),
                &request.view_layout,
                before_metadata,
                &overlay_before,
            );
            write_history_root(&history_file, "before", &before_root)
        };
        if let Some((rows, columns, _, _)) = text_history_dimensions {
            document.tables[target_index].set_dimensions(rows, columns);
        }
        before_write?;
        Some(history_file)
    } else {
        None
    };

    if !matches!(
        operation,
        StructureOperation::SetTextCell { .. } | StructureOperation::SetTextCells { .. }
    ) {
        for table in &mut document.tables {
            check_cancelled(cancelled)?;
            table.apply_structure_plan(&plan, &formula_updates)?;
        }
    }
    if let StructureOperation::SetTextCell {
        table_id,
        address,
        cell,
    } = operation
    {
        let table = document
            .tables
            .iter_mut()
            .find(|table| table.id == *table_id)
            .ok_or_else(|| {
                Message::new("status-table-id-unavailable")
                    .with_number("table_id", *table_id as i64)
            })?;
        table.set_loaded_cell(*address, Cell::clone(cell));
    }
    if let StructureOperation::SetTextCells {
        table_id,
        changes,
        string_overrides,
        ..
    } = operation
    {
        let table = document
            .tables
            .iter_mut()
            .find(|table| table.id == *table_id)
            .ok_or_else(|| {
                Message::new("status-table-id-unavailable")
                    .with_number("table_id", *table_id as i64)
            })?;
        for (address, cell) in changes {
            table.apply_snapshot_cell(*address, cell.clone());
        }
        for (address, _, after) in string_overrides {
            table.set_string_cell(*address, *after);
        }
    }
    if let StructureOperation::Fill {
        table_id,
        source,
        target,
        mode,
    } = operation
    {
        let table = document
            .tables
            .iter_mut()
            .find(|table| table.id == *table_id)
            .ok_or_else(|| {
                Message::new("status-table-id-unavailable")
                    .with_number("table_id", *table_id as i64)
            })?;
        table.apply_fill(*source, *target, *mode, &formula_updates)?;
    }
    check_cancelled(cancelled)?;

    let source_after = if operation_outside_source_extent {
        apply_outside_extent_structure(
            &source_before,
            &formula_updates,
            &document,
            operation,
            &plan,
        )?
    } else {
        transform_source_after(
            &source_before,
            operation,
            &plan,
            &formula_updates,
            &document,
        )?
    };
    document.set_source(Some(Arc::new(source_after.clone())));
    if text_format
        && !matches!(
            operation,
            StructureOperation::SetTextCell { .. } | StructureOperation::SetTextCells { .. }
        )
    {
        let (rows, columns) = plan.after_dimensions();
        document.tables[target_index].set_dimensions(rows, columns);
    }
    let text_overlay_after = collect_structure_text_overlay(
        &document.tables,
        matches!(
            &source_after,
            SourceDocument::Csv(_) | SourceDocument::Markdown(_)
        ),
        &overlay_before,
        &plan,
    );
    let after_calculation = Arc::new(CalculationSnapshot::from_document(
        &document,
        request.cross_sheet_references,
    ));
    validate_recovery_root(&document.tables, &view_layout_after, request.scroll)?;
    if let Some(history_file) = &history_file {
        let after_root = history_root_ref_with_text_overlay(
            &document.tables,
            Some(&source_after),
            &view_layout_after,
            StructureHistoryMetadata {
                document_id: request.document_id,
                view_table_id: request.view_table_id,
                selection: selection_after,
                scroll: request.scroll,
                target: target_after,
                cross_sheet_references: request.cross_sheet_references,
            },
            &text_overlay_after,
        );
        write_history_root(history_file, "after", &after_root)?;
    }
    let keep_in_memory_history = history_file.is_none();

    Ok(crate::document::PreparedStructure {
        tables: document.tables,
        calculation_revision: request.calculation_revision,
        replace_table_set: false,
        text_overlay_before: overlay_before,
        text_overlay_after,
        source_before: keep_in_memory_history.then(|| Arc::new(source_before)),
        source_after: Some(Arc::new(source_after)),
        view_table_id: request.view_table_id,
        view_layout_revision: request.view_layout.revision(),
        view_layout_after,
        selection_after,
        scroll_after: request.scroll,
        target: target_after,
        target_before: None,
        before_calculation: keep_in_memory_history.then_some(before_calculation),
        after_calculation,
        history_file,
        conversion_report: None,
        status: match operation {
            StructureOperation::SetTextCells {
                completion_status, ..
            } => completion_status.clone(),
            _ => None,
        },
    })
}
