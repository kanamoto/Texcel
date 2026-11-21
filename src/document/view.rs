//!  Stores table view state and keeps selections valid after layout changes.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::*;
use crate::document::TableId;

// Horizontal and vertical scroll offset for a table.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ScrollPosition {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CellDisplayMode {
    #[default]
    View,
    Source,
}

/// One display mode shared by every table in a document.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentDisplayMode {
    #[default]
    Live,
    Source,
}

// Per-table display state such as selection and scroll position.
#[derive(Clone, Debug, Default)]
pub struct TableViewState {
    pub selection: CellRange,
    /// The input cell stays separate from range endpoints during selection navigation.
    pub active_cell: Option<CellAddress>,
    /// Additional disjoint rectangles; the active rectangle remains in `selection`.
    pub additional_selections: Vec<CellRange>,
    pub scroll: ScrollPosition,
    /// Display-only text wrapping for the whole table; it never changes the file.
    pub wrap_text: bool,
    /// Display-only column widths and freeze panes.
    pub layout: Arc<TableLayout>,
    /// Header row for CSV (unset means no header); Markdown fixes this at row zero.
    pub header_row: Option<usize>,
}

impl Document {
    /// Returns the row that sorting, duplicate checks, and bold display treat as the header.
    /// The view's own choice wins; otherwise a Markdown table's first row is its header.
    pub fn header_row(&self, table_id: TableId) -> Option<usize> {
        self.view(table_id)
            .and_then(|view| view.header_row)
            .or_else(|| {
                matches!(
                    self.source.as_deref(),
                    Some(crate::source::SourceDocument::Markdown(_))
                )
                .then_some(0)
            })
    }
}

impl TableViewState {
    /// Returns the selected cell ranges in normalized order.
    pub fn selected_ranges(&self) -> impl Iterator<Item = CellRange> + '_ {
        std::iter::once(self.selection).chain(self.additional_selections.iter().copied())
    }

    /// Updates the active and additional selections for a table.
    pub fn set_selection(&mut self, selection: CellRange) {
        self.selection = selection;
        self.active_cell = Some(selection.anchor);
        self.additional_selections.clear();
    }

    /// Updates a range while keeping its input cell separate from its endpoints.
    pub fn set_selection_with_active(&mut self, selection: CellRange, active: CellAddress) {
        self.selection = selection;
        self.active_cell = Some(active);
        self.additional_selections.clear();
    }

    /// Returns the input cell, falling back to the first cell of restored ranges.
    pub fn active_cell(&self) -> CellAddress {
        self.active_cell.unwrap_or(self.selection.anchor)
    }

    /// Adds an independent rectangle or removes that exact rectangle when clicked again.
    pub fn toggle_disjoint_selection(
        &mut self,
        selection: CellRange,
    ) -> Result<(), crate::i18n::Message> {
        if selection == self.selection {
            if let Some(previous) = self.additional_selections.pop() {
                self.selection = previous;
                self.active_cell = Some(previous.anchor);
            }
            return Ok(());
        }
        if let Some(index) = self
            .additional_selections
            .iter()
            .position(|candidate| *candidate == selection)
        {
            self.additional_selections.remove(index);
            return Ok(());
        }
        if self.additional_selections.len() >= 1_024 {
            return Err(crate::i18n::Message::new("status-selection-range-limit")
                .with_number("limit", 1_024));
        }
        let (start, end) = selection.normalized();
        let selected = MergedRange { start, end };
        if self.selected_ranges().any(|candidate| {
            let (start, end) = candidate.normalized();
            selected.intersects(MergedRange { start, end })
        }) {
            return Err(crate::i18n::Message::new("status-selection-range-overlap"));
        }
        self.additional_selections.push(self.selection);
        self.selection = selection;
        self.active_cell = Some(selection.anchor);
        Ok(())
    }

    /// Applies a view-only row, column, merge, or freeze setting.
    pub fn set_layout_value(&mut self, target: LayoutTarget, value: LayoutValue) {
        Arc::make_mut(&mut self.layout).apply_value(target, value);
    }

    /// Clamps stored selection endpoints after the table's dimensions shrink.
    pub fn clamp_selection_bounds(&mut self, rows: usize, columns: usize) {
        if rows == 0 || columns == 0 {
            return;
        }
        let clamp = |address: CellAddress| CellAddress {
            row: address.row.min(rows - 1),
            column: address.column.min(columns - 1),
        };
        self.selection = CellRange {
            anchor: clamp(self.selection.anchor),
            head: clamp(self.selection.head),
        };
        for range in &mut self.additional_selections {
            range.anchor = clamp(range.anchor);
            range.head = clamp(range.head);
        }
    }

    /// Normalizes an active input cell while keeping it inside the primary range.
    pub fn normalized_active_address(
        selection: CellRange,
        active_cell: CellAddress,
        rows: usize,
        columns: usize,
    ) -> Option<CellAddress> {
        if rows == 0 || columns == 0 {
            return None;
        }
        let clamp = |address: CellAddress| CellAddress {
            row: address.row.min(rows - 1),
            column: address.column.min(columns - 1),
        };
        let selection = CellRange {
            anchor: clamp(selection.anchor),
            head: clamp(selection.head),
        };
        let (start, end) = selection.normalized();
        let requested = clamp(active_cell);
        let requested = CellAddress {
            row: requested.row.clamp(start.row, end.row),
            column: requested.column.clamp(start.column, end.column),
        };
        Some(CellAddress {
            row: requested.row.clamp(start.row, end.row),
            column: requested.column.clamp(start.column, end.column),
        })
    }
}

