use super::*;

/// Clicks one app-exit choice in its stable child-window position.
fn click_host_app_close_button(
    harness: &mut Harness<'_, TexcelAppHost>,
    document_id: DocumentId,
    key: &'static str,
) {
    let host_id = harness.state().id;
    let viewport = with_host(host_id, |host| host.document_viewports[&document_id]);
    let mut previous = None;
    let mut rect = None;
    for _ in 0..5 {
        host_window_frame(harness, document_id, []);
        rect = with_host(host_id, |host| {
            host.app.window_states[&viewport]
                .test_app_close_button_rects
                .get(key)
                .copied()
        });
        if rect.is_some() && rect == previous {
            break;
        }
        previous = rect;
    }
    let point = rect
        .unwrap_or_else(|| panic!("app-close button {key:?} was drawn"))
        .center();
    host_window_frame(harness, document_id, [egui::Event::WindowFocused(true)]);
    host_window_frame(harness, document_id, [egui::Event::PointerMoved(point)]);
    host_window_frame(
        harness,
        document_id,
        [egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    host_window_frame(
        harness,
        document_id,
        [egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
}

/// Builds a new-document host with isolated settings and recovery storage.
fn host_with_blank_document(settings_path: PathBuf) -> Harness<'static, TexcelAppHost> {
    let mut harness = Harness::builder()
        .with_size([1000.0, 700.0])
        .allow_missing_glyphs()
        .build_eframe(move |creation| {
            creation.egui_ctx.set_embed_viewports(false);
            let app = TexcelApp::new_with_settings_path(creation, Some(settings_path));
            TexcelAppHost::new(app, creation.egui_ctx.viewport_id())
        });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        harness.step();
        let ready = with_host(harness.state().id, |host| {
            host.app.settings_loaded && host.root_document.is_some()
        });
        if ready {
            break;
        }
        assert!(Instant::now() < deadline, "the new host document starts");
    }
    harness
}

/// Sends a keyboard shortcut with matching key-up and modifier-release events.
fn send_host_shortcut(
    harness: &mut Harness<'_, TexcelAppHost>,
    document_id: DocumentId,
    key: egui::Key,
) {
    let mut modifiers = egui::Modifiers::COMMAND;
    if harness.ctx.os().is_mac() {
        modifiers.mac_cmd = true;
    } else {
        modifiers.ctrl = true;
    }
    host_window_frame(
        harness,
        document_id,
        [
            egui::Event::ModifiersChanged(modifiers),
            egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            },
        ],
    );
    host_window_frame(
        harness,
        document_id,
        [
            egui::Event::Key {
                key,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers,
            },
            egui::Event::ModifiersChanged(egui::Modifiers::NONE),
        ],
    );
}

/// Types and commits one value through the grid editor.
fn type_host_cell(harness: &mut Harness<'_, TexcelAppHost>, document_id: DocumentId, value: &str) {
    click_host_cell(harness, document_id);
    host_window_frame(harness, document_id, [egui::Event::Text(value.to_owned())]);
    host_window_frame(
        harness,
        document_id,
        [egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    host_window_frame(
        harness,
        document_id,
        [egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
}

/// Injects the selected path as the result of the real save chooser boundary.
fn select_host_save_path(
    harness: &Harness<'_, TexcelAppHost>,
    document_id: DocumentId,
    path: PathBuf,
) {
    let host_id = harness.state().id;
    with_host(host_id, |host| {
        let viewport = host.document_viewports[&document_id];
        host.app
            .with_window_state(viewport, Some(document_id), |app| {
                app.test_save_path_selection = Some(path);
            });
    });
}

/// Waits for one whole-document save and returns its assigned backing path.
fn wait_for_host_save(harness: &mut Harness<'_, TexcelAppHost>, document_id: DocumentId) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        harness.step();
        let saved = with_host(harness.state().id, |host| {
            !host.app.pending_saves.contains_key(&document_id)
                && host
                    .app
                    .workspace
                    .documents
                    .get(&document_id)
                    .is_some_and(|document| !document.is_dirty())
                && host.app.file_backings.contains_key(&document_id)
        });
        if saved {
            return;
        }
        if Instant::now() >= deadline {
            let state = with_host(harness.state().id, |host| {
                let viewport = host.document_viewports[&document_id];
                let document = &host.app.workspace.documents[&document_id];
                (
                    host.app.pending_saves.get(&document_id).copied(),
                    host.app.pending_save_intents.contains_key(&document_id),
                    host.app.file_backings.contains_key(&document_id),
                    document.is_dirty(),
                    host.app.file_worker.is_stopped(),
                    host.app.window_states[&viewport]
                        .status
                        .as_ref()
                        .map(|message| Message::key(message).to_owned()),
                    host.app.window_states[&viewport].export_dialog.is_some(),
                )
            });
            panic!("the host save completes: {state:?}");
        }
    }
}

/// Waits until the edit and its worker-backed structure have both been installed.
fn wait_for_host_edits(harness: &mut Harness<'_, TexcelAppHost>, document_id: DocumentId) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        harness.step();
        let settled = with_host(harness.state().id, |host| {
            !host.app.pending_structures.contains_key(&document_id)
                && !host.app.pending_history_restores.contains_key(&document_id)
                && !host
                    .app
                    .pending_text_growth_sessions
                    .contains_key(&document_id)
        });
        if settled {
            return;
        }
        assert!(Instant::now() < deadline, "the host edit worker settles");
    }
}

/// Restarts the normal host and opens the saved file through its load worker.
fn restart_host_with_file(
    settings_path: PathBuf,
    file_path: PathBuf,
) -> (Harness<'static, TexcelAppHost>, DocumentId) {
    let open_path = file_path.clone();
    let mut harness = Harness::builder()
        .with_size([1000.0, 700.0])
        .allow_missing_glyphs()
        .build_eframe(move |creation| {
            creation.egui_ctx.set_embed_viewports(false);
            let mut app = TexcelApp::new_with_settings_path(creation, Some(settings_path));
            app.queue_open_paths([open_path]);
            TexcelAppHost::new(app, creation.egui_ctx.viewport_id())
        });
    let expected_path = fs::canonicalize(file_path).expect("the saved file has a canonical path");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        harness.step();
        let loaded = with_host(harness.state().id, |host| {
            host.app
                .file_backings
                .iter()
                .find_map(|(id, backing)| (backing.path == expected_path).then_some(*id))
        });
        if let Some(document_id) = loaded {
            return (harness, document_id);
        }
        assert!(
            Instant::now() < deadline,
            "the saved file reloads after restart"
        );
    }
}
#[test]
// Keeps an untitled document clean when undo returns it to its initial content.
fn new_document_edit_undo_exits_without_a_recovery_record() {
    let directory = crate::ui::tests::TestDirectory::new();
    let settings_path = directory.0.join("settings.json");
    let mut harness = host_with_blank_document(settings_path.clone());
    let document_id = with_host(harness.state().id, |host| {
        host.root_document
            .expect("the blank document owns the root window")
    });

    type_host_cell(&mut harness, document_id, "Undo to blank");
    wait_for_host_edits(&mut harness, document_id);
    send_host_shortcut(&mut harness, document_id, egui::Key::Z);
    wait_for_host_edits(&mut harness, document_id);
    with_host(harness.state().id, |host| {
        let document = &host.app.workspace.documents[&document_id];
        assert!(
            !host.app.document_has_unsaved_changes(document_id),
            "undo returns to initial content: revision={}/{}, state={}, dirty={}, buffer={:?}, pending_growth={:?}",
            document.revision,
            document.saved_revision,
            document.history_state_id(),
            document.is_dirty(),
            host.app.edit.buffer,
            host.app.pending_text_growth_sessions.get(&document_id)
        );
        let cell = host.app.workspace.documents[&document_id]
            .active_table()
            .and_then(|table| table.cell(CellAddress { row: 1, column: 0 }));
        assert!(cell.is_none_or(|cell| cell.input.source_text().is_empty()));
        assert!(!host.app.window_states[&host.root_viewport].app_close_dialog_open);
    });

    request_host_app_quit(&mut harness, document_id);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        harness.step();
        if with_host(harness.state().id, |host| host.app.app_exit_ready) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the undone untitled document exits cleanly"
        );
    }
    let recovery_path = directory.0.join("recovery");
    let recovery_is_empty = fs::read_dir(&recovery_path)
        .map(|entries| entries.count() == 0)
        .unwrap_or(true);
    assert!(
        recovery_is_empty,
        "undo to the initial state leaves no recovery data"
    );
    drop(harness);

    let restarted = host_with_blank_document(settings_path);
    assert!(with_host(restarted.state().id, |host| {
        host.app.recovery_items.is_empty() && host.app.recovery_restore_pending.is_empty()
    }));
}

