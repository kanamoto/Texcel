use super::*;

impl TexcelApp {
    // Turns supported formula-edit keys into picker commands.
    pub(super) fn formula_input_command(&self, event: &egui::Event) -> Option<FormulaInputCommand> {
        if !self.formula_reference_edit_is_current() {
            return None;
        }
        match event {
            egui::Event::Key {
                key: Key::F2,
                pressed: true,
                ..
            } => Some(FormulaInputCommand::Toggle),
            egui::Event::Key {
                key: Key::F4,
                pressed: true,
                ..
            } => Some(FormulaInputCommand::Cycle),
            egui::Event::Key {
                key: Key::T,
                pressed: true,
                modifiers: egui::Modifiers { command: true, .. },
                ..
            } if cfg!(target_os = "macos") => Some(FormulaInputCommand::Cycle),
            egui::Event::Key {
                key: key @ (Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight),
                pressed: true,
                modifiers,
                ..
            } if self.edit.mode == EditMode::FormulaReference
                && !modifiers.command
                && !modifiers.ctrl
                && !modifiers.alt =>
            {
                Some(FormulaInputCommand::Move(*key, modifiers.shift))
            }
            _ => None,
        }
    }

    // Applies one picker command while preserving the editor's text position.
    pub(super) fn apply_formula_input_command(
        &mut self,
        command: FormulaInputCommand,
        ctx: &egui::Context,
    ) -> bool {
        match command {
            FormulaInputCommand::Toggle => self.toggle_formula_reference_mode(),
            FormulaInputCommand::Cycle => self.cycle_formula_reference(ctx),
            FormulaInputCommand::Move(key, extend) => {
                let (row_delta, column_delta) = match key {
                    Key::ArrowUp => (-1, 0),
                    Key::ArrowDown => (1, 0),
                    Key::ArrowLeft => (0, -1),
                    Key::ArrowRight => (0, 1),
                    _ => return false,
                };
                self.move_formula_reference(row_delta, column_delta, extend, ctx)
            }
        }
    }
}

// Prevents a picker key from moving ahead of an earlier edit or lifecycle event.
pub(super) fn is_formula_input_lifecycle_barrier(event: &egui::Event) -> bool {
    matches!(
        event,
        egui::Event::Key {
            key: Key::Enter | Key::Tab | Key::Escape,
            pressed: true,
            ..
        }
    ) || matches!(
        event,
        egui::Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } if (modifiers.command || modifiers.ctrl)
            && !(cfg!(target_os = "macos") && *key == Key::T)
    )
}

#[derive(Clone, Copy)]
pub(super) enum FormulaInputCommand {
    Toggle,
    Cycle,
    Move(Key, bool),
}
