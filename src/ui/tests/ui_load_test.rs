use super::*;
use crate::file_worker::FileResult;
use egui_kittest::{
    Harness,
    kittest::{By, Queryable},
};
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[test]
// Open quoted data with a delimiter that the standard candidates cannot parse.
fn initial_csv_open_offers_a_custom_delimiter_for_quoted_pipe_fields() {
    let directory = TestDirectory::new();
    let path = directory.0.join("quoted-pipe.csv");
    fs::write(&path, b"\"a\"|\"b\"\n").expect("write quoted pipe input");
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    harness.step();
    harness.state_mut().open_path(path);
    let document_id = *harness
        .state()
        .pending_csv_opens
        .keys()
        .next()
        .expect("initial open starts CSV settings analysis");
    wait_for_csv_prompt(&mut harness, document_id);
    let CsvLoadPrompt::Settings(settings) = harness.state().pending_csv_opens[&document_id]
        .prompt
        .as_ref()
        .expect("CSV settings are shown");
    assert!(settings.candidates.is_empty());

    let delimiter_id = Id::unique(egui::IdSalt::new(("csv_custom_delimiter", document_id)));
    harness
        .ctx
        .memory_mut(|memory| memory.request_focus(delimiter_id));
    harness.step();
    harness.event(key(Key::A, Modifiers::COMMAND));
    harness.event(Event::Text("|".to_owned()));
    harness.step();
    assert_eq!(
        match harness.state().pending_csv_opens[&document_id]
            .prompt
            .as_ref()
            .expect("CSV settings remain open")
        {
            CsvLoadPrompt::Settings(settings) => settings.delimiter_text.as_str(),
        },
        "|"
    );

    click_csv_load(&mut harness);
    wait_for_csv_load(&mut harness, document_id);
    let document = &harness.state().workspace.documents[&document_id];
    assert_eq!(
        document.tables[0]
            .cell(CellAddress::default())
            .unwrap()
            .input,
        CellInput::Text("a".to_owned())
    );
    assert_eq!(
        document.tables[0]
            .cell(CellAddress { row: 0, column: 1 })
            .unwrap()
            .input,
        CellInput::Text("b".to_owned())
    );
}

#[test]
// Let the old cell editor consume same-frame text before adopting a completed file.
fn completed_load_defers_adoption_until_same_frame_editor_text_is_consumed() {
    let directory = TestDirectory::new();
    let path = directory.0.join("opened.md");
    fs::write(&path, b"| new |\n| --- |\n| value |\n").expect("write Markdown input");
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    harness.step();
    let original_id = harness.state().workspace.active_document.unwrap();
    let (table_id, address, original_revision) = {
        let app = harness.state_mut();
        let (_, table_id, address) = edit_blank_cell(app);
        app.edit.buffer = "before".to_owned();
        app.edit.cursor_byte = app.edit.buffer.len();
        app.edit.anchor_byte = app.edit.buffer.len();
        (
            table_id,
            address,
            app.workspace.documents[&original_id].revision,
        )
    };
    harness.step();

    let new_document_id = harness
        .state_mut()
        .start_path_load(path)
        .expect("Markdown load is submitted");
    let token = harness.state().pending_markdown_loads[&new_document_id];
    let ctx = harness.ctx.clone();
    let result = take_file_result(harness.state_mut(), &ctx, token);
    harness.state_mut().deferred_file_results.push(result);

    harness.event(Event::Text(" tail".to_owned()));
    harness.step();
    assert_eq!(harness.state().edit.buffer, "before tail");
    assert_eq!(
        harness.state().workspace.documents[&original_id].revision,
        original_revision
    );
    assert!(
        !harness
            .state()
            .workspace
            .documents
            .contains_key(&new_document_id)
    );

    harness.step();
    assert_eq!(
        harness.state().workspace.active_document,
        Some(new_document_id)
    );
    wait_for_condition(harness.state_mut(), |app| {
        app.workspace.documents[&original_id]
            .tables
            .iter()
            .find(|table| table.id == table_id)
            .and_then(|table| table.cell(address))
            .is_some_and(|cell| cell.input == CellInput::Text("before tail".to_owned()))
    });
    assert_eq!(
        harness.state().workspace.documents[&original_id]
            .tables
            .iter()
            .find(|table| table.id == table_id)
            .unwrap()
            .cell(address)
            .unwrap()
            .input,
        CellInput::Text("before tail".to_owned())
    );
    assert!(harness.state().workspace.documents[&original_id].revision > original_revision);
}

