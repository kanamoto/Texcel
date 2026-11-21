use super::*;
use std::time::{Duration, Instant};

// Creates a TexcelApp through the normal production constructor.
fn blank_application() -> TexcelApp {
    let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
    TexcelApp::new_with_settings_path(&creation, None)
}

// Pumps the normal file-result path until the selected document is no longer preparing work.
fn wait_for_structure(app: &mut TexcelApp, document_id: DocumentId) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.pending_structures.contains_key(&document_id) {
        app.process_file_results();
        assert!(Instant::now() < deadline, "structure worker timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
    app.process_file_results();
}

#[test]
// Verify that the production constructor opens one clean, editable Markdown table.
fn production_constructor_starts_with_a_blank_markdown_document() {
    let app = blank_application();
    let document = app
        .workspace
        .active_document()
        .expect("a blank document is active at startup");
    let Some(SourceDocument::Markdown(source)) = document.source.as_deref() else {
        panic!("startup document uses Markdown source");
    };
    assert_eq!(document.tables.len(), 1);
    assert_eq!(source.tables.len(), 1);
    assert_eq!(source.tables[0].rows.len(), 2);
    assert!(source.tables[0].rows[1][0].value.is_empty());
    assert_eq!(source.source_extent(0), Some((1, 1)));
    assert_eq!(document.tables[0].row_count(), 1);
    assert_eq!(document.tables[0].column_count(), 1);
    assert_eq!(
        document.extent_end(document.active_table),
        Some(CellAddress::default())
    );
    assert!(!document.is_dirty());
}

#[test]
// Verify Enter commits adjacent growth through one worker result and moves selection afterward.
fn cell_editor_commits_adjacent_growth_and_moves_after_worker_ack() {
    let mut app = blank_application();
    let document_id = app
        .workspace
        .active_document
        .expect("startup document is active");
    let table_id = app
        .workspace
        .active_document()
        .and_then(|document| document.active_table())
        .expect("startup table exists")
        .id;
    let address = CellAddress { row: 2, column: 0 };
    app.begin_cell_edit(table_id, address);
    app.edit.buffer = "new row".to_owned();
    app.commit_edit(Some((1, 0)));
    assert!(app.pending_structures.contains_key(&document_id));
    assert!(!app.edit.is_editing());

    wait_for_structure(&mut app, document_id);
    let document = app
        .workspace
        .documents
        .get(&document_id)
        .expect("the original document remains open");
    assert_eq!(
        document.tables[0].cell(address).map(|cell| &cell.input),
        Some(&CellInput::Text("new row".to_owned()))
    );
    assert_eq!(
        document
            .view(table_id)
            .expect("table view")
            .selection
            .head
            .row,
        3
    );
    assert!(app.pending_text_growth_sessions.is_empty());
    assert_eq!(document.extent_end(table_id), Some(address));

    app.begin_cell_edit(table_id, CellAddress { row: 3, column: 0 });
    app.edit.buffer = "next row".to_owned();
    app.commit_edit(None);
    wait_for_structure(&mut app, document_id);
    let document = &app.workspace.documents[&document_id];
    let Some(SourceDocument::Markdown(source)) = document.source.as_deref() else {
        panic!("the document keeps Markdown as its source");
    };
    assert_eq!(source.tables[0].rows.len(), 4);
    assert_eq!(
        document.extent_end(table_id),
        Some(CellAddress { row: 3, column: 0 })
    );
    assert_eq!(
        document.tables[0]
            .cell(CellAddress { row: 3, column: 0 })
            .map(|cell| &cell.input),
        Some(&CellInput::Text("next row".to_owned()))
    );
}

#[test]
// Keep range cycling and disjoint selections after a CSV tail-cell growth commit.
fn csv_growth_ack_preserves_the_primary_range_and_additional_ranges() {
    let mut app = blank_application();
    let document_id = app.workspace.active_document.expect("a document is active");
    let csv = crate::csv_adapter::CsvDocument::parse(
        b"name,value\nfirst,1\n".to_vec(),
        crate::csv_adapter::EncodingChoice::Auto,
        ',',
    )
    .expect("the test CSV parses");
    let mut document = csv.to_document(document_id, "Two-row CSV");
    document.set_source(Some(Arc::new(SourceDocument::Csv(Arc::new(csv)))));
    document.mark_saved(document.revision);
    let table_id = document.active_table().expect("CSV table exists").id;
    app.workspace.documents.insert(document_id, document);
    app.workspace.select_document(document_id);

    let range = CellRange {
        anchor: CellAddress { row: 0, column: 0 },
        head: CellAddress { row: 2, column: 0 },
    };
    let view = app
        .workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .view_mut(table_id)
        .unwrap();
    view.set_selection_with_active(range, range.anchor);
    assert!(app.cycle_selection_cell(false));
    assert!(app.cycle_selection_cell(false));
    let view = app.workspace.documents[&document_id]
        .view(table_id)
        .unwrap();
    assert_eq!(view.active_cell(), range.head);

    app.begin_cell_edit(table_id, range.head);
    app.edit.buffer = "third".to_owned();
    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .view_mut(table_id)
        .unwrap()
        .additional_selections
        .push(CellRange::single(CellAddress { row: 0, column: 1 }));
    app.commit_edit(Some((1, 0)));
    assert!(app.pending_structures.contains_key(&document_id));
    wait_for_structure(&mut app, document_id);

    let document = &app.workspace.documents[&document_id];
    let view = document.view(table_id).unwrap();
    assert_eq!(view.selection, range);
    assert_eq!(view.active_cell(), range.anchor);
    assert_eq!(
        view.additional_selections,
        [CellRange::single(CellAddress { row: 0, column: 1 })]
    );
    assert_eq!(
        document.tables[0].cell(range.head).map(|cell| &cell.input),
        Some(&CellInput::Text("third".to_owned()))
    );

    let next_range = CellRange {
        anchor: range.anchor,
        head: CellAddress { row: 3, column: 0 },
    };
    let view = app
        .workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .view_mut(table_id)
        .unwrap();
    view.set_selection_with_active(next_range, next_range.head);
    view.additional_selections.clear();
    app.begin_cell_edit(table_id, next_range.head);
    app.edit.buffer = "fourth".to_owned();
    app.commit_edit(Some((1, 0)));
    let view = app
        .workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .view_mut(table_id)
        .unwrap();
    view.set_selection(CellRange::single(CellAddress { row: 0, column: 1 }));
    wait_for_structure(&mut app, document_id);

    let view = app.workspace.documents[&document_id]
        .view(table_id)
        .unwrap();
    assert_eq!(
        view.selection,
        CellRange::single(CellAddress { row: 0, column: 1 })
    );
    assert_eq!(view.active_cell(), CellAddress { row: 0, column: 1 });
}

#[test]
// Verify cancellation keeps the source bytes and restores the uncommitted cell draft.
fn cancelling_adjacent_growth_keeps_the_editor_draft() {
    let mut app = blank_application();
    let document_id = app
        .workspace
        .active_document
        .expect("startup document is active");
    let table_id = app
        .workspace
        .active_document()
        .and_then(|document| document.active_table())
        .expect("startup table exists")
        .id;
    let address = CellAddress { row: 2, column: 0 };
    let original_source = match app
        .workspace
        .active_document()
        .and_then(|document| document.source.as_deref())
    {
        Some(SourceDocument::Markdown(source)) => source.source_bytes().to_vec(),
        _ => panic!("startup uses Markdown source"),
    };
    app.begin_cell_edit(table_id, address);
    app.edit.buffer = "keep this draft".to_owned();
    app.commit_edit(None);
    app.cancel_structure_for(document_id);
    assert_eq!(app.edit.buffer, "keep this draft");
    assert!(app.edit.is_editing());
    assert!(!app.pending_structures.contains_key(&document_id));
    let document = app
        .workspace
        .documents
        .get(&document_id)
        .expect("document exists");
    let Some(SourceDocument::Markdown(source)) = document.source.as_deref() else {
        panic!("source remains Markdown after cancellation");
    };
    assert_eq!(source.source_bytes(), original_source);
    assert!(document.tables[0].cell(address).is_none());
}

#[test]
// Verify completion does not focus or move the selection in another active document.
fn adjacent_growth_completion_does_not_steal_another_document_focus() {
    let mut app = blank_application();
    let original_id = app
        .workspace
        .active_document
        .expect("startup document is active");
    let table_id = app
        .workspace
        .active_document()
        .and_then(|document| document.active_table())
        .expect("startup table exists")
        .id;
    app.begin_cell_edit(table_id, CellAddress { row: 2, column: 0 });
    app.edit.buffer = "background result".to_owned();
    app.commit_edit(Some((1, 0)));

    app.create_new_document("Other".to_owned())
        .expect("second blank document is created");
    let other_id = app
        .workspace
        .active_document
        .expect("second document is active");
    app.request_grid_focus = false;
    wait_for_structure(&mut app, original_id);
    assert_eq!(app.workspace.active_document, Some(other_id));
    assert!(!app.request_grid_focus);
    assert!(!app.edit.is_editing());
    let other = app
        .workspace
        .documents
        .get(&other_id)
        .expect("other document exists");
    assert_eq!(
        other
            .view(other.active_table().expect("other table").id)
            .expect("other view")
            .selection
            .head,
        CellAddress::default()
    );
}

#[test]
// Save only the revision created after adjacent growth succeeds.
fn save_requested_during_growth_waits_for_the_worker_ack() {
    let mut app = blank_application();
    let document_id = app.workspace.active_document.expect("document is active");
    let table_id = app
        .workspace
        .active_document()
        .and_then(|document| document.active_table())
        .expect("table exists")
        .id;
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock is after the epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("texcel-growth-save-{unique}.md"));
    app.begin_cell_edit(table_id, CellAddress { row: 2, column: 0 });
    app.edit.buffer = "saved after growth".to_owned();
    app.commit_edit(None);
    let saved_revision_before = app.workspace.documents[&document_id].saved_revision;
    app.save_document_to_path(document_id, path.clone());
    assert!(!app.pending_saves.contains_key(&document_id));
    assert_eq!(
        app.pending_text_growth_sessions[&document_id].deferred_save_path,
        Some(path.clone())
    );
    assert_eq!(
        app.workspace.documents[&document_id].saved_revision,
        saved_revision_before
    );

    wait_for_structure(&mut app, document_id);
    let committed_revision = app.workspace.documents[&document_id].revision;
    if let Some(token) = app.pending_saves.get(&document_id) {
        assert_eq!(token.revision, committed_revision);
        assert_eq!(
            app.workspace.documents[&document_id].saved_revision,
            saved_revision_before
        );
    } else {
        assert_eq!(
            app.workspace.documents[&document_id].saved_revision,
            committed_revision
        );
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.pending_saves.contains_key(&document_id) {
        app.process_file_results();
        assert!(Instant::now() < deadline, "save worker timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
    let document = &app.workspace.documents[&document_id];
    assert_eq!(document.saved_revision, committed_revision);
    assert!(!document.is_dirty());
    assert!(
        std::fs::read_to_string(&path)
            .expect("saved Markdown file exists")
            .contains("saved after growth")
    );
    std::fs::remove_file(path).expect("temporary Markdown file is removed");
}

#[test]
// Keep a deferred Save after exit is cancelled without letting its acknowledgement close the app.
fn cancelling_app_close_disables_deferred_growth_exit_but_keeps_save() {
    let mut app = blank_application();
    let document_id = app.workspace.active_document.expect("document is active");
    let table_id = app
        .workspace
        .active_document()
        .and_then(|document| document.active_table())
        .expect("table exists")
        .id;
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock is after the epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("texcel-growth-cancel-save-{unique}.md"));

    app.begin_cell_edit(table_id, CellAddress { row: 2, column: 0 });
    app.edit.buffer = "saved after close cancel".to_owned();
    app.commit_edit(None);
    app.app_close_dialog_open = true;
    app.save_document_to_path(document_id, path.clone());
    assert!(app.pending_text_growth_sessions[&document_id].app_close_save);
    assert_eq!(
        app.pending_text_growth_sessions[&document_id].deferred_save_path,
        Some(path.clone())
    );

    app.cancel_app_close();
    assert!(!app.pending_text_growth_sessions[&document_id].app_close_save);
    assert!(!app.app_close_after_documents);
    assert!(!app.app_close_discard_exit_ready);
    wait_for_structure(&mut app, document_id);

    let deadline = Instant::now() + Duration::from_secs(5);
    while app.pending_saves.contains_key(&document_id) {
        app.process_file_results();
        assert!(Instant::now() < deadline, "save worker timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!app.app_close_after_documents);
    assert!(!app.app_close_discard_exit_ready);
    assert!(!app.app_close_save_tokens.contains_key(&document_id));
    assert!(
        std::fs::read_to_string(&path)
            .expect("deferred Save still writes the Markdown file")
            .contains("saved after close cancel")
    );
    std::fs::remove_file(path).expect("temporary Markdown file is removed");
}

#[test]
// Keep a failed inactive growth draft in the same state used by close and recovery.
fn inactive_growth_failure_remains_recoverable_and_blocks_close() {
    let mut app = blank_application();
    let document_id = app.workspace.active_document.expect("document is active");
    let table_id = app
        .workspace
        .active_document()
        .and_then(|document| document.active_table())
        .expect("table exists")
        .id;
    app.begin_cell_edit(table_id, CellAddress { row: 2, column: 0 });
    app.edit.buffer = "keep as a draft".to_owned();
    app.commit_edit(None);
    let recovered = app
        .recovery_snapshot(document_id, 1)
        .expect("pending input can be copied into recovery");
    assert_eq!(
        recovered
            .cell_edit
            .as_ref()
            .map(|edit| edit.buffer.as_str()),
        Some("keep as a draft")
    );

    app.create_new_document("Other".to_owned())
        .expect("another document becomes active");
    app.cancel_structure_for(document_id);
    assert_eq!(
        app.pending_recovered_cell_edits[&document_id].buffer,
        "keep as a draft"
    );
    assert!(app.app_close_needs_save());
    app.request_close_document(document_id);
    assert_eq!(app.pending_close_document, Some(document_id));
    assert!(app.workspace.documents.contains_key(&document_id));

    app.workspace.select_document(document_id);
    app.restore_recovered_cell_edit(document_id);
    assert!(app.edit.is_editing());
    assert_eq!(app.edit.buffer, "keep as a draft");
}
