use super::*;

/// Leaves a file source at its current shape when an edit is beyond its data extent.
pub(crate) fn structure_operation_outside_extent(
    operation: &StructureOperation,
    rows: usize,
    columns: usize,
) -> bool {
    match operation {
        StructureOperation::Insert { axis, index, .. }
        | StructureOperation::Delete { axis, index, .. }
        | StructureOperation::Duplicate { axis, index, .. } => match axis {
            StructureAxis::Rows => *index >= rows,
            StructureAxis::Columns => *index >= columns,
        },
        StructureOperation::Move {
            axis,
            index,
            destination,
            ..
        } => match axis {
            StructureAxis::Rows => *index >= rows && *destination >= rows,
            StructureAxis::Columns => *index >= columns && *destination >= columns,
        },
        _ => false,
    }
}

/// Keeps source shape while applying formulas and remapping temporary alignment.
pub(crate) fn apply_outside_extent_structure(
    source: &SourceDocument,
    formulas: &BTreeMap<(TableId, CellAddress), String>,
    document: &Document,
    operation: &StructureOperation,
    plan: &StructurePlan,
) -> Result<SourceDocument, Message> {
    match source {
        SourceDocument::Csv(source) => {
            let changes = formulas
                .iter()
                .map(|((_, address), formula)| CsvCellChange {
                    row: address.row,
                    column: address.column,
                    value: Some(formula.clone()),
                })
                .collect::<Vec<_>>();
            Ok(SourceDocument::Csv(Arc::new(
                source
                    .apply_changes(&changes)
                    .map_err(crate::i18n::csv_error_message)?,
            )))
        }
        SourceDocument::Markdown(source) => {
            let table_index = document
                .tables
                .iter()
                .position(|table| table.id == operation_table_id(operation))
                .ok_or_else(|| {
                    Message::new("status-table-id-unavailable")
                        .with_number("table_id", operation_table_id(operation) as i64)
                })?;
            let changes = formulas
                .iter()
                .filter_map(|((table_id, address), formula)| {
                    document
                        .tables
                        .iter()
                        .position(|table| table.id == *table_id)
                        .map(|table| crate::markdown_adapter::MarkdownCellChange {
                            table,
                            row: address.row,
                            column: address.column,
                            value: formula.clone(),
                        })
                })
                .collect::<Vec<_>>();
            let mut updated = source
                .apply_changes(&changes)
                .map_err(crate::i18n::markdown_error_message)?;
            source
                .carry_structure_alignments(
                    table_index,
                    &mut updated,
                    operation,
                    &plan.column_sources,
                )
                .map_err(crate::i18n::markdown_error_message)?;
            Ok(SourceDocument::Markdown(Arc::new(updated)))
        }
    }
}
