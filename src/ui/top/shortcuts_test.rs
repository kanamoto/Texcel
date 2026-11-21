use super::*;

/// Builds a platform command modifier set for shortcut checks.
fn command_modifiers(is_mac: bool, shift: bool) -> egui::Modifiers {
    let mut modifiers = egui::Modifiers::COMMAND;
    if is_mac {
        modifiers.mac_cmd = true;
    } else {
        modifiers.ctrl = true;
    }
    modifiers.shift = shift;
    modifiers
}

/// Builds one key press with its own modifier snapshot.
fn key_event(key: Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// Builds one OS key-repeat event.
fn key_repeat_event(key: Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: true,
        modifiers,
    }
}

#[test]
// Verify IME and modal gates preserve input and repeated keys trigger only once.
fn shortcut_collection_respects_ime_modal_and_repeat_gates() {
    let ctx = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(ctx.clone());
    let mut app = TexcelApp::new_with_settings_path(&creation, None);
    let is_mac = ctx.os().is_mac();
    let key = key_event(Key::N, command_modifiers(is_mac, false));
    ctx.input_mut(|input| input.events = vec![key.clone(), key]);

    assert_eq!(app.take_app_shortcut(&ctx, true), None);
    assert_eq!(ctx.input(|input| input.events.len()), 2);
    app.edit.set_composing(true);
    assert_eq!(app.take_app_shortcut(&ctx, false), None);
    app.edit.set_composing(false);

    assert_eq!(
        app.take_app_shortcut(&ctx, false),
        Some(AppShortcut::NewDocument)
    );
    assert!(ctx.input(|input| input.events.is_empty()));
    assert_eq!(app.take_app_shortcut(&ctx, false), None);

    ctx.input_mut(|input| {
        input.events = vec![key_repeat_event(Key::N, command_modifiers(is_mac, false))]
    });
    assert_eq!(app.take_app_shortcut(&ctx, false), None);
    assert!(ctx.input(|input| input.events.is_empty()));

    let format = key_event(Key::B, command_modifiers(is_mac, false));
    ctx.input_mut(|input| input.events = vec![format.clone()]);
    app.edit.set_composing(true);
    assert_eq!(app.take_app_shortcut(&ctx, false), None);
    assert_eq!(ctx.input(|input| input.events.clone()), vec![format]);
    app.edit.set_composing(false);
    assert_eq!(
        app.take_app_shortcut(&ctx, false),
        Some(AppShortcut::Format(CellFormatAction::Bold))
    );
    assert!(ctx.input(|input| input.events.is_empty()));
}
