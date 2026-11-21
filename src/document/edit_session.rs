//!  Tracks cell editing text, IME composition, and commit keys.

use super::*;

// Where the active input is displayed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditTarget {
    Cell,
    FormulaBar,
    DetailPanel,
}

// Describes what arrow keys do in the active edit session.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EditMode {
    Input,
    #[default]
    Edit,
    FormulaReference,
}

// A key kind that describes an editing key event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputKey {
    Enter,
    AltEnter,
    Tab,
    Escape,
    F2,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
}

// How the UI handles an editing key: commit, cancel, or move.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputAction {
    PropagateToIme,
    InsertLineBreak,
    CommitAndMoveDown,
    CommitAndMoveUp,
    CommitAndMoveRight,
    CommitAndMoveLeft,
    Cancel,
    ToggleMode,
    MoveSelection,
    Ignore,
}

// The source buffer and IME composition state shared by the cell and formula bar.
#[derive(Clone, Debug, Default)]
pub struct EditSession {
    pub table_id: Option<TableId>,
    pub address: Option<CellAddress>,
    pub target: Option<EditTarget>,
    pub mode: EditMode,
    pub original: Option<CellInput>,
    pub buffer: String,
    /// Absolute UTF-8 offsets used when a long edit buffer is shown through a bounded window.
    pub anchor_byte: usize,
    pub cursor_byte: usize,
    pub edit_window_start: Option<usize>,
    pub composing: bool,
    /// Keeps a recovered selection anchor until the user interacts with its editor.
    pub preserve_recovered_anchor: bool,
    // A generation number that keeps egui edit widget state separate for each session.
    pub session_id: u64,
    // A generation number for cached views of the current text buffer.
    pub buffer_revision: u64,
}

impl EditSession {
    // Returns whether text editing is active.
    pub fn is_editing(&self) -> bool {
        self.target.is_some()
    }

    // Starts editing a cell and copies its source input into the shared buffer.
    pub fn begin(
        &mut self,
        table_id: TableId,
        address: CellAddress,
        original: CellInput,
        target: EditTarget,
    ) {
        self.session_id = self.session_id.saturating_add(1).max(1);
        self.mark_buffer_changed();
        self.table_id = Some(table_id);
        self.address = Some(address);
        self.buffer = original.source_text();
        self.anchor_byte = self.buffer.len();
        self.cursor_byte = self.buffer.len();
        self.edit_window_start = None;
        self.original = Some(original);
        self.target = Some(target);
        self.mode = EditMode::Edit;
        self.composing = false;
        self.preserve_recovered_anchor = false;
    }

    // Changes the input target between the formula bar and cell while keeping uncommitted text.
    pub fn switch_target(&mut self, target: EditTarget) {
        if self.is_editing() {
            self.target = Some(target);
        }
    }

    // Updates the IME composition state received from the UI.
    pub fn set_composing(&mut self, composing: bool) {
        self.composing = composing;
    }

    // Invalidates cached views after the source text changes.
    pub fn mark_buffer_changed(&mut self) {
        self.buffer_revision = self.buffer_revision.saturating_add(1);
    }

    // Passes IME keys through and classifies other editing keys as actions.
    pub fn key_action(&self, key: InputKey, shift: bool) -> InputAction {
        if self.composing {
            return InputAction::PropagateToIme;
        }
        match (self.is_editing(), key, shift) {
            (true, InputKey::AltEnter, _) => InputAction::InsertLineBreak,
            (true, InputKey::F2, _) => InputAction::ToggleMode,
            (true, InputKey::Enter, false) => InputAction::CommitAndMoveDown,
            (true, InputKey::Enter, true) => InputAction::CommitAndMoveUp,
            (true, InputKey::Tab, false) => InputAction::CommitAndMoveRight,
            (true, InputKey::Tab, true) => InputAction::CommitAndMoveLeft,
            (true, InputKey::Escape, _) => InputAction::Cancel,
            (true, InputKey::ArrowUp, false) => match self.mode {
                EditMode::Input => InputAction::CommitAndMoveUp,
                EditMode::Edit | EditMode::FormulaReference => InputAction::MoveSelection,
            },
            (true, InputKey::ArrowDown, false) => match self.mode {
                EditMode::Input => InputAction::CommitAndMoveDown,
                EditMode::Edit | EditMode::FormulaReference => InputAction::MoveSelection,
            },
            (true, InputKey::ArrowLeft, false) => match self.mode {
                EditMode::Input => InputAction::CommitAndMoveLeft,
                EditMode::Edit | EditMode::FormulaReference => InputAction::MoveSelection,
            },
            (true, InputKey::ArrowRight, false) => match self.mode {
                EditMode::Input => InputAction::CommitAndMoveRight,
                EditMode::Edit | EditMode::FormulaReference => InputAction::MoveSelection,
            },
            (
                true,
                InputKey::ArrowUp
                | InputKey::ArrowDown
                | InputKey::ArrowLeft
                | InputKey::ArrowRight,
                true,
            ) => InputAction::MoveSelection,
            (
                false,
                InputKey::ArrowUp
                | InputKey::ArrowDown
                | InputKey::ArrowLeft
                | InputKey::ArrowRight,
                _,
            ) => InputAction::MoveSelection,
            _ => InputAction::Ignore,
        }
    }

    // Builds committed input from the shared buffer.
    pub fn completed_input(&self) -> Option<CellInput> {
        self.original
            .as_ref()
            .map(|original| original.edited_source(self.buffer.clone()))
    }

    // Discards uncommitted edits and returns to the initial state.
    pub fn reset(&mut self) {
        let next_session_id = self.session_id.saturating_add(1).max(1);
        *self = Self {
            session_id: next_session_id,
            ..Self::default()
        };
    }
}
