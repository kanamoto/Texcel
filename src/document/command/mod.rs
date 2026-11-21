//!  Commands that apply edits and preserve the shared undo history.

mod helpers;

use std::collections::BTreeMap;
use std::sync::Arc;

use super::*;
use crate::source::SourceDocument;

pub(crate) use helpers::{apply_cell_changes, cell_inputs_differ};

/// A cell whose formula-free text result must stay literal text:
/// the cell, whether it was forced to text before the edit, and whether it is after.
pub(crate) type StringOverride = (CellAddress, bool, bool);

/// A text edit whose exact source size the background worker must check before it applies.
#[derive(Clone, Debug)]
pub(crate) struct PendingTextSourceChanges {
    pub(crate) table_id: TableId,
    pub(crate) changes: Vec<(CellAddress, Option<Cell>)>,
    pub(crate) string_overrides: Vec<StringOverride>,
}

// A unit of document change, also used for undo and redo.
#[expect(
    clippy::large_enum_variant,
    reason = "structural roots already live behind Vec/Arc/maps; keeping the root swap inline \
              avoids an allocation for every small edit"
)]
#[derive(Clone, Debug)]
pub enum EditCommand {
    SetCells {
        table_id: TableId,
        changes: Vec<CellChange>,
    },
    SetFormulaResults {
        table_id: TableId,
        changes: Vec<CellChange>,
        /// Text results beginning with `=` are literalized as part of the same Undo command.
        string_overrides: Vec<StringOverride>,
    },
    ReplaceStructure {
        /// Inactive roots. Applying this command swaps them with the current tables.
        tables: Vec<Table>,
        text_overlay_before: Option<Arc<Vec<TextOverlayCell>>>,
        text_overlay_after: Option<Arc<Vec<TextOverlayCell>>>,
        swap_source: bool,
        /// Inactive source after the structural operation. A successful save may replace the
        /// current source; undo/redo always swaps that live source with this root.
        source: Option<Arc<SourceDocument>>,
        view_layout: Option<(TableId, Arc<TableLayout>)>,
        selection: Option<(TableId, CellRange)>,
        scroll: Option<(TableId, ScrollPosition)>,
        target: (TableId, CellAddress),
        target_before: Option<(TableId, CellAddress)>,
        replace_table_set: bool,
        /// View states moved out of the live map while IDs are absent, keyed by stable table ID.
        view_archive: Option<BTreeMap<TableId, TableViewState>>,
        /// String-cell sets of removed tables survive table-set Undo/Redo.
        string_cells_archive: Option<BTreeMap<TableId, TableStringCells>>,
        before_calculation: Option<Arc<crate::calc::CalculationSnapshot>>,
        after_calculation: Option<Arc<crate::calc::CalculationSnapshot>>,
        history_file: Option<Arc<crate::structure::StructureHistoryFile>>,
        use_after: bool,
    },
}

impl EditCommand {
    /// Returns the first table and cell affected by this command.
    pub(super) fn first_target(&self, forward: bool) -> Option<(TableId, CellAddress)> {
        match self {
            Self::SetCells { table_id, changes } => {
                changes.first().map(|change| (*table_id, change.address))
            }
            Self::SetFormulaResults {
                table_id, changes, ..
            } => changes.first().map(|change| (*table_id, change.address)),
            Self::ReplaceStructure {
                target,
                target_before,
                replace_table_set,
                ..
            } => Some(if !forward && *replace_table_set {
                target_before.unwrap_or(*target)
            } else {
                *target
            }),
        }
    }

    /// Lists the cells whose input changes in this command.
    pub(super) fn changed_addresses(&self) -> impl Iterator<Item = (TableId, CellAddress)> {
        let changed = match self {
            Self::SetCells { table_id, changes } => changes
                .iter()
                .map(|change| (*table_id, change.address))
                .collect(),
            Self::SetFormulaResults {
                table_id, changes, ..
            } => changes
                .iter()
                .map(|change| (*table_id, change.address))
                .collect(),
            Self::ReplaceStructure { .. } => Vec::new(),
        };
        changed.into_iter()
    }

