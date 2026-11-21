use super::*;

const STATUS_NOTICE_DURATION: Duration = Duration::from_secs(5);

impl TexcelApp {
    /// Clears a previous notice before the next user operation is handled.
    pub(super) fn clear_status_for_user_action(&mut self, ctx: &egui::Context) {
        let has_action = ctx.input(|input| {
            !input.raw.dropped_files.is_empty() || input.events.iter().any(is_user_action)
        });
        if has_action {
            self.status = None;
            self.status_lifetime = None;
        }
    }

    /// Expires old status messages and schedules one repaint at their deadline.
    pub(super) fn refresh_status_lifetime(&mut self, ctx: &egui::Context) {
        let Some(message) = self.status.clone() else {
            self.status_lifetime = None;
            return;
        };
        let now = Instant::now();
        let started_at = self
            .status_lifetime
            .as_ref()
            .filter(|(previous, _)| previous == &message)
            .map(|(_, started_at)| *started_at)
            .unwrap_or_else(|| {
                self.status_lifetime = Some((message, now));
                now
            });
        let elapsed = now.saturating_duration_since(started_at);
        if elapsed >= STATUS_NOTICE_DURATION {
            self.status = None;
            self.status_lifetime = None;
        } else {
            ctx.request_repaint_after(STATUS_NOTICE_DURATION - elapsed);
        }
    }
}

/// Recognizes user actions while ignoring pointer motion and focus changes.
fn is_user_action(event: &egui::Event) -> bool {
    match event {
        egui::Event::Key { pressed: true, .. }
        | egui::Event::PointerButton { pressed: true, .. }
        | egui::Event::MouseWheel { .. }
        | egui::Event::Cut
        | egui::Event::Copy
        | egui::Event::AccessKitActionRequest(_) => true,
        egui::Event::Text(text) | egui::Event::Paste(text) => !text.is_empty(),
        egui::Event::Ime(_) => true,
        _ => false,
    }
}

#[cfg(test)]
#[path = "status_notice_test.rs"]
mod tests;
