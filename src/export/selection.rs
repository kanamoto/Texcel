use super::*;

// Resolve the requested table selection in document order.
pub(super) fn selected_tables(
    document: &Document,
    request: &ExportRequest,
    scope: ExportScope,
    cancelled: Option<&AtomicBool>,
) -> Result<Vec<ExportTable>, crate::i18n::Message> {
    let wanted = match scope {
        ExportScope::AllTables => None,
        ExportScope::Selected(id) => Some(id),
    };
    let mut result = Vec::new();
    for (order, table) in document.tables.iter().enumerate() {
        check_cancelled(cancelled)?;
        if wanted.is_some_and(|id| id != table.id) {
            continue;
        }
        let mut last_row = 0_usize;
        let mut last_column = 0_usize;
        let mut string_cells = BTreeSet::new();
        for (address, _) in table.stored_cells() {
            check_cancelled(cancelled)?;
            last_row = last_row.max(address.row + 1);
            last_column = last_column.max(address.column + 1);
            if table.is_string_cell(address) {
                string_cells.insert(address);
            }
        }
        let source_shape = match &request.source {
            Some(ExportSource::Csv(source)) if order == 0 => (
                source.records.len(),
                source
                    .records
                    .iter()
                    .map(|record| record.fields.len())
                    .max()
                    .unwrap_or(0),
            ),
            Some(ExportSource::Markdown(source)) => {
                source.tables.get(order).map_or((0, 0), |source_table| {
                    (
                        source_table.rows.len(),
                        source_table.rows.iter().map(Vec::len).max().unwrap_or(0),
                    )
                })
            }
            Some(ExportSource::Xlsx(source)) => source
                .sheets
                .iter()
                .find(|sheet| sheet.table_id == table.id)
                .map_or((0, 0), xlsx_cell_dimensions),
            Some(ExportSource::Html(source)) => {
                source.tables.get(order).map_or((0, 0), |source_table| {
                    (
                        source_table.rows.len(),
                        source_table.rows.iter().map(Vec::len).max().unwrap_or(0),
                    )
                })
            }
            _ => (0, 0),
        };
        let layout_shape = table
            .layout
            .merges()
            .fold((0, 0), |(rows, columns), merge| {
                (
                    rows.max(merge.end.row + 1),
                    columns.max(merge.end.column + 1),
                )
            });
        let changed_shape = request
            .changes
            .iter()
            .filter(|(table_id, _, _)| *table_id == table.id)
            .fold((0, 0), |(rows, columns), (_, address, _)| {
                (rows.max(address.row + 1), columns.max(address.column + 1))
            });
        let row_count = last_row
            .max(source_shape.0)
            .max(layout_shape.0)
            .max(changed_shape.0)
            .max(1);
        let column_count = last_column
            .max(source_shape.1)
            .max(layout_shape.1)
            .max(changed_shape.1)
            .max(1);
        if row_count.saturating_mul(column_count) > 500_000 {
            return Err(crate::i18n::Message::new("status-export-table-area-limit")
                .with_text("table", &table.name)
                .with_number("limit", 500_000));
        }
        let mut rows = vec![vec![Cell::text(""); column_count]; row_count];
        for (address, cell) in table.stored_cells() {
            check_cancelled(cancelled)?;
            if table
                .layout
                .merge_at(address)
                .is_some_and(|merge| merge.start != address)
            {
                continue;
            }
            if let Some(row) = rows.get_mut(address.row)
                && let Some(slot) = row.get_mut(address.column)
            {
                *slot = cell.clone();
            }
        }
        // Keep only formats whose cell was placed in this export, not covered merge cells.
        let formats = table
            .formats()
            .filter(|(address, _)| {
                address.row < row_count
                    && address.column < column_count
                    && table.cell(*address).is_some()
                    && !table
                        .layout
                        .merge_at(*address)
                        .is_some_and(|merge| merge.start != *address)
            })
            .map(|(address, format)| (address, format.clone()))
            .collect();
        result.push(ExportTable {
            order,
            id: table.id,
            name: table.name.clone(),
            rows,
            string_cells,
            formats,
        });
    }
    if result.is_empty() {
        return Err(crate::i18n::Message::new("status-export-no-tables"));
    }
    if wanted.is_some() && result.len() != 1 {
        return Err(crate::i18n::Message::new(
            "status-export-selected-table-missing",
        ));
    }
    Ok(result)
}
