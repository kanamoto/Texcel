//!  Document operations for undo.

use std::sync::Arc;

use crate::document::*;

impl Document {
    // Reverts the last edit and returns the table and cell to show.
    pub fn undo(&mut self) -> Result<Option<(TableId, CellAddress)>, DocumentError> {
        let Some(previous_index) = self.history_cursor.checked_sub(1) else {
            return Ok(None);
        };
        let mut command = self.history.remove(previous_index);
        let changes_text_overlay = matches!(&command, EditCommand::ReplaceStructure { .. });
        let target = command.first_target(false);
        let calculation_changed = command
            .calculation_changed_addresses(false)
            .collect::<Vec<_>>();
        let changes_calculation = !calculation_changed.is_empty();
        if let Err(error) = command.apply(self, false, true) {
            self.history.insert(previous_index, command);
            return Err(error);
        }
        if let Some((table_id, _)) = target
            && self.views.contains_key(&table_id)
        {
            self.active_table = table_id;
        }
        let changed = command.changed_addresses().collect::<Vec<_>>();
        self.record_changed_cells(changed.iter().copied());
        self.calculation_cells.extend(calculation_changed);
        self.history_cursor -= 1;
        self.revision = self.revision.saturating_add(1);
        if changes_text_overlay {
            self.note_structure_change();
        }
        if changes_text_overlay {
            self.advance_cell_input_generation();
        } else {
            self.mark_cell_inputs_changed(&changed);
        }
        if changes_text_overlay {
            self.text_overlay_revision = self.revision;
        }
        if changes_calculation {
            self.advance_calculation_revision();
        }
        if command.resets_calculation() {
            self.advance_calculation_revision();
            self.pending_calculation_reset = command.current_calculation_snapshot();
            if let Some(snapshot) = &mut self.pending_calculation_reset {
                Arc::make_mut(snapshot).preserve_string_cells(&self.tables);
            }
        }
        self.history.insert(previous_index, command);
        Ok(target)
    }

    // Reapplies the next edit and returns the table and cell to show.
    pub fn redo(&mut self) -> Result<Option<(TableId, CellAddress)>, DocumentError> {
        if self.history_cursor >= self.history.len() {
            return Ok(None);
        }
        let current_index = self.history_cursor;
        let mut command = self.history.remove(current_index);
        let changes_text_overlay = matches!(&command, EditCommand::ReplaceStructure { .. });
        let target = command.first_target(true);
        let calculation_changed = command
            .calculation_changed_addresses(true)
            .collect::<Vec<_>>();
        let changes_calculation = !calculation_changed.is_empty();
        if let Err(error) = command.apply(self, true, true) {
            self.history.insert(current_index, command);
            return Err(error);
        }
        if let Some((table_id, _)) = target
            && self.views.contains_key(&table_id)
        {
            self.active_table = table_id;
        }
        let changed = command.changed_addresses().collect::<Vec<_>>();
        self.record_changed_cells(changed.iter().copied());
        self.calculation_cells.extend(calculation_changed);
        self.history_cursor += 1;
        self.revision = self.revision.saturating_add(1);
        if changes_text_overlay {
            self.note_structure_change();
        }
        if changes_text_overlay {
            self.advance_cell_input_generation();
        } else {
            self.mark_cell_inputs_changed(&changed);
        }
        if changes_text_overlay {
            self.text_overlay_revision = self.revision;
        }
        if changes_calculation {
            self.advance_calculation_revision();
        }
        if command.resets_calculation() {
            self.advance_calculation_revision();
            self.pending_calculation_reset = command.current_calculation_snapshot();
            if let Some(snapshot) = &mut self.pending_calculation_reset {
                Arc::make_mut(snapshot).preserve_string_cells(&self.tables);
            }
        }
        self.history.insert(current_index, command);
        Ok(target)
    }
}
