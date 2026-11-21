use super::*;

#[path = "save_restart_test.rs"]
mod save_restart;
#[test]
// Includes a parked child-window draft in the app-exit save and recovery paths.
fn quitting_from_one_window_saves_another_windows_parked_editor() {
    let directory = crate::ui::tests::TestDirectory::new();
    let (mut harness, paths) = host_with_three_documents(&directory.0);
    let document_ids = host_document_ids(&harness, &paths);
    let (window_a, window_b) = (document_ids[0], document_ids[1]);

    host_window_frame(&mut harness, window_b, []);
    click_host_cell(&mut harness, window_b);
    host_window_frame(
        &mut harness,
        window_b,
        [egui::Event::Text("Parked draft".to_owned())],
    );
    host_window_frame(&mut harness, window_a, []);
    let recovery_text = with_host(harness.state().id, |host| {
        let viewport_b = host.document_viewports[&window_b];
        let draft = &host.app.window_states[&viewport_b].edit;
        assert!(
            draft.is_editing(),
            "B editor was parked: edit_document={:?}, target={:?}, buffer={:?}, status={:?}",
            host.app.window_states[&viewport_b].edit_document_id,
            draft.target,
            draft.buffer,
            host.app.status
        );
        assert!(host.app.app_close_needs_save());
        host.app
            .recovery_snapshot(window_b, 1)
            .and_then(|snapshot| snapshot.cell_edit.map(|edit| edit.buffer))
    });
    assert_eq!(recovery_text.as_deref(), Some("Parked draft"));

    request_host_app_quit(&mut harness, window_a);
    host_window_frame(&mut harness, window_a, []);
    let viewport_a = with_host(harness.state().id, |host| {
        host.document_viewports[&window_a]
    });
    assert!(with_host(harness.state().id, |host| {
        host.app.app_exit_transaction
            && host.app.window_states[&viewport_a].app_close_dialog_open
            && host.app.document_has_unsaved_changes(window_b)
    }));

    let save_all = crate::i18n::context_text(&harness.ctx, "action-save-all-and-exit");
    click_host_button(&mut harness, window_a, &save_all);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        harness.step();
        let saved = with_host(harness.state().id, |host| {
            host.app.app_exit_ready
                && host
                    .app
                    .workspace
                    .documents
                    .get(&window_b)
                    .is_some_and(|document| document.revision == document.saved_revision)
                && !host.app.pending_saves.contains_key(&window_b)
        });
        if saved {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Save All commits the parked editor"
        );
    }
    let bytes = fs::read_to_string(&paths[1]).expect("the child file remains readable");
    assert!(bytes.contains("Parked draft"));
    for document_id in [window_b, document_ids[2]] {
        let viewport = with_host(harness.state().id, |host| {
            host.document_viewports[&document_id]
        });
        let output = host_window_frame(&mut harness, document_id, []);
        assert!(
            output
                .viewport_output
                .get(&viewport)
                .is_some_and(|view| view.commands.contains(&egui::ViewportCommand::Close)),
            "the production child callback closes document {document_id:?} after app exit is ready"
        );
    }
}

#[test]
// Routes a child document's save-and-close result back to its owning window.
fn child_save_and_close_result_returns_to_its_document_window() {
    let directory = crate::ui::tests::TestDirectory::new();
    let (mut harness, paths) = host_with_three_documents(&directory.0);
    let document_ids = host_document_ids(&harness, &paths);
    let document_id = document_ids[1];

    click_host_cell(&mut harness, document_id);
    host_window_frame(
        &mut harness,
        document_id,
        [egui::Event::Text("Saved from child close".to_owned())],
    );
    let save_close = crate::i18n::context_text(&harness.ctx, "action-save-close");
    with_host(harness.state().id, |host| {
        let viewport = host.document_viewports[&document_id];
        let window = &host.app.window_states[&viewport];
        assert!(window.edit.is_editing());
        assert_eq!(window.edit.buffer, "Saved from child close");
        assert!(host.app.document_has_unsaved_changes(document_id));
    });
    close_host_window(&mut harness, document_id);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let output = host_window_frame(&mut harness, document_id, []);
        let close_tree = output
            .platform_output
            .accesskit_update
            .clone()
            .map(State::new);
        if close_tree
            .as_ref()
            .is_some_and(|tree| find_host_button(tree.root(), &save_close).is_some())
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the real child close dialog appears"
        );
    }
    click_host_button(&mut harness, document_id, &save_close);

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        harness.step();
        let closed = with_host(harness.state().id, |host| {
            !host.app.workspace.documents.contains_key(&document_id)
        });
        if closed {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the child closes after its save ACK"
        );
    }
    let bytes = fs::read_to_string(&paths[1]).expect("the child file remains readable");
    assert!(bytes.contains("Saved from child close"));
}

#[test]
// Restores a failed save-and-close draft in the window that owns it.
fn stopped_worker_save_close_failure_returns_edit_to_child_owner() {
    let directory = crate::ui::tests::TestDirectory::new();
    let (mut harness, paths) = host_with_three_documents(&directory.0);
    let document_id = host_document_ids(&harness, &paths)[1];
    click_host_cell(&mut harness, document_id);
    host_window_frame(
        &mut harness,
        document_id,
        [egui::Event::Text("Keep this draft".to_owned())],
    );
    close_host_window(&mut harness, document_id);

    let save_close = crate::i18n::context_text(&harness.ctx, "action-save-close");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let output = host_window_frame(&mut harness, document_id, []);
        let close_tree = output
            .platform_output
            .accesskit_update
            .clone()
            .map(State::new);
        if close_tree
            .as_ref()
            .is_some_and(|tree| find_host_button(tree.root(), &save_close).is_some())
        {
            break;
        }
        assert!(Instant::now() < deadline, "the child close dialog appears");
    }
    click_host_button(&mut harness, document_id, &save_close);
    let pending_token = with_host(harness.state().id, |host| {
        let viewport = host.document_viewports[&document_id];
        assert!(
            host.app.window_states[&viewport]
                .close_after_save
                .contains(&document_id)
        );
        host.app.pending_saves[&document_id]
    });
    with_host(harness.state().id, |host| {
        host.app.file_worker = crate::file_worker::test_support::stopped_worker();
    });

    harness.step();
    with_host(harness.state().id, |host| {
        let viewport = host.document_viewports[&document_id];
        let owner = &host.app.window_states[&viewport];
        assert!(!host.app.pending_saves.contains_key(&document_id));
        assert!(!owner.close_after_save.contains(&document_id));
        assert!(!owner.close_after_save_edit.contains_key(&document_id));
        assert_eq!(owner.pending_close_document, Some(document_id));
        assert!(owner.edit.is_editing());
        assert_eq!(owner.edit_document_id, Some(document_id));
        assert_eq!(owner.edit.buffer, "Keep this draft");
        assert_eq!(
            owner.status.as_ref().map(Message::key),
            Some("status-file-worker-stopped")
        );
        assert!(host.app.document_has_unsaved_changes(document_id));
        assert_eq!(host.app.pending_saves.get(&document_id), None);
        assert_eq!(pending_token.document_id, document_id);
    });
    assert!(fs::read_to_string(&paths[1]).is_ok());
}
