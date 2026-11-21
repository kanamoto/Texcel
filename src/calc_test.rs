use super::*;
use std::sync::{Arc, Mutex};

/// Builds a worker whose one-item queue is already full.
pub(crate) fn queue_full_for_test() -> CalculationWorker {
    static BLOCKED_RECEIVERS: std::sync::OnceLock<std::sync::Mutex<Vec<Receiver<Message>>>> =
        std::sync::OnceLock::new();
    let (sender, receiver) = mpsc::sync_channel(1);
    sender
        .try_send(Message::Stop)
        .expect("fill deterministic test queue");
    let (_result_sender, results) = mpsc::sync_channel(1);
    BLOCKED_RECEIVERS
        .get_or_init(Default::default)
        .lock()
        .expect("test receiver lock")
        .push(receiver);
    CalculationWorker {
        sender,
        results,
        thread: None,
        cancelled: Arc::new(Mutex::new(std::collections::HashSet::new())),
    }
}

/// Builds a calculation worker with controllable job and result channels.
pub(crate) fn calculation_channels_for_test() -> (
    CalculationWorker,
    Receiver<Message>,
    SyncSender<CalculationResult>,
) {
    let (sender, jobs) = mpsc::sync_channel(8);
    let (result_sender, results) = mpsc::sync_channel(8);
    (
        CalculationWorker {
            sender,
            results,
            thread: None,
            cancelled: Arc::new(Mutex::new(std::collections::HashSet::new())),
        },
        jobs,
        result_sender,
    )
}
use std::time::Duration;

