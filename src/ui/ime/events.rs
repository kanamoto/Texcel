use super::super::*;

/// Identifies any event that belongs to an IME stream.
pub(super) fn is_ime_event(event: &egui::Event) -> bool {
    matches!(event, egui::Event::Ime(_))
}

/// Accepts the terminal event still sent by older native IME backends.
#[expect(
    deprecated,
    reason = "Keep compatibility with the fixed egui IME event stream."
)]
pub(super) fn is_ime_disabled_event(event: &egui::Event) -> bool {
    matches!(event, egui::Event::Ime(egui::ImeEvent::Disabled))
}

/// Identifies IME events that end or cancel one composition.
pub(super) fn is_ime_terminal_event(event: &egui::Event) -> bool {
    match event {
        egui::Event::Ime(egui::ImeEvent::Commit(_)) => true,
        _ => is_ime_disabled_event(event),
    }
}

/// Checks whether one event is an empty unmark that can precede its commit.
#[allow(deprecated)]
pub(super) fn is_empty_preedit(event: &egui::Event) -> bool {
    matches!(event, egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) if text.is_empty())
}

/// Identifies normal input that ends a pending unmark-only composition.
pub(super) fn interrupts_unmarked_ime(event: &egui::Event) -> bool {
    matches!(
        event,
        egui::Event::Text(_)
            | egui::Event::Paste(_)
            | egui::Event::WindowFocused(false)
            | egui::Event::PointerButton { pressed: true, .. }
            | egui::Event::Key { pressed: true, .. }
    )
}

/// Keeps Escape in the editor's composition-cancel path.
pub(super) fn is_escape_key(event: &egui::Event) -> bool {
    matches!(
        event,
        egui::Event::Key {
            key: Key::Escape,
            pressed: true,
            ..
        }
    )
}

/// Identifies modal events that move keyboard focus away from its text field.
pub(super) fn is_modal_leave_event(event: &egui::Event) -> bool {
    matches!(
        event,
        egui::Event::WindowFocused(false)
            | egui::Event::Key {
                key: Key::Tab,
                pressed: true,
                ..
            }
            | egui::Event::PointerButton { pressed: true, .. }
    )
}

/// Identifies a pointer press that leaves an editor after an IME commit.
pub(super) fn is_pointer_press(event: &egui::Event) -> bool {
    matches!(event, egui::Event::PointerButton { pressed: true, .. })
}

/// Checks for cursor input that the snapshot cannot safely reproduce.
pub(super) fn is_untracked_cursor_input(event: &egui::Event) -> bool {
    matches!(
        event,
        egui::Event::Key { pressed: true, .. } | egui::Event::PointerButton { pressed: true, .. }
    )
}

/// Checks for text that the snapshot already contains before the click.
pub(super) fn is_tracked_text_event(event: &egui::Event) -> bool {
    matches!(event, egui::Event::Text(_) | egui::Event::Paste(_))
}

/// Finds an earlier commit already included in a later snapshot.
pub(super) fn is_ime_commit_event(event: &egui::Event) -> bool {
    matches!(event, egui::Event::Ime(egui::ImeEvent::Commit(_)))
}

/// Identifies keys that move or delete around the caret inside an open preedit.
pub(super) fn is_preedit_cursor_key(key: Key) -> bool {
    matches!(
        key,
        Key::ArrowLeft
            | Key::ArrowRight
            | Key::ArrowUp
            | Key::ArrowDown
            | Key::Home
            | Key::End
            | Key::PageUp
            | Key::PageDown
            | Key::Backspace
            | Key::Delete
    )
}
