use super::*;
use std::time::{Duration, Instant};

const MARKDOWN_WITH_TEXT_AND_FORMULA: &[u8] = b"# Keep this heading\n\nOutside text stays here.\n\n| ID | Formula | Text |\n| :--- | ---: | :--- |\n| 00123 | =CONCATENATE(\"=literal\") | **keep** |\n\nTail text stays here.\n";

/// Pumps the real file and calculation workers until the loaded formula version is current.
fn wait_for_document_and_calculation(
    app: &mut TexcelApp,
    context: &egui::Context,
    document_id: DocumentId,
) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while !app.workspace.documents.contains_key(&document_id) {
        let _ = app.file_worker.poll_load_progress();
        app.process_file_results();
        assert!(
            Instant::now() < deadline,
            "production loader did not adopt {document_id}; pending={:?}, status={:?}",
            app.pending_markdown_loads.get(&document_id),
            app.status,
        );
        std::thread::sleep(Duration::from_millis(3));
    }
    loop {
        let _ = app.file_worker.poll_load_progress();
        app.process_file_results();
        app.process_calculation_results(context);
        let ready = app
            .workspace
            .documents
            .get(&document_id)
            .is_some_and(|document| {
                app.calculation_snapshots
                    .get(&document_id)
                    .is_some_and(|(revision, _)| *revision == document.calculation_revision)
            });
        if ready {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "document calculation did not settle; document={:?}, snapshot={:?}, initial={:?}, pending={:?}, status={:?}",
            app.workspace.documents.get(&document_id).map(|document| (
                document.revision,
                document.calculation_revision,
                document.has_pending_calculation_changes(),
            )),
            app.calculation_snapshots
                .get(&document_id)
                .map(|(revision, _)| *revision),
            app.pending_initial_calculations.contains_key(&document_id),
            app.pending_calculations.get(&document_id),
            app.status,
        );
        std::thread::sleep(Duration::from_millis(3));
    }
}

/// Waits for the production preflight to return a report for the requested document.
fn wait_for_export_report(app: &mut TexcelApp, document_id: DocumentId) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while !app
        .active_export_confirmation
        .as_ref()
        .is_some_and(|export| export.document_id == document_id)
    {
        app.process_file_results();
        assert!(
            Instant::now() < deadline,
            "Save As preflight did not settle"
        );
        std::thread::sleep(Duration::from_millis(3));
    }
}

/// Waits for the output-only export worker to finish writing its selected file.
fn wait_for_export_write(app: &mut TexcelApp, document_id: DocumentId) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while app.pending_export_writes.contains_key(&document_id) {
        app.process_file_results();
        assert!(Instant::now() < deadline, "Save As output did not settle");
        std::thread::sleep(Duration::from_millis(3));
    }
    app.process_file_results();
}

fn markdown_result_options() -> ExportOptions {
    ExportOptions {
        format: ExportFormat::Markdown,
        scope: ExportScope::AllTables,
        formulas: FormulaOutput::LatestResult,
        modifiers: ModifierOutput::Convert,
        markdown_header: MarkdownHeader::UseFirstRow,
    }
}

