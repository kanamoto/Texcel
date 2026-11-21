use super::*;

/// Keeps spreadsheet navigation keys inside the focused grid control.
pub(super) fn lock_grid_navigation(ui: &egui::Ui, response: &egui::Response) {
    if response.has_focus() {
        ui.memory_mut(|memory| memory.set_focus_lock_filter(response.id, grid_navigation_filter()));
    }
}

/// Cancels egui's pending focus move before it reaches the focused grid widget.
pub(super) fn retain_grid_keyboard_focus(
    ctx: &egui::Context,
    owner: Option<Id>,
    can_restore_escape: bool,
) {
    let Some(owner) = owner else {
        return;
    };
    let has_navigation_key = ctx.input(|input| {
        input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Key {
                    key: Key::ArrowUp
                        | Key::ArrowDown
                        | Key::ArrowLeft
                        | Key::ArrowRight
                        | Key::Tab
                        | Key::Escape,
                    pressed: true,
                    ..
                }
            )
        })
    });
    if !has_navigation_key {
        return;
    }
    let can_restore_escape = can_restore_escape
        && ctx.input(|input| {
            !input.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::WindowFocused(false)
                        | egui::Event::PointerButton {
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            ..
                        }
                )
            })
        });
    let escape_pressed = ctx.input(|input| input.key_pressed(Key::Escape));
    ctx.memory_mut(|memory| match memory.focused() {
        Some(focused) if focused == owner => {
            memory.move_focus(egui::FocusDirection::None);
            memory.set_focus_lock_filter(owner, grid_navigation_filter());
        }
        None if escape_pressed && can_restore_escape => {
            memory.move_focus(egui::FocusDirection::None);
            memory.request_focus(owner);
            memory.set_focus_lock_filter(owner, grid_navigation_filter());
        }
        _ => (),
    });
}

/// Keeps the grid's arrows, Tab, and Escape inside its current owner.
fn grid_navigation_filter() -> egui::EventFilter {
    egui::EventFilter {
        tab: true,
        horizontal_arrows: true,
        vertical_arrows: true,
        escape: true,
    }
}
