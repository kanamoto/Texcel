//! Applies bounded source overlays and frozen worker snapshots.

use super::super::*;
use std::sync::atomic::{AtomicBool, Ordering};

/// Applies requested text values to a structural source snapshot.
pub(crate) fn apply_requested_text_overlay(
    document: &mut Document,
    base_revision: u64,
    current_revision: u64,
    existing: &[TextOverlayCell],
    changes: &[(TableId, CellAddress, Option<crate::document::Cell>)],
    enabled: bool,
) -> Result<Arc<Vec<TextOverlayCell>>, Message> {
    apply_requested_text_overlay_checked(
        document,
        base_revision,
        current_revision,
        existing,
        changes,
        enabled,
        || Ok(()),
    )
}

/// Applies text overlays while checking worker cancellation during large changes.
pub(crate) fn apply_requested_text_overlay_with_cancel(
    document: &mut Document,
    base_revision: u64,
    current_revision: u64,
    existing: &[TextOverlayCell],
    changes: &[(TableId, CellAddress, Option<crate::document::Cell>)],
    enabled: bool,
    cancelled: &AtomicBool,
) -> Result<Arc<Vec<TextOverlayCell>>, Message> {
    apply_requested_text_overlay_checked(
        document,
        base_revision,
        current_revision,
        existing,
        changes,
        enabled,
        || check_cancelled(cancelled),
    )
}

// Applies text changes only when the source revision still matches.
fn apply_requested_text_overlay_checked(
    document: &mut Document,
    base_revision: u64,
    current_revision: u64,
    existing: &[TextOverlayCell],
    changes: &[(TableId, CellAddress, Option<crate::document::Cell>)],
    enabled: bool,
    mut check: impl FnMut() -> Result<(), Message>,
) -> Result<Arc<Vec<TextOverlayCell>>, Message> {
    check()?;
    if base_revision > current_revision {
        return Err(Message::new("status-structure-overlay-from-future"));
    }
    if !enabled {
        return Ok(Arc::new(Vec::new()));
    }
    let mut overlay = BTreeMap::new();
    for (table_id, address, cell) in existing {
        check()?;
        overlay.insert((*table_id, *address), cell.clone());
    }
    for (table_id, address, cell) in changes {
        check()?;
        match cell {
            Some(cell) if crate::document::cell_needs_text_overlay(cell) => {
                overlay.insert((*table_id, *address), cell.clone());
            }
            _ => {
                overlay.remove(&(*table_id, *address));
            }
        }
    }
    let mut values = Vec::with_capacity(overlay.len());
    for ((table_id, address), cell) in overlay {
        check()?;
        let table = document
            .tables
            .iter_mut()
            .find(|table| table.id == table_id)
            .ok_or_else(|| {
                Message::new("status-table-id-unavailable").with_number("table_id", table_id as i64)
            })?;
        table.apply_snapshot_cell(address, Some(cell.clone()));
        values.push((table_id, address, cell));
    }
    Ok(Arc::new(values))
}

/// Collects text values that must survive a structural edit.
pub(crate) fn collect_text_overlay(
    tables: &[crate::document::Table],
    enabled: bool,
) -> Arc<Vec<TextOverlayCell>> {
    if !enabled {
        return Arc::new(Vec::new());
    }
    Arc::new(
        tables
            .iter()
            .flat_map(|table| {
                table
                    .text_overlay_cells()
                    .map(|(address, cell)| (table.id, address, cell))
            })
            .collect(),
    )
}

/// Keeps sparse source cells when a worker changes cell values without moving their addresses.
pub(crate) fn collect_preserved_text_overlay(
    tables: &[crate::document::Table],
    enabled: bool,
    previous: &[TextOverlayCell],
) -> Arc<Vec<TextOverlayCell>> {
    if !enabled {
        return Arc::new(Vec::new());
    }
    let mut cells = collect_text_overlay(tables, true)
        .iter()
        .map(|(table, address, cell)| ((*table, *address), cell.clone()))
        .collect::<BTreeMap<_, _>>();
    for (table_id, address, _) in previous {
        if let Some(cell) = tables
            .iter()
            .find(|table| table.id == *table_id)
            .and_then(|table| table.cell(*address))
        {
            cells.insert((*table_id, *address), cell.clone());
        }
    }
    Arc::new(
        cells
            .into_iter()
            .map(|((table_id, address), cell)| (table_id, address, cell))
            .collect(),
    )
}