/// Uses a loaded Markdown document so formula results change without rebuilding other source bytes.
#[test]
fn latest_result_save_as_preserves_markdown_text_divider_and_literal_formula_text() {
    let directory = TestDirectory::new();
    let source_path = directory.0.join("source.md");
    let destination = directory.0.join("results.md");
    std::fs::write(&source_path, MARKDOWN_WITH_TEXT_AND_FORMULA).expect("write the source fixture");
    let context = egui::Context::default();
    let mut app = new_app(&context);
    app.settings_loaded = true;
    let document_id = app
        .start_path_load(source_path.clone())
        .expect("start the production Markdown load");
    wait_for_document_and_calculation(&mut app, &context, document_id);
    let document = &app.workspace.documents[&document_id];
    let formula_address = CellAddress { row: 1, column: 1 };
    assert!(matches!(
        app.calculation_snapshots[&document_id]
            .1
            .get(document.active_table, formula_address),
        Some((
            CalcStatus::Fresh,
            Some(crate::document::CalcValue::Text(value))
        )) if value == "=literal"
    ));
    let source_revision = document.revision;
    let saved_revision = document.saved_revision;
    let recent_before = app.recent_files.clone();
    let source_backing = app.file_backings[&document_id].path.clone();

    app.save_as_to_path_with_options(
        document_id,
        PendingSaveIntent {
            path: destination.clone(),
            options: markdown_result_options(),
            track_recent: true,
            close_after_save: false,
            close_edit: None,
            app_close_generation: None,
        },
    );
    wait_for_export_report(&mut app, document_id);
    let export = app
        .active_export_confirmation
        .take()
        .expect("result conversion shows its report before writing");
    assert!(export.prepared.report.can_export());
    assert_eq!(export.prepared.report.summary().omitted, 0);
    app.submit_prepared_export(export);
    wait_for_export_write(&mut app, document_id);

    let original = std::fs::read(&source_path).expect("source stays available");
    assert_eq!(original, MARKDOWN_WITH_TEXT_AND_FORMULA);
    let output = std::fs::read(&destination).expect("result Markdown was written");
    let expected = String::from_utf8_lossy(MARKDOWN_WITH_TEXT_AND_FORMULA)
        .replace("=CONCATENATE(\"=literal\")", "'=literal")
        .into_bytes();
    assert_eq!(
        output, expected,
        "unrelated Markdown bytes remain unchanged"
    );
    let parsed = crate::markdown_adapter::MarkdownDocument::parse(output)
        .expect("result Markdown remains parseable");
    let display = parsed
        .cell_display(0, formula_address.row, formula_address.column)
        .expect("the result cell has a Markdown display");
    assert_eq!(display.plain_text, "=literal");
    assert!(display.literal_marker_consumed);
    assert_eq!(
        app.file_backings[&document_id].path, source_backing,
        "result output does not replace the source backing"
    );
    assert_eq!(app.recent_files, recent_before);
    let document = &app.workspace.documents[&document_id];
    assert_eq!(document.revision, source_revision);
    assert_eq!(document.saved_revision, saved_revision);
    assert!(!document.is_dirty());
}

/// Rejects a result output that aliases the input before it can overwrite the open source.
#[test]
fn result_save_as_rejects_a_symlink_alias_without_recent_or_close_ack() {
    let directory = TestDirectory::new();
    let source_path = directory.0.join("source.md");
    #[cfg(unix)]
    let destination = directory.0.join("source-alias.md");
    #[cfg(not(unix))]
    let destination = source_path.clone();
    std::fs::write(&source_path, MARKDOWN_WITH_TEXT_AND_FORMULA).expect("write the source fixture");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&source_path, &destination).expect("create a file alias");

    let context = egui::Context::default();
    let mut app = new_app(&context);
    app.settings_loaded = true;
    let document_id = app
        .start_path_load(source_path.clone())
        .expect("start the production Markdown load");
    wait_for_document_and_calculation(&mut app, &context, document_id);
    let source_backing = app.file_backings[&document_id].path.clone();
    let recent_before = app.recent_files.clone();
    app.save_as_to_path_with_options(
        document_id,
        PendingSaveIntent {
            path: destination.clone(),
            options: markdown_result_options(),
            track_recent: true,
            close_after_save: true,
            close_edit: None,
            app_close_generation: None,
        },
    );
    wait_for_export_report(&mut app, document_id);
    let report = &app
        .active_export_confirmation
        .as_ref()
        .unwrap()
        .prepared
        .report;
    assert!(!report.can_export());
    assert!(report.entries.iter().any(|entry| {
        entry
            .message
            .as_ref()
            .is_some_and(|message| message.key() == "report-reason-same-source-path")
    }));
    let export = app.active_export_confirmation.take().unwrap();
    app.submit_prepared_export(export);

    assert_eq!(
        std::fs::read(&source_path).expect("the source was not overwritten"),
        MARKDOWN_WITH_TEXT_AND_FORMULA
    );
    assert!(!app.pending_export_writes.contains_key(&document_id));
    assert_eq!(app.recent_files, recent_before);
    assert!(app.pending_close_document.is_none());
    assert!(!app.close_after_save.contains(&document_id));
    assert!(app.workspace.documents.contains_key(&document_id));
    assert_eq!(app.file_backings[&document_id].path, source_backing);
}

