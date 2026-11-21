use super::*;

impl TexcelApp {
    // Retargets ordinary input after the earlier pointer action selects its owner.
    pub(in crate::ui) fn finish_ordered_input_owner_change(&mut self) {
        let Some(mut pending) = self.pending_edit_input.take() else {
            return;
        };
        if !pending.owner_change_pending {
            self.pending_edit_input = Some(pending);
            return;
        }
        if pending.events.iter().any(is_ordered_input_owner_change) {
            self.pending_edit_input = Some(pending);
            return;
        }
        let active_document = self.workspace.active_document;
        if active_document == Some(pending.target.document_id) {
            pending.owner_change_pending = false;
            self.pending_edit_input = Some(pending);
            self.finish_ordered_edit_input_frame();
            return;
        }
        if pending.mode == EditMode::FormulaReference {
            pending
                .events
                .retain(|event| !is_stale_formula_picker_command(event));
        }
        let Some(target) = self.selected_input_target() else {
            self.pending_edit_input = Some(pending);
            return;
        };
        let starts_grid_text = pending.events.iter().any(is_ordered_input_text);
        pending.target = target;
        pending.position = None;
        pending.draft = None;
        pending.mode = EditMode::Input;
        pending.boundary_sent = false;
        pending.waiting_growth = false;
        pending.owner_change_pending = false;
        if starts_grid_text && !self.modal_input_blocked() {
            self.grid_focused = true;
            self.request_grid_focus = true;
        }
        self.pending_edit_input = (!pending.events.is_empty()).then_some(pending);
    }

    // Holds events after a click until the new cell or document owns them.
    pub(super) fn defer_ordered_owner_events(
        &mut self,
        pending: Option<PendingEditInput>,
        target: EditInputTarget,
        events: Vec<egui::Event>,
    ) {
        let mut item = self
            .pending_edit_input
            .take()
            .or(pending)
            .unwrap_or_else(|| PendingEditInput {
                target,
                position: self
                    .current_edit_input_target()
                    .map(|_| EditInputPosition::from_edit(&self.edit)),
                draft: self.edit.is_editing().then(|| self.edit.clone()),
                mode: self.edit.mode,
                events: Vec::new(),
                boundary_sent: false,
                waiting_growth: false,
                owner_change_pending: false,
            });
        item.target = target;
        item.owner_change_pending |= events
            .iter()
            .any(|event| matches!(event, egui::Event::PointerButton { .. }));
        item.events.extend(events);
        item.boundary_sent = true;
        self.pending_edit_input = Some(item);
    }
}

// Prevents old editor input from crossing focus changes or starting IME composition.
pub(in crate::ui) fn is_ordered_input_owner_change(event: &egui::Event) -> bool {
    matches!(
        event,
        egui::Event::WindowFocused(false) | egui::Event::PointerButton { .. }
    )
}

// Keeps a formula pick from an old document out of the newly selected grid.
fn is_stale_formula_picker_command(event: &egui::Event) -> bool {
    matches!(
        event,
        egui::Event::Key {
            key: Key::F2 | Key::F4,
            pressed: true,
            ..
        }
    ) || matches!(
        event,
        egui::Event::Key {
            key: Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight,
            pressed: true,
            modifiers,
            ..
        } if !modifiers.command && !modifiers.ctrl && !modifiers.alt
    ) || cfg!(target_os = "macos")
        && matches!(
            event,
            egui::Event::Key {
                key: Key::T,
                pressed: true,
                modifiers: egui::Modifiers { command: true, .. },
                ..
            }
        )
}