#[test]
// Let Escape cancel the old edit before a completed load changes documents.
fn completed_load_and_escape_preserve_old_document_and_undo() {
    let directory = TestDirectory::new();
    let path = directory.0.join("escape.md");
    fs::write(&path, b"| keep |\n| --- |\n").expect("write Markdown input");
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let original_id = app.workspace.active_document.unwrap();
    let (table_id, address) = {
        let (_, table_id, address) = edit_blank_cell(&mut app);
        app.edit.buffer = "unfinished".to_owned();
        app.edit.cursor_byte = app.edit.buffer.len();
        app.edit.anchor_byte = app.edit.buffer.len();
        (table_id, address)
    };
    let revision = app.workspace.documents[&original_id].revision;
    let can_undo = app.workspace.documents[&original_id].can_undo();
    let new_id = queue_completed_markdown_load(&mut app, &ctx, path);

    run_load_result_and_keyboard(&mut app, &ctx, key(Key::Escape, Modifiers::NONE), false);

    let original = &app.workspace.documents[&original_id];
    assert_eq!(app.workspace.active_document, Some(original_id));
    assert!(!app.workspace.documents.contains_key(&new_id));
    assert_eq!(original.revision, revision);
    assert_eq!(original.can_undo(), can_undo);
    assert!(
        original
            .tables
            .iter()
            .find(|table| table.id == table_id)
            .unwrap()
            .cell(address)
            .is_none_or(|cell| cell.input.source_text().is_empty())
    );
    assert!(!app.edit.is_editing());

    run_empty_load_result_frame(&mut app, &ctx);
    assert_eq!(app.workspace.active_document, Some(new_id));
}

#[test]
// Keep Delete and Cut aimed at the old grid when a load completes in that frame.
fn completed_load_does_not_receive_same_frame_delete_or_cut() {
    for event in [key(Key::Delete, Modifiers::NONE), Event::Cut] {
        let directory = TestDirectory::new();
        let path = directory.0.join("grid-action.md");
        fs::write(&path, b"| keep |\n| --- |\n").expect("write Markdown input");
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let original_id = app.workspace.active_document.unwrap();
        let new_id = queue_completed_markdown_load(&mut app, &ctx, path);
        let original_table = app.workspace.documents[&original_id]
            .active_table()
            .unwrap()
            .id;
        app.workspace
            .documents
            .get_mut(&original_id)
            .unwrap()
            .view_mut(original_table)
            .unwrap()
            .selection = CellRange::single(CellAddress::default());
        app.grid_focused = true;

        run_load_result_and_keyboard(&mut app, &ctx, event, true);
        assert_eq!(app.workspace.active_document, Some(original_id));
        assert!(!app.workspace.documents.contains_key(&new_id));

        run_empty_load_result_frame(&mut app, &ctx);
        assert_eq!(app.workspace.active_document, Some(new_id));
        assert_eq!(
            app.workspace.documents[&new_id].tables[0]
                .cell(CellAddress::default())
                .unwrap()
                .input,
            CellInput::Text("keep".to_owned())
        );
    }
}

#[test]
// Keep a completed open from replacing the target before a native menu action runs.
fn completed_load_waits_for_native_document_interaction() {
    let directory = TestDirectory::new();
    let path = directory.0.join("native-target.md");
    fs::write(&path, b"| new |\n| --- |\n").expect("write Markdown input");
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let original_id = app.workspace.active_document.unwrap();
    let new_document_id = app
        .start_path_load(path)
        .expect("Markdown load is submitted");
    let token = app.pending_markdown_loads[&new_document_id];
    let result = take_file_result(&mut app, &ctx, token);
    app.deferred_file_results.push(result);

    app.process_file_results_with_native_interaction(&ctx, true);

    assert_eq!(app.workspace.active_document, Some(original_id));
    assert!(!app.workspace.documents.contains_key(&new_document_id));
    assert_eq!(app.deferred_file_results.len(), 1);

    app.process_file_results_with_native_interaction(&ctx, false);

    assert_eq!(app.workspace.active_document, Some(new_document_id));
}

