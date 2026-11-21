//!  Sparse table contents and table-local editing operations.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;

use super::*;
use crate::i18n::Message;
use index::AxisIndex;
use index::cell_counts_as_data;

mod index;

// Sparse sheet data. Empty cells stay out of the map until explicitly saved.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Table {
    pub id: TableId,
    pub name: String,
    pub(super) string_cells: Arc<BTreeSet<CellAddress>>,
    pub(super) row_count: usize,
    pub(super) column_count: usize,
    #[serde(with = "map_entries")]
    pub(super) cells: BTreeMap<CellAddress, Cell>,
    /// Import-only formatting kept beside the cells; editing never reads or changes it.
    #[serde(default, with = "map_entries")]
    formats: BTreeMap<CellAddress, CellFormat>,
    #[serde(default)]
    axis_index: Arc<AxisIndex>,
    /// File-backed row, column, pane and merge metadata used by XLSX.
    pub layout: TableLayout,
}

impl Table {
    // Creates an empty table that fills the whole sheet.
    pub fn new(id: TableId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            string_cells: Arc::new(BTreeSet::new()),
            row_count: MAX_ROWS,
            column_count: MAX_COLUMNS,
            cells: BTreeMap::new(),
            formats: BTreeMap::new(),
            axis_index: Arc::new(AxisIndex::default()),
            layout: TableLayout::default(),
        }
    }

    // Returns the logical row count.
    pub fn row_count(&self) -> usize {
        self.row_count
    }

    // Returns the logical column count.
    pub fn column_count(&self) -> usize {
        self.column_count
    }

    /// Returns the bottom-right corner of the occupied-cell bounding rectangle.
    pub fn last_used_cell(&self) -> Option<CellAddress> {
        let row = *self.axis_index.rows.keys().next_back()?;
        let column = *self.axis_index.columns.keys().next_back()?;
        Some(CellAddress { row, column })
    }

    /// Returns the saved source extent expanded to include sparse edited cells.
    pub fn extent_end(&self) -> CellAddress {
        let used = self.last_used_cell().unwrap_or_default();
        CellAddress {
            row: self.row_count.saturating_sub(1).max(used.row),
            column: self.column_count.saturating_sub(1).max(used.column),
        }
    }

    /// Moves to the end of a continuous row/column input region without scanning the grid.
    pub fn data_region_edge(
        &self,
        current: CellAddress,
        row_delta: isize,
        column_delta: isize,
    ) -> CellAddress {
        if row_delta != 0 {
            let row = self.axis_index.columns.get(&current.column).map_or_else(
                || if row_delta > 0 { MAX_ROWS - 1 } else { 0 },
                |runs| runs.region_edge(current.row, row_delta, MAX_ROWS - 1),
            );
            CellAddress {
                row,
                column: current.column,
            }
        } else if column_delta != 0 {
            let column = self.axis_index.rows.get(&current.row).map_or_else(
                || {
                    if column_delta > 0 { MAX_COLUMNS - 1 } else { 0 }
                },
                |runs| runs.region_edge(current.column, column_delta, MAX_COLUMNS - 1),
            );
            CellAddress {
                row: current.row,
                column,
            }
        } else {
            current
        }
    }

    // Returns the stored cell at a coordinate. An empty cell returns `None`.
    pub fn cell(&self, address: CellAddress) -> Option<&Cell> {
        self.cells.get(&address)
    }

    /// Returns the filled cell nearest to A1 by straight-line distance.
    ///
    /// Cells are stored row by row, so the scan stops once a row is farther than the best match.
    pub fn nearest_filled_cell(&self) -> Option<(CellAddress, &Cell)> {
        let mut best: Option<(u128, CellAddress, &Cell)> = None;
        for (address, cell) in &self.cells {
            let row = address.row as u128;
            if best
                .as_ref()
                .is_some_and(|(distance, ..)| row * row >= *distance)
            {
                break;
            }
            if cell_input_is_blank(&cell.input) {
                continue;
            }
            let column = address.column as u128;
            let distance = row * row + column * column;
            if best
                .as_ref()
                .is_none_or(|(nearest, ..)| distance < *nearest)
            {
                best = Some((distance, *address, cell));
            }
        }
        best.map(|(_, address, cell)| (address, cell))
    }

    /// Returns the sparse cells that need to survive text-source reparsing.
    pub(crate) fn text_overlay_cells(&self) -> impl Iterator<Item = (CellAddress, Cell)> + '_ {
        self.cells
            .iter()
            .filter(|(_, cell)| cell_needs_text_overlay(cell))
            .map(|(address, cell)| (*address, cell.clone()))
    }

    /// Returns the imported formatting for one cell, if the file gave it any.
    pub fn format(&self, address: CellAddress) -> Option<&CellFormat> {
        self.formats.get(&address)
    }

    /// Iterates over all imported formatting in row and column order.
    pub fn formats(&self) -> impl Iterator<Item = (CellAddress, &CellFormat)> {
        self.formats
            .iter()
            .map(|(address, format)| (*address, format))
    }

    /// Stores imported formatting without edit history. An empty format removes the stored one.
    pub(crate) fn set_loaded_format(&mut self, address: CellAddress, format: CellFormat) {
        if format.is_empty() {
            self.formats.remove(&address);
        } else {
            self.formats.insert(address, format);
        }
    }

    // Returns whether text starting with `=` in this cell is kept as text instead of a formula.
    pub fn is_string_cell(&self, address: CellAddress) -> bool {
        self.string_cells.contains(&address)
    }

    /// Returns the shared set of cells forced to text.
    pub(crate) fn string_cells_shared(&self) -> Arc<BTreeSet<CellAddress>> {
        Arc::clone(&self.string_cells)
    }

    /// Sets imported text overrides without adding edit history.
    pub(crate) fn set_loaded_string_cells(&mut self, cells: Arc<BTreeSet<CellAddress>>) {
        self.string_cells = cells;
    }

    // Changes the per-cell text override in a text table.
    pub fn set_string_cell(&mut self, address: CellAddress, is_string: bool) {
        // After load, the document owner advances calculation_revision and queues this cell.
        if is_string {
            Arc::make_mut(&mut self.string_cells).insert(address);
        } else {
            Arc::make_mut(&mut self.string_cells).remove(&address);
        }
    }

    // Sets the logical display size within the specified limits.
    pub fn set_dimensions(&mut self, rows: usize, columns: usize) {
        self.row_count = rows.clamp(1, MAX_ROWS);
        self.column_count = columns.clamp(1, MAX_COLUMNS);
    }

    /// Installs layout metadata loaded from a file without creating an edit-history entry.
    pub fn set_loaded_layout(&mut self, layout: TableLayout) {
        self.layout = layout;
    }

    /// Applies a cell input change through the document model.
    pub(super) fn set_cell(&mut self, address: CellAddress, cell: Option<Cell>) {
        let was_occupied = self.cells.get(&address).is_some_and(cell_counts_as_data);
        let is_occupied = cell.as_ref().is_some_and(cell_counts_as_data);
        if was_occupied != is_occupied {
            Arc::make_mut(&mut self.axis_index).set(address, is_occupied);
        }
        if let Some(cell) = cell {
            self.cells.insert(address, cell);
        } else {
            self.cells.remove(&address);
        }
    }

    // Sets initial values imported from a file without history. Do not use this for user edits.
    pub(crate) fn set_loaded_cell(&mut self, address: CellAddress, cell: Cell) {
        self.set_cell(address, Some(cell));
    }

    /// Rebuilds row and column indexes after structural changes.
    pub(crate) fn rebuild_axis_index(&mut self) {
        self.axis_index = Arc::new(AxisIndex::from_cells(&self.cells));
    }

    // Applies immutable export worker changes to the initial model, including deletions.
    pub(crate) fn apply_snapshot_cell(&mut self, address: CellAddress, cell: Option<Cell>) {
        self.set_cell(address, cell);
    }

    /// Applies a prepared structure plan to table data.
    pub(crate) fn apply_structure_plan(
        &mut self,
        plan: &StructurePlan,
        formula_updates: &BTreeMap<(TableId, CellAddress), String>,
    ) -> Result<(), Message> {
        let target_table_id = structure_table_id(&plan.operation);
        if self.id == target_table_id {
            let mut cells = BTreeMap::new();
            let mut string_cells = BTreeSet::new();
            for (address, cell) in std::mem::take(&mut self.cells) {
                for output in plan.output_positions(address) {
                    let mut cell = cell.clone();
                    if let Some(formula) = formula_updates.get(&(self.id, output)) {
                        replace_formula_input(&mut cell.input, formula.clone());
                        cell.imported_result = None;
                        cell.calculation = None;
                    }
                    cells.insert(output, cell);
                    if self.string_cells.contains(&address) {
                        string_cells.insert(output);
                    }
                }
            }
            self.string_cells = Arc::new(string_cells);
            self.cells = cells;
            self.axis_index = Arc::new(AxisIndex::from_cells(&self.cells));
            let (rows, columns) = plan.after_dimensions();
            self.set_dimensions(rows, columns);
            self.layout = self.layout.transformed(plan)?;
        }
        for ((table_id, address), formula) in formula_updates {
            if *table_id != self.id || target_table_id == self.id {
                continue;
            }
            if let Some(cell) = self.cells.get_mut(address) {
                replace_formula_input(&mut cell.input, formula.clone());
                cell.imported_result = None;
                cell.calculation = None;
            }
        }
        for cell in self.cells.values_mut() {
            if matches!(cell.input, CellInput::Formula(_))
                || matches!(&cell.input, CellInput::Text(value) if value.starts_with('='))
            {
                cell.calculation = None;
            }
        }
        Ok(())
    }

    /// Copies one rectangular source range to an enclosing target range on the worker.
    pub(crate) fn apply_fill(
        &mut self,
        source: CellRange,
        target: CellRange,
        mode: FillMode,
        formula_updates: &BTreeMap<(TableId, CellAddress), String>,
    ) -> Result<(), Message> {
        let (source_start, source_end) = source.normalized();
        let (target_start, target_end) = target.normalized();
        let area = target_end
            .row
            .saturating_sub(target_start.row)
            .saturating_add(1)
            .saturating_mul(
                target_end
                    .column
                    .saturating_sub(target_start.column)
                    .saturating_add(1),
            );
        if area > MAX_FORMAT_RANGE_CELLS {
            return Err(Message::new("status-fill-range-limit")
                .with_number("limit", MAX_FORMAT_RANGE_CELLS as i64));
        }
        let series_kind = match mode {
            FillMode::Copy => None,
            FillMode::Series => Some(validate_series_source(
                self,
                source_start,
                source_end,
                target_start,
                target_end,
            )?),
            // Formulas and unreadable sequences keep the plain copy, as the fill handle did.
            FillMode::Auto => {
                validate_series_source(self, source_start, source_end, target_start, target_end)
                    .ok()
                    .filter(|kind| !matches!(kind, SeriesKind::Formula))
            }
        };
        let seed_cells = self
            .stored_cells_in_range(source)
            .map(|(address, cell)| (address, cell.clone()))
            .collect::<BTreeMap<_, _>>();
        let seed_strings = self
            .string_cells
            .iter()
            .filter(|address| {
                address.row >= source_start.row
                    && address.row <= source_end.row
                    && address.column >= source_start.column
                    && address.column <= source_end.column
            })
            .copied()
            .collect::<BTreeSet<_>>();
        let remove = self
            .stored_cells_in_range(target)
            .filter_map(|(address, _)| {
                let in_source = address.row >= source_start.row
                    && address.row <= source_end.row
                    && address.column >= source_start.column
                    && address.column <= source_end.column;
                (!in_source).then_some(address)
            })
            .collect::<Vec<_>>();
        for address in remove {
            self.cells.remove(&address);
            Arc::make_mut(&mut self.string_cells).remove(&address);
        }

        for row in target_start.row..=target_end.row {
            for column in target_start.column..=target_end.column {
                let destination = CellAddress { row, column };
                if row >= source_start.row
                    && row <= source_end.row
                    && column >= source_start.column
                    && column <= source_end.column
                {
                    continue;
                }
                let source_address = source.repeated_source(CellAddress { row, column });
                let Some(mut cell) = seed_cells.get(&source_address).cloned() else {
                    continue;
                };
                if let Some(series_kind) = &series_kind {
                    match series_kind.value_for(destination)? {
                        SeriesValue::Number(value) => cell.input = CellInput::Number(value),
                        SeriesValue::Text(value) => cell.input = CellInput::Text(value),
                        SeriesValue::Formula => (),
                    }
                    cell.imported_result = None;
                    cell.calculation = None;
                }
                if let Some(formula) = formula_updates.get(&(self.id, destination)) {
                    replace_formula_input(&mut cell.input, formula.clone());
                    cell.imported_result = None;
                    cell.calculation = None;
                }
                self.cells.insert(destination, cell);
                if seed_strings.contains(&source_address) {
                    Arc::make_mut(&mut self.string_cells).insert(destination);
                } else {
                    Arc::make_mut(&mut self.string_cells).remove(&destination);
                }
            }
        }
        self.rebuild_axis_index();
        Ok(())
    }

    // Returns only stored cells needed for serialization, in order.
    pub(crate) fn stored_cells(&self) -> impl Iterator<Item = (CellAddress, &Cell)> {
        self.cells.iter().map(|(address, cell)| (*address, cell))
    }

    /// Reads sparse cells intersecting the visible columns of one row.
    pub(crate) fn stored_cells_in_row_columns(
        &self,
        row: usize,
        columns: std::ops::Range<usize>,
    ) -> impl Iterator<Item = (CellAddress, &Cell)> {
        let start = CellAddress {
            row,
            column: columns.start.min(MAX_COLUMNS),
        };
        let end = CellAddress {
            row,
            column: columns.end.min(MAX_COLUMNS),
        };
        self.cells
            .range((
                std::ops::Bound::Included(start),
                std::ops::Bound::Excluded(end),
            ))
            .map(|(address, cell)| (*address, cell))
    }

    /// Reads only occupied cells in one column for worker-side layout measurement.
    pub(crate) fn stored_cells_in_column(
        &self,
        column: usize,
    ) -> impl Iterator<Item = (CellAddress, &Cell)> + '_ {
        self.axis_index
            .columns
            .get(&column)
            .into_iter()
            .flat_map(|runs| runs.positions())
            .filter_map(move |row| {
                let address = CellAddress { row, column };
                self.cells.get(&address).map(|cell| (address, cell))
            })
    }

    /// Reads sparse cells in a range; large callers should run outside the UI thread.
    pub(crate) fn stored_cells_in_range(
        &self,
        range: CellRange,
    ) -> impl Iterator<Item = (CellAddress, &Cell)> {
        let (start, end) = range.normalized();
        self.cells
            .range(start..=end)
            .filter(move |(address, _)| {
                address.row <= end.row
                    && address.column >= start.column
                    && address.column <= end.column
            })
            .map(|(address, cell)| (*address, cell))
    }

    /// Returns stored cells within a cell range.
    pub(super) fn cells_in_range(&self, range: CellRange) -> Vec<(CellAddress, &Cell)> {
        let (start, end) = range.normalized();
        self.cells
            .range(start..=end)
            .filter(|(address, _)| {
                address.row <= end.row
                    && address.column >= start.column
                    && address.column <= end.column
            })
            .map(|(address, cell)| (*address, cell))
            .collect()
    }
}

/// Checks whether a cell input must be kept in the text overlay.
pub(crate) fn cell_needs_text_overlay(cell: &Cell) -> bool {
    !matches!(cell.input, CellInput::Empty | CellInput::Text(_))
}

/// Treats missing input and empty text as no value for table summaries.
fn cell_input_is_blank(input: &CellInput) -> bool {
    match input {
        CellInput::Empty => true,
        CellInput::Text(text) | CellInput::Formula(text) | CellInput::Error(text) => {
            text.is_empty()
        }
        CellInput::Number(_) | CellInput::Boolean(_) => false,
    }
}
