use super::formula_reference_input::is_formula_input_lifecycle_barrier;
use super::*;
#[path = "ordered_edit_input/growth.rs"]
mod growth;
#[path = "ordered_edit_input/owner.rs"]
mod owner;
#[path = "ordered_edit_input/state.rs"]
mod state;
pub(super) use owner::is_ordered_input_owner_change;
pub(super) use state::PendingEditInput;
use state::{EditInputPosition, EditInputTarget};

impl TexcelApp {
    // Stages ordered cell input before egui derives focus and IME state.
    pub(super) fn stage_ordered_edit_input(
        &mut self,
        ctx: &egui::Context,
        raw: &mut egui::RawInput,
    ) {
        let fresh = std::mem::take(&mut raw.events);
        let mut pending = self.pending_edit_input.take();
        if pending.is_none()
            && self.grid_focused
            && !self.edit.is_editing()
            && !self.modal_input_blocked()
            && fresh.iter().any(growth::is_growth_ordered_event)
            && let Some(document_id) = self.workspace.active_document
            && let Some(growth) = self.pending_text_growth_sessions.get(&document_id)
            && growth.pending
            && let (Some(table_id), Some(address)) =
                (growth.session.table_id, growth.session.address)
        {
            // Capture the next frame while the earlier edge commit waits for its source update.
            pending = Some(PendingEditInput {
                target: EditInputTarget {
                    document_id,
                    table_id,
                    address,
                    session_id: Some(growth.session.session_id),
                },
                position: Some(EditInputPosition::from_edit(&growth.session)),
                draft: Some(growth.session.clone()),
                mode: growth.session.mode,
                events: Vec::new(),
                boundary_sent: true,
                waiting_growth: true,
                owner_change_pending: false,
            });
        }
        if let Some(item) = pending.as_mut()
            && (item.waiting_growth || self.pending_growth_matches_input(item))
        {
            item.waiting_growth = true;
            item.events.extend(fresh);
            self.pending_edit_input = pending;
            raw.events.clear();
            return;
        }
        if pending.is_some() && self.formula_input_blocked() {
            self.pending_edit_input = pending;
            raw.events = fresh;
            return;
        }
        if pending.is_none() && self.formula_input_blocked() {
            raw.events = fresh;
            return;
        }

        let mut events = fresh;
        let mut suppress_references = self.edit.composing;
        if let Some(mut item) = pending.take() {
            let target_matches = if let Some(session_id) = item.target.session_id {
                self.edit_input_target_matches(item.target, session_id)
            } else {
                self.selected_input_target() == Some(item.target)
            };
            if !target_matches {
                if item.waiting_growth {
                    self.pending_edit_input = Some(item);
                    raw.events = events;
                    return;
                }
                if !item.owner_change_pending
                    && item.mode == EditMode::FormulaReference
                    && item.draft.is_some()
                    && self.workspace.active_document == Some(item.target.document_id)
                    && !self.edit.is_editing()
                    && self.edit_document_id.is_none()
                {
                    self.restore_pending_edit_draft(&mut item);
                    if let Some(session_id) = item.target.session_id {
                        if self.edit_input_target_matches(item.target, session_id) {
                            suppress_references = item.position.is_some_and(|position| {
                                position != EditInputPosition::from_edit(&self.edit)
                            });
                        } else {
                            item.events.append(&mut events);
                            self.pending_edit_input = Some(item);
                            raw.events = Vec::new();
                            return;
                        }
                    }
                } else if !item.owner_change_pending {
                    raw.events = events;
                    return;
                }
            } else if let Some(position) = item.position {
                suppress_references |= position != EditInputPosition::from_edit(&self.edit);
            }
            let mut ordered = std::mem::take(&mut item.events);
            ordered.append(&mut events);
            events = ordered;
            pending = Some(item);
        }

        let mut deferred_owner_events = None;
        if let Some(index) = events.iter().position(is_ordered_input_owner_change)
            && events[..index]
                .iter()
                .any(|event| matches!(event, egui::Event::Ime(_)))
            && !events[..index].iter().any(|event| {
                self.ordered_edit_boundary(
                    event,
                    if self.edit.is_editing() {
                        self.edit.mode
                    } else {
                        EditMode::Input
                    },
                )
                .is_some()
            })
        {
            // Keep IME snapshots and their following pointer in one ordered frame.
            raw.events = events;
            self.pending_edit_input = pending;
            return;
        }
        if let Some(index) = events.iter().position(is_ordered_input_owner_change) {
            let tail = events.split_off(index);
            if index == 0 {
                let mut tail = tail;
                let owner_event = tail.remove(0);
                raw.events = vec![owner_event];
                if !tail.is_empty() {
                    let target = pending
                        .as_ref()
                        .map(|item| item.target)
                        .or_else(|| self.current_edit_input_target())
                        .or_else(|| self.selected_input_target());
                    if let Some(target) = target {
                        self.defer_ordered_owner_events(pending.take(), target, tail);
                    } else {
                        raw.events.extend(tail);
                    }
                }
                return;
            }
            deferred_owner_events = Some(tail);
        }

        let has_formula_target = self.formula_reference_edit_is_current();
        let routing_mode = if self.edit.is_editing() {
            self.edit.mode
        } else {
            EditMode::Input
        };
        let has_boundary = events
            .iter()
            .any(|event| self.ordered_edit_boundary(event, routing_mode).is_some());
        let has_text = events.iter().any(is_ordered_input_text);
        let has_pending = pending.is_some();
        if !has_pending && !has_formula_target && !(has_boundary && has_text) {
            raw.events = events;
            if let Some(tail) = deferred_owner_events {
                if let Some(target) = self
                    .current_edit_input_target()
                    .or_else(|| self.selected_input_target())
                {
                    self.defer_ordered_owner_events(pending.take(), target, tail);
                } else {
                    raw.events.extend(tail);
                }
            }
            return;
        }

        if !self.edit.is_editing()
            && self.grid_focused
            && !self.modal_input_blocked()
            && self.edit_document_id.is_none()
            && let Some(target) = pending
                .as_ref()
                .map(|item| item.target)
                .or_else(|| self.selected_input_target())
        {
            let first_boundary = events
                .iter()
                .position(|event| self.ordered_edit_boundary(event, EditMode::Input).is_some());
            let text_before_boundary = events
                .iter()
                .take(first_boundary.unwrap_or(events.len()))
                .any(is_ordered_input_text);
            let starts_with_text = events.first().is_some_and(is_ordered_input_text);
            if text_before_boundary || pending.is_some() && starts_with_text {
                self.begin_selected_replacing(String::new());
            } else if first_boundary.is_some() && has_text && pending.is_none() {
                pending = Some(PendingEditInput {
                    target,
                    position: None,
                    draft: None,
                    mode: EditMode::Input,
                    events: Vec::new(),
                    boundary_sent: false,
                    waiting_growth: false,
                    owner_change_pending: false,
                });
            }
        }

        let Some(target) = self
            .current_edit_input_target()
            .or_else(|| pending.as_ref().map(|item| item.target))
            .or_else(|| self.selected_input_target())
        else {
            raw.events = events;
            if let Some(tail) = deferred_owner_events {
                raw.events.extend(tail);
            }
            return;
        };
        let mut output = Vec::with_capacity(events.len());
        let mut barrier = false;
        let mut routing_allowed = !suppress_references && !self.edit.composing;
        let mut index = 0;
        while index < events.len() {
            let event = &events[index];
            if matches!(event, egui::Event::Ime(_)) {
                routing_allowed = false;
            }
            if matches!(event, egui::Event::WindowFocused(false)) {
                routing_allowed = false;
            }
            if is_formula_input_lifecycle_barrier(event) {
                routing_allowed = false;
            }
            if routing_allowed && let Some(command) = self.formula_input_command(event) {
                if barrier {
                    self.pending_edit_input = Some(PendingEditInput {
                        target,
                        position: None,
                        draft: None,
                        mode: routing_mode,
                        events: events[index..].to_vec(),
                        boundary_sent: false,
                        waiting_growth: false,
                        owner_change_pending: false,
                    });
                    break;
                }
                if self.apply_formula_input_command(command, ctx) {
                    index += 1;
                    continue;
                }
                output.push(event.clone());
                barrier = true;
                index += 1;
                continue;
            }
            if self.ordered_edit_boundary(event, routing_mode).is_some() {
                if barrier {
                    self.pending_edit_input = Some(PendingEditInput {
                        target,
                        position: None,
                        draft: None,
                        mode: routing_mode,
                        events: events[index..].to_vec(),
                        boundary_sent: false,
                        waiting_growth: false,
                        owner_change_pending: false,
                    });
                    break;
                }
                output.push(event.clone());
                if index + 1 < events.len() {
                    self.pending_edit_input = Some(PendingEditInput {
                        target,
                        position: None,
                        draft: None,
                        mode: routing_mode,
                        events: events[index + 1..].to_vec(),
                        boundary_sent: true,
                        waiting_growth: false,
                        owner_change_pending: false,
                    });
                }
                break;
            }
            output.push(event.clone());
            barrier = true;
            index += 1;
        }
        raw.events = output;
        if let Some(tail) = deferred_owner_events {
            self.defer_ordered_owner_events(pending.take(), target, tail);
        }
    }

