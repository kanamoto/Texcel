use super::*;

/// Identifies the one real text field allowed to survive the shared Edit popup.
#[cfg(any(test, not(any(target_os = "macos", target_os = "windows"))))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SharedEditMenuTarget {
    CellEditor {
        document_id: DocumentId,
        table_id: TableId,
        session_id: u64,
        address: Option<CellAddress>,
        focus: Id,
        cursor_range: Option<egui::text::CCursorRange>,
    },
    NameBox {
        document_id: DocumentId,
        table_id: TableId,
        focus: Id,
        cursor_range: Option<egui::text::CCursorRange>,
    },
}

#[cfg(any(test, not(any(target_os = "macos", target_os = "windows"))))]
impl SharedEditMenuTarget {
    /// Returns the stable focus ID captured from the actual text widget.
    pub(crate) fn focus(self) -> Id {
        match self {
            Self::CellEditor { focus, .. } | Self::NameBox { focus, .. } => focus,
        }
    }

    /// Checks whether a repeated menu frame still refers to the same text widget.
    #[cfg(any(test, not(any(target_os = "macos", target_os = "windows"))))]
    pub(crate) fn same_widget(self, other: Self) -> bool {
        match (self, other) {
            (
                Self::CellEditor {
                    document_id: left_document,
                    table_id: left_table,
                    session_id: left_session,
                    address: left_address,
                    focus: left_focus,
                    ..
                },
                Self::CellEditor {
                    document_id: right_document,
                    table_id: right_table,
                    session_id: right_session,
                    address: right_address,
                    focus: right_focus,
                    ..
                },
            ) => {
                left_document == right_document
                    && left_table == right_table
                    && left_session == right_session
                    && left_address == right_address
                    && left_focus == right_focus
            }
            (
                Self::NameBox {
                    document_id: left_document,
                    table_id: left_table,
                    focus: left_focus,
                    ..
                },
                Self::NameBox {
                    document_id: right_document,
                    table_id: right_table,
                    focus: right_focus,
                    ..
                },
            ) => {
                left_document == right_document
                    && left_table == right_table
                    && left_focus == right_focus
            }
            _ => false,
        }
    }
}

/// Shares the name box's stable ID between rendering and menu ownership checks.
pub(crate) fn name_box_id(document_id: DocumentId) -> Id {
    Id::unique(egui::IdSalt::new(("name_box", document_id)))
}

/// Reports an IME update before a popup can change the active text owner.
pub(crate) fn context_has_ime_event(ctx: &egui::Context) -> bool {
    ctx.input(|input| {
        input
            .events
            .iter()
            .any(|event| matches!(event, egui::Event::Ime(_)))
    })
}
