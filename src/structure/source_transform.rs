//!  Worker-side operations for source transform.

use super::*;
mod outside_extent;
mod snapshots;

pub(crate) use outside_extent::{
    apply_outside_extent_structure, structure_operation_outside_extent,
};
pub(crate) use snapshots::{
    apply_changed_cells_with_cancel, apply_requested_text_overlay,
    apply_requested_text_overlay_with_cancel, apply_table_metadata,
    apply_table_metadata_with_cancel, collect_preserved_text_overlay,
    collect_structure_text_overlay,
};

/// Transforms source metadata after a structural operation.
pub(crate) fn transform_source_after(
    source: &SourceDocument,
    operation: &StructureOperation,
    plan: &StructurePlan,
    formulas: &BTreeMap<(TableId, CellAddress), String>,
    document: &Document,
) -> Result<SourceDocument, Message> {
    if let StructureOperation::SetTextCells {
        table_id, changes, ..
    } = operation
    {
        let table = document
            .tables
            .iter()
            .find(|table| table.id == *table_id)
            .ok_or_else(|| {
                Message::new("status-table-id-unavailable")
                    .with_number("table_id", *table_id as i64)
            })?;
        return match source {
            SourceDocument::Csv(source) if document.tables.len() == 1 => {
                let changes = changes
                    .iter()
                    .map(|(address, cell)| crate::csv_adapter::CsvCellChange {
                        row: address.row,
                        column: address.column,
                        value: cell.as_ref().map(|cell| {
                            cell.source_text_for_text_file(table.is_string_cell(*address))
                        }),
                    })
                    .collect::<Vec<_>>();
                let updated = source
                    .apply_changes(&changes)
                    .map_err(crate::i18n::csv_error_message)?;
                Ok(SourceDocument::Csv(Arc::new(updated)))
            }
            SourceDocument::Csv(_) => Err(Message::new("status-table-count-changed-after-parse")),
            SourceDocument::Markdown(source) => {
                let table_index = source_table_index(source, *table_id, document)?;
                let changes = changes
                    .iter()
                    .map(
                        |(address, cell)| crate::markdown_adapter::MarkdownCellChange {
                            table: table_index,
                            row: address.row,
                            column: address.column,
                            value: cell.as_ref().map_or_else(String::new, |cell| {
                                cell.source_text_for_text_file(table.is_string_cell(*address))
                            }),
                        },
                    )
                    .collect::<Vec<_>>();
                let updated = source
                    .apply_changes(&changes)
                    .map_err(crate::i18n::markdown_error_message)?;
                Ok(SourceDocument::Markdown(Arc::new(updated)))
            }
        };
    }
    if let StructureOperation::SetTextCell {
        table_id,
        address,
        cell,
    } = operation
    {
        let table = document
            .tables
            .iter()
            .find(|table| table.id == *table_id)
            .ok_or_else(|| {
                Message::new("status-table-id-unavailable")
                    .with_number("table_id", *table_id as i64)
            })?;
        let value = cell.source_text_for_text_file(table.is_string_cell(*address));
        return match source {
            SourceDocument::Csv(source) if document.tables.len() == 1 => {
                let updated = source
                    .apply_changes(&[CsvCellChange {
                        row: address.row,
                        column: address.column,
                        value: Some(value),
                    }])
                    .map_err(crate::i18n::csv_error_message)?;
                Ok(SourceDocument::Csv(Arc::new(updated)))
            }
            SourceDocument::Csv(_) => Err(Message::new("status-table-count-changed-after-parse")),
            SourceDocument::Markdown(source) => {
                let table = source_table_index(source, *table_id, document)?;
                let updated = source
                    .apply_changes(&[MarkdownCellChange {
                        table,
                        row: address.row,
                        column: address.column,
                        value,
                    }])
                    .map_err(crate::i18n::markdown_error_message)?;
                Ok(SourceDocument::Markdown(Arc::new(updated)))
            }
        };
    }
    let table_id = operation_table_id(operation);
    match source {
        SourceDocument::Csv(source) => {
            if let StructureOperation::Fill {
                source: fill_source,
                target,
                ..
            } = operation
            {
                let table = document
                    .tables
                    .iter()
                    .find(|table| table.id == table_id)
                    .ok_or_else(|| Message::new("status-csv-filtered-table-missing"))?;
                let (start, end) = target.normalized();
                let (seed_start, seed_end) = fill_source.normalized();
                let changes = (start.row..=end.row)
                    .flat_map(|row| (start.column..=end.column).map(move |column| (row, column)))
                    .filter(|(row, column)| {
                        *row < seed_start.row
                            || *row > seed_end.row
                            || *column < seed_start.column
                            || *column > seed_end.column
                    })
                    .map(|(row, column)| CsvCellChange {
                        row,
                        column,
                        value: table.cell(CellAddress { row, column }).map(|cell| {
                            cell.source_text_for_text_file(
                                table.is_string_cell(CellAddress { row, column }),
                            )
                        }),
                    })
                    .collect::<Vec<_>>();
                let updated = source
                    .apply_changes(&changes)
                    .map_err(crate::i18n::csv_error_message)?;
                Ok(SourceDocument::Csv(Arc::new(updated)))
            } else {
                let updated = source
                    .apply_structure(plan, formulas, table_id)
                    .map_err(crate::i18n::csv_error_message)?;
                Ok(SourceDocument::Csv(Arc::new(updated)))
            }
        }
        SourceDocument::Markdown(source) => {
            let table_index = source_table_index(source, table_id, document)?;
            let mut updated = source
                .apply_structure(table_index, table_id, plan, formulas)
                .map_err(crate::i18n::markdown_error_message)?;
            if let StructureOperation::Fill { source, target, .. } = operation {
                let table = document
                    .tables
                    .iter()
                    .find(|table| table.id == table_id)
                    .ok_or_else(|| Message::new("status-markdown-filtered-table-missing"))?;
                let (start, end) = target.normalized();
                let (seed_start, seed_end) = source.normalized();
                let changes = (start.row..=end.row)
                    .flat_map(|row| (start.column..=end.column).map(move |column| (row, column)))
                    .filter(|(row, column)| {
                        *row < seed_start.row
                            || *row > seed_end.row
                            || *column < seed_start.column
                            || *column > seed_end.column
                    })
                    .map(|(row, column)| MarkdownCellChange {
                        table: table_index,
                        row,
                        column,
                        value: table.cell(CellAddress { row, column }).map_or_else(
                            String::new,
                            |cell| {
                                cell.source_text_for_text_file(
                                    table.is_string_cell(CellAddress { row, column }),
                                )
                            },
                        ),
                    })
                    .collect::<Vec<_>>();
                updated = updated
                    .apply_changes(&changes)
                    .map_err(crate::i18n::markdown_error_message)?;
            }
            Ok(SourceDocument::Markdown(Arc::new(updated)))
        }
    }
}

