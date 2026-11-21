use super::*;

#[test]
// Keep an active shared editor focused without interrupting its IME.
fn repeated_preedit_does_not_interrupt_shared_editor_composition() {
    for target in [
        crate::document::EditTarget::Cell,
        crate::document::EditTarget::FormulaBar,
        crate::document::EditTarget::DetailPanel,
    ] {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let (document_id, _, _) = edit_blank_cell(&mut app);
        app.edit.switch_target(target);
        app.long_text_panel_open = target == crate::document::EditTarget::DetailPanel;
        let editor = editor_id(document_id, app.edit.session_id);
        ctx.memory_mut(|memory| memory.request_focus(editor));
        let mut frame = eframe::Frame::_new_kittest();
        let mut time = 0.0;
        run_frame(&mut app, &ctx, &mut frame, &mut time, Vec::new());
        assert!(ctx.memory(|memory| memory.has_focus(editor)));
        let editor_name = format!("shared editor {target:?}");

        assert!(!preedit_interrupts(
            &mut app,
            &ctx,
            &mut frame,
            &mut time,
            "に",
            &editor_name
        ));
        assert!(!preedit_interrupts(
            &mut app,
            &ctx,
            &mut frame,
            &mut time,
            "日本",
            &editor_name
        ));
    }
}

#[test]
// Keep the formula editor enabled through IME commit and preserve its final text.
fn formula_editor_keeps_preedit_and_commit_frames_active() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let (document_id, _, _) = edit_blank_cell(&mut app);
    app.edit
        .switch_target(crate::document::EditTarget::FormulaBar);
    let editor = editor_id(document_id, app.edit.session_id);
    ctx.memory_mut(|memory| memory.request_focus(editor));
    let mut frame = eframe::Frame::_new_kittest();
    let mut time = 0.0;
    run_frame(&mut app, &ctx, &mut frame, &mut time, Vec::new());
    assert!(ctx.memory(|memory| memory.has_focus(editor)));

    assert!(!ime_event_interrupts(
        &mut app,
        &ctx,
        &mut frame,
        &mut time,
        Event::Ime(egui::ImeEvent::Preedit {
            text: "にほん".to_owned(),
            active_range_chars: None,
        }),
        "formula editor",
    ));
    assert!(!ime_event_interrupts(
        &mut app,
        &ctx,
        &mut frame,
        &mut time,
        Event::Ime(egui::ImeEvent::Commit("日本".to_owned())),
        "formula editor",
    ));
    assert_eq!(app.edit.buffer, "日本");
}

#[test]
// Keep a pending document-close lock stronger than IME navigation blocking.
fn formula_ime_does_not_override_a_pending_close_lock() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let (document_id, _, _) = edit_blank_cell(&mut app);
    app.edit
        .switch_target(crate::document::EditTarget::FormulaBar);
    app.close_after_save.insert(document_id);
    let editor = editor_id(document_id, app.edit.session_id);
    ctx.memory_mut(|memory| memory.request_focus(editor));
    let mut frame = eframe::Frame::_new_kittest();
    let mut time = 0.0;
    run_frame(&mut app, &ctx, &mut frame, &mut time, Vec::new());

    assert!(
        ime_event_output(
            &mut app,
            &ctx,
            &mut frame,
            &mut time,
            Event::Ime(egui::ImeEvent::Preedit {
                text: "にほん".to_owned(),
                active_range_chars: None,
            }),
        )
        .is_none()
    );
}

// Return whether a preedit frame asks egui to interrupt composition.
fn preedit_interrupts(
    app: &mut TexcelApp,
    ctx: &egui::Context,
    frame: &mut eframe::Frame,
    time: &mut f64,
    text: &str,
    editor_name: &str,
) -> bool {
    ime_event_interrupts(
        app,
        ctx,
        frame,
        time,
        Event::Ime(egui::ImeEvent::Preedit {
            text: text.to_owned(),
            active_range_chars: None,
        }),
        editor_name,
    )
}

// Return whether an IME input frame asks egui to interrupt composition.
fn ime_event_interrupts(
    app: &mut TexcelApp,
    ctx: &egui::Context,
    frame: &mut eframe::Frame,
    time: &mut f64,
    event: Event,
    editor_name: &str,
) -> bool {
    ime_event_output(app, ctx, frame, time, event)
        .unwrap_or_else(|| panic!("{editor_name} exposes IME output"))
}

// Return IME output only when an editor owns composition in this frame.
fn ime_event_output(
    app: &mut TexcelApp,
    ctx: &egui::Context,
    frame: &mut eframe::Frame,
    time: &mut f64,
    event: Event,
) -> Option<bool> {
    *time += 1.0 / 60.0;
    let mut raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 620.0))),
        time: Some(*time),
        events: vec![event],
        ..RawInput::default()
    };
    eframe::App::raw_input_hook(app, ctx, &mut raw);
    let mut output = ctx.run_ui(raw, |ui| eframe::App::ui(app, ui, frame));
    let interrupted = output
        .platform_output
        .ime
        .take()
        .map(|ime| ime.should_interrupt_composition);
    output.textures_delta.clear();
    interrupted
}
