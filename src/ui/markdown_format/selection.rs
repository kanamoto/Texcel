use super::*;

impl TexcelApp {
    /// Formats stored cells in selected ranges as one undoable operation.
    pub(super) fn apply_markdown_modifier_to_selection(&mut self, action: CellFormatAction) {
        let Some(document) = self.workspace.active_document() else {
            return;
        };
        let document_id = document.id;
        if self.cell_edit_locked(document_id) || self.close_after_save.contains(&document_id) {
            return;
        }
        let source_is_csv = matches!(document.source.as_deref(), Some(SourceDocument::Csv(_)));
        if !source_is_csv
            && !matches!(
                document.source.as_deref(),
                Some(SourceDocument::Markdown(_))
            )
        {
            return;
        }
        let Some(table_id) = document.active_table().map(|table| table.id) else {
            return;
        };
        let Some(view) = document.view(table_id) else {
            return;
        };
        let ranges = view.selected_ranges().collect::<Vec<_>>();
        let active_cell = view.active_cell();
        let Some(table) = document.tables.iter().find(|table| table.id == table_id) else {
            return;
        };
        let extent = table.extent_end();
        let selected = stored_selection_cells(table, &ranges, extent);
        if selected.len() > crate::document::MAX_FORMAT_RANGE_CELLS {
            self.status = Some(crate::i18n::document_error_message(
                crate::document::DocumentError::FormatRangeTooLarge(selected.len()),
            ));
            return;
        }
        let apply_modifier = !table
            .cell(active_cell)
            .and_then(cell_text_for_format)
            .is_some_and(|raw| cell_has_full_modifier(&raw, action, source_is_csv));
        let FormattedSelection {
            cells: formatted_cells,
            csv_changes,
        } = match format_selected_cells(table, selected, action, source_is_csv, apply_modifier) {
            Ok(changes) => changes,
            Err(message_key) => {
                self.status = Some(Message::new(message_key));
                return;
            }
        };
        if formatted_cells.is_empty() {
            return;
        }
        if source_is_csv {
            self.pending_csv_promotion_cell_batches
                .insert(document_id, (table_id, csv_changes));
            self.request_table_management(StructureAction::PromoteCsv, Some(table_id), None);
            if !self.pending_table_management.contains(&document_id) {
                self.pending_csv_promotion_cell_batches.remove(&document_id);
            }
            return;
        }
        self.apply_selected_text_changes(document_id, table_id, formatted_cells);
    }

    /// Applies one formatted cell batch or hands exact source checks to the worker.
    fn apply_selected_text_changes(
        &mut self,
        document_id: DocumentId,
        table_id: TableId,
        changes: Vec<(CellAddress, Option<Cell>)>,
    ) {
        let result = self
            .workspace
            .documents
            .get_mut(&document_id)
            .expect("the active document was checked above")
            .set_cells(table_id, changes);
        match result {
            Ok(_) => {
                self.schedule_calculation(document_id);
                self.request_grid_focus = true;
            }
            Err(error) => {
                let pending = self
                    .workspace
                    .documents
                    .get_mut(&document_id)
                    .and_then(Document::take_pending_text_source_changes);
                if let Some(pending) = pending {
                    self.submit_text_source_changes(pending, None);
                } else {
                    self.status = Some(crate::i18n::document_error_message(error));
                }
            }
        }
    }
}

/// Collects only stored cells and clips whole-sheet selections to the saved extent.
fn stored_selection_cells(
    table: &crate::document::Table,
    ranges: &[CellRange],
    extent: CellAddress,
) -> Vec<(CellAddress, Cell)> {
    let mut selected = std::collections::BTreeMap::new();
    for range in ranges {
        let (start, end) = range.normalized();
        if start.row > extent.row || start.column > extent.column {
            continue;
        }
        let bounded = CellRange {
            anchor: start,
            head: CellAddress {
                row: end.row.min(extent.row),
                column: end.column.min(extent.column),
            },
        };
        selected.extend(
            table
                .stored_cells_in_range(bounded)
                .map(|(address, cell)| (address, cell.clone())),
        );
    }
    selected.into_iter().collect()
}

/// Formatted cells for a Markdown table, and the same edits as raw text for a CSV table.
#[derive(Debug)]
struct FormattedSelection {
    cells: Vec<(CellAddress, Option<Cell>)>,
    csv_changes: Vec<(CellAddress, String)>,
}

