use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct EditInputTarget {
    pub(super) document_id: DocumentId,
    pub(super) table_id: TableId,
    pub(super) address: CellAddress,
    pub(super) session_id: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct EditInputPosition {
    pub(super) buffer_revision: u64,
    pub(super) anchor_byte: usize,
    pub(super) cursor_byte: usize,
}

impl EditInputPosition {
    // Captures cursor state to reject stale picker commands after the editor moves.
    pub(super) fn from_edit(edit: &EditSession) -> Self {
        Self {
            buffer_revision: edit.buffer_revision,
            anchor_byte: edit.anchor_byte,
            cursor_byte: edit.cursor_byte,
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::ui) struct PendingEditInput {
    pub(super) target: EditInputTarget,
    pub(super) position: Option<EditInputPosition>,
    pub(super) draft: Option<EditSession>,
    pub(super) mode: EditMode,
    pub(super) events: Vec<egui::Event>,
    pub(super) boundary_sent: bool,
    pub(super) waiting_growth: bool,
    pub(super) owner_change_pending: bool,
}
