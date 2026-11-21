//!  Worker-side operations for data sort.

use super::*;
use crate::i18n::Message;

#[derive(Clone)]
pub(crate) enum SortAtom {
    Text(String),
    Number(f64),
    Date(i64),
    Invalid,
    Blank,
}

// Prepare reordered rows from the selected sort keys.
pub(crate) fn prepare_sorted_rows(
    request: &DataEditRequest,
    document: &Document,
    source: Option<&SourceDocument>,
    table_id: TableId,
    keys: &[SortKey],
    header_row: Option<usize>,
    cancelled: &AtomicBool,
) -> Result<PreparedDataEdit, Message> {
    if keys.is_empty() || keys.len() > 8 {
        return Err(Message::new("status-sort-key-count-invalid"));
    }
    let table = find_table(document, table_id)?;
    let markdown_table_index = source
        .filter(|source| matches!(source, SourceDocument::Markdown(_)))
        .and_then(|_| {
            document
                .tables
                .iter()
                .position(|candidate| candidate.id == table_id)
        });
    let (row_count, column_count) =
        used_dimensions(source, table_id, &request.structure.tables, table);
    if let Some(header) = header_row
        && header >= row_count
    {
        return Err(Message::new("status-sort-header-out-of-range"));
    }
    if keys.iter().any(|key| key.column >= column_count) {
        return Err(Message::new("status-sort-column-out-of-range"));
    }
    let formulas = table
        .stored_cells()
        .filter_map(|(address, cell)| {
            (Some(address.row) != header_row
                && keys.iter().any(|key| key.column == address.column)
                && is_formula_cell(table, address, cell))
            .then_some(address)
        })
        .collect::<BTreeSet<_>>();
    let formula_results =
        if request.formula_results_revision == Some(request.structure.calculation_revision) {
            request.formula_results.as_deref()
        } else {
            None
        };
    let mut formula_values = BTreeMap::new();
    if !formulas.is_empty() {
        let Some(formula_results) = formula_results else {
            let address = *formulas.iter().next().expect("not empty");
            return Err(Message::new("status-sort-formula-result-stale")
                .with_text("cell", crate::xlsx_adapter::cell_reference(address)));
        };
        for address in &formulas {
            if let Some((status, value)) = formula_results.get(table_id, *address) {
                formula_values.insert(*address, (status, value));
            }
        }
        for address in &formulas {
            if !formula_values.get(address).is_some_and(|(status, value)| {
                matches!(
                    (status, value),
                    (crate::document::CalcStatus::Fresh, Some(_))
                        | (
                            crate::document::CalcStatus::Error,
                            Some(crate::document::CalcValue::Error(_)),
                        )
                )
            }) {
                return Err(Message::new("status-sort-formula-result-stale")
                    .with_text("cell", crate::xlsx_adapter::cell_reference(*address)));
            }
        }
    }

    let mut sort_values = Vec::with_capacity(keys.len());
    for key in keys {
        check_cancelled(cancelled)?;
        let mut values = Vec::with_capacity(row_count);
        for row in 0..row_count {
            if row % 1024 == 0 {
                check_cancelled(cancelled)?;
            }
            if Some(row) == header_row {
                values.push(SortAtom::Blank);
                continue;
            }
            let address = CellAddress {
                row,
                column: key.column,
            };
            let cell = table.cell(address);
            let atom = if let Some(cell) = cell {
                let is_formula = is_formula_cell(table, address, cell);
                if is_formula {
                    let value = formula_values
                        .get(&address)
                        .and_then(|(_, value)| *value)
                        .ok_or_else(|| {
                            Message::new("status-sort-formula-result-missing")
                                .with_text("cell", crate::xlsx_adapter::cell_reference(address))
                        })?;
                    calc_value_sort_atom(value, key.kind)
                } else {
                    match &cell.input {
                        CellInput::Empty => SortAtom::Blank,
                        CellInput::Text(text) if text.is_empty() => SortAtom::Blank,
                        CellInput::Formula(_) => unreachable!("handled above"),
                        CellInput::Error(_) => SortAtom::Invalid,
                        CellInput::Text(text) => {
                            let projected_text = if key.kind == SortValueKind::Number {
                                markdown_table_index.map_or_else(
                                    || text.clone(),
                                    |table_index| {
                                        let projected = crate::calc::calculation_cell_for_document(
                                            document,
                                            table_index,
                                            address,
                                            cell,
                                        );
                                        match projected.input {
                                            CellInput::Text(text) => {
                                                crate::document::literal_text_value(&text)
                                                    .to_owned()
                                            }
                                            _ => text.clone(),
                                        }
                                    },
                                )
                            } else {
                                text.clone()
                            };
                            sort_text_atom(&projected_text, key.kind)
                        }
                        CellInput::Number(number) => sort_number_atom(*number, key.kind),
                        CellInput::Boolean(value) => {
                            sort_text_atom(if *value { "TRUE" } else { "FALSE" }, key.kind)
                        }
                    }
                }
            } else {
                SortAtom::Blank
            };
            values.push(atom);
        }
        sort_values.push(values);
    }
    let mut order = (0..row_count).collect::<Vec<_>>();
    let mut sortable = (0..row_count)
        .filter(|row| Some(*row) != header_row)
        .collect::<Vec<_>>();
    sortable.sort_by(|left, right| {
        for (index, key) in keys.iter().enumerate() {
            let ordering = compare_sort_atoms(
                &sort_values[index][*left],
                &sort_values[index][*right],
                key.descending,
            );
            if !ordering.is_eq() {
                return ordering;
            }
        }
        std::cmp::Ordering::Equal
    });
    let mut sorted_rows = sortable.into_iter();
    for (output, source) in order.iter_mut().enumerate() {
        if Some(output) != header_row {
            *source = sorted_rows.next().expect("all non-header rows were sorted");
        }
    }
    if order.iter().enumerate().all(|(row, source)| row == *source) {
        return Err(Message::new("status-sort-unchanged"));
    }
    let operation = StructureOperation::ReorderRows {
        table_id,
        source_row_count: row_count,
        row_sources: order.clone(),
    };
    let mut structure = request.structure.clone();
    structure.operation = Some(operation);
    let prepared = prepare_structure(&structure, cancelled)?;
    let before_table = find_table(document, table_id)?;
    let after_table = prepared
        .tables
        .iter()
        .find(|table| table.id == table_id)
        .ok_or_else(|| Message::new("status-sort-result-missing"))?;
    let mut addresses = before_table
        .stored_cells()
        .map(|(address, _)| address)
        .chain(after_table.stored_cells().map(|(address, _)| address))
        .collect::<BTreeSet<_>>();
    let mut preview = Vec::new();
    let mut changed_cells = 0;
    while let Some(address) = addresses.pop_first() {
        check_cancelled(cancelled)?;
        let before = before_table.cell(address);
        let after = after_table.cell(address);
        if before == after {
            continue;
        }
        changed_cells += 1;
        if preview.len() < 500 {
            preview.push(DataCellDiff {
                table_id,
                address,
                before: before.map_or_else(String::new, |cell| cell.input.source_text()),
                after: after.map_or_else(String::new, |cell| cell.input.source_text()),
            });
        }
    }
    let changed_rows = order
        .iter()
        .enumerate()
        .filter(|(output, source)| *output != **source)
        .count();
    Ok(PreparedDataEdit {
        document_id: request.structure.document_id,
        document_revision: request.structure.revision,
        calculation_revision: request.structure.calculation_revision,
        changed_cells,
        changed_rows: Some(changed_rows),
        duplicate_count: 0,
        duplicate_groups: 0,
        duplicate_rows: Vec::new(),
        preview,
        preview_truncated: changed_cells > 500,
        prepared: Some(prepared),
    })
}