/// Finds the source index for a table ID.
pub(crate) fn source_table_index(
    source: &MarkdownDocument,
    table_id: TableId,
    document: &Document,
) -> Result<usize, Message> {
    let index = document
        .tables
        .iter()
        .position(|table| table.id == table_id)
        .ok_or_else(|| {
            Message::new("status-markdown-table-id-missing")
                .with_number("table_id", table_id as i64)
        })?;
    if source.tables.get(index).is_none() {
        return Err(Message::new("status-markdown-source-table-missing")
            .with_number("table", index as i64 + 1));
    }
    Ok(index)
}

/// Returns the table affected by a structure operation.
pub(crate) fn operation_table_id(operation: &StructureOperation) -> TableId {
    match operation {
        StructureOperation::Insert { table_id, .. }
        | StructureOperation::Delete { table_id, .. }
        | StructureOperation::Duplicate { table_id, .. }
        | StructureOperation::Move { table_id, .. }
        | StructureOperation::ReorderRows { table_id, .. }
        | StructureOperation::Fill { table_id, .. }
        | StructureOperation::SetTextCell { table_id, .. }
        | StructureOperation::SetTextCells { table_id, .. } => *table_id,
    }
}

/// Returns the source dimensions for one table.
pub(crate) fn source_extent(
    source: &SourceDocument,
    table_id: TableId,
    tables: &[StructureTableMeta],
) -> (usize, usize) {
    match source {
        SourceDocument::Csv(source) => (
            source.records.len(),
            source
                .records
                .iter()
                .map(|record| record.fields.len())
                .max()
                .unwrap_or(0),
        ),
        SourceDocument::Markdown(source) => tables
            .iter()
            .position(|table| table.id == table_id)
            .and_then(|index| source.source_extent(index))
            .unwrap_or((0, 0)),
    }
}

