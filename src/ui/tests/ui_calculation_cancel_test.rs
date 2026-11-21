use super::*;

/// Creates a document with one source formula and one input cell.
fn formula_document() -> (Workspace, DocumentId, TableId, CellAddress) {
    let mut workspace = Workspace::default();
    let document_id = workspace.add_document("calculation", &["Sheet1"]);
    let document = workspace.documents.get_mut(&document_id).unwrap();
    let table_id = document.tables[0].id;
    let value_address = CellAddress { row: 0, column: 0 };
    let formula_address = CellAddress { row: 0, column: 1 };
    document.tables[0].set_loaded_cell(value_address, Cell::number(1.0));
    document.tables[0].set_loaded_cell(formula_address, Cell::formula("=A1+1", None));
    (workspace, document_id, table_id, formula_address)
}

/// Builds a complete result root for one formula cell.
fn root_result(
    token: crate::file_worker::JobToken,
    table_id: TableId,
    address: CellAddress,
    value: f64,
) -> crate::calc::CalculationResult {
    crate::calc::CalculationResult {
        token,
        full_snapshot: Some(Arc::new(crate::calc::FormulaResultRoot::new(
            token.revision,
            vec![(
                table_id,
                address,
                CalcStatus::Fresh,
                Some(crate::document::CalcValue::Number(value)),
            )],
        ))),
        failed_changes: Vec::new(),
        error: None,
        done: true,
    }
}

/// Confirms a terminal calculation result reaches the document root.
fn finish_result(
    app: &mut TexcelApp,
    ctx: &egui::Context,
    sender: &std::sync::mpsc::SyncSender<crate::calc::CalculationResult>,
    result: crate::calc::CalculationResult,
) {
    sender.send(result).unwrap();
    app.process_calculation_results(ctx);
}

#[test]
// A cancelled update keeps old results stale and rejects its late terminal result.
fn cancelled_update_retries_same_revision_and_publishes_only_terminal_root() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let (workspace, document_id, table_id, formula_address) = formula_document();
    let startup_document = app.workspace.active_document.unwrap();
    app.clear_calculation_jobs_for_reset(startup_document);
    app.workspace = workspace;
    let original_revision = app.workspace.documents[&document_id].calculation_revision;
    let original_root = Arc::new(crate::calc::FormulaResultRoot::new(
        original_revision,
        vec![(
            table_id,
            formula_address,
            CalcStatus::Fresh,
            Some(crate::document::CalcValue::Number(2.0)),
        )],
    ));
    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .install_calculation_results(Arc::clone(&original_root));
    app.calculation_snapshots
        .insert(document_id, (original_revision, original_root));

    let (worker, jobs, results) = crate::calc::tests::calculation_channels_for_test();
    app.calculation_worker = worker;
    let value_address = CellAddress { row: 0, column: 0 };
    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .set_cell(table_id, value_address, Some(Cell::number(2.0)))
        .expect("the input change queues calculation work");
    let revision = app.workspace.documents[&document_id].calculation_revision;
    app.schedule_calculation(document_id);
    let old_token = app.pending_calculations[&document_id];
    assert!(
        matches!(jobs.try_recv(), Ok(crate::calc::Message::Update { token, .. }) if token == old_token)
    );

    app.cancel_calculation(document_id);
    assert!(app.calculation_retry_available.contains(&document_id));
    assert!(app.workspace.documents[&document_id].has_pending_calculation_changes());
    assert_eq!(
        app.workspace.documents[&document_id]
            .calculation_result_for(table_id, formula_address)
            .map(|result| result.status),
        Some(CalcStatus::Stale)
    );

    app.retry_calculation(document_id);
    let new_token = app.pending_calculations[&document_id];
    assert_ne!(old_token.job_id, new_token.job_id);
    assert_eq!(old_token.revision, new_token.revision);
    assert_eq!(revision, new_token.revision);
    assert!(
        matches!(jobs.try_recv(), Ok(crate::calc::Message::Update { token, .. }) if token == new_token)
    );

    finish_result(
        &mut app,
        &ctx,
        &results,
        root_result(old_token, table_id, formula_address, 99.0),
    );
    assert_eq!(app.pending_calculations.get(&document_id), Some(&new_token));
    assert_eq!(
        app.workspace.documents[&document_id]
            .calculation_result_for(table_id, formula_address)
            .map(|result| result.status),
        Some(CalcStatus::Stale)
    );

    results
        .send(crate::calc::CalculationResult {
            token: new_token,
            full_snapshot: None,
            failed_changes: Vec::new(),
            error: None,
            done: false,
        })
        .unwrap();
    app.process_calculation_results(&ctx);
    assert_eq!(
        app.workspace.documents[&document_id]
            .calculation_result_for(table_id, formula_address)
            .map(|result| result.status),
        Some(CalcStatus::Stale),
        "partial cell values never become Fresh"
    );

    finish_result(
        &mut app,
        &ctx,
        &results,
        root_result(new_token, table_id, formula_address, 3.0),
    );
    let current = app.workspace.documents[&document_id]
        .calculation_result_for(table_id, formula_address)
        .expect("the complete root is visible");
    assert_eq!(current.revision, revision);
    assert_eq!(current.status, CalcStatus::Fresh);
    assert_eq!(current.value, Some(crate::document::CalcValue::Number(3.0)));
    let document = &app.workspace.documents[&document_id];
    let cell = document.tables[0]
        .cell(formula_address)
        .expect("formula source remains in the document");
    assert_eq!(cell.display_text_with_calculation(Some(&current)), "3");
    assert_eq!(
        crate::clipboard::ClipboardCell::from_cell_with_calculation(cell, Some(&current))
            .fresh_formula_value,
        Some(CellInput::Number(3.0))
    );
    finish_result(
        &mut app,
        &ctx,
        &results,
        root_result(old_token, table_id, formula_address, 99.0),
    );
    assert_eq!(
        app.workspace.documents[&document_id]
            .calculation_result_for(table_id, formula_address)
            .and_then(|result| result.value),
        Some(crate::document::CalcValue::Number(3.0)),
        "an old result arriving after the retry cannot replace the current root"
    );
}