#[test]
// Keeps a new document's first save and undo state through a clean restart.
fn new_document_save_as_and_undo_return_to_the_saved_state() {
    let directory = crate::ui::tests::TestDirectory::new();
    let settings_path = directory.0.join("settings.json");
    let source_path = directory.0.join("untitled-first-save.md");
    let mut harness = host_with_blank_document(settings_path.clone());
    let document_id = with_host(harness.state().id, |host| {
        host.root_document
            .expect("the blank document owns the root window")
    });

    type_host_cell(&mut harness, document_id, "Saved baseline");
    wait_for_host_edits(&mut harness, document_id);
    select_host_save_path(&harness, document_id, source_path.clone());
    send_host_shortcut(&mut harness, document_id, egui::Key::S);
    let choose_destination = crate::i18n::context_text(&harness.ctx, "export-choose-destination");
    click_host_button(&mut harness, document_id, &choose_destination);
    wait_for_host_save(&mut harness, document_id);
    assert!(
        fs::read_to_string(&source_path)
            .expect("the first Save As writes Markdown")
            .contains("Saved baseline")
    );

    type_host_cell(&mut harness, document_id, "Temporary edit");
    wait_for_host_edits(&mut harness, document_id);
    send_host_shortcut(&mut harness, document_id, egui::Key::Z);
    wait_for_host_edits(&mut harness, document_id);
    with_host(harness.state().id, |host| {
        let document = &host.app.workspace.documents[&document_id];
        assert!(
            !host.app.document_has_unsaved_changes(document_id),
            "undo returns to saved content: revision={}/{}, state={}, dirty={}, pending_growth={:?}",
            document.revision,
            document.saved_revision,
            document.history_state_id(),
            document.is_dirty(),
            host.app.pending_text_growth_sessions.get(&document_id)
        );
        let cell = host.app.workspace.documents[&document_id]
            .active_table()
            .and_then(|table| table.cell(CellAddress { row: 1, column: 0 }));
        assert_eq!(
            cell.map(|cell| cell.input.source_text()),
            Some("Saved baseline".to_owned())
        );
    });

    request_host_app_quit(&mut harness, document_id);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        harness.step();
        if with_host(harness.state().id, |host| host.app.app_exit_ready) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "undo to the save point exits cleanly"
        );
    }
    let recovery_path = directory.0.join("recovery");
    let recovery_is_empty = fs::read_dir(&recovery_path)
        .map(|entries| entries.count() == 0)
        .unwrap_or(true);
    assert!(recovery_is_empty, "clean exit removes recovery data");
    drop(harness);

    let (restarted, reopened_id) = restart_host_with_file(settings_path, source_path.clone());
    let reopened = with_host(restarted.state().id, |host| {
        host.app.workspace.documents[&reopened_id]
            .active_table()
            .and_then(|table| table.cell(CellAddress { row: 1, column: 0 }))
            .map(|cell| cell.input.source_text())
    });
    assert_eq!(reopened.as_deref(), Some("Saved baseline"));
    assert!(with_host(restarted.state().id, |host| host
        .app
        .recovery_restore_pending
        .is_empty()));
}