/// Returns the dimensions occupied by stored values.
pub(crate) fn used_dimensions(
    source: Option<&SourceDocument>,
    table_id: TableId,
    tables: &[StructureTableMeta],
    table: &crate::document::Table,
) -> (usize, usize) {
    let source_dimensions = source.map_or_else(
        || {
            (
                if table.row_count() < crate::document::MAX_ROWS {
                    table.row_count()
                } else {
                    0
                },
                if table.column_count() < crate::document::MAX_COLUMNS {
                    table.column_count()
                } else {
                    0
                },
            )
        },
        |source| source_extent(source, table_id, tables),
    );
    (
        source_dimensions.0.max(last_cell_row(table)).max(1),
        source_dimensions.1.max(last_cell_column(table)).max(1),
    )
}

/// Returns the rows and columns touched by an operation.
pub(crate) fn operation_extent(operation: &StructureOperation) -> (usize, usize) {
    match operation {
        StructureOperation::Insert {
            axis, index, count, ..
        }
        | StructureOperation::Delete {
            axis, index, count, ..
        }
        | StructureOperation::Duplicate {
            axis, index, count, ..
        } => match axis {
            StructureAxis::Rows => (index.saturating_add(*count), 1),
            StructureAxis::Columns => (1, index.saturating_add(*count)),
        },
        StructureOperation::Move {
            axis,
            index,
            count,
            destination,
            ..
        } => match axis {
            StructureAxis::Rows => (
                index
                    .saturating_add(*count)
                    .max(destination.saturating_add(*count)),
                1,
            ),
            StructureAxis::Columns => (
                1,
                index
                    .saturating_add(*count)
                    .max(destination.saturating_add(*count)),
            ),
        },
        StructureOperation::ReorderRows { row_sources, .. } => (row_sources.len(), 1),
        StructureOperation::Fill { source, target, .. } => {
            let (_, source_end) = source.normalized();
            let (_, target_end) = target.normalized();
            (
                source_end.row.max(target_end.row) + 1,
                source_end.column.max(target_end.column) + 1,
            )
        }
        StructureOperation::SetTextCell { .. } => (0, 0),
        StructureOperation::SetTextCells { changes, .. } => {
            changes
                .iter()
                .fold((0, 0), |(rows, columns), (address, _)| {
                    (
                        rows.max(address.row.saturating_add(1)),
                        columns.max(address.column.saturating_add(1)),
                    )
                })
        }
    }
}

/// Finds the last row containing stored cell data.
pub(crate) fn last_cell_row(table: &crate::document::Table) -> usize {
    table
        .stored_cells()
        .map(|(address, _)| address.row + 1)
        .max()
        .unwrap_or(0)
}

/// Finds the last column containing stored cell data.
pub(crate) fn last_cell_column(table: &crate::document::Table) -> usize {
    table
        .stored_cells()
        .map(|(address, _)| address.column + 1)
        .max()
        .unwrap_or(0)
}