/// Prepares the full batch before applying any cell so an unsupported cell cannot partly format it.
fn format_selected_cells(
    table: &crate::document::Table,
    selected: Vec<(CellAddress, Cell)>,
    action: CellFormatAction,
    source_is_csv: bool,
    apply_modifier: bool,
) -> Result<FormattedSelection, &'static str> {
    let mut formatted_cells = Vec::with_capacity(selected.len());
    let mut csv_changes = Vec::new();
    for (address, mut cell) in selected {
        if crate::structure::is_formula_cell(table, address, &cell) {
            return Err("status-formula-markdown-format-unsupported");
        }
        let Some(raw) = cell_text_for_format(&cell) else {
            continue;
        };
        if raw.is_empty() {
            continue;
        }
        let formatted = format_full_cell(&raw, action, source_is_csv, apply_modifier)
            .ok_or_else(|| format_error_key(action))?;
        if formatted == raw {
            continue;
        }
        cell.input = CellInput::Text(formatted.clone());
        cell.imported_result = None;
        cell.calculation = None;
        formatted_cells.push((address, Some(cell)));
        csv_changes.push((address, formatted));
    }
    Ok(FormattedSelection {
        cells: formatted_cells,
        csv_changes,
    })
}

/// Reads constant values using the same source spelling used by file output.
fn cell_text_for_format(cell: &Cell) -> Option<String> {
    match &cell.input {
        CellInput::Text(value) => Some(value.clone()),
        CellInput::Number(value) => Some(value.to_string()),
        CellInput::Boolean(value) => Some(if *value { "TRUE" } else { "FALSE" }.to_owned()),
        CellInput::Error(value) => Some(value.clone()),
        CellInput::Empty | CellInput::Formula(_) => None,
    }
}

/// Returns the message for a whole-cell formatting error.
fn format_error_key(action: CellFormatAction) -> &'static str {
    if action == CellFormatAction::Code {
        "status-markdown-code-unrepresentable"
    } else {
        "status-markdown-format-unrepresentable"
    }
}

/// Toggles a whole-cell marker to the state chosen by the active cell.
fn format_full_cell(
    raw: &str,
    action: CellFormatAction,
    source_is_csv: bool,
    apply_modifier: bool,
) -> Option<String> {
    let current = cell_has_full_modifier(raw, action, source_is_csv);
    if current == apply_modifier {
        return Some(raw.to_owned());
    }
    if action == CellFormatAction::Code {
        return super::markdown_code_candidate(raw, 0, raw.chars().count(), source_is_csv)
            .map(|(formatted, _, _)| formatted);
    }
    let markers = modifier_markers(action);
    let semantic = crate::document::literal_text_value(raw);
    let visible = if source_is_csv {
        semantic.to_owned()
    } else {
        crate::markdown_adapter::MarkdownCellDisplay::parse_fragment(raw).plain_text
    };
    let escaped = if source_is_csv {
        crate::export::escape_plain_markdown_cell(semantic)
    } else {
        semantic.to_owned()
    };
    let candidate = if apply_modifier {
        let marker = markers[0];
        format!("{marker}{escaped}{marker}")
    } else if let Some(inner) = markers.iter().find_map(|marker| {
        semantic
            .strip_prefix(marker)
            .and_then(|source| source.strip_suffix(marker))
            .filter(|inner| {
                crate::markdown_adapter::MarkdownCellDisplay::parse_fragment(inner).plain_text
                    == visible
            })
    }) {
        inner.to_owned()
    } else {
        return Some(raw.to_owned());
    };
    crate::markdown_adapter::MarkdownCellDisplay::persist_literal_marker(&candidate, &visible)
        .map(|(protected, _)| protected)
}

/// Checks whether a full cell already has the requested Markdown modifier.
fn cell_has_full_modifier(raw: &str, action: CellFormatAction, source_is_csv: bool) -> bool {
    if source_is_csv {
        return false;
    }
    if action == CellFormatAction::Code {
        let display = crate::markdown_adapter::MarkdownCellDisplay::parse_fragment(raw);
        return !display.runs.is_empty() && display.runs.iter().all(|run| run.code && !run.link);
    }
    let semantic = crate::document::literal_text_value(raw);
    let visible = crate::markdown_adapter::MarkdownCellDisplay::parse_fragment(raw).plain_text;
    modifier_markers(action).iter().any(|marker| {
        has_marked_edges(semantic, marker)
            && semantic
                .strip_prefix(marker)
                .and_then(|source| source.strip_suffix(marker))
                .is_some_and(|inner| {
                    crate::markdown_adapter::MarkdownCellDisplay::parse_fragment(inner).plain_text
                        == visible
                })
    })
}

/// Lists GFM markers that can represent one whole-cell modifier.
fn modifier_markers(action: CellFormatAction) -> &'static [&'static str] {
    match action {
        CellFormatAction::Bold => &["**", "__"],
        CellFormatAction::Italic => &["*", "_"],
        CellFormatAction::Strike => &["~~"],
        CellFormatAction::Code => &[],
    }
}

/// Avoids treating the inner half of bold markup as an existing italic marker.
fn has_marked_edges(source: &str, marker: &str) -> bool {
    if marker != "*" {
        return source.starts_with(marker) && source.ends_with(marker);
    }
    let leading = source
        .chars()
        .take_while(|character| *character == '*')
        .count();
    let trailing = source
        .chars()
        .rev()
        .take_while(|character| *character == '*')
        .count();
    leading % 2 == 1 && trailing % 2 == 1
}

#[cfg(test)]
#[path = "selection_test.rs"]
mod tests;
