use super::super::*;

impl TexcelApp {
    /// Keeps a Space shortcut tied to the focused grid selection.
    pub(super) fn current_axis_space_target(
        &self,
        ctx: &egui::Context,
    ) -> Option<PendingAxisSpaceText> {
        if !self.grid_focused
            || self.edit.is_editing()
            || self.edit.composing
            || ctx.text_edit_focused()
        {
            return None;
        }
        let document = self.workspace.active_document()?;
        let table = document.active_table()?;
        let view = document.view(table.id)?;
        Some(PendingAxisSpaceText {
            document_id: document.id,
            table_id: table.id,
            selection: view.selection,
            active_cell: view.active_cell(),
            focus: self.grid_keyboard_owner?,
        })
    }
}