// Wait until the calculation worker reports the requested revision.
fn wait_for_calculation(
    worker: &CalculationWorker,
    token: JobToken,
) -> Vec<(TableId, CellAddress, CalcStatus, Option<CalcValue>)> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        for result in worker.poll() {
            assert_eq!(result.token, token);
            assert!(result.error.is_none(), "{:?}", result.error);
            if result.done {
                return result
                    .full_snapshot
                    .map_or_else(Vec::new, |snapshot| snapshot.entries().to_vec());
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "calculation timed out"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
// Verify the formula default and the exact source escape for formula-like text.
fn text_formula_default_and_literal_escape_keep_values() {
    assert_eq!(
        crate::calc::formula_source(&CellInput::Text("=A1".to_owned()), false),
        Some("=A1".to_owned())
    );
    assert_eq!(crate::document::literal_text_value("'=A1"), "=A1");
    assert_eq!(crate::document::literal_text_value("''=A1"), "'=A1");
    assert_eq!(crate::document::literal_text_value("'hello"), "'hello");
    assert_eq!(crate::document::literal_text_value("=\"!\""), "=\"!\"");
    assert_eq!(crate::document::encode_literal_text("=A1"), "'=A1");
    assert_eq!(crate::document::encode_literal_text("'=A1"), "''=A1");
    assert_eq!(crate::document::encode_literal_text("'hello"), "'hello");

    let mut model = Model::new_empty("literal-escape", "en", "UTC", "en")
        .expect("IronCalc creates a one-sheet model");
    for (column, source) in ["5", "'=A1", "''=A1", "'hello", "=\"!\""]
        .into_iter()
        .enumerate()
    {
        let input = CellInput::Text(source.to_owned());
        let formula = crate::calc::engine::formula_source(&input, false);
        assert_eq!(formula.is_some(), column == 4);
        crate::calc::engine::formula::update_model_cell(
            &mut model,
            0,
            CellAddress { row: 0, column },
            &input,
            false,
            false,
        )
        .expect("cell input is accepted");
    }
    model.evaluate();
    let expected = [
        CalcValue::Number(5.0),
        CalcValue::Text("=A1".to_owned()),
        CalcValue::Text("'=A1".to_owned()),
        CalcValue::Text("'hello".to_owned()),
        CalcValue::Text("!".to_owned()),
    ];
    for (column, expected) in expected.into_iter().enumerate() {
        let (_, value) =
            crate::calc::engine::formula::read_result(&model, 0, CellAddress { row: 0, column })
                .expect("cell result is readable");
        assert_eq!(value, Some(expected));
    }
}

#[test]
// Verify that deleted-sheet formulas become invalid and do not bind to a later sheet with the same name.
fn deleting_a_sheet_marks_references_and_prevents_same_name_rebinding() {
    let mut model = Model::new_empty("sheet-delete-regression", "en", "UTC", "en")
        .expect("create a focused IronCalc model");
    model.new_sheet();
    model.new_sheet();
    model
        .rename_sheet("Sheet2", "Kept")
        .expect("name kept sheet");
    model
        .rename_sheet("Sheet3", "Gone")
        .expect("name deleted sheet");
    model
        .update_cell_with_formula(0, 1, 1, "=Gone!A1".to_owned())
        .expect("write single-cell reference");
    model
        .update_cell_with_formula(0, 2, 1, "=SUM(Gone!A1:B2)".to_owned())
        .expect("write range reference");
    model
        .update_cell_with_formula(0, 3, 1, "=IF(Gone!A1>0,Kept!A1,0)".to_owned())
        .expect("write nested reference");
    model
        .update_cell_with_formula(0, 4, 1, "=Kept!A1+1".to_owned())
        .expect("write unrelated reference");
    model.evaluate();
    let unrelated = model
        .get_cell_formula(0, 4, 1)
        .expect("read unrelated formula")
        .expect("unrelated formula exists");

    model.delete_sheet(2).expect("delete referenced sheet");
    model.evaluate();
    assert_eq!(
        model.get_cell_formula(0, 1, 1).unwrap().as_deref(),
        Some("=#REF!")
    );
    assert_eq!(
        model.get_cell_formula(0, 2, 1).unwrap().as_deref(),
        Some("=SUM(#REF!)")
    );
    assert_eq!(
        model.get_cell_formula(0, 3, 1).unwrap().as_deref(),
        Some("=IF(#REF!>0,Kept!A1,0)")
    );
    assert_eq!(
        model.get_cell_formula(0, 4, 1).unwrap().as_deref(),
        Some(unrelated.as_str())
    );

    model
        .insert_sheet("Gone", 2, None)
        .expect("recreate the deleted sheet name");
    model.evaluate();
    assert_eq!(
        model.get_cell_formula(0, 1, 1).unwrap().as_deref(),
        Some("=#REF!")
    );
    assert_eq!(
        model.get_cell_formula(0, 2, 1).unwrap().as_deref(),
        Some("=SUM(#REF!)")
    );
    assert_eq!(
        model.get_cell_formula(0, 3, 1).unwrap().as_deref(),
        Some("=IF(#REF!>0,Kept!A1,0)")
    );
    assert_eq!(
        model.get_cell_formula(0, 4, 1).unwrap().as_deref(),
        Some(unrelated.as_str())
    );
}

#[test]
// Verify that a full initialization queue returns the calculation snapshot to its owner.
fn initialization_queue_full_returns_the_owned_snapshot() {
    let mut document = Document::new(41, "queued", &["Sheet1"]);
    document.tables[0].set_loaded_cell(CellAddress { row: 0, column: 0 }, Cell::text("retained"));
    let snapshot = CalculationSnapshot::from_document(&document, false);
    let (sender, receiver) = mpsc::sync_channel(1);
    sender
        .try_send(Message::Stop)
        .expect("fill the bounded queue deterministically");
    let (_result_sender, results) = mpsc::sync_channel(1);
    let worker = CalculationWorker {
        sender,
        results,
        thread: None,
        cancelled: Arc::new(Mutex::new(std::collections::HashSet::new())),
    };
    let token = JobToken {
        document_id: 41,
        revision: 0,
        job_id: 1,
    };
    let (error, retained) = worker
        .initialize_retaining(token, snapshot)
        .expect_err("zero-capacity queue has no waiting receiver");
    assert_eq!(error, CalculationSubmitError::QueueFull);
    assert_eq!(retained.tables.len(), 1);
    assert_eq!(
        retained.tables[0].cells[0].1.input,
        CellInput::Text("retained".to_owned())
    );
    assert!(matches!(receiver.try_recv(), Ok(Message::Stop)));
    worker
        .initialize_retaining(token, retained)
        .expect("retry retained initial snapshot after freeing the slot");
    assert!(matches!(
        receiver.try_recv(),
        Ok(Message::Initialize { token: queued, snapshot })
            if queued == token && snapshot.tables[0].cells[0].1.input == CellInput::Text("retained".to_owned())
    ));
}

#[test]
// Skip a cancelled job before it can report a model error or publish a result.
fn queued_calculation_cancellation_skips_worker_evaluation() {
    let token = JobToken {
        document_id: 901,
        revision: 4,
        job_id: 7,
    };
    let (job_sender, job_receiver) = mpsc::sync_channel(2);
    let (result_sender, result_receiver) = mpsc::sync_channel(2);
    job_sender
        .send(Message::Update {
            token,
            changes: Vec::new(),
            markdown_source: None,
        })
        .expect("queue a calculation job before starting the worker");
    job_sender
        .send(Message::Stop)
        .expect("queue a deterministic worker stop");
    let cancelled = Arc::new(Mutex::new(std::collections::HashSet::from([token])));
    let worker_cancelled = Arc::clone(&cancelled);
    let thread =
        thread::spawn(move || worker::worker_loop(job_receiver, result_sender, worker_cancelled));
    thread.join().expect("worker exits after the queued stop");
    assert!(
        result_receiver.try_recv().is_err(),
        "a cancelled queued update never evaluates or publishes"
    );
}

#[test]
// Verify that supported formulas calculate while unsupported formula text stays intact.
fn computes_supported_formulas_and_keeps_unsupported_source_separate() {
    let mut document = Document::new(7, "calc", &["Sheet1"]);
    let table = &mut document.tables[0];
    table.set_loaded_cell(CellAddress { row: 0, column: 0 }, Cell::number(2.0));
    table.set_loaded_cell(
        CellAddress { row: 0, column: 1 },
        Cell::formula("=SUM(A1,3)", None),
    );
    table.set_loaded_cell(
        CellAddress { row: 0, column: 2 },
        Cell {
            input: CellInput::Formula("=NOW()".to_owned()),
            imported_result: Some(CalcValue::Number(99.0)),
            ..Cell::text("")
        },
    );
    let snapshot = CalculationSnapshot::from_document(&document, false);
    assert_eq!(
        document.tables[0]
            .cell(CellAddress { row: 0, column: 1 })
            .unwrap()
            .input,
        CellInput::Formula("=SUM(A1,3)".to_owned())
    );
    let worker = CalculationWorker::new();
    let token = JobToken {
        document_id: 7,
        revision: 0,
        job_id: 1,
    };
    worker.initialize(token, snapshot).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(result) = worker.poll().into_iter().next() {
            assert_eq!(result.token, token);
            assert!(result.error.is_none());
            let snapshot = result.full_snapshot.as_ref().expect("complete result root");
            assert!(snapshot.entries().contains(&(
                1,
                CellAddress { row: 0, column: 1 },
                CalcStatus::Fresh,
                Some(CalcValue::Number(5.0)),
            )));
            assert!(snapshot.entries().contains(&(
                1,
                CellAddress { row: 0, column: 2 },
                CalcStatus::Unsupported,
                Some(CalcValue::Number(99.0)),
            )));
            assert!(result.done);
            assert_eq!(snapshot.revision, token.revision);
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "calculation timed out"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
// Verify that text formulas convert ASCII numeric strings and leave other text unchanged.
fn only_ascii_numeric_text_is_converted_for_text_formulas() {
    assert_eq!(ascii_number("-1.25e2"), Some(-125.0));
    assert_eq!(ascii_number("00123"), None);
    assert_eq!(ascii_number("+5"), None);
    assert_eq!(ascii_number("12345678901234567890"), None);
    assert_eq!(ascii_number("1200"), Some(1200.0));
    assert_eq!(ascii_number("1,000"), None);
    assert_eq!(ascii_number("１２３"), None);
    assert_eq!(ascii_number("NaN"), None);
}

#[test]
// Load source text through the CSV adapter and verify calculation coercion rules.
fn csv_numeric_text_is_ignored_by_aggregates_but_kept_for_explicit_coercion() {
    let csv = crate::csv_adapter::CsvDocument::parse(
        b"id,total,count,add,value\n00123,=SUM(A2:A5),=COUNT(A2:A5),=A2+1,=VALUE(A2)\n+5,,,,\n12345678901234567890,,,,\n1200,,,,\n".to_vec(),
        crate::csv_adapter::EncodingChoice::Auto,
        ',',
    )
    .expect("CSV sample parses");
    let document = csv.to_document(91, "numeric-text.csv");
    let worker = CalculationWorker::new();
    let token = JobToken {
        document_id: document.id,
        revision: document.revision,
        job_id: 1,
    };
    worker
        .initialize(token, CalculationSnapshot::from_document(&document, false))
        .expect("loaded CSV snapshot is accepted");
    let results = wait_for_calculation(&worker, token);
    for (column, expected) in [(1, 1200.0), (2, 1.0), (3, 124.0), (4, 123.0)] {
        assert!(
            results.contains(&(
                document.tables[0].id,
                CellAddress { row: 1, column },
                CalcStatus::Fresh,
                Some(CalcValue::Number(expected)),
            )),
            "column {column}, expected {expected}: {results:?}"
        );
    }
}

#[test]
// Use Markdown's visible text for formula inputs while keeping identifiers as text.
fn markdown_inline_numeric_text_uses_its_plain_display_for_formulas() {
    let markdown = Arc::new(
        crate::markdown_adapter::MarkdownDocument::parse(
            b"| value | sum | count | add | explicit |\n| --- | --- | --- | --- | --- |\n| **20** | =SUM(A2:A5) | =COUNT(A2:A5) | =A2+1 | =VALUE(A4) |\n| [20](https://example.test) | =1+2 | =1 | =SUM(A2:A5) | =VALUE(A4) |\n| **00123** | =1+2 | =1 | =SUM(A2:A5) | =VALUE(A4) |\n| 5 | =1+2 | =1 | =SUM(A2:A5) | =VALUE(A4) |\n".to_vec(),
        )
        .expect("Markdown fixture parses"),
    );
    let mut document = markdown.to_document(92, "numeric-text.md");
    document.set_source(Some(Arc::new(crate::source::SourceDocument::Markdown(
        Arc::clone(&markdown),
    ))));
    let worker = CalculationWorker::new();
    let token = JobToken {
        document_id: document.id,
        revision: document.revision,
        job_id: 1,
    };
    worker
        .initialize(token, CalculationSnapshot::from_document(&document, false))
        .expect("loaded Markdown snapshot is accepted");
    let results = wait_for_calculation(&worker, token);
    for (row, column, expected) in [
        (1, 1, 45.0),
        (1, 2, 3.0),
        (1, 3, 21.0),
        (1, 4, 123.0),
        (2, 1, 3.0),
        (2, 2, 1.0),
        (2, 3, 45.0),
        (2, 4, 123.0),
    ] {
        assert!(results.contains(&(
            document.tables[0].id,
            CellAddress { row, column },
            CalcStatus::Fresh,
            Some(CalcValue::Number(expected)),
        )));
    }
}

#[test]
// Verify that unsupported formulas and their dependents never appear as fresh results.
fn unsupported_formula_and_dependent_formulas_are_not_evaluated_as_fresh() {
    let mut document = Document::new(18, "unsupported", &["Sheet1"]);
    let table = &mut document.tables[0];
    table.set_loaded_cell(
        CellAddress { row: 0, column: 0 },
        Cell::formula("=RAND()", None),
    );
    table.set_loaded_cell(
        CellAddress { row: 0, column: 1 },
        Cell::formula("=A1+1", None),
    );
    table.set_loaded_cell(
        CellAddress { row: 0, column: 2 },
        Cell::formula("=SUM(A1:A1)", None),
    );
    table.set_loaded_cell(
        CellAddress { row: 0, column: 3 },
        Cell::formula("=2+3", None),
    );
    let worker = CalculationWorker::new();
    let initial = JobToken {
        document_id: 18,
        revision: 0,
        job_id: 1,
    };
    worker
        .initialize(
            initial,
            CalculationSnapshot::from_document(&document, false),
        )
        .unwrap();
    let updates = wait_for_calculation(&worker, initial);
    for column in 0..3 {
        assert!(updates.contains(&(
            1,
            CellAddress { row: 0, column },
            CalcStatus::Unsupported,
            None,
        )));
    }
    assert!(updates.contains(&(
        1,
        CellAddress { row: 0, column: 3 },
        CalcStatus::Fresh,
        Some(CalcValue::Number(5.0)),
    )));

    let replacement = CalculationChange {
        table_id: 1,
        address: CellAddress { row: 0, column: 0 },
        cell: Some(Cell::number(4.0)),
        string_cell: false,
    };
    let token = JobToken {
        document_id: 18,
        revision: 1,
        job_id: 2,
    };
    worker.update(token, vec![replacement]).unwrap();
    let updates = wait_for_calculation(&worker, token);
    assert!(updates.contains(&(
        1,
        CellAddress { row: 0, column: 1 },
        CalcStatus::Fresh,
        Some(CalcValue::Number(5.0)),
    )));
    assert!(updates.contains(&(
        1,
        CellAddress { row: 0, column: 2 },
        CalcStatus::Fresh,
        Some(CalcValue::Number(4.0)),
    )));
}

#[test]
// Verify that failed calculation batches expose no latest results and report retry keys.
fn failed_change_batch_returns_no_latest_results_and_identifies_retry_keys() {
    let mut document = Document::new(21, "bad update", &["Sheet1"]);
    document.tables[0].set_loaded_cell(
        CellAddress { row: 0, column: 0 },
        Cell::formula("=1+1", None),
    );
    let worker = CalculationWorker::new();
    let initial = JobToken {
        document_id: 21,
        revision: 0,
        job_id: 1,
    };
    worker
        .initialize(
            initial,
            CalculationSnapshot::from_document(&document, false),
        )
        .unwrap();
    let _ = wait_for_calculation(&worker, initial);
    let address = CellAddress {
        row: usize::MAX,
        column: 0,
    };
    let token = JobToken {
        document_id: 21,
        revision: 1,
        job_id: 2,
    };
    worker
        .update(
            token,
            vec![CalculationChange {
                table_id: 1,
                address,
                cell: Some(Cell::formula("=3+3", None)),
                string_cell: false,
            }],
        )
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(result) = worker.poll().into_iter().next() {
            assert_eq!(result.token, token);
            assert!(result.done);
            assert!(result.error.is_some());
            assert_eq!(result.failed_changes, [(1, address)]);
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "calculation timed out"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
// Keep Markdown presentation separate from typed calculation input.
fn markdown_calculation_uses_plain_values_and_only_unescapes_table_pipes() {
    let bytes = b"| Value | Count | Pipe | Stars | Link | Styled formula | Literal reference | Styled apostrophe | Apostrophe reference |\n| --- | --- | --- | --- | --- | --- | --- | --- | --- |\n| **5** | =COUNT(A2) | =\"a\\|b\" | =\"a*b*c\" | =\"[x](u)\" | **=1+1** | =F2 | **''=1+1** | =H2 |\n";
    let source = Arc::new(
        crate::markdown_adapter::MarkdownDocument::parse(bytes.to_vec())
            .expect("Markdown source parses"),
    );
    let mut document = source.to_document(25, "Imported");
    document.set_source(Some(Arc::new(crate::source::SourceDocument::Markdown(
        Arc::clone(&source),
    ))));
    assert_eq!(
        source
            .cell_display_text_for_search(
                0,
                1,
                5,
                &source.tables[0].rows[1][5].value,
                crate::i18n::Language::English,
            )
            .expect("styled literal has a display value"),
        "=1+1"
    );
    assert_eq!(
        source
            .cell_display_text_for_search(
                0,
                1,
                7,
                &source.tables[0].rows[1][7].value,
                crate::i18n::Language::English,
            )
            .expect("styled apostrophe literal has a display value"),
        "'=1+1"
    );
    let snapshot = CalculationSnapshot::from_document(&document, false);
    let cells = &snapshot.tables[0].cells;
    let input_at = |row, column| {
        &cells
            .iter()
            .find(|(address, _)| *address == CellAddress { row, column })
            .expect("calculation cell exists")
            .1
            .input
    };
    assert_eq!(input_at(1, 0), &CellInput::Text("5".to_owned()));
    assert_eq!(input_at(1, 2), &CellInput::Text("=\"a|b\"".to_owned()));
    assert_eq!(input_at(1, 3), &CellInput::Text("=\"a*b*c\"".to_owned()));
    assert_eq!(input_at(1, 4), &CellInput::Text("=\"[x](u)\"".to_owned()));
    assert_eq!(input_at(1, 5), &CellInput::Text("'=1+1".to_owned()));
    assert_eq!(input_at(1, 6), &CellInput::Text("=F2".to_owned()));
    assert_eq!(input_at(1, 7), &CellInput::Text("''=1+1".to_owned()));
    assert_eq!(input_at(1, 8), &CellInput::Text("=H2".to_owned()));
    assert_eq!(
        crate::document::literal_text_value(input_at(1, 5).source_text().as_str()),
        "=1+1"
    );

    let worker = CalculationWorker::new();
    let token = JobToken {
        document_id: document.id,
        revision: document.calculation_revision,
        job_id: 1,
    };
    worker
        .initialize(token, snapshot)
        .expect("enqueue Markdown calculation");
    let results = wait_for_calculation(&worker, token);
    assert!(results.contains(&(
        1,
        CellAddress { row: 1, column: 1 },
        CalcStatus::Fresh,
        Some(CalcValue::Number(1.0)),
    )));
    assert!(results.contains(&(
        1,
        CellAddress { row: 1, column: 2 },
        CalcStatus::Fresh,
        Some(CalcValue::Text("a|b".to_owned())),
    )));
    assert!(results.contains(&(
        1,
        CellAddress { row: 1, column: 3 },
        CalcStatus::Fresh,
        Some(CalcValue::Text("a*b*c".to_owned())),
    )));
    assert!(results.contains(&(
        1,
        CellAddress { row: 1, column: 4 },
        CalcStatus::Fresh,
        Some(CalcValue::Text("[x](u)".to_owned())),
    )));
    assert!(results.contains(&(
        1,
        CellAddress { row: 1, column: 6 },
        CalcStatus::Fresh,
        Some(CalcValue::Text("=1+1".to_owned())),
    )));
    assert!(results.contains(&(
        1,
        CellAddress { row: 1, column: 8 },
        CalcStatus::Fresh,
        Some(CalcValue::Text("'=1+1".to_owned())),
    )));
}

#[test]
// Projects a changed Markdown batch before the calculation model applies it.
fn markdown_worker_update_projects_changed_cells_from_one_source_batch() {
    let source = Arc::new(
        crate::markdown_adapter::MarkdownDocument::parse(
            b"| Value | Count |\n| --- | --- |\n| 1 | =COUNT(A2) |\n".to_vec(),
        )
        .expect("Markdown source parses"),
    );
    let mut document = source.to_document(26, "Edited");
    document.set_source(Some(Arc::new(crate::source::SourceDocument::Markdown(
        Arc::clone(&source),
    ))));
    let table_id = document.tables[0].id;
    let formula = CellAddress { row: 1, column: 1 };
    let input = CellAddress { row: 1, column: 0 };
    let worker = CalculationWorker::new();
    let first_token = JobToken {
        document_id: document.id,
        revision: document.calculation_revision,
        job_id: 1,
    };
    worker
        .initialize(
            first_token,
            CalculationSnapshot::from_document(&document, false),
        )
        .expect("initialize the calculation model");
    let initial = wait_for_calculation(&worker, first_token);
    assert!(initial.contains(&(
        table_id,
        formula,
        CalcStatus::Fresh,
        Some(CalcValue::Number(1.0)),
    )));

    document
        .set_cell(table_id, input, Some(Cell::text("**5**")))
        .expect("edit the source cell with Markdown formatting");
    let changes = document
        .calculation_cell_snapshot()
        .into_iter()
        .map(|(table_id, address, cell)| CalculationChange {
            table_id,
            address,
            cell,
            string_cell: false,
        })
        .collect();
    let context = MarkdownCalculationSource::from_document(&document)
        .expect("the document provides Markdown projection context");
    let update_token = JobToken {
        document_id: document.id,
        revision: document.calculation_revision,
        job_id: 2,
    };
    worker
        .update_markdown(update_token, changes, context)
        .expect("enqueue the raw edit batch for worker projection");
    let updated = wait_for_calculation(&worker, update_token);
    assert!(updated.contains(&(
        table_id,
        formula,
        CalcStatus::Fresh,
        Some(CalcValue::Number(1.0)),
    )));
}
