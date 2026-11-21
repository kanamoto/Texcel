use super::*;

impl TexcelApp {
    // Refreshes formula tokens once when the active edit buffer changes.
    pub(super) fn refresh_formula_references(&mut self) {
        let Some(document_id) = self.edit_document_id else {
            self.formula_references.clear();
            return;
        };
        let Some(table_id) = self.edit.table_id else {
            self.formula_references.clear();
            return;
        };
        let Some(address) = self.edit.address else {
            self.formula_references.clear();
            return;
        };
        let is_formula = self.edit.buffer.starts_with('=');
        if is_formula && self.edit.mode == EditMode::Input {
            self.edit.mode = EditMode::FormulaReference;
        } else if !is_formula && self.edit.mode == EditMode::FormulaReference {
            self.edit.mode = EditMode::Edit;
        }
        let has_table = self
            .workspace
            .documents
            .get(&document_id)
            .is_some_and(|document| document.tables.iter().any(|table| table.id == table_id));
        if !has_table {
            self.formula_references.clear();
            return;
        }
        self.formula_references.refresh(
            Some(document_id),
            Some(table_id),
            Some(address),
            &self.edit,
            MAX_ROWS,
            MAX_COLUMNS,
        );
    }

    // Checks that the formula picker still belongs to the current document edit.
    pub(super) fn formula_reference_edit_is_current(&self) -> bool {
        let Some((document_id, table_id, address, session_id)) = self.formula_references.target()
        else {
            return false;
        };
        self.workspace.active_document().is_some_and(|document| {
            document.id == document_id
                && document
                    .active_table()
                    .is_some_and(|table| table.id == table_id)
        }) && self.edit_document_id == Some(document_id)
            && self.edit.table_id == Some(table_id)
            && self.edit.address == Some(address)
            && self.edit.session_id == session_id
            && matches!(
                self.edit.target,
                Some(crate::document::EditTarget::Cell | crate::document::EditTarget::FormulaBar)
            )
            && self.edit.buffer.starts_with('=')
    }

    // Inserts or replaces the reference selected in the grid.
    pub(super) fn insert_formula_reference(
        &mut self,
        range: CellRange,
        ctx: &egui::Context,
    ) -> bool {
        if !self.formula_reference_edit_is_current()
            || self.edit.mode != EditMode::FormulaReference
            || self.edit.composing
            || !matches!(
                self.edit.target,
                Some(crate::document::EditTarget::Cell | crate::document::EditTarget::FormulaBar)
            )
        {
            return false;
        }
        self.formula_references.sync_cursor(&self.edit);
        let captured_selection = self
            .text_selection
            .filter(|selection| {
                Some(selection.document_id) == self.edit_document_id
                    && Some(selection.table_id) == self.edit.table_id
                    && Some(selection.address) == self.edit.address
                    && selection.session_id == self.edit.session_id
                    && selection.buffer_revision == self.edit.buffer_revision
                    && {
                        let start = byte_index_for_char(&self.edit.buffer, selection.start);
                        let end = byte_index_for_char(&self.edit.buffer, selection.end);
                        self.edit.anchor_byte.min(self.edit.cursor_byte) == start
                            && self.edit.anchor_byte.max(self.edit.cursor_byte) == end
                    }
            })
            .map(|selection| {
                byte_index_for_char(&self.edit.buffer, selection.start)
                    ..byte_index_for_char(&self.edit.buffer, selection.end)
            });
        let insertion = self
            .formula_references
            .insertion_range(&self.edit, captured_selection);
        if insertion.start > insertion.end
            || insertion.end > self.edit.buffer.len()
            || !self.edit.buffer.is_char_boundary(insertion.start)
            || !self.edit.buffer.is_char_boundary(insertion.end)
        {
            self.refresh_formula_references();
            return false;
        }
        let (start, end) = range.normalized();
        let first = crate::xlsx_adapter::cell_reference(range.anchor);
        let last = crate::xlsx_adapter::cell_reference(range.head);
        let value = if start == end {
            first
        } else {
            format!("{first}:{last}")
        };
        self.edit.buffer.replace_range(insertion.clone(), &value);
        let value_end = insertion.start + value.len();
        self.edit.anchor_byte = value_end;
        self.edit.cursor_byte = value_end;
        self.edit.mark_buffer_changed();
        self.text_selection = None;
        self.edit
            .switch_target(crate::document::EditTarget::FormulaBar);
        self.grid_focused = false;
        self.request_editor_focus = true;
        self.refresh_formula_references();
        self.formula_references.set_owned_inserted_reference(
            &self.edit,
            insertion.start..value_end,
            range,
        );
        self.sync_formula_editor_cursor(ctx);
        true
    }