#[test]
// Keep editor state unchanged when an open request fails or is cancelled.
fn failed_and_cancelled_opens_preserve_the_live_editor_and_undo_history() {
    let directory = TestDirectory::new();
    let valid_path = directory.0.join("cancelled.md");
    fs::write(&valid_path, b"| value |\n| --- |\n").expect("write Markdown input");
    let missing_path = directory.0.join("missing.md");
    let ctx = egui::Context::default();

    for (path, cancel) in [(missing_path, false), (valid_path, true)] {
        let mut app = new_app(&ctx);
        let original_id = app.workspace.active_document.unwrap();
        let (_, table_id, address) = edit_blank_cell(&mut app);
        app.edit.buffer = "unfinished".to_owned();
        app.edit.cursor_byte = app.edit.buffer.len();
        app.edit.anchor_byte = app.edit.buffer.len();
        let session_id = app.edit.session_id;
        let revision = app.workspace.documents[&original_id].revision;
        let can_undo = app.workspace.documents[&original_id].can_undo();

        let new_document_id = app
            .start_path_load(path)
            .expect("Markdown request is queued");
        let token = app.pending_markdown_loads[&new_document_id];
        if cancel {
            assert!(app.file_worker.cancel(token));
        }
        let result = take_file_result(&mut app, &ctx, token);
        assert_eq!(matches!(result.kind, FileResultKind::Cancelled), cancel);
        app.deferred_file_results.push(result);
        app.process_file_results();

        let document = &app.workspace.documents[&original_id];
        assert_eq!(app.workspace.order, vec![original_id]);
        assert_eq!(document.revision, revision);
        assert_eq!(document.active_table, table_id);
        assert_eq!(document.view(table_id).unwrap().selection.head, address);
        assert_eq!(document.can_undo(), can_undo);
        assert_eq!(app.edit.session_id, session_id);
        assert_eq!(app.edit.buffer, "unfinished");
        assert!(app.edit.is_editing());
    }
}

#[test]
// Keep pending load progress moving with a short repaint wake.
fn active_load_schedules_a_short_progress_wake() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let delays = Arc::new(Mutex::new(Vec::new()));
    ctx.set_repaint_observer({
        let delays = Arc::clone(&delays);
        move |info| delays.lock().unwrap().push(info.delay)
    });
    let document_id = app.workspace.active_document.unwrap();
    app.pending_markdown_loads.insert(
        document_id,
        JobToken {
            document_id,
            revision: 0,
            job_id: 1,
        },
    );

    app.process_load_progress(&ctx);

    assert!(delays.lock().unwrap().contains(&Duration::from_millis(50)));
}

// Waits for a worker result while keeping its bounded preview queue moving.
fn take_file_result(app: &mut TexcelApp, _ctx: &egui::Context, token: JobToken) -> FileResult {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let _ = app.file_worker.poll_load_progress();
        if let Some(result) = app
            .file_worker
            .poll()
            .into_iter()
            .find(|result| result.token == token)
        {
            return result;
        }
        assert!(Instant::now() < deadline, "load worker did not finish");
        std::thread::sleep(Duration::from_millis(2));
    }
}

// Submit and collect a Markdown load through the real file worker.
fn queue_completed_markdown_load(
    app: &mut TexcelApp,
    ctx: &egui::Context,
    path: std::path::PathBuf,
) -> DocumentId {
    let document_id = app.start_path_load(path).expect("Markdown load starts");
    let token = app.pending_markdown_loads[&document_id];
    let result = take_file_result(app, ctx, token);
    app.deferred_file_results.push(result);
    document_id
}

// Deliver file results before handling the current frame's keyboard action.
fn run_load_result_and_keyboard(
    app: &mut TexcelApp,
    ctx: &egui::Context,
    event: Event,
    focus_grid_after_results: bool,
) {
    let mut output = ctx.run_ui(
        egui::RawInput {
            events: vec![event],
            ..egui::RawInput::default()
        },
        |ui| {
            app.process_file_results_with_native_interaction(ui.ctx(), false);
            if focus_grid_after_results {
                app.grid_focused = true;
            }
            app.process_keyboard(ui.ctx(), false);
        },
    );
    output.textures_delta.clear();
}

// Adopt a deferred load after the old document has handled its input.
fn run_empty_load_result_frame(app: &mut TexcelApp, ctx: &egui::Context) {
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        app.process_file_results_with_native_interaction(ui.ctx(), false);
    });
    output.textures_delta.clear();
}

// Waits until the real CSV options request opens its settings window.
fn wait_for_csv_prompt(harness: &mut Harness<'_, TexcelApp>, document_id: DocumentId) {
    for _ in 0..200 {
        harness.step();
        if harness
            .state()
            .pending_csv_opens
            .get(&document_id)
            .is_some_and(|pending| pending.prompt.is_some())
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    panic!("CSV settings did not appear");
}

// Clicks the load action exposed by the CSV settings window.
fn click_csv_load(harness: &mut Harness<'_, TexcelApp>) {
    let label = context_text(&harness.ctx, "dialog-csv-load");
    harness
        .query_all(By::new().role(egui::Role::Button).label_contains(&label))
        .next()
        .expect("CSV load action is visible")
        .click();
    harness.step();
}

// Waits until the loaded CSV document is installed in the workspace.
fn wait_for_csv_load(harness: &mut Harness<'_, TexcelApp>, document_id: DocumentId) {
    for _ in 0..200 {
        harness.step();
        if !harness.state().pending_csv_opens.contains_key(&document_id) {
            return;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    panic!("CSV load did not finish");
}
