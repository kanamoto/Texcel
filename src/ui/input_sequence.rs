use super::*;

// Keeps the original column while a direct-entry Tab sequence is active.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct InputSequenceOrigin {
    pub(super) document_id: DocumentId,
    pub(super) table_id: TableId,
    pub(super) column: usize,
    pub(super) expected_cell: CellAddress,
}

impl TexcelApp {
    /// Ends Tab entry when the active document, table, or cell changes.
    pub(super) fn validate_input_sequence_context(&mut self) {
        let matches_expected = self.input_sequence_origin.is_some_and(|origin| {
            self.workspace.active_document().is_some_and(|document| {
                document.id == origin.document_id
                    && document.active_table().is_some_and(|table| {
                        table.id == origin.table_id
                            && document
                                .view(table.id)
                                .is_some_and(|view| view.active_cell() == origin.expected_cell)
                    })
            })
        });
        if !matches_expected {
            self.input_sequence_origin = None;
        }
    }

    /// Keeps the first column for a continuous Tab entry sequence.
    pub(super) fn start_input_sequence(
        &mut self,
        document_id: DocumentId,
        table_id: TableId,
        address: CellAddress,
    ) {
        if self.input_sequence_origin.is_none_or(|origin| {
            origin.document_id != document_id
                || origin.table_id != table_id
                || origin.expected_cell != address
        }) {
            self.input_sequence_origin = Some(InputSequenceOrigin {
                document_id,
                table_id,
                column: address.column,
                expected_cell: address,
            });
        }
    }

    /// Moves the expected cell or ends the sequence after a commit.
    pub(super) fn finish_input_sequence_move(
        &mut self,
        document_id: DocumentId,
        table_id: TableId,
        continue_tab_input: bool,
    ) {
        if !continue_tab_input {
            self.input_sequence_origin = None;
            return;
        }
        let Some(active_cell) = self
            .workspace
            .documents
            .get(&document_id)
            .and_then(|document| document.view(table_id))
            .map(|view| view.active_cell())
        else {
            self.input_sequence_origin = None;
            return;
        };
        if let Some(origin) = self.input_sequence_origin.as_mut()
            && origin.document_id == document_id
            && origin.table_id == table_id
        {
            origin.expected_cell = active_cell;
        } else {
            self.input_sequence_origin = None;
        }
    }

    /// Advances a waiting Tab entry and keeps its original column.
    pub(super) fn move_waiting_tab_input(&mut self, shift: bool) -> bool {
        let Some(origin) = self.input_sequence_origin else {
            return false;
        };
        self.move_selection(0, if shift { -1 } else { 1 }, false);
        self.finish_input_sequence_move(origin.document_id, origin.table_id, true);
        true
    }

    /// Returns a waiting Enter move to the Tab sequence's original column.
    pub(super) fn move_waiting_enter_input(&mut self, shift: bool) -> bool {
        let Some(origin) = self.input_sequence_origin else {
            return false;
        };
        self.move_to_input_sequence_column(
            origin.document_id,
            origin.table_id,
            origin.column,
            if shift { -1 } else { 1 },
        );
        self.input_sequence_origin = None;
        true
    }

    /// Moves by visible row, then selects the exact starting column.
    pub(super) fn move_to_input_sequence_column(
        &mut self,
        document_id: DocumentId,
        table_id: TableId,
        column: usize,
        row_delta: i32,
    ) -> bool {
        if self.workspace.active_document != Some(document_id) {
            return false;
        }
        let Some(active_cell) = self
            .workspace
            .documents
            .get(&document_id)
            .and_then(|document| document.view(table_id))
            .map(|view| view.active_cell())
        else {
            return false;
        };
        self.move_selection(row_delta, 0, false);
        let moved_row = self
            .workspace
            .documents
            .get(&document_id)
            .and_then(|document| document.view(table_id))
            .map(|view| view.active_cell().row)
            .unwrap_or(active_cell.row);
        let Some(last_column) = self
            .workspace
            .documents
            .get(&document_id)
            .and_then(|document| document.tables.iter().find(|table| table.id == table_id))
            .map(|_| MAX_COLUMNS - 1)
        else {
            return false;
        };
        self.select_cell(
            table_id,
            CellAddress {
                row: moved_row,
                column: column.min(last_column),
            },
            false,
        );
        true
    }
}