#[test]
// Saves a new untitled document through the app-exit Save All path and reloads it.
fn app_exit_save_all_uses_first_save_path_and_persists_the_file() {
    let directory = crate::ui::tests::TestDirectory::new();
    let settings_path = directory.0.join("settings.json");
    let source_path = directory.0.join("untitled-save-all.md");
    let mut harness = host_with_blank_document(settings_path.clone());
    let document_id = with_host(harness.state().id, |host| {
        host.root_document
            .expect("the blank document owns the root window")
    });

    type_host_cell(&mut harness, document_id, "Saved at exit");
    wait_for_host_edits(&mut harness, document_id);
    select_host_save_path(&harness, document_id, source_path.clone());
    request_host_app_quit(&mut harness, document_id);
    click_host_app_close_button(&mut harness, document_id, "save");
    let choose_destination = crate::i18n::context_text(&harness.ctx, "export-choose-destination");
    host_window_frame(&mut harness, document_id, []);
    let dialog_state = with_host(harness.state().id, |host| {
        let viewport = host.document_viewports[&document_id];
        (
            host.app.window_states[&viewport].export_dialog.is_some(),
            host.app.window_states[&viewport].app_close_dialog_open,
            host.app.app_close_save_failed,
            host.app.pending_app_close_save_queue.clone(),
            host.app.app_close_has_active_composition(),
            host.app.app_close_has_pending_operations(),
            host.app.document_has_unsaved_changes(document_id),
            host.app.window_states[&viewport].edit.is_editing(),
            (
                host.app.window_states[&viewport]
                    .status
                    .as_ref()
                    .map(|message| Message::key(message).to_owned()),
                host.app.app_close_save_tokens.is_empty(),
                host.app.pending_saves.is_empty(),
            ),
        )
    });
    assert!(
        dialog_state.0,
        "Save All opens the destination dialog after a usable save choice: {dialog_state:?}"
    );
    click_host_button(&mut harness, document_id, &choose_destination);

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        harness.step();
        let state = with_host(harness.state().id, |host| {
            (
                host.app.app_exit_ready
                    && !host.app.pending_saves.contains_key(&document_id)
                    && host.app.file_backings.contains_key(&document_id),
                host.app.app_close_dialog_open,
                host.app.app_close_save_failed,
                host.app.pending_saves.get(&document_id).copied(),
                host.app.pending_save_intents.contains_key(&document_id),
                host.app.test_save_path_selection.clone(),
                host.app.status.clone(),
            )
        });
        if state.0 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Save All completes the untitled save: dialog={}, failed={}, pending={:?}, intent={}, selected={:?}, status={:?}",
            state.1,
            state.2,
            state.3,
            state.4,
            state.5,
            state.6
        );
    }
    assert!(
        fs::read_to_string(&source_path)
            .expect("Save All writes the selected Markdown path")
            .contains("Saved at exit")
    );
    drop(harness);

    let (restarted, reopened_id) = restart_host_with_file(settings_path, source_path);
    let reopened = with_host(restarted.state().id, |host| {
        host.app.workspace.documents[&reopened_id]
            .active_table()
            .and_then(|table| table.cell(CellAddress { row: 1, column: 0 }))
            .map(|cell| cell.input.source_text())
    });
    assert_eq!(reopened.as_deref(), Some("Saved at exit"));
    assert!(with_host(restarted.state().id, |host| host
        .app
        .recovery_restore_pending
        .is_empty()));
}