/// Stops result output when a real edit has no matching calculation snapshot yet.
#[test]
fn latest_result_save_as_blocks_an_unrecalculated_formula() {
    let directory = TestDirectory::new();
    let source_path = directory.0.join("source.md");
    let destination = directory.0.join("stale-results.md");
    std::fs::write(&source_path, b"| Value |\n| --- |\n| old |\n")
        .expect("write the source fixture");
    let context = egui::Context::default();
    let mut app = new_app(&context);
    app.settings_loaded = true;
    let document_id = app
        .start_path_load(source_path.clone())
        .expect("start the production Markdown load");
    wait_for_document_and_calculation(&mut app, &context, document_id);
    let source_backing = app.file_backings[&document_id].path.clone();
    let table_id = app.workspace.documents[&document_id].active_table;
    let previous_calc_revision = app.workspace.documents[&document_id].calculation_revision;
    app.begin_cell_edit(table_id, CellAddress { row: 1, column: 0 });
    app.edit.buffer = "=1+1".to_owned();
    app.commit_edit(None);
    let document = &app.workspace.documents[&document_id];
    assert!(document.calculation_revision > previous_calc_revision);
    assert_ne!(
        app.calculation_snapshots[&document_id].0, document.calculation_revision,
        "the old snapshot cannot stand in for the changed formula"
    );
    let recent_before = app.recent_files.clone();

    app.save_as_to_path_with_options(
        document_id,
        PendingSaveIntent {
            path: destination.clone(),
            options: markdown_result_options(),
            track_recent: true,
            close_after_save: false,
            close_edit: None,
            app_close_generation: None,
        },
    );
    wait_for_export_report(&mut app, document_id);
    let report = &app
        .active_export_confirmation
        .as_ref()
        .unwrap()
        .prepared
        .report;
    assert!(!report.can_export());
    assert!(report.summary().unprocessable > 0);
    assert!(!destination.exists());
    assert!(app.pending_export_writes.is_empty());
    assert_eq!(app.recent_files, recent_before);
    assert_eq!(app.file_backings[&document_id].path, source_backing);
    assert!(app.document_requires_save(document_id));
    assert_eq!(
        std::fs::read(&source_path).expect("source stays unchanged"),
        b"| Value |\n| --- |\n| old |\n"
    );
}

#[test]
/// Offers the formula choice when Save needs a destination, but not when closing saves.
fn save_destination_dialog_offers_formula_output_like_save_as() {
    let context = egui::Context::default();
    let mut app = new_app(&context);
    let document_id = app.workspace.active_document.unwrap();

    app.save_active(false);
    let intent = app
        .export_dialog
        .take()
        .and_then(|dialog| dialog.save_intent)
        .expect("an untitled document asks for a destination");
    assert!(intent.allow_formula_results);
    assert!(!intent.close_after_save);
    app.cancel_save_as_dialog(document_id, intent);

    // Closing must leave a saved document, so its Save keeps formulas as written.
    app.save_and_close_document(document_id);
    let intent = app
        .export_dialog
        .take()
        .and_then(|dialog| dialog.save_intent)
        .expect("closing an untitled document asks for a destination");
    assert!(!intent.allow_formula_results);
    app.cancel_save_as_dialog(document_id, intent);
}
