//!  Sparse document inputs sent to the isolated calculation worker.

use std::collections::{BTreeSet, HashMap};
use std::ops::Deref;
use std::sync::Arc;

use crate::document::{
    CalcStatus, CalcValue, Cell, CellAddress, CellInput, Document, TableId, encode_literal_text,
};
use crate::file_worker::JobToken;
use crate::markdown_adapter::{MarkdownCellChange, MarkdownDocument};
use crate::source::SourceDocument;

pub type FormulaResultSnapshot = Arc<FormulaResultRoot>;

/// Stores one complete calculation result root with ordered and indexed access.
#[derive(Clone, Debug)]
pub struct FormulaResultRoot {
    pub revision: u64,
    entries: Vec<(TableId, CellAddress, CalcStatus, Option<CalcValue>)>,
    index: HashMap<(TableId, CellAddress), usize>,
}

impl FormulaResultRoot {
    /// Builds the lookup index once on the calculation worker.
    pub fn new(
        revision: u64,
        entries: Vec<(TableId, CellAddress, CalcStatus, Option<CalcValue>)>,
    ) -> Self {
        let index = entries
            .iter()
            .enumerate()
            .map(|(position, (table_id, address, _, _))| ((*table_id, *address), position))
            .collect();
        Self {
            revision,
            entries,
            index,
        }
    }

    /// Looks up one calculation result without scanning ordered entries.
    pub fn get(
        &self,
        table_id: TableId,
        address: CellAddress,
    ) -> Option<(CalcStatus, Option<&CalcValue>)> {
        let entry = self.entries.get(*self.index.get(&(table_id, address))?)?;
        Some((entry.2, entry.3.as_ref()))
    }

    /// Returns results in stable table and cell order for worker operations.
    pub fn entries(&self) -> &[(TableId, CellAddress, CalcStatus, Option<CalcValue>)] {
        &self.entries
    }
}

impl Deref for FormulaResultRoot {
    type Target = [(TableId, CellAddress, CalcStatus, Option<CalcValue>)];

    /// Exposes the stored results as a slice.
    fn deref(&self) -> &Self::Target {
        self.entries()
    }
}

impl<'a> IntoIterator for &'a FormulaResultRoot {
    type Item = &'a (TableId, CellAddress, CalcStatus, Option<CalcValue>);
    type IntoIter = std::slice::Iter<'a, (TableId, CellAddress, CalcStatus, Option<CalcValue>)>;

    /// Iterates over the stored results without copying them.
    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter()
    }
}

// Sparse calculation input for the whole document. Build it on the file worker thread.
#[derive(Clone, Debug)]
pub struct CalculationSnapshot {
    pub cross_sheet_references: bool,
    /// Workbook date system, passed into the isolated calculation model.
    pub tables: Vec<CalculationTable>,
}

// Calculation settings for one table and only its stored cells.
#[derive(Clone, Debug)]
pub struct CalculationTable {
    pub id: TableId,
    pub name: String,
    pub string_cells: Arc<BTreeSet<CellAddress>>,
    pub cells: Arc<Vec<(CellAddress, Cell)>>,
}

/// Markdown source context used to project one calculation change batch on the worker.
#[derive(Clone, Debug)]
pub(crate) struct MarkdownCalculationSource {
    source: Arc<MarkdownDocument>,
    table_indices: HashMap<TableId, usize>,
}

impl MarkdownCalculationSource {
    /// Captures the stable table order for Markdown cell projection.
    pub(crate) fn from_document(document: &Document) -> Option<Self> {
        let SourceDocument::Markdown(source) = document.source.as_deref()? else {
            return None;
        };
        Some(Self {
            source: Arc::clone(source),
            table_indices: document
                .tables
                .iter()
                .enumerate()
                .map(|(index, table)| (table.id, index))
                .collect(),
        })
    }
}

impl CalculationSnapshot {
    // Builds calculation worker input by listing only stored document cells.
    pub fn from_document(document: &Document, cross_sheet_references: bool) -> Self {
        Self {
            cross_sheet_references,
            tables: document
                .tables
                .iter()
                .enumerate()
                .map(|(table_index, table)| CalculationTable {
                    id: table.id,
                    name: table.name.clone(),
                    string_cells: Arc::new(
                        table
                            .stored_cells()
                            .filter_map(|(address, cell)| {
                                (table.is_string_cell(address)
                                    && matches!(cell.input, CellInput::Text(_)))
                                .then_some(address)
                            })
                            .collect(),
                    ),
                    cells: Arc::new(
                        table
                            .stored_cells()
                            .map(|(address, cell)| {
                                (
                                    address,
                                    calculation_cell_for_document(
                                        document,
                                        table_index,
                                        address,
                                        cell,
                                    ),
                                )
                            })
                            .collect(),
                    ),
                })
                .collect(),
        }
    }

    /// Copies each table's string-cell set into the snapshot without cloning its cells.
    pub(crate) fn preserve_string_cells(&mut self, tables: &[crate::document::Table]) {
        for calculation_table in &mut self.tables {
            let Some(table) = tables.iter().find(|table| table.id == calculation_table.id) else {
                continue;
            };
            calculation_table.string_cells = table.string_cells_shared();
        }
    }
}