    /// Lists cells whose calculation results may change.
    pub(super) fn calculation_changed_addresses(
        &self,
        forward: bool,
    ) -> impl Iterator<Item = (TableId, CellAddress)> + '_ {
        let (table_id, changes) = match self {
            Self::SetCells { table_id, changes } => (table_id, changes),
            Self::SetFormulaResults {
                table_id, changes, ..
            } => (table_id, changes),
            Self::ReplaceStructure { .. } => {
                return Vec::new().into_iter();
            }
        };
        let mut changed = changes
            .iter()
            .filter_map(move |change| {
                let (before, after) = if forward {
                    (change.before.as_ref(), change.after.as_ref())
                } else {
                    (change.after.as_ref(), change.before.as_ref())
                };
                cell_inputs_differ(before, after).then_some((*table_id, change.address))
            })
            .collect::<Vec<_>>();
        if let Self::SetFormulaResults {
            table_id,
            string_overrides,
            ..
        } = self
        {
            changed.extend(
                string_overrides
                    .iter()
                    .filter(|(_, before, after)| before != after)
                    .map(|(address, _, _)| (*table_id, *address)),
            );
            changed.sort_unstable();
            changed.dedup();
        }
        changed.into_iter()
    }

    /// Applies this command and records the resulting document state.
    pub(super) fn apply(
        &mut self,
        document: &mut Document,
        forward: bool,
        preserve_current_calculation: bool,
    ) -> Result<(), DocumentError> {
        if let Self::ReplaceStructure {
            tables,
            text_overlay_before,
            text_overlay_after,
            replace_table_set,
            view_archive,
            string_cells_archive,
            swap_source,
            source,
            view_layout,
            selection,
            scroll,
            history_file,
            use_after,
            ..
        } = self
        {
            if history_file.is_some() {
                return Err(DocumentError::NeedsWorkerHistoryRestore);
            }
            if (!*replace_table_set
                && tables
                    .iter()
                    .any(|state| !document.tables.iter().any(|table| table.id == state.id)))
                || view_layout
                    .as_ref()
                    .is_some_and(|(table_id, _)| !document.views.contains_key(table_id))
                || selection
                    .as_ref()
                    .is_some_and(|(table_id, _)| !document.views.contains_key(table_id))
                || scroll
                    .as_ref()
                    .is_some_and(|(table_id, _)| !document.views.contains_key(table_id))
            {
                return Err(DocumentError::MissingTable(
                    tables
                        .iter()
                        .find(|state| !document.tables.iter().any(|table| table.id == state.id))
                        .map_or(0, |state| state.id),
                ));
            }
            if *replace_table_set {
                if tables.is_empty() {
                    return Err(DocumentError::MissingTable(0));
                }
                let view_archive = view_archive
                    .as_mut()
                    .ok_or(DocumentError::MissingTable(0))?;
                let string_cells_archive = string_cells_archive
                    .as_mut()
                    .ok_or(DocumentError::MissingTable(0))?;
                let current_settings =
                    prepare_table_set_string_cells(tables, &document.tables, string_cells_archive);
                std::mem::swap(&mut document.tables, tables);
                reconcile_table_views(&mut document.views, &document.tables, view_archive);
                archive_removed_table_settings(
                    current_settings,
                    &document.tables,
                    string_cells_archive,
                );
            } else {
                for state in tables {
                    let table = document
                        .tables
                        .iter_mut()
                        .find(|table| table.id == state.id)
                        .ok_or(DocumentError::MissingTable(state.id))?;
                    std::mem::swap(table, state);
                }
            }
            if let Some(text_overlay) = if *use_after {
                text_overlay_before
            } else {
                text_overlay_after
            } {
                document.text_overlay_cells = Arc::clone(text_overlay);
            }
            if let Some((table_id, layout)) = view_layout {
                let view = document
                    .views
                    .get_mut(table_id)
                    .ok_or(DocumentError::MissingTable(*table_id))?;
                std::mem::swap(&mut view.layout, layout);
            }
            if let Some((table_id, range)) = selection {
                let view = document
                    .views
                    .get_mut(table_id)
                    .ok_or(DocumentError::MissingTable(*table_id))?;
                std::mem::swap(&mut view.selection, range);
                view.additional_selections.clear();
            }
            if let Some((table_id, scroll)) = scroll {
                let view = document
                    .views
                    .get_mut(table_id)
                    .ok_or(DocumentError::MissingTable(*table_id))?;
                std::mem::swap(&mut view.scroll, scroll);
            }
            if *swap_source {
                std::mem::swap(&mut document.source, source);
            }
            *use_after = !*use_after;
            document.clear_changed_cells();
            document.calculation_cells.clear();
            return Ok(());
        }
        match self {
            Self::SetCells { table_id, changes } => {
                apply_cell_changes(
                    document,
                    *table_id,
                    changes,
                    forward,
                    preserve_current_calculation,
                )?;
                Ok(())
            }
            Self::SetFormulaResults {
                table_id,
                changes,
                string_overrides,
            } => {
                apply_cell_changes(
                    document,
                    *table_id,
                    changes,
                    forward,
                    preserve_current_calculation,
                )?;
                let table = document
                    .tables
                    .iter_mut()
                    .find(|table| table.id == *table_id)
                    .ok_or(DocumentError::MissingTable(*table_id))?;
                for (address, before, after) in string_overrides {
                    table.set_string_cell(*address, if forward { *after } else { *before });
                }
                Ok(())
            }
            Self::ReplaceStructure { .. } => unreachable!("handled above"),
        }
    }

    /// Checks whether this command resets the calculation model.
    pub(super) fn resets_calculation(&self) -> bool {
        matches!(self, Self::ReplaceStructure { .. })
    }

    /// Builds a calculation snapshot after this command.
    pub(super) fn current_calculation_snapshot(
        &self,
    ) -> Option<Arc<crate::calc::CalculationSnapshot>> {
        match self {
            Self::ReplaceStructure {
                before_calculation,
                after_calculation,
                use_after,
                ..
            } => (if *use_after {
                after_calculation.as_ref()
            } else {
                before_calculation.as_ref()
            })
            .cloned(),
            _ => None,
        }
    }
}