/// Keeps live view state for stable table IDs and archives only IDs absent from the root.
pub(crate) fn reconcile_table_views(
    views: &mut BTreeMap<TableId, TableViewState>,
    tables: &[Table],
    archive: &mut BTreeMap<TableId, TableViewState>,
) {
    let ids = tables.iter().map(|table| table.id).collect::<BTreeSet<_>>();
    let removed = views
        .keys()
        .filter(|table_id| !ids.contains(table_id))
        .copied()
        .collect::<Vec<_>>();
    for table_id in removed {
        if let Some(view) = views.remove(&table_id) {
            archive.insert(table_id, view);
        }
    }
    for table_id in ids {
        views
            .entry(table_id)
            .or_insert_with(|| archive.remove(&table_id).unwrap_or_default());
    }
}

/// Takes the string-cell set of a table so it can be restored later.
pub(crate) fn table_string_cells(table: &Table) -> TableStringCells {
    TableStringCells {
        string_cells: table.string_cells_shared(),
    }
}

/// Puts a saved string-cell set back on a table.
pub(crate) fn apply_table_string_cells(table: &mut Table, settings: &TableStringCells) {
    table.set_loaded_string_cells(Arc::clone(&settings.string_cells));
}

/// Reuses live string-cell sets for stable IDs and retrieves those archived while an ID was absent.
pub(crate) fn prepare_table_set_string_cells(
    replacement: &mut [Table],
    current: &[Table],
    archive: &mut BTreeMap<TableId, TableStringCells>,
) -> BTreeMap<TableId, TableStringCells> {
    let current_settings = current
        .iter()
        .map(|table| (table.id, table_string_cells(table)))
        .collect::<BTreeMap<_, _>>();
    for table in replacement {
        if let Some(settings) = current_settings
            .get(&table.id)
            .or_else(|| archive.get(&table.id))
        {
            apply_table_string_cells(table, settings);
            archive.remove(&table.id);
        }
    }
    current_settings
}

/// Keeps settings for a table that was removed.
pub(crate) fn archive_removed_table_settings(
    current: BTreeMap<TableId, TableStringCells>,
    replacement: &[Table],
    archive: &mut BTreeMap<TableId, TableStringCells>,
) {
    let replacement_ids = replacement
        .iter()
        .map(|table| table.id)
        .collect::<BTreeSet<_>>();
    for (table_id, settings) in current {
        if !replacement_ids.contains(&table_id) {
            archive.insert(table_id, settings);
        }
    }
}

#[cfg(test)]
impl TableViewState {
    // Moves selection within table bounds and keeps its anchor while extending a range.
    pub fn move_selection(
        &mut self,
        row_delta: isize,
        column_delta: isize,
        rows: usize,
        columns: usize,
        extend: bool,
    ) -> CellAddress {
        let row_max = rows.saturating_sub(1);
        let column_max = columns.saturating_sub(1);
        let active = self.active_cell();
        let head = if extend && self.selection.anchor == active {
            self.selection.head
        } else {
            active
        };
        let next = CellAddress {
            row: head.row.saturating_add_signed(row_delta).min(row_max),
            column: head
                .column
                .saturating_add_signed(column_delta)
                .min(column_max),
        };
        if extend {
            if self.selection.anchor != active {
                self.selection.anchor = active;
            }
            self.selection.head = next;
        } else {
            self.set_selection(CellRange::single(next));
        }
        next
    }
}