    // Moves the reference target through visible cells without moving the formula cell.
    pub(super) fn move_formula_reference(
        &mut self,
        row_delta: i32,
        column_delta: i32,
        extend: bool,
        ctx: &egui::Context,
    ) -> bool {
        if !self.formula_reference_edit_is_current()
            || self.edit.mode != EditMode::FormulaReference
            || self.edit.composing
        {
            return false;
        }
        let Some((document_id, table_id, base, anchor, target)) =
            self.workspace.active_document().and_then(|document| {
                let table = document.active_table()?;
                let view = document.view(table.id)?;
                let existing = self.formula_references.current_range;
                let base = existing.map_or_else(|| view.active_cell(), |range| range.head);
                let anchor = existing.map_or_else(|| view.active_cell(), |range| range.anchor);
                let target =
                    grid_navigation_target(base, row_delta, column_delta, MAX_ROWS, MAX_COLUMNS);
                Some((document.id, table.id, base, anchor, target))
            })
        else {
            return false;
        };
        if target == base {
            return true;
        }
        let range = if extend {
            CellRange {
                anchor,
                head: target,
            }
        } else {
            CellRange::single(target)
        };
        if self.insert_formula_reference(range, ctx) {
            self.request_scroll_to_selection = false;
            self.request_scroll_to_address = Some((document_id, table_id, target));
        }
        true
    }

    // Applies F4 to the reference under the text cursor or selection.
    pub(super) fn cycle_formula_reference(&mut self, ctx: &egui::Context) -> bool {
        if !self.formula_reference_edit_is_current()
            || self.edit.composing
            || !matches!(
                self.edit.target,
                Some(crate::document::EditTarget::Cell | crate::document::EditTarget::FormulaBar)
            )
        {
            return false;
        }
        self.formula_references.sync_cursor(&self.edit);
        let Some(cells) = self
            .formula_references
            .token_at_cursor(&self.edit)
            .map(|token| token.cells)
        else {
            return false;
        };
        if self
            .formula_references
            .cycle_reference(&mut self.edit)
            .is_none()
        {
            return false;
        }
        self.refresh_formula_references();
        self.formula_references.set_current_range(Some(cells));
        self.sync_formula_editor_cursor(ctx);
        true
    }

    // Keeps the visible editor cursor aligned with byte offsets in the shared buffer.
    pub(super) fn sync_formula_editor_cursor(&self, ctx: &egui::Context) {
        let Some(document_id) = self.edit_document_id else {
            return;
        };
        let id = editor_id(document_id, self.edit.session_id);
        let Some(mut state) = egui::widgets::text_edit::TextEditState::load(ctx, id) else {
            return;
        };
        let anchor = self.edit.buffer[..self.edit.anchor_byte].chars().count();
        let cursor = self.edit.buffer[..self.edit.cursor_byte].chars().count();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(anchor),
                egui::text::CCursor::new(cursor),
            )));
        state.store(ctx, id);
    }

    // Switches between formula text editing and grid reference selection.
    pub(super) fn toggle_formula_reference_mode(&mut self) -> bool {
        if !self.formula_reference_edit_is_current() || self.edit.composing {
            return false;
        }
        self.formula_references.sync_cursor(&self.edit);
        self.edit.mode = if self.edit.mode == EditMode::FormulaReference {
            EditMode::Edit
        } else {
            EditMode::FormulaReference
        };
        if self.edit.mode != EditMode::FormulaReference {
            self.grid_selection_drag = None;
            self.formula_references.drag_anchor = None;
        }
        true
    }

    // Begins a grid drag as a formula reference instead of a table selection.
    pub(super) fn start_formula_reference_drag(
        &mut self,
        drag: GridSelectionDrag,
        ctx: &egui::Context,
    ) -> bool {
        if !self.formula_reference_edit_is_current() || self.edit.mode != EditMode::FormulaReference
        {
            return false;
        }
        let Some(session_id) = self.formula_references.target().map(|target| target.3) else {
            return false;
        };
        let anchor = if drag.extend {
            self.formula_references
                .current_range
                .map_or(drag.pointer_start, |range| range.anchor)
        } else {
            drag.pointer_start
        };
        let drag = GridSelectionDrag {
            anchor,
            formula_reference_session: Some(session_id),
            ..drag
        };
        if !self.insert_formula_reference(
            CellRange {
                anchor,
                head: drag.pointer_start,
            },
            ctx,
        ) {
            return false;
        }
        self.formula_references.drag_anchor = Some(anchor);
        self.grid_selection_drag = Some(drag);
        true
    }

    // Extends the formula range to the cell under the pointer.
    pub(super) fn update_formula_reference_drag(
        &mut self,
        drag: GridSelectionDrag,
        head: CellAddress,
        ctx: &egui::Context,
    ) -> bool {
        if !self.formula_reference_drag_is_current(drag) {
            return false;
        }
        if !self.insert_formula_reference(
            CellRange {
                anchor: drag.anchor,
                head,
            },
            ctx,
        ) {
            return false;
        }
        self.formula_references.drag_anchor = Some(drag.anchor);
        true
    }

    // Keeps a pointer gesture attached to the edit session that started it.
    pub(super) fn formula_reference_drag_is_current(&self, drag: GridSelectionDrag) -> bool {
        drag.formula_reference_session == Some(self.edit.session_id)
            && self.formula_reference_edit_is_current()
            && self.edit.mode == EditMode::FormulaReference
    }

    // Cancels a formula-owned pointer gesture when its edit ends.
    pub(super) fn cancel_formula_reference_drag(&mut self) {
        if self
            .grid_selection_drag
            .is_some_and(|drag| drag.formula_reference_session.is_some())
        {
            self.grid_selection_drag = None;
            self.formula_references.drag_anchor = None;
        }
    }
}
