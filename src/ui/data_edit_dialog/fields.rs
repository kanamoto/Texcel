use super::*;

/// Aligns compact data fields on one label and input column.
pub(super) fn data_edit_text_row(
    ui: &mut egui::Ui,
    label: String,
    value: &mut String,
    field_width: f32,
) -> egui::Response {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(144.0, visual::BUTTON_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_width(144.0);
                ui.set_max_width(144.0);
                ui.add(egui::Label::new(label).halign(egui::Align::Min));
            },
        );
        ui.add_sized(
            [field_width, visual::BUTTON_HEIGHT],
            egui::TextEdit::singleline(value),
        )
    })
    .inner
}
