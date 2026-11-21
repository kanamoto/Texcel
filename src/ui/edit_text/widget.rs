use super::*;

/// Draws the requested editor and keeps its shared text behavior.
pub(super) fn show_cell_edit_widget(
    ui: &mut egui::Ui,
    text: &mut String,
    id: Id,
    widget: CellEditWidget,
) -> egui::text_edit::TextEditOutput {
    match widget {
        CellEditWidget::Singleline { width, frame_none } => {
            let editor = egui::TextEdit::singleline(text)
                .id(id)
                .return_key(None)
                .desired_width(width);
            if frame_none {
                editor.frame(egui::Frame::NONE).show(ui)
            } else {
                editor.show(ui)
            }
        }
        CellEditWidget::FormulaBarSingleline { width, height } => {
            ui.scope(|ui| {
                let radius = egui::CornerRadius::same(FORMULA_BAR_CORNER_RADIUS);
                let visuals = ui.visuals_mut();
                visuals.widgets.noninteractive.corner_radius = radius;
                visuals.widgets.inactive.corner_radius = radius;
                visuals.widgets.hovered.corner_radius = radius;
                visuals.widgets.active.corner_radius = radius;
                visuals.widgets.open.corner_radius = radius;
                egui::TextEdit::singleline(text)
                    .id(id)
                    .return_key(None)
                    .desired_width(width)
                    .font(FontId::monospace(14.0))
                    .margin(egui::Margin::symmetric(8, 0))
                    .align(Align2::LEFT_CENTER)
                    .min_size(Vec2::new(width, height))
                    .show(ui)
            })
            .inner
        }
        CellEditWidget::Multiline {
            width,
            rows,
            lock_focus,
        } => egui::TextEdit::multiline(text)
            .id(id)
            .desired_width(width)
            .desired_rows(rows)
            .lock_focus(lock_focus)
            .show(ui),
    }
}
