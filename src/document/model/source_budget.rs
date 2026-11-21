use std::collections::BTreeMap;

use crate::document::{Cell, CellAddress, DocumentError, EditCommand, TableId};
use crate::i18n::ResourceLimit;
use crate::source::SourceDocument;

use super::Document;

/// Describes whether a text-cell edit fits the source reader's exact byte limit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TextCellResourceCheck {
    NotApplicable,
    WithinLimit,
    ReachedLimit(ResourceLimit),
    ExceedsLimit(ResourceLimit),
    NeedsWorker,
}

impl Document {
    /// Gives a committed exact-cap warning to the UI that ran the command.
    pub(crate) fn take_resource_limit_notice(&mut self) -> Option<ResourceLimit> {
        self.resource_limit_notice.take()
    }

    /// Takes a command whose exact source budget is checked by the background worker.
    pub(crate) fn take_pending_text_source_changes(
        &mut self,
    ) -> Option<crate::document::PendingTextSourceChanges> {
        self.pending_text_source_changes.take()
    }

    /// Checks all cell changes in one command before the undo history is changed.
    pub(crate) fn check_command_text_source_limit(
        &self,
        command: &EditCommand,
    ) -> Result<Option<ResourceLimit>, DocumentError> {
        let Some(source) = self.source.as_deref() else {
            return Ok(None);
        };
        let Some((table_id, changes, string_overrides)) = command.text_source_changes() else {
            return Ok(None);
        };
        if !matches!(source, SourceDocument::Markdown(_) | SourceDocument::Csv(_)) {
            return Ok(None);
        }
        let overrides = string_overrides
            .iter()
            .map(|(address, _, after)| (*address, *after))
            .collect::<BTreeMap<_, _>>();
        let Some(table) = self.tables.iter().find(|table| table.id == table_id) else {
            return Err(DocumentError::MissingTable(table_id));
        };
        let candidates = changes
            .iter()
            .filter(|change| {
                let before_is_string = table.is_string_cell(change.address);
                let after_is_string = overrides
                    .get(&change.address)
                    .copied()
                    .unwrap_or(before_is_string);
                let before = change
                    .before
                    .as_ref()
                    .map(|cell| cell.source_text_for_text_file(before_is_string));
                let after = change
                    .after
                    .as_ref()
                    .map(|cell| cell.source_text_for_text_file(after_is_string));
                before != after
            })
            .map(|change| (change.address, change.after.as_ref()))
            .collect::<BTreeMap<_, _>>();
        if candidates.is_empty() {
            return Ok(None);
        }
        let source_bytes = match source {
            SourceDocument::Markdown(source) => source.source_bytes().len(),
            SourceDocument::Csv(source) => source.source_bytes().len(),
        };
        let byte_limit = match source {
            SourceDocument::Markdown(_) => crate::markdown_adapter::MAX_MARKDOWN_BYTES,
            SourceDocument::Csv(_) => crate::csv_adapter::MAX_CSV_SOURCE_BYTES,
        };
        if self.source_byte_delta_unknown > 0 {
            return Err(DocumentError::TextSourceValidationRequired);
        }
        let mut projected_bytes = source_bytes as i128 + self.source_byte_delta_total;
        for (address, after) in &candidates {
            let is_string = overrides
                .get(address)
                .copied()
                .unwrap_or_else(|| table.is_string_cell(*address));
            let value = after.map(|cell| cell.source_text_for_text_file(is_string));
            let Some(next_delta) =
                source_field_byte_delta(source, &self.tables, table_id, *address, value.as_deref())
            else {
                return Err(DocumentError::TextSourceValidationRequired);
            };
            let previous_delta = self
                .source_byte_deltas
                .get(&(table_id, *address))
                .copied()
                .flatten()
                .unwrap_or_default();
            projected_bytes = projected_bytes
                .saturating_sub(previous_delta)
                .saturating_add(next_delta);
        }
        if projected_bytes > byte_limit as i128 {
            return Err(DocumentError::ResourceLimit(ResourceLimit::FileSize));
        }
        if projected_bytes == byte_limit as i128 {
            Ok(Some(ResourceLimit::FileSize))
        } else {
            Ok(self.reached_text_source_shape_limit())
        }
    }

    /// Reports an exact source limit after a worker has applied a structural edit.
    pub(crate) fn reached_text_source_limit(&self) -> Option<ResourceLimit> {
        match self.source.as_deref()? {
            SourceDocument::Markdown(source) => source.reached_resource_limit(),
            SourceDocument::Csv(source) => source.reached_resource_limit(),
        }
    }

    /// Returns current row, column, or cell limits without reusing the source's old byte length.
    pub(crate) fn reached_text_source_shape_limit(&self) -> Option<ResourceLimit> {
        match self.source.as_deref()? {
            SourceDocument::Markdown(source) => source.reached_shape_limit(),
            SourceDocument::Csv(source) => source.reached_shape_limit(),
        }
    }

