use super::*;

/// Builds a document with one formula and its numeric input.
fn formula_document() -> (Workspace, DocumentId, TableId, CellAddress) {
    let mut workspace = Workspace::default();
    let document_id = workspace.add_document("calculation", &["Sheet1"]);
    let document = workspace.documents.get_mut(&document_id).unwrap();
    let table_id = document.tables[0].id;
    let input = CellAddress { row: 0, column: 0 };
    let formula = CellAddress { row: 0, column: 1 };
    document.tables[0].set_loaded_cell(input, Cell::number(1.0));
    document.tables[0].set_loaded_cell(formula, Cell::formula("=A1+1", None));
    (workspace, document_id, table_id, formula)
}

/// Creates a terminal worker result containing one formula value.
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

/// Sends a complete result through the app's regular ACK path.
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
// Keep an edited input out of a retried initial result until its update completes.
fn retried_initialization_waits_for_edits_queued_before_its_ack() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let (workspace, document_id, table_id, formula_address) = formula_document();
    let startup_document = app.workspace.active_document.unwrap();
    app.clear_calculation_jobs_for_reset(startup_document);
    app.workspace = workspace;
    let (worker, jobs, results) = crate::calc::tests::calculation_channels_for_test();
    app.calculation_worker = worker;

    let value_address = CellAddress { row: 0, column: 0 };
    let initial_revision = app.workspace.documents[&document_id].calculation_revision;
    let first_token = app.next_job_token(document_id, initial_revision);
    let snapshot = Arc::new(crate::calc::CalculationSnapshot::from_document(
        &app.workspace.documents[&document_id],
        false,
    ));
    app.initialize_calculation_shared(first_token, snapshot);
    assert!(
        matches!(jobs.try_recv(), Ok(crate::calc::Message::Initialize { token, .. }) if token == first_token)
    );

    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .set_cell(table_id, value_address, Some(Cell::number(10.0)))
        .expect("editing the referenced input queues a calculation change");
    app.cancel_calculation(document_id);
    app.retry_calculation(document_id);
    let retry_token = app.pending_calculations[&document_id];
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
            .is_none(),
        "the old initial snapshot is not published as Fresh"
    );
    assert!(!app.calculation_snapshots.contains_key(&document_id));

    let update_token = app.pending_calculations[&document_id];
    let changes = match jobs
        .try_recv()
        .expect("the queued input is sent after initialization")
    {
        crate::calc::Message::Update { token, changes, .. } => {
            assert_eq!(token, update_token);
            changes
        }
        _ => panic!("the retry must apply the queued input as an update"),
    };
    assert!(changes.iter().any(|change| {
        change.address == value_address
            && change
                .cell
                .as_ref()
                .is_some_and(|cell| cell.input == CellInput::Number(10.0))
    }));

    finish_result(
        &mut app,
        &ctx,
        &results,
        root_result(update_token, table_id, formula_address, 11.0),
    );
    let current = app.workspace.documents[&document_id]
        .calculation_result_for(table_id, formula_address)
        .expect("only the completed update becomes current");
    assert_eq!(current.status, CalcStatus::Fresh);
    assert_eq!(
        current.value,
        Some(crate::document::CalcValue::Number(11.0))
    );
}

#[test]
// Sends raw Markdown edits to the calculation worker with source projection context.
fn markdown_updates_send_raw_cells_for_worker_projection() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let source = Arc::new(
        crate::markdown_adapter::MarkdownDocument::parse(
            b"| Value | Count |\n| --- | --- |\n| 1 | =COUNT(A2) |\n".to_vec(),
        )
        .expect("Markdown source parses"),
    );
    let mut workspace = Workspace::default();
    let _unused_document_id = workspace.add_document("unused", &["Unused"]);
    let document_id = workspace.add_document("markdown", &["Table"]);
    let mut document = source.to_document(document_id, "markdown");
    let table_id = document.tables[0].id;
    document.set_source(Some(Arc::new(crate::source::SourceDocument::Markdown(
        source,
    ))));
    document
        .set_cell(
            table_id,
            CellAddress { row: 1, column: 0 },
            Some(Cell::text("**5**")),
        )
        .expect("edit one source-formatted cell");
    workspace.documents.insert(document_id, document);
    let startup_document = app.workspace.active_document.unwrap();
    app.clear_calculation_jobs_for_reset(startup_document);
    app.workspace = workspace;
    let (worker, jobs, _results) = crate::calc::tests::calculation_channels_for_test();
    app.calculation_worker = worker;

    app.schedule_calculation(document_id);
    let token = app.pending_calculations[&document_id];
    let changes = match jobs.try_recv().expect("worker receives the update") {
        crate::calc::Message::Update {
            token: queued,
            changes,
            markdown_source,
        } => {
            assert_eq!(queued, token);
            assert!(markdown_source.is_some());
            changes
        }
        _ => panic!("Markdown edits use the worker projection path"),
    };
    assert!(changes.iter().any(|change| {
        change.address == CellAddress { row: 1, column: 0 }
            && change
                .cell
                .as_ref()
                .is_some_and(|cell| cell.input == CellInput::Text("**5**".to_owned()))
    }));
}
