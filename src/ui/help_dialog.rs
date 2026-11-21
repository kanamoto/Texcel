use super::*;

impl TexcelApp {
    // Shows concise operating instructions, formula support, and app information.
    pub(super) fn draw_help_dialog(&mut self, ctx: &egui::Context) {
        if !self.help_window_open {
            return;
        }
        let language = self.language;
        let mut close = false;
        let modal_id = Id::unique("texcel_help");
        let cancel_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, egui::Key::Escape, self.edit.composing);
        egui::Modal::new(modal_id).show(ctx, |ui| {
            let width = (ctx.content_rect().width() - 40.0).clamp(320.0, 680.0);
            ui.set_min_width(width.min(480.0));
            ui.set_max_width(width);
            ui.set_max_height((ctx.content_rect().height() - 40.0).max(240.0));
            visual::dialog_surface(ui, |ui| {
                visual::prepare_dialog_body(ui);
                visual::dialog_title(ui, &text(language, "help-title"));
                visual::group_gap(ui);
                let footer_height = visual::BUTTON_HEIGHT + visual::SPACE_8;
                let body_height = (ui.available_height() - footer_height).max(120.0);
                egui::ScrollArea::vertical()
                    .max_height(body_height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        visual::section_title(ui, &text(language, "help-operations-heading"));
                        ui.label(text(language, "help-operations"));
                        visual::caption(
                            ui,
                            &text(
                                language,
                                if cfg!(target_os = "macos") {
                                    "help-undo-command"
                                } else {
                                    "help-undo-control"
                                },
                            ),
                        );
                        visual::group_gap(ui);
                        visual::section_title(ui, &text(language, "help-display-heading"));
                        ui.label(text(language, "help-display"));
                        visual::group_gap(ui);
                        visual::section_title(ui, &text(language, "help-formulas-heading"));
                        ui.label(text(language, "help-formulas"));
                        visual::group_gap(ui);
                        ui.separator();
                        visual::section_title(ui, &text(language, "help-about-heading"));
                        ui.label(format!("Texcel {}", env!("CARGO_PKG_VERSION")));
                    });
                ui.separator();
                visual::dialog_footer(ui, |ui| {
                    if ui
                        .add_sized(
                            [112.0, visual::BUTTON_HEIGHT],
                            egui::Button::new(text(language, "action-close")),
                        )
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
        });
        self.help_window_open = !(close || cancel_key);
    }
}
