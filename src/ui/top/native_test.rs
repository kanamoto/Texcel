use super::*;
use egui_kittest::Harness;

/// Waits until the opened path belongs to a document with an active table.
fn load_backed_markdown(
    harness: &mut Harness<'_, TexcelApp>,
    path: &std::path::Path,
) -> (u64, u64) {
    let document_id = harness
        .state_mut()
        .start_path_load(path.to_path_buf())
        .expect("Markdown load is submitted");
    for _ in 0..200 {
        harness.step();
        if harness
            .state()
            .user_file_paths
            .get(&document_id)
            .is_some_and(|user_path| user_path == path)
        {
            let table_id = harness.state().workspace.documents[&document_id]
                .active_table()
                .expect("loaded Markdown has a table")
                .id;
            return (document_id, table_id);
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("Markdown load did not finish");
}

fn command_modifiers(ctx: &egui::Context) -> egui::Modifiers {
    let mut modifiers = egui::Modifiers::COMMAND;
    if ctx.os().is_mac() {
        modifiers.mac_cmd = true;
    } else {
        modifiers.ctrl = true;
    }
    modifiers
}

fn key_event(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn test_app() -> (TexcelApp, egui::Context) {
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    (TexcelApp::new_with_settings_path(&creation, None), context)
}

#[test]
// Verify native edits use the live modal text owner and shared deferred command path.
fn modal_text_commands_require_the_current_dialog_field() {
    let (mut app, context) = test_app();
    app.start_search_dialog();
    let document_id = app.workspace.active_document.expect("a document is active");
    let field_id = egui::Id::unique(egui::IdSalt::new(("search_query", document_id)));
    app.last_modal_text_input_focus = Some(field_id);
    app.active_modal_text_input_focus = Some(field_id);
    app.last_text_input_focus = Some(field_id);
    let mut state = TopActionState {
        command_owner: Some(InputOwner::OtherText),
        command_focus: Some(field_id),
        ..TopActionState::default()
    };

    assert!(app.menu_item_enabled("edit.copy", &state, true));
    assert!(app.menu_item_enabled("edit.undo", &state, true));
    assert!(app.menu_item_enabled("edit.clear", &state, true));
    assert!(!app.menu_item_enabled("edit.copy_source", &state, true));
    app.apply_native_menu_event(
        crate::native_menu::NativeMenuEvent {
            id: "edit.copy".to_owned(),
            recent_path: None,
            target: app.native_action_target(&context, true),
        },
        &mut state,
        true,
    );
    assert!(state.preserve_text_focus);
    assert_eq!(state.commands.len(), 1);

    app.apply_top_actions(state, true, &context);
    assert_eq!(app.deferred_commands.len(), 1);

    app.search_dialog = None;
    let table_id = app
        .workspace
        .active_document()
        .and_then(|document| document.active_table())
        .expect("the active table remains open")
        .id;
    app.layout_dialog = Some(LayoutDialogState {
        document_id,
        table_id,
        address: CellAddress { row: 0, column: 0 },
        value: "24".to_owned(),
        focus_input: false,
    });
    app.active_modal_text_input_focus = Some(egui::Id::unique(egui::IdSalt::new("layout_field")));
    let stale_state = TopActionState {
        command_owner: Some(InputOwner::OtherText),
        command_focus: Some(field_id),
        ..TopActionState::default()
    };
    assert!(!app.menu_item_enabled("edit.copy", &stale_state, true));
}

#[test]
// Verify native accelerators remove the matching egui key event exactly once.
fn native_accelerator_suppresses_only_its_matching_key() {
    let (app, context) = test_app();
    let is_mac = context.os().is_mac();
    let mut modifiers = egui::Modifiers::COMMAND;
    if is_mac {
        modifiers.mac_cmd = true;
    } else {
        modifiers.ctrl = true;
    }
    for (action, key, shift) in [
        ("format.bold", egui::Key::B, false),
        ("format.italic", egui::Key::I, false),
        ("format.strike", egui::Key::X, true),
        ("format.code", egui::Key::E, false),
    ] {
        let mut shortcut_modifiers = modifiers;
        shortcut_modifiers.shift = shift;
        assert!(native_accelerator_matches(
            action,
            key,
            shortcut_modifiers,
            is_mac
        ));
    }
    let matching = egui::Event::Key {
        key: egui::Key::C,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    };
    let unmatched = egui::Event::Key {
        key: egui::Key::C,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::default(),
    };
    context.input_mut(|input| input.events = vec![matching, unmatched.clone()]);

    app.suppress_native_menu_accelerators(
        &context,
        &[crate::native_menu::NativeMenuEvent {
            id: "edit.copy".to_owned(),
            recent_path: None,
            target: app.native_action_target(&context, false),
        }],
    );
    assert_eq!(context.input(|input| input.events.clone()), vec![unmatched]);
}

#[test]
// Verify a structural native menu action follows the common action state and lock gate.
fn structure_actions_keep_edit_group_and_lock_state() {
    let (app, _) = test_app();
    let mut state = TopActionState {
        has_table: true,
        has_source: true,
        command_owner: Some(InputOwner::Chrome),
        ..TopActionState::default()
    };

    assert!(app.menu_item_enabled("edit.delete_rows", &state, false));
    assert!(app.menu_item_enabled("insert.rows", &state, false));
    state.document_edit_locked = true;
    assert!(!app.menu_item_enabled("edit.delete_rows", &state, false));
    assert!(!app.menu_item_enabled("insert.rows", &state, false));
    state.can_markdown_modifiers = true;
    assert!(!app.menu_item_enabled("format.bold", &state, false));
}

#[test]
// Allow File > New and omit the old no-op current-table menu row.
fn native_file_new_and_removed_noop_action_cover_empty_workspace() {
    let (mut app, _) = test_app();
    let state = app.native_menu_states(&TopActionState::default(), false);
    assert!(state["file.new"].enabled);
    assert!(!state.contains_key("view.current_table"));

    app.workspace.documents.clear();
    app.workspace.order.clear();
    app.workspace.active_document = None;
    let empty_state = app.native_menu_states(&TopActionState::default(), false);
    for id in [
        "file.new",
        "file.open",
        "app.settings",
        "help.open",
        "app.quit",
    ] {
        assert!(
            empty_state[id].enabled,
            "{id} remains available without documents"
        );
    }
    assert!(empty_state["file.new"].enabled);
    assert!(!empty_state["file.save"].enabled);
}

#[test]
// Routes global native actions through the hidden-root path without a document target.
fn app_wide_native_actions_work_after_the_last_window_closes() {
    let (mut app, context) = test_app();
    app.multi_window_mode = true;
    app.workspace.documents.clear();
    app.workspace.order.clear();
    app.workspace.active_document = None;
    let target = crate::native_menu::NativeActionTarget::default();

    assert!(app.apply_native_global_menu_events(
        vec![crate::native_menu::NativeMenuEvent {
            id: "app.settings".to_owned(),
            recent_path: None,
            target: target.clone(),
        }],
        &context,
    ));
    assert!(app.settings_window_open);
    app.settings_window_open = false;

    assert!(app.apply_native_global_menu_events(
        vec![crate::native_menu::NativeMenuEvent {
            id: "file.new".to_owned(),
            recent_path: None,
            target: target.clone(),
        }],
        &context,
    ));
    assert_eq!(app.workspace.documents.len(), 1);

    app.workspace.documents.clear();
    app.workspace.order.clear();
    app.workspace.active_document = None;
    assert!(!app.apply_native_global_menu_events(
        vec![crate::native_menu::NativeMenuEvent {
            id: "app.quit".to_owned(),
            recent_path: None,
            target,
        }],
        &context,
    ));
    assert!(app.app_exit_start_pending);
}

#[test]
// Keep all recent menu paths in event order and reject a stale edit target.
fn native_events_keep_recent_fifo_and_reject_changed_selection() {
    let (mut app, context) = test_app();
    let current_dir = std::env::current_dir().expect("read current directory");
    let path_a = current_dir.join("first.md");
    let path_b = current_dir.join("second.md");
    let mut state = TopActionState::default();
    for path in [&path_a, &path_b] {
        app.apply_native_menu_event(
            crate::native_menu::NativeMenuEvent {
                id: "file.recent.0".to_owned(),
                recent_path: Some(path.clone()),
                target: crate::native_menu::NativeActionTarget::default(),
            },
            &mut state,
            false,
        );
    }
    assert_eq!(
        state.open_recent_paths,
        vec![path_a.clone(), path_b.clone()]
    );
    app.apply_top_actions(state, false, &context);
    assert_eq!(
        app.queued_open_paths.iter().cloned().collect::<Vec<_>>(),
        vec![path_a, path_b]
    );

    let stale_target = app.native_action_target(&context, false);
    let event = crate::native_menu::NativeMenuEvent {
        id: "edit.clear".to_owned(),
        recent_path: None,
        target: stale_target,
    };
    let (document_id, table_id) = {
        let document = app.workspace.active_document().unwrap();
        (document.id, document.active_table().unwrap().id)
    };
    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .view_mut(table_id)
        .unwrap()
        .set_selection(CellRange::single(CellAddress { row: 1, column: 1 }));
    let mut actions = TopActionState::default();
    let late = app.apply_native_menu_events(vec![event], &mut actions, false, &context);
    assert!(late.is_empty());
    assert!(actions.commands.is_empty());
}

#[test]
// Keep a native edit command on clean A when Cmd+W switches to document B.
fn native_edit_command_cannot_follow_close_shortcut_into_next_document() {
    let directory = crate::ui::tests::TestDirectory::new();
    let path = directory.0.join("clean-a.md");
    std::fs::write(&path, b"| value |\n| --- |\n| A |\n").expect("write clean document");
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    harness.step();
    let (document_a, _) = load_backed_markdown(&mut harness, &path);

    harness
        .state_mut()
        .create_new_document("B".to_owned())
        .expect("create document B");
    let document_b = harness.state().workspace.active_document.unwrap();
    let table_b = harness.state().workspace.documents[&document_b]
        .active_table()
        .expect("B has a table")
        .id;
    harness
        .state_mut()
        .workspace
        .documents
        .get_mut(&document_b)
        .unwrap()
        .set_cell(table_b, CellAddress::default(), Some(Cell::text("keep B")))
        .expect("write a marker into B");
    let revision_b = harness.state().workspace.documents[&document_b].revision;
    let undo_b = harness.state().workspace.documents[&document_b].can_undo();

    harness.state_mut().workspace.select_document(document_a);
    harness.state_mut().request_grid_focus = true;
    harness.step();
    let target = harness.state().native_action_target(&harness.ctx, false);
    assert_eq!(target.document_id, Some(document_a));
    harness.state_mut().native_menu_events_for_test = vec![crate::native_menu::NativeMenuEvent {
        id: "edit.clear".to_owned(),
        recent_path: None,
        target,
    }];
    harness.event(key_event(egui::Key::W, command_modifiers(&harness.ctx)));
    harness.step();

    assert!(
        !harness
            .state()
            .workspace
            .documents
            .contains_key(&document_a)
    );
    assert_eq!(harness.state().workspace.active_document, Some(document_b));
    let document = &harness.state().workspace.documents[&document_b];
    assert_eq!(document.revision, revision_b);
    assert_eq!(document.can_undo(), undo_b);
    assert_eq!(
        document.tables[0]
            .cell(CellAddress::default())
            .unwrap()
            .input,
        CellInput::Text("keep B".to_owned())
    );
}

#[test]
// Allow Save after same-frame Enter commit and movement within the same document.
fn native_save_keeps_the_document_target_after_enter_moves_selection() {
    let directory = crate::ui::tests::TestDirectory::new();
    let path = directory.0.join("save-after-enter.md");
    std::fs::write(&path, b"| value |\n| --- |\n| old |\n").expect("write source document");
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    harness.step();
    let (document_id, table_id) = load_backed_markdown(&mut harness, &path);
    let address = CellAddress { row: 1, column: 0 };
    harness.state_mut().begin_cell_edit(table_id, address);
    harness.state_mut().edit.buffer = "saved".to_owned();
    harness.state_mut().edit.cursor_byte = 5;
    harness.state_mut().edit.anchor_byte = 5;
    harness.step();
    let target = harness.state().native_action_target(&harness.ctx, false);
    assert_eq!(target.document_id, Some(document_id));
    assert!(target.edit_session.is_some());
    harness.state_mut().native_menu_events_for_test = vec![crate::native_menu::NativeMenuEvent {
        id: "file.save".to_owned(),
        recent_path: None,
        target,
    }];
    harness.event(key_event(egui::Key::Enter, egui::Modifiers::default()));
    harness.step();

    assert!(!harness.state().edit.is_editing());
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .view(table_id)
            .unwrap()
            .selection
            .head
            .row,
        2
    );
    for _ in 0..200 {
        harness.step();
        let contents = std::fs::read_to_string(&path).expect("saved file remains readable");
        if contents.contains("saved") {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("native Save did not write the committed cell value");
}

#[test]
// Reject a queued native Save after its original document is no longer active.
fn native_save_rejects_a_stale_document_target() {
    let directory = crate::ui::tests::TestDirectory::new();
    let path = directory.0.join("stale-save.md");
    std::fs::write(&path, b"| value |\n| --- |\n| keep |\n").expect("write source document");
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    harness.step();
    let (document_a, _) = load_backed_markdown(&mut harness, &path);
    let target = harness.state().native_action_target(&harness.ctx, false);
    harness
        .state_mut()
        .create_new_document("B".to_owned())
        .expect("create document B");
    let document_b = harness.state().workspace.active_document.unwrap();
    harness.state_mut().native_menu_events_for_test = vec![crate::native_menu::NativeMenuEvent {
        id: "file.save".to_owned(),
        recent_path: None,
        target,
    }];

    harness.step();

    assert_eq!(harness.state().workspace.active_document, Some(document_b));
    assert!(!harness.state().pending_saves.contains_key(&document_a));
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "| value |\n| --- |\n| keep |\n"
    );
}

#[test]
// Verify late-drawn modal text fields enable and receive native Edit commands.
fn adapter_applies_native_text_commands_after_layout_field_draws() {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let (document_id, table_id) = {
        let document = harness
            .state()
            .workspace
            .active_document()
            .expect("a document is active");
        (
            document.id,
            document.active_table().expect("a table is active").id,
        )
    };
    harness.state_mut().layout_dialog = Some(LayoutDialogState {
        document_id,
        table_id,
        address: CellAddress { row: 0, column: 0 },
        value: "1234".to_owned(),
        focus_input: false,
    });
    harness.step();

    let field_id = egui::Id::unique(egui::IdSalt::new((
        "layout_dialog_value",
        document_id,
        table_id,
    )));
    harness
        .ctx
        .memory_mut(|memory| memory.request_focus(field_id));
    harness.step();
    assert!(harness.ctx.text_edit_focused());
    assert_eq!(
        harness.state().active_modal_text_input_focus,
        Some(field_id)
    );
    let menu_states =
        harness
            .state()
            .native_menu_states_for_frame(TopActionState::default(), true, &harness.ctx);
    assert!(menu_states["edit.copy"].enabled);
    assert!(menu_states["edit.clear"].enabled);

    harness.state_mut().native_menu_events_for_test = vec![
        crate::native_menu::NativeMenuEvent {
            id: "edit.select_all".to_owned(),
            recent_path: None,
            target: harness.state().native_action_target(&harness.ctx, true),
        },
        crate::native_menu::NativeMenuEvent {
            id: "edit.clear".to_owned(),
            recent_path: None,
            target: harness.state().native_action_target(&harness.ctx, true),
        },
    ];
    harness.step();
    harness.step();
    assert_eq!(
        harness
            .state()
            .layout_dialog
            .as_ref()
            .expect("the layout dialog stays open")
            .value,
        ""
    );
}

#[test]
// Run a native Edit-menu command through the active detail editor session.
fn adapter_applies_native_clear_to_detail_editor_text() {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    harness.step();
    let (document_id, table_id) = {
        let document = harness.state().workspace.active_document().unwrap();
        (document.id, document.active_table().unwrap().id)
    };
    {
        let app = harness.state_mut();
        app.begin_cell_edit(table_id, CellAddress::default());
        app.edit
            .switch_target(crate::document::EditTarget::DetailPanel);
        app.edit.buffer = "draft".to_owned();
        app.edit.cursor_byte = app.edit.buffer.len();
        app.edit.anchor_byte = app.edit.cursor_byte;
        app.long_text_panel_open = true;
        app.request_editor_focus = true;
    }
    harness.step();
    let editor = editor_id(document_id, harness.state().edit.session_id);
    assert_eq!(harness.ctx.memory(|memory| memory.focused()), Some(editor));
    assert_eq!(
        harness.state().command_input_owner(&harness.ctx, true),
        (InputOwner::OtherText, Some(editor)),
        "the detail editor is the active modal text owner"
    );

    let mut text_state = egui::widgets::text_edit::TextEditState::load(&harness.ctx, editor)
        .expect("the real detail TextEdit stored its cursor");
    text_state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::select_all_str("draft")));
    text_state.store(&harness.ctx, editor);
    let states =
        harness
            .state()
            .native_menu_states_for_frame(TopActionState::default(), true, &harness.ctx);
    assert!(states["edit.clear"].enabled);
    let target = harness.state().native_action_target(&harness.ctx, true);
    assert_eq!(
        target.modal_session,
        Some((9, document_id, harness.state().edit.session_id))
    );
    harness.state_mut().native_menu_events_for_test = vec![crate::native_menu::NativeMenuEvent {
        id: "edit.clear".to_owned(),
        recent_path: None,
        target,
    }];
    harness.step();
    harness.step();
    assert!(harness.state().edit.is_editing());
    assert_eq!(harness.state().edit.buffer, "");
}
