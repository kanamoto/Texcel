use super::*;

/// Stores an edit until its worker-prepared table growth is accepted.
#[derive(Clone, Debug)]
pub(super) struct PendingTextGrowthEdit {
    pub(super) session: EditSession,
    pub(super) movement: Option<(i32, i32)>,
    pub(super) selection: CellRange,
    pub(super) active_cell: CellAddress,
    pub(super) additional_selections: Vec<CellRange>,
    pub(super) pending: bool,
    pub(super) deferred_save_path: Option<PathBuf>,
    pub(super) close_after_save: bool,
    pub(super) close_edit: Option<EditSession>,
    pub(super) app_close_save: bool,
    pub(super) continue_tab_input: bool,
    pub(super) return_column: Option<usize>,
}

/// Remembers the grid target while keyboard input waits for a coordinate change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PendingAxisSpaceText {
    pub(super) document_id: DocumentId,
    pub(super) table_id: TableId,
    pub(super) selection: CellRange,
    pub(super) active_cell: CellAddress,
    pub(super) focus: Id,
}

/// Retains a chosen save path and close intent across worker responses.
#[derive(Clone, Debug)]
pub(super) struct PendingSaveIntent {
    pub(super) path: PathBuf,
    pub(super) options: ExportOptions,
    pub(super) track_recent: bool,
    pub(super) close_after_save: bool,
    pub(super) close_edit: Option<EditSession>,
    pub(super) app_close_generation: Option<u64>,
}

/// Keeps the active IME composition tied to its original editor session.
#[derive(Clone, Debug)]
pub(super) struct ImeCompositionState {
    pub(super) document_id: DocumentId,
    pub(super) table_id: TableId,
    pub(super) session_id: u64,
    pub(super) editor_id: Id,
    pub(super) buffer: String,
    pub(super) anchor_byte: usize,
    pub(super) cursor_byte: usize,
    pub(super) edit_window_start: Option<usize>,
    pub(super) preedit_active: bool,
}