    // Finishes the queued suffix with the editor or selection produced by this frame.
    pub(super) fn finish_ordered_edit_input_frame(&mut self) {
        let Some(mut pending) = self.pending_edit_input.take() else {
            return;
        };
        if self.pending_growth_matches_input(&pending) {
            pending.waiting_growth = true;
            self.pending_edit_input = Some(pending);
            return;
        }
        if pending.owner_change_pending {
            pending.waiting_growth = false;
            self.pending_edit_input = Some(pending);
            return;
        }
        if let Some(session_id) = pending.target.session_id
            && self.edit_input_target_matches(pending.target, session_id)
        {
            pending.position = Some(EditInputPosition::from_edit(&self.edit));
            pending.draft = Some(self.edit.clone());
            pending.mode = self.edit.mode;
            pending.waiting_growth = false;
            pending.boundary_sent = false;
            self.pending_edit_input = Some(pending);
            return;
        }
        if (pending.waiting_growth || pending.boundary_sent)
            && let Some(target) = self.selected_input_target()
            && target.document_id == pending.target.document_id
            && target.table_id == pending.target.table_id
        {
            pending.target = target;
            pending.position = None;
            pending.draft = None;
            pending.mode = EditMode::Input;
            pending.boundary_sent = false;
            pending.waiting_growth = false;
            self.pending_edit_input = Some(pending);
            return;
        }
        self.pending_edit_input = None;
    }

