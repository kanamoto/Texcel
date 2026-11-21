use crate::ui::{visual, *};

pub(super) enum DataEditFooterAction {
    Apply,
    Preview,
    Close,
    None,
}

/// Draws the platform-ordered actions and reports the one the user selected.
pub(super) fn draw_data_edit_footer(
    ui: &mut egui::Ui,
    language: crate::i18n::Language,
    pending: bool,
    options_unchanged: bool,
    preview_ready: bool,
) -> DataEditFooterAction {
    let can_apply = !pending && options_unchanged && preview_ready;
    let mut action = DataEditFooterAction::None;
    if cfg!(target_os = "macos") {
        if data_edit_button(
            ui,
            text(language, "action-apply-diff"),
            can_apply,
            can_apply,
        ) {
            action = DataEditFooterAction::Apply;
        }
        if data_edit_button(
            ui,
            text(language, "dialog-data-create-preview"),
            !pending,
            !can_apply,
        ) {
            action = DataEditFooterAction::Preview;
        }
        if ui.button(text(language, "action-close")).clicked() {
            action = DataEditFooterAction::Close;
        }
    } else {
        if ui.button(text(language, "action-close")).clicked() {
            action = DataEditFooterAction::Close;
        }
        if data_edit_button(
            ui,
            text(language, "dialog-data-create-preview"),
            !pending,
            !can_apply,
        ) {
            action = DataEditFooterAction::Preview;
        }
        if data_edit_button(
            ui,
            text(language, "action-apply-diff"),
            can_apply,
            can_apply,
        ) {
            action = DataEditFooterAction::Apply;
        }
    }
    action
}

/// Keeps sort and data-edit actions aligned while showing the ready action.
fn data_edit_button(ui: &mut egui::Ui, label: String, enabled: bool, primary: bool) -> bool {
    let button = egui::Button::new(label);
    let button = if primary {
        button.fill(ui.visuals().selection.bg_fill)
    } else {
        button
    };
    ui.add_enabled_ui(enabled, |ui| {
        ui.add_sized([132.0, visual::BUTTON_HEIGHT], button)
    })
    .inner
    .clicked()
}
