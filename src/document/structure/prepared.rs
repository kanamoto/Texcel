//!  Immutable worker output ready to be installed on a document.

use std::sync::Arc;

use crate::document::{
    CellAddress, CellRange, ScrollPosition, Table, TableId, TableLayout, TextOverlayCell,
};
use crate::source::SourceDocument;

/// Full immutable result built away from the UI thread.
#[derive(Debug)]
pub struct PreparedStructure {
    pub tables: Vec<Table>,
    /// Calculation input version captured with the source used by the worker.
    pub calculation_revision: u64,
    /// The prepared source can add/remove IDs, so application swaps the complete table set.
    pub replace_table_set: bool,
    pub text_overlay_before: Arc<Vec<TextOverlayCell>>,
    pub text_overlay_after: Arc<Vec<TextOverlayCell>>,
    pub source_before: Option<Arc<SourceDocument>>,
    pub source_after: Option<Arc<SourceDocument>>,
    pub view_table_id: TableId,
    pub view_layout_revision: u64,
    pub view_layout_after: Arc<TableLayout>,
    pub selection_after: CellRange,
    pub scroll_after: ScrollPosition,
    pub target: (TableId, CellAddress),
    /// Target selected before the table-set change, used when Undo restores that root.
    pub target_before: Option<(TableId, CellAddress)>,
    pub before_calculation: Option<Arc<crate::calc::CalculationSnapshot>>,
    pub after_calculation: Arc<crate::calc::CalculationSnapshot>,
    pub history_file: Option<Arc<crate::structure::StructureHistoryFile>>,
    /// Conversion notes produced when a source becomes editable Markdown.
    pub conversion_report: Option<Arc<crate::conversion_report::ExportReport>>,
    /// A short operation result for a paste that could not keep all requested formatting.
    pub status: Option<crate::i18n::Message>,
}

pub(crate) type StructureSwapResult = ((TableId, CellAddress), Option<Vec<Table>>);
