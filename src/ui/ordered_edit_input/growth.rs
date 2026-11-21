// Captures useful input while a one-cell source expansion waits for acknowledgement.
use super::super::*;

pub(super) fn is_growth_ordered_event(event: &egui::Event) -> bool {
    matches!(
        event,
        egui::Event::Text(_)
            | egui::Event::Ime(_)
            | egui::Event::Key { .. }
            | egui::Event::PointerButton { .. }
            | egui::Event::Paste(_)
            | egui::Event::Copy
            | egui::Event::Cut
            | egui::Event::WindowFocused(false)
    )
}
