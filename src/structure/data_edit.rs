//!  Worker-side operations for data edit.

use super::*;
use crate::i18n::Message;

// Prepare a sort, filter, or related table data operation.
pub(crate) fn prepare_data_edit(
    request: &DataEditRequest,
    cancelled: &AtomicBool,
) -> Result<PreparedDataEdit, Message> {
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
        .ok_or_else(|| Message::new("status-data-edit-source-missing"))?;
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

    if let DataEditAction::SortRows {
        table_id,
        keys,
        header_row,
    } = &request.action
    {
        return prepare_sorted_rows(
            request,
            &document,
            source_before.as_ref(),
            *table_id,
            keys,
            *header_row,
            cancelled,
        );
    }
    if let DataEditAction::Duplicates {
        table_id,
        range,
        header_row,
        delete,
    } = &request.action
    {
        return prepare_duplicates(
            request,
            &document,
            source_before.as_ref(),
            DuplicateScan {
                table_id: *table_id,
                range: *range,
                header_row: *header_row,
                delete: *delete,
            },
            cancelled,
        );
    }

    let mut changes = BTreeMap::<(TableId, CellAddress), Option<Cell>>::new();
    let mut expanded_dimensions = BTreeMap::<TableId, (usize, usize)>::new();
    let mut string_cell_updates = BTreeMap::<(TableId, CellAddress), bool>::new();
    match &request.action {
        DataEditAction::TrimWhitespace { ranges } => {
            for (table_id, range) in ranges {
                let table = find_table(&document, *table_id)?;
                for (address, cell) in table.stored_cells_in_range(*range) {
                    check_cancelled(cancelled)?;
                    if is_formula_cell(table, address, cell) {
                        continue;
                    }
                    let CellInput::Text(text) = &cell.input else {
                        continue;
                    };
                    let trimmed = text.trim();
                    if trimmed == text {
                        continue;
                    }
                    let mut updated = cell.clone();
                    updated.input = CellInput::Text(trimmed.to_owned());
                    changes.insert((*table_id, address), Some(updated));
                }
            }
        }
        DataEditAction::SplitColumn {
            table_id,
            rows,
            source_column,
            delimiter,
            destination_column,
            overwrite,
        } => {
            validate_delimiter(delimiter)?;
            if *destination_column <= *source_column {
                return Err(Message::new("status-split-destination-before-source"));
            }
            let table = find_table(&document, *table_id)?;
            let (start, end) = rows.normalized();
            if end.row >= table.row_count() || *source_column >= table.column_count() {
                return Err(Message::new("status-split-range-out-of-bounds"));
            }
            let mut maximum_column = table.column_count();
            let mut row_outputs = Vec::new();
            for row in start.row..=end.row {
                check_cancelled(cancelled)?;
                let source_address = CellAddress {
                    row,
                    column: *source_column,
                };
                let source_cell = table.cell(source_address);
                if source_cell.is_some_and(|cell| is_formula_cell(table, source_address, cell)) {
                    return Err(Message::new("status-split-formula-cell-unsupported")
                        .with_text("cell", crate::xlsx_adapter::cell_reference(source_address)));
                }
                let source_value =
                    source_cell.map_or_else(String::new, |cell| cell.input.source_text());
                let parts = source_value.split(delimiter.as_str()).collect::<Vec<_>>();
                if parts.len() <= 1 {
                    continue;
                }
                if destination_column.saturating_add(parts.len()) > crate::document::MAX_COLUMNS {
                    return Err(Message::new("status-split-column-limit"));
                }
                maximum_column = maximum_column.max(destination_column + parts.len());
                for (offset, part) in parts.iter().enumerate() {
                    let address = CellAddress {
                        row,
                        column: destination_column + offset,
                    };
                    let existing = table.cell(address);
                    if !overwrite && existing.is_some_and(cell_has_content) {
                        return Err(Message::new("status-edit-overwrite-required")
                            .with_text("cell", crate::xlsx_adapter::cell_reference(address)));
                    }
                    let mut cell = source_cell.cloned().unwrap_or_else(|| Cell::text(""));
                    cell.input = CellInput::Text((*part).to_owned());
                    cell.imported_result = None;
                    cell.calculation = None;
                    row_outputs.push((address, Some(cell)));
                }
            }
            expanded_dimensions.insert(*table_id, (table.row_count(), maximum_column));
            for (address, cell) in row_outputs {
                changes.insert((*table_id, address), cell);
            }
        }
        DataEditAction::JoinColumns {
            table_id,
            range,
            delimiter,
            destination_column,
            overwrite,
        } => {
            validate_delimiter(delimiter)?;
            let table = find_table(&document, *table_id)?;
            let (start, end) = range.normalized();
            if end.column <= start.column || end.row >= table.row_count() {
                return Err(Message::new("status-join-range-invalid"));
            }
            if (start.column..=end.column).contains(destination_column) {
                return Err(Message::new("status-join-destination-inside-source"));
            }
            if *destination_column >= crate::document::MAX_COLUMNS {
                return Err(Message::new("status-join-destination-out-of-bounds"));
            }
            let mut row_outputs = Vec::new();
            for row in start.row..=end.row {
                check_cancelled(cancelled)?;
                let address = CellAddress {
                    row,
                    column: *destination_column,
                };
                let existing = table.cell(address);
                if !overwrite && existing.is_some_and(cell_has_content) {
                    return Err(Message::new("status-edit-overwrite-required")
                        .with_text("cell", crate::xlsx_adapter::cell_reference(address)));
                }
                let mut values = Vec::new();
                for column in start.column..=end.column {
                    let source_address = CellAddress { row, column };
                    if let Some(cell) = table.cell(source_address) {
                        if is_formula_cell(table, source_address, cell) {
                            return Err(Message::new("status-join-formula-cell-unsupported")
                                .with_text(
                                    "cell",
                                    crate::xlsx_adapter::cell_reference(source_address),
                                ));
                        }
                        if !matches!(cell.input, CellInput::Empty) {
                            let value = cell.input.source_text();
                            if !value.is_empty() {
                                values.push(value);
                            }
                        }
                    }
                }
                if values.is_empty() {
                    continue;
                }
                row_outputs.push((address, Some(Cell::text(values.join(delimiter)))));
            }
            let maximum_column = table
                .column_count()
                .max(destination_column.saturating_add(1));
            expanded_dimensions.insert(*table_id, (table.row_count(), maximum_column));
            for (address, cell) in row_outputs {
                changes.insert((*table_id, address), cell);
            }
        }
        DataEditAction::Transpose {
            table_id,
            range,
            overwrite,
        } => {
            let table = find_table(&document, *table_id)?;
            let (start, end) = range.normalized();
            if end.row >= table.row_count() || end.column >= table.column_count() {
                return Err(Message::new("status-transpose-range-out-of-bounds"));
            }
            let height = end.row - start.row + 1;
            let width = end.column - start.column + 1;
            let area = height.saturating_mul(width);
            if area > crate::clipboard::MAX_CLIPBOARD_CELLS {
                return Err(Message::new("status-transpose-cell-limit")
                    .with_number("limit", crate::clipboard::MAX_CLIPBOARD_CELLS as i64));
            }
            let target_end = CellAddress {
                row: start.row.saturating_add(width).saturating_sub(1),
                column: start.column.saturating_add(height).saturating_sub(1),
            };
            if target_end.row >= crate::document::MAX_ROWS
                || target_end.column >= crate::document::MAX_COLUMNS
            {
                return Err(Message::new("status-transpose-destination-out-of-bounds"));
            }
            let source_range = crate::document::MergedRange { start, end };
            let target_range = crate::document::MergedRange {
                start,
                end: target_end,
            };
            let source_cells = table
                .stored_cells_in_range(*range)
                .map(|(address, cell)| (address, cell.clone()))
                .collect::<BTreeMap<_, _>>();
            if let Some((address, _)) = source_cells
                .iter()
                .find(|(address, cell)| is_formula_cell(table, **address, cell))
            {
                return Err(Message::new("status-transpose-formula-cell-unsupported")
                    .with_text("cell", crate::xlsx_adapter::cell_reference(*address)));
            }
            let mut target_cells = BTreeSet::new();
            for row in start.row..=target_end.row {
                for column in start.column..=target_end.column {
                    let address = CellAddress { row, column };
                    target_cells.insert(address);
                    if !overwrite
                        && !source_range.contains(address)
                        && table.cell(address).is_some_and(cell_has_content)
                    {
                        return Err(Message::new("status-edit-overwrite-required")
                            .with_text("cell", crate::xlsx_adapter::cell_reference(address)));
                    }
                }
            }
            let mut affected = target_cells;
            for row in start.row..=end.row {
                for column in start.column..=end.column {
                    affected.insert(CellAddress { row, column });
                }
            }
            for address in affected {
                check_cancelled(cancelled)?;
                let (value, is_string) = if target_range.contains(address) {
                    let source_address = CellAddress {
                        row: start.row + address.column - start.column,
                        column: start.column + address.row - start.row,
                    };
                    (
                        source_cells.get(&source_address).cloned(),
                        table.is_string_cell(source_address),
                    )
                } else {
                    (None, false)
                };
                if table.cell(address).cloned() != value
                    || table.is_string_cell(address) != is_string
                {
                    changes.insert((*table_id, address), value);
                }
                if table.is_string_cell(address) != is_string {
                    string_cell_updates.insert((*table_id, address), is_string);
                }
            }
            expanded_dimensions.insert(
                *table_id,
                (
                    table.row_count().max(target_end.row.saturating_add(1)),
                    table
                        .column_count()
                        .max(target_end.column.saturating_add(1)),
                ),
            );
        }
        DataEditAction::SortRows { .. } => unreachable!("handled before cell edits"),
        DataEditAction::Duplicates { .. } => unreachable!("handled before cell edits"),
    }
    for ((table_id, address), is_string) in string_cell_updates {
        let table = document
            .tables
            .iter_mut()
            .find(|table| table.id == table_id)
            .ok_or_else(|| {
                Message::new("status-table-string-state-unavailable")
                    .with_number("table_id", table_id as i64)
            })?;
        table.set_string_cell(address, is_string);
    }
    if changes.is_empty() {
        return Err(Message::new("status-data-edit-no-changes"));
    }
    let mut preview = Vec::new();
    let changed_cells = changes.len();
    for ((table_id, address), after) in &changes {
        let table = find_table(&document, *table_id)?;
        let before = table
            .cell(*address)
            .map_or_else(String::new, |cell| cell.input.source_text());
        let after_text = after
            .as_ref()
            .map_or_else(String::new, |cell| cell.input.source_text());
        if preview.len() < 500 {
            preview.push(DataCellDiff {
                table_id: *table_id,
                address: *address,
                before,
                after: after_text,
            });
        }
    }
    for (table_id, (rows, columns)) in expanded_dimensions {
        let table = document
            .tables
            .iter_mut()
            .find(|table| table.id == table_id)
            .ok_or_else(|| {
                Message::new("status-table-id-unavailable").with_number("table_id", table_id as i64)
            })?;
        table.set_dimensions(rows, columns);
    }
    let changes = changes
        .into_iter()
        .map(|((table_id, address), cell)| (table_id, address, cell))
        .collect::<Vec<_>>();
    let prepared = prepare_cell_changes(
        &request.structure,
        document,
        source_before,
        overlay_before,
        changes,
        cancelled,
    )?;
    Ok(PreparedDataEdit {
        document_id: request.structure.document_id,
        document_revision: request.structure.revision,
        calculation_revision: request.structure.calculation_revision,
        changed_cells,
        changed_rows: None,
        duplicate_count: 0,
        duplicate_groups: 0,
        duplicate_rows: Vec::new(),
        preview,
        preview_truncated: changed_cells > 500,
        prepared: Some(prepared),
    })
}