    // Clears an ordered suffix when its edit follows normal commit or cancel behavior.
    pub(super) fn clear_pending_edit_input(&mut self) {
        let matches_edit = self.pending_edit_input.as_ref().is_some_and(|pending| {
            self.edit_document_id == Some(pending.target.document_id)
                && self.edit.table_id == Some(pending.target.table_id)
                && self.edit.address == Some(pending.target.address)
                && pending.target.session_id == Some(self.edit.session_id)
        });
        if matches_edit {
            self.pending_edit_input = None;
        }
    }

    // Keeps picker commands out of blocked or unrelated input owners.
    fn formula_input_blocked(&self) -> bool {
        self.modal_input_blocked()
            || self
                .workspace
                .active_document()
                .is_some_and(|document| self.cell_edit_locked(document.id))
    }

    // Restores a deferred draft only when its original document is active again.
    fn restore_pending_edit_draft(&mut self, pending: &mut PendingEditInput) {
        if self.edit.is_editing() || self.edit_document_id.is_some() {
            return;
        }
        if pending.waiting_growth
            || pending.target.session_id.is_none()
            || self.workspace.active_document != Some(pending.target.document_id)
        {
            return;
        }
        let Some(draft) = pending.draft.take() else {
            return;
        };
        self.edit = draft;
        self.edit_document_id = Some(pending.target.document_id);
        self.refresh_formula_references();
        self.edit.mode = pending.mode;
    }

    // Captures the active text editor for an ordered event suffix.
    fn current_edit_input_target(&self) -> Option<EditInputTarget> {
        let document_id = self.edit_document_id?;
        let table_id = self.edit.table_id?;
        let address = self.edit.address?;
        (self.workspace.active_document == Some(document_id)
            && self.edit.is_editing()
            && self
                .workspace
                .active_document()
                .and_then(|document| document.active_table())
                .is_some_and(|table| table.id == table_id))
        .then_some(EditInputTarget {
            document_id,
            table_id,
            address,
            session_id: Some(self.edit.session_id),
        })
    }

    // Captures the selected cell before its first replacement editor is opened.
    fn selected_input_target(&self) -> Option<EditInputTarget> {
        let document = self.workspace.active_document()?;
        let table = document.active_table()?;
        let address = document.view(table.id)?.active_cell();
        Some(EditInputTarget {
            document_id: document.id,
            table_id: table.id,
            address,
            session_id: None,
        })
    }

    // Confirms that an ordered suffix still belongs to the same edit session.
    fn edit_input_target_matches(&self, target: EditInputTarget, session_id: u64) -> bool {
        self.current_edit_input_target().is_some_and(|current| {
            current.document_id == target.document_id
                && current.table_id == target.table_id
                && current.address == target.address
                && current.session_id == Some(session_id)
        })
    }

    // Keeps a suffix paused while an adjacent-cell edit waits for the worker.
    fn pending_growth_matches_input(&self, pending: &PendingEditInput) -> bool {
        let Some(session_id) = pending.target.session_id else {
            return false;
        };
        self.pending_text_growth_sessions
            .get(&pending.target.document_id)
            .is_some_and(|growth| {
                growth.pending
                    && growth.session.session_id == session_id
                    && growth.session.table_id == Some(pending.target.table_id)
                    && growth.session.address == Some(pending.target.address)
            })
    }

    // Identifies keys that finish, cancel, or change a selected-cell input.
    fn ordered_edit_boundary(&self, event: &egui::Event, mode: EditMode) -> Option<Key> {
        let egui::Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } = event
        else {
            return None;
        };
        if modifiers.command || modifiers.ctrl || modifiers.alt {
            return None;
        }
        match mode {
            EditMode::Input => matches!(
                key,
                Key::F2
                    | Key::ArrowUp
                    | Key::ArrowDown
                    | Key::ArrowLeft
                    | Key::ArrowRight
                    | Key::Enter
                    | Key::Tab
                    | Key::Escape
            )
            .then_some(*key),
            EditMode::Edit | EditMode::FormulaReference => {
                matches!(key, Key::Enter | Key::Tab | Key::Escape).then_some(*key)
            }
        }
    }
}

// Identifies ordinary text that can begin a replacement edit.
fn is_ordered_input_text(event: &egui::Event) -> bool {
    matches!(event, egui::Event::Text(text) if !text.is_empty())
}