#[test]
// A cancelled initial model stays retained until a fresh job completes.
fn cancelled_initial_calculation_retries_without_accepting_old_result() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let (workspace, document_id, table_id, formula_address) = formula_document();
    let startup_document = app.workspace.active_document.unwrap();
    app.clear_calculation_jobs_for_reset(startup_document);
    app.workspace = workspace;
    let snapshot =
        CalculationSnapshot::from_document(&app.workspace.documents[&document_id], false);
    let (worker, jobs, results) = crate::calc::tests::calculation_channels_for_test();
    app.calculation_worker = worker;
    let revision = app.workspace.documents[&document_id].calculation_revision;
    let old_token = app.next_job_token(document_id, revision);
    app.initialize_calculation(old_token, snapshot);
    assert!(
        matches!(jobs.try_recv(), Ok(crate::calc::Message::Initialize { token, .. }) if token == old_token)
    );

    app.cancel_calculation(document_id);
    assert!(app.calculation_initial_snapshots.contains_key(&document_id));
    finish_result(
        &mut app,
        &ctx,
        &results,
        root_result(old_token, table_id, formula_address, 99.0),
    );
    assert!(
        app.workspace.documents[&document_id]
            .calculation_result_for(table_id, formula_address)
            .is_none()
    );

    app.retry_calculation(document_id);
    let retry_token = app.pending_calculations[&document_id];
    assert_eq!(retry_token.revision, old_token.revision);
    assert_ne!(retry_token.job_id, old_token.job_id);
    assert!(
        matches!(jobs.try_recv(), Ok(crate::calc::Message::Initialize { token, .. }) if token == retry_token)
    );
    finish_result(
        &mut app,
        &ctx,
        &results,
        root_result(retry_token, table_id, formula_address, 2.0),
    );
    assert!(
        app.workspace.documents[&document_id]
            .calculation_result_for(table_id, formula_address)
            .is_some_and(|result| result.status == CalcStatus::Fresh)
    );
    assert!(!app.calculation_initial_snapshots.contains_key(&document_id));
}

#[test]
// Cancel a queue-full retry without losing its edits or submitting them automatically.
fn queue_full_calculation_retry_can_be_cancelled_without_auto_retry() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let (workspace, document_id, table_id, _) = formula_document();
    let startup_document = app.workspace.active_document.unwrap();
    app.clear_calculation_jobs_for_reset(startup_document);
    app.workspace = workspace;
    app.calculation_worker = crate::calc::tests::queue_full_for_test();
    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .set_cell(
            table_id,
            CellAddress { row: 1, column: 0 },
            Some(Cell::number(8.0)),
        )
        .expect("an edit queues calculation work");
    app.schedule_calculation(document_id);
    assert!(app.calculation_retries.contains(&document_id));
    app.cancel_calculation(document_id);
    assert!(!app.calculation_retries.contains(&document_id));
    assert!(app.calculation_retry_available.contains(&document_id));
    assert!(app.workspace.documents[&document_id].has_pending_calculation_changes());

    app.process_calculation_results(&ctx);
    assert!(!app.calculation_retries.contains(&document_id));
    assert!(!app.pending_calculations.contains_key(&document_id));
}