/// Projects Markdown table cells into the values that formulas should calculate.
pub(crate) fn calculation_cell_for_document(
    document: &Document,
    table_index: usize,
    address: CellAddress,
    cell: &Cell,
) -> Cell {
    let Some(crate::source::SourceDocument::Markdown(source)) = document.source.as_deref() else {
        return cell.clone();
    };
    let CellInput::Text(raw) = &cell.input else {
        return cell.clone();
    };
    let mut projected = cell.clone();
    if raw.starts_with('=') {
        projected.input = CellInput::Text(decode_markdown_formula_source(raw));
        return projected;
    }
    let Some(display) = cached_markdown_display_text(source, table_index, address, raw) else {
        return cell.clone();
    };
    projected.input = CellInput::Text(encode_literal_text(&display));
    projected
}

/// Applies one batch to a Markdown source, then projects only its changed cells.
pub(crate) fn project_markdown_changes(
    context: &MarkdownCalculationSource,
    changes: &[CalculationChange],
) -> Result<Vec<CalculationChange>, crate::i18n::Message> {
    let source_changes = changes
        .iter()
        .map(|change| {
            let table = context
                .table_indices
                .get(&change.table_id)
                .copied()
                .ok_or_else(|| {
                    crate::i18n::Message::new("status-table-id-unavailable")
                        .with_number("table_id", change.table_id as i64)
                })?;
            Ok(MarkdownCellChange {
                table,
                row: change.address.row,
                column: change.address.column,
                value: change
                    .cell
                    .as_ref()
                    .map_or_else(String::new, |cell| cell.input.source_text()),
            })
        })
        .collect::<Result<Vec<_>, crate::i18n::Message>>()?;
    let updated = context
        .source
        .apply_changes(&source_changes)
        .map_err(crate::i18n::markdown_error_message)?;

    changes
        .iter()
        .map(|change| {
            let Some(cell) = change.cell.as_ref() else {
                return Ok(change.clone());
            };
            let table_index = context
                .table_indices
                .get(&change.table_id)
                .copied()
                .ok_or_else(|| {
                    crate::i18n::Message::new("status-table-id-unavailable")
                        .with_number("table_id", change.table_id as i64)
                })?;
            let mut projected = change.clone();
            projected.cell = Some(calculation_cell_for_markdown_source(
                &updated,
                table_index,
                change.address,
                cell,
            )?);
            Ok(projected)
        })
        .collect()
}

/// Projects Markdown text from its already parsed source cache without reparsing.
fn calculation_cell_for_markdown_source(
    source: &MarkdownDocument,
    table_index: usize,
    address: CellAddress,
    cell: &Cell,
) -> Result<Cell, crate::i18n::Message> {
    let CellInput::Text(raw) = &cell.input else {
        return Ok(cell.clone());
    };
    let mut projected = cell.clone();
    if raw.starts_with('=') {
        projected.input = CellInput::Text(decode_markdown_formula_source(raw));
        return Ok(projected);
    }
    let display = cached_markdown_display_text(source, table_index, address, raw)
        .ok_or_else(|| crate::i18n::Message::new("status-markdown-display-unavailable"))?;
    projected.input = CellInput::Text(encode_literal_text(&display));
    Ok(projected)
}

/// Reads the shared plain-text display projection for one Markdown cell.
fn cached_markdown_display_text(
    source: &MarkdownDocument,
    table_index: usize,
    address: CellAddress,
    raw: &str,
) -> Option<String> {
    source
        .cell_display_text_for_search(
            table_index,
            address.row,
            address.column,
            raw,
            crate::i18n::Language::English,
        )
        .ok()
}

/// Removes only Markdown table escapes around pipes inside formula source.
pub(crate) fn decode_markdown_formula_source(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut characters = source.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        let mut slash_count = 1_usize;
        while characters.peek() == Some(&'\\') {
            characters.next();
            slash_count = slash_count.saturating_add(1);
        }
        if characters.peek() == Some(&'|') {
            let _ = characters.next();
            output.extend(std::iter::repeat_n('\\', slash_count.saturating_sub(1) / 2));
            output.push('|');
        } else {
            output.extend(std::iter::repeat_n('\\', slash_count));
        }
    }
    output
}

// A changed cell for the calculation model. Use `cell: None` for deletion.
#[derive(Clone, Debug)]
pub struct CalculationChange {
    pub table_id: TableId,
    pub address: CellAddress,
    pub cell: Option<Cell>,
    pub string_cell: bool,
}

// A result returned after calculation. The UI applies it only when the document version matches.
#[derive(Debug)]
pub struct CalculationResult {
    pub token: JobToken,
    /// Present only after a complete successful calculation; consumers share this immutable data.
    pub full_snapshot: Option<FormulaResultSnapshot>,
    /// Keys to retry if the worker could not apply this update batch.
    pub failed_changes: Vec<(TableId, CellAddress)>,
    pub error: Option<crate::i18n::Message>,
    pub done: bool,
}