    /// Checks one existing field using tracked byte deltas instead of rebuilding the source.
    pub(crate) fn check_text_cell_resource_limit(
        &self,
        table_id: TableId,
        address: CellAddress,
        after: Option<&Cell>,
    ) -> TextCellResourceCheck {
        let Some(source) = self.source.as_deref() else {
            return TextCellResourceCheck::NotApplicable;
        };
        let (source_bytes, byte_limit) = match source {
            SourceDocument::Markdown(source) => (
                source.source_bytes().len(),
                crate::markdown_adapter::MAX_MARKDOWN_BYTES,
            ),
            SourceDocument::Csv(source) => (
                source.source_bytes().len(),
                crate::csv_adapter::MAX_CSV_SOURCE_BYTES,
            ),
        };
        if self.source_byte_delta_unknown > 0 {
            return TextCellResourceCheck::NeedsWorker;
        }
        let Some(next_delta) = self.source_cell_byte_delta(source, table_id, address, after) else {
            return TextCellResourceCheck::NeedsWorker;
        };
        let prior_delta = self
            .source_byte_deltas
            .get(&(table_id, address))
            .copied()
            .flatten()
            .unwrap_or_default();
        let resulting_bytes = (source_bytes as i128)
            .saturating_add(self.source_byte_delta_total)
            .saturating_sub(prior_delta)
            .saturating_add(next_delta);
        if resulting_bytes > byte_limit as i128 {
            TextCellResourceCheck::ExceedsLimit(ResourceLimit::FileSize)
        } else if resulting_bytes == byte_limit as i128 {
            TextCellResourceCheck::ReachedLimit(ResourceLimit::FileSize)
        } else if let Some(limit) = self.reached_text_source_shape_limit() {
            TextCellResourceCheck::ReachedLimit(limit)
        } else {
            TextCellResourceCheck::WithinLimit
        }
    }

    /// Rebuilds tracked deltas after a source replacement such as Save As.
    pub(crate) fn rebuild_source_byte_deltas(&mut self) {
        self.source_byte_deltas.clear();
        self.source_byte_delta_unknown = 0;
        self.source_byte_delta_total = 0;
        let changed = self.changed_cells.iter().copied().collect::<Vec<_>>();
        for (table_id, address) in changed {
            self.refresh_source_byte_delta(table_id, address);
        }
    }

    /// Updates one sparse source contribution after a cell command or undo.
    pub(super) fn refresh_source_byte_delta(&mut self, table_id: TableId, address: CellAddress) {
        if let Some(previous) = self.source_byte_deltas.remove(&(table_id, address)) {
            match previous {
                Some(delta) => self.source_byte_delta_total -= delta,
                None => {
                    self.source_byte_delta_unknown =
                        self.source_byte_delta_unknown.saturating_sub(1)
                }
            }
        }
        let Some(source) = self.source.as_deref() else {
            return;
        };
        if !matches!(source, SourceDocument::Csv(_) | SourceDocument::Markdown(_)) {
            return;
        }
        let delta = self.source_cell_byte_delta(
            source,
            table_id,
            address,
            self.tables
                .iter()
                .find(|table| table.id == table_id)
                .and_then(|table| table.cell(address)),
        );
        match delta {
            Some(delta) => {
                self.source_byte_delta_total = self.source_byte_delta_total.saturating_add(delta);
                self.source_byte_deltas
                    .insert((table_id, address), Some(delta));
            }
            None => {
                self.source_byte_delta_unknown = self.source_byte_delta_unknown.saturating_add(1);
                self.source_byte_deltas.insert((table_id, address), None);
            }
        }
    }

    /// Measures the replacement against the cell's original source field.
    fn source_cell_byte_delta(
        &self,
        source: &SourceDocument,
        table_id: TableId,
        address: CellAddress,
        cell: Option<&Cell>,
    ) -> Option<i128> {
        let table = self.tables.iter().find(|table| table.id == table_id)?;
        let value = cell.map(|cell| cell.source_text_for_text_file(table.is_string_cell(address)));
        source_field_byte_delta(source, &self.tables, table_id, address, value.as_deref())
    }
}

/// Measures one replacement with the source adapter's exact cell encoding.
fn source_field_byte_delta(
    source: &SourceDocument,
    tables: &[crate::document::Table],
    table_id: TableId,
    address: CellAddress,
    value: Option<&str>,
) -> Option<i128> {
    match source {
        SourceDocument::Markdown(source) => {
            let table_index = tables.iter().position(|table| table.id == table_id)?;
            source.source_cell_byte_delta(table_index, address.row, address.column, value)
        }
        SourceDocument::Csv(source) if tables.len() == 1 => {
            source.source_cell_byte_delta(address.row, address.column, value)
        }
        SourceDocument::Csv(_) => None,
    }
}
