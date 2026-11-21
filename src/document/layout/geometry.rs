//!  Calculates column positions and applies sparse layout edits.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::document::frozen_prefix;
use crate::document::{
    CellAddress, ColumnLayout, DEFAULT_COLUMN_WIDTH_PIXELS, FrozenPanes, LayoutTarget, LayoutValue,
    MAX_COLUMN_WIDTH_UNITS, MAX_COLUMNS, MAX_ROWS, MERGE_ROW_BLOCK_SIZE, StructureAxis,
    StructurePlan, TableLayout,
};
use crate::i18n::Message;

impl TableLayout {
    /// Returns the column layout stored at an index.
    pub fn column(&self, index: usize) -> Option<ColumnLayout> {
        self.columns.get(&index).copied()
    }

    /// Iterates over the stored column layouts.
    pub fn columns(&self) -> impl Iterator<Item = (usize, ColumnLayout)> + '_ {
        self.columns.iter().map(|(index, layout)| (*index, *layout))
    }

    /// Reports whether an imported sheet had row heights or hidden rows.
    pub fn has_imported_row_layout(&self) -> bool {
        self.imported_row_layout
    }

    /// Records that an imported sheet had row heights or hidden rows.
    pub(crate) fn mark_imported_row_layout(&mut self) {
        self.imported_row_layout = true;
    }

    /// Returns the frozen pane settings.
    pub fn frozen(&self) -> &FrozenPanes {
        &self.frozen
    }

    /// Returns the layout revision used by caches.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the visible width of a column in pixels.
    pub fn column_width_pixels(&self, index: usize) -> f64 {
        self.column(index)
            .unwrap_or_default()
            .width
            .filter(|width| width.is_finite() && *width >= 0.0)
            .map_or(DEFAULT_COLUMN_WIDTH_PIXELS, |width| {
                if width == 0.0 {
                    0.0
                } else {
                    xlsx_width_pixels(width)
                }
            })
    }

    /// Returns the total width before a column index.
    pub fn prefix_column_width_pixels(&self, columns: usize) -> f64 {
        columns.min(MAX_COLUMNS) as f64 * DEFAULT_COLUMN_WIDTH_PIXELS
            + fenwick_prefix(&self.column_width_deltas, columns.min(MAX_COLUMNS))
    }

    /// Reads one layout value for an undo target.
    pub fn value(&self, target: LayoutTarget) -> LayoutValue {
        match target {
            LayoutTarget::Column(index) => LayoutValue::Column(self.column(index)),
            LayoutTarget::Merge(range) => {
                LayoutValue::Merge(self.merge_geometry.merges.contains(&range))
            }
            LayoutTarget::Freeze => LayoutValue::Freeze(self.frozen.clone()),
        }
    }

    /// Applies a column, merge, or freeze layout value.
    pub fn apply_value(&mut self, target: LayoutTarget, value: LayoutValue) {
        match (target, value) {
            (LayoutTarget::Column(index), LayoutValue::Column(layout)) => {
                let previous = self.column(index).unwrap_or_default();
                let next = layout.unwrap_or_default();
                if previous == next {
                    return;
                }
                update_fenwick(
                    &mut self.column_width_deltas,
                    index,
                    column_delta(next) - column_delta(previous),
                    MAX_COLUMNS,
                );
                if next == ColumnLayout::default() {
                    self.columns.remove(&index);
                } else {
                    self.columns.insert(index, next);
                }
                self.revision = self.revision.saturating_add(1);
            }
            (LayoutTarget::Merge(range), LayoutValue::Merge(merged)) => {
                let geometry = Arc::make_mut(&mut self.merge_geometry);
                let changed = if merged {
                    let changed = geometry.merges.insert(range);
                    if changed {
                        geometry
                            .merges_by_start_row
                            .entry(range.start.row)
                            .or_default()
                            .insert(range);
                        Self::refresh_merge_block(geometry, range.start.row / MERGE_ROW_BLOCK_SIZE);
                    }
                    changed
                } else {
                    let changed = geometry.merges.remove(&range);
                    if changed
                        && let Some(ranges) = geometry.merges_by_start_row.get_mut(&range.start.row)
                    {
                        ranges.remove(&range);
                        if ranges.is_empty() {
                            geometry.merges_by_start_row.remove(&range.start.row);
                        }
                        Self::refresh_merge_block(geometry, range.start.row / MERGE_ROW_BLOCK_SIZE);
                    }
                    changed
                };
                if changed {
                    self.revision = self.revision.saturating_add(1);
                }
            }
            (LayoutTarget::Freeze, LayoutValue::Freeze(frozen)) => {
                if self.frozen != frozen {
                    self.frozen = frozen;
                    self.revision = self.revision.saturating_add(1);
                }
            }
            _ => unreachable!("layout target and value kinds must match"),
        }
    }

    /// Builds the layout after a structural row or column operation.
    pub(crate) fn transformed(&self, plan: &StructurePlan) -> Result<Self, Message> {
        let mut next = Self::default();
        {
            for (index, value) in self.columns() {
                for output in plan.output_positions(CellAddress {
                    row: 0,
                    column: index,
                }) {
                    next.apply_value(
                        LayoutTarget::Column(output.column),
                        LayoutValue::Column(Some(value)),
                    );
                }
            }
            // Edited documents never hold merged cells, so there are no merges to move.
            let old_frozen = &self.frozen;
            let rows = frozen_prefix(
                &plan.row_sources,
                old_frozen.rows,
                &plan.operation,
                StructureAxis::Rows,
            );
            let columns = frozen_prefix(
                &plan.column_sources,
                old_frozen.columns,
                &plan.operation,
                StructureAxis::Columns,
            );
            next.apply_value(
                LayoutTarget::Freeze,
                LayoutValue::Freeze(FrozenPanes {
                    rows: rows.min(MAX_ROWS.saturating_sub(1)),
                    columns: columns.min(MAX_COLUMNS.saturating_sub(1)),
                }),
            );
        }
        Ok(next)
    }
}

/// Converts an XLSX column width to pixels.
fn xlsx_width_pixels(width: f64) -> f64 {
    width.clamp(0.1, MAX_COLUMN_WIDTH_UNITS) * 7.0 + 5.0
}

/// Returns the column width change from the default width.
fn column_delta(layout: ColumnLayout) -> f64 {
    layout
        .width
        .filter(|width| width.is_finite() && *width >= 0.0)
        .map_or(0.0, |width| {
            if width == 0.0 {
                -DEFAULT_COLUMN_WIDTH_PIXELS
            } else {
                xlsx_width_pixels(width) - DEFAULT_COLUMN_WIDTH_PIXELS
            }
        })
}

/// Updates one value in the prefix-sum tree.
fn update_fenwick(tree: &mut BTreeMap<usize, f64>, index: usize, delta: f64, limit: usize) {
    if delta.abs() < 0.000_001 {
        return;
    }
    let mut node = index + 1;
    while node <= limit {
        let value = tree.get(&node).copied().unwrap_or_default() + delta;
        if value.abs() < 0.000_001 {
            tree.remove(&node);
        } else {
            tree.insert(node, value);
        }
        node += node.isolate_lowest_one();
    }
}

/// Sums the stored changes before an index.
fn fenwick_prefix(tree: &BTreeMap<usize, f64>, count: usize) -> f64 {
    let mut node = count;
    let mut total = 0.0;
    while node > 0 {
        total += tree.get(&node).copied().unwrap_or_default();
        node &= node - 1;
    }
    total
}