/// Keeps sparse source cells over the same worker-prepared structure change.
pub(crate) fn collect_structure_text_overlay(
    tables: &[crate::document::Table],
    enabled: bool,
    previous: &[TextOverlayCell],
    plan: &StructurePlan,
) -> Arc<Vec<TextOverlayCell>> {
    if !enabled {
        return Arc::new(Vec::new());
    }
    let mut cells = collect_text_overlay(tables, true)
        .iter()
        .map(|(table, address, cell)| ((*table, *address), cell.clone()))
        .collect::<BTreeMap<_, _>>();
    let operation_table = operation_table_id(&plan.operation);
    for (table_id, address, _) in previous {
        let outputs = if *table_id == operation_table {
            plan.output_positions(*address)
        } else {
            vec![*address]
        };
        let Some(table) = tables.iter().find(|table| table.id == *table_id) else {
            continue;
        };
        for output in outputs {
            if let Some(cell) = table.cell(output) {
                cells.insert((*table_id, output), cell.clone());
            }
        }
    }
    if let StructureOperation::SetTextCell {
        table_id, address, ..
    } = &plan.operation
        && let Some(cell) = tables
            .iter()
            .find(|table| table.id == *table_id)
            .and_then(|table| table.cell(*address))
    {
        cells.insert((*table_id, *address), cell.clone());
    }
    if let StructureOperation::SetTextCells {
        table_id, changes, ..
    } = &plan.operation
        && let Some(table) = tables.iter().find(|table| table.id == *table_id)
    {
        for (address, _) in changes {
            if let Some(cell) = table.cell(*address) {
                cells.insert((*table_id, *address), cell.clone());
            } else {
                cells.remove(&(*table_id, *address));
            }
        }
    }
    Arc::new(
        cells
            .into_iter()
            .map(|((table_id, address), cell)| (table_id, address, cell))
            .collect(),
    )
}

/// Applies table names and order from the current document.
pub(crate) fn apply_table_metadata(
    document: &mut Document,
    metadata: &[StructureTableMeta],
) -> Result<(), Message> {
    apply_table_metadata_checked(document, metadata, || Ok(()))
}

/// Applies table metadata while checking worker cancellation between tables.
pub(crate) fn apply_table_metadata_with_cancel(
    document: &mut Document,
    metadata: &[StructureTableMeta],
    cancelled: &AtomicBool,
) -> Result<(), Message> {
    apply_table_metadata_checked(document, metadata, || check_cancelled(cancelled))
}

// Updates table names and properties after checking for cancellation.
fn apply_table_metadata_checked(
    document: &mut Document,
    metadata: &[StructureTableMeta],
    mut check: impl FnMut() -> Result<(), Message>,
) -> Result<(), Message> {
    for item in metadata {
        check()?;
        let table = document
            .tables
            .iter_mut()
            .find(|table| table.id == item.id)
            .ok_or_else(|| {
                Message::new("status-table-id-unavailable").with_number("table_id", item.id as i64)
            })?;
        table.name.clone_from(&item.name);
        table.set_dimensions(item.rows.max(1), item.columns.max(1));
        table.set_loaded_string_cells(Arc::clone(&item.string_cells));
    }
    Ok(())
}

/// Applies the document's unsaved cell changes, checking for cancellation between them.
pub(crate) fn apply_changed_cells_with_cancel(
    document: &mut Document,
    request: &StructureRequest,
    cancelled: &AtomicBool,
) -> Result<(), Message> {
    apply_changed_cells_checked(document, request, || check_cancelled(cancelled))
}

// Applies typed cell changes after checking for cancellation.
fn apply_changed_cells_checked(
    document: &mut Document,
    request: &StructureRequest,
    mut check: impl FnMut() -> Result<(), Message>,
) -> Result<(), Message> {
    for (table_id, address, cell) in &request.changes {
        check()?;
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
    Ok(())
}

// Stops a source update when its worker receives a cancellation request.
fn check_cancelled(cancelled: &AtomicBool) -> Result<(), Message> {
    if cancelled.load(Ordering::Acquire) {
        Err(Message::new("status-operation-cancelled"))
    } else {
        Ok(())
    }
}
