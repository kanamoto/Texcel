//!  Document operations for history.

use crate::document::*;

impl Document {
    /// Returns the identity of the content at the current undo position.
    pub(crate) fn history_state_id(&self) -> u64 {
        self.history_state_ids[self.history_cursor]
    }

    /// Gives a new branch a distinct identity after its command is appended.
    pub(super) fn record_history_state(&mut self) {
        self.history_state_ids.truncate(self.history_cursor);
        let state_id = self.next_history_state_id;
        self.next_history_state_id = self.next_history_state_id.saturating_add(1);
        self.history_state_ids.push(state_id);
    }

    /// Marks restored recovery content as a state beyond its saved base.
    pub(crate) fn record_recovered_history_state(&mut self) {
        if self.revision != self.saved_revision {
            let state_id = self.next_history_state_id;
            self.next_history_state_id = self.next_history_state_id.saturating_add(1);
            self.history_state_ids[self.history_cursor] = state_id;
        }
    }

    // Applies a command, clears redo history, and advances the document version.
    pub fn execute(&mut self, mut command: EditCommand) -> Result<(), DocumentError> {
        self.resource_limit_notice = None;
        self.pending_text_source_changes = None;
        let resource_limit_notice = match self.check_command_text_source_limit(&command) {
            Ok(notice) => notice,
            Err(DocumentError::TextSourceValidationRequired) => {
                if let Some((table_id, changes, string_overrides)) = command.text_source_changes() {
                    self.pending_text_source_changes =
                        Some(crate::document::PendingTextSourceChanges {
                            table_id,
                            changes: changes
                                .iter()
                                .map(|change| (change.address, change.after.clone()))
                                .collect(),
                            string_overrides: string_overrides.to_vec(),
                        });
                }
                return Err(DocumentError::TextSourceValidationRequired);
            }
            Err(error) => return Err(error),
        };
        let changes_text_overlay = matches!(&command, EditCommand::ReplaceStructure { .. });
        // Record the affected content before applying the command to the document.
        let changed = command.changed_addresses().collect::<Vec<_>>();
        let calculation_changed = command
            .calculation_changed_addresses(true)
            .collect::<Vec<_>>();
        let changes_calculation = !calculation_changed.is_empty();
        let resets_calculation = command.resets_calculation();
        // Apply the command before adding it to the undo history.
        command.apply(self, true, false)?;
        let reset_snapshot = resets_calculation
            .then(|| command.current_calculation_snapshot())
            .flatten();
        // Save the command and update document versions for later workers and undo.
        self.record_changed_cells(changed.iter().copied());
        self.calculation_cells.extend(calculation_changed);
        self.history.truncate(self.history_cursor);
        self.history.push(command);
        self.history_cursor += 1;
        self.record_history_state();
        self.revision = self.revision.saturating_add(1);
        // Advance input and calculation versions only for the changes they track.
        if changes_text_overlay {
            self.note_structure_change();
        }
        if changes_text_overlay {
            self.advance_cell_input_generation();
        } else {
            self.mark_cell_inputs_changed(&changed);
        }
        if changes_calculation {
            self.advance_calculation_revision();
        }
        if resets_calculation {
            self.advance_calculation_revision();
            self.pending_calculation_reset = reset_snapshot;
        }
        self.resource_limit_notice = resource_limit_notice;
        Ok(())
    }
}
