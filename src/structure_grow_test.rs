use super::*;
use std::sync::atomic::AtomicBool;

// Build one Markdown worker request for a sparse cell edit.
fn markdown_growth_request(address: CellAddress, input: CellInput) -> (Document, StructureRequest) {
    markdown_growth_request_from_bytes(
        b"# Notes\n\n| key | value |\n| --- | --- |\n| alpha | 1 |\n".to_vec(),
        address,
        input,
    )
}

// Build one growth request from the current document root.
fn growth_request(document: &Document, address: CellAddress, input: CellInput) -> StructureRequest {
    let source = document.source.as_ref().expect("Markdown source exists");
    let table = &document.tables[0];
    let view = document.view(table.id).expect("table view exists");
    StructureRequest {
        document_id: document.id,
        revision: document.revision,
        calculation_revision: document.calculation_revision,
        source: Some(Arc::clone(source)),
        operation: Some(StructureOperation::SetTextCell {
            table_id: table.id,
            address,
            cell: Box::new(cell(input)),
        }),
        tables: vec![StructureTableMeta {
            id: table.id,
            name: table.name.clone(),
            rows: table.row_count(),
            columns: table.column_count(),
            string_cells: table.string_cells_shared(),
            ..StructureTableMeta::default()
        }],
        changes: Vec::new(),
        text_overlay_cells: document.text_overlay_cells(),
        text_overlay_revision: document.text_overlay_snapshot().revision,
        view_table_id: table.id,
        view_layout: Arc::clone(&view.layout),
        selection: CellRange::single(address),
        scroll: ScrollPosition::default(),
        cross_sheet_references: false,
        clipboard_paste: None,
        prepared_selection_after: None,
    }
}

// Creates a plain cell payload for a worker request.
fn cell(input: CellInput) -> crate::document::Cell {
    crate::document::Cell {
        input,
        imported_result: None,
        calculation: None,
    }
}

// Parse a Markdown source and build its first growth request.
fn markdown_growth_request_from_bytes(
    bytes: Vec<u8>,
    address: CellAddress,
    input: CellInput,
) -> (Document, StructureRequest) {
    let markdown =
        crate::markdown_adapter::MarkdownDocument::parse(bytes).expect("Markdown fixture parses");
    let source = Arc::new(SourceDocument::Markdown(Arc::new(markdown.clone())));
    let mut document = markdown.to_document(301, "notes.md");
    document.set_source(Some(Arc::clone(&source)));
    let request = growth_request(&document, address, input);
    (document, request)
}

#[test]
// Verify a sparse edit commits with source, cell, calculation, and Undo at one revision.
fn markdown_sparse_cell_edit_is_atomic_and_undoable() {
    let original = b"# Notes\n\n| key | value |\n| --- | --- |\n| alpha | 1 |\n";
    let address = CellAddress { row: 2, column: 0 };
    let (mut document, request) =
        markdown_growth_request(address, CellInput::Text("beta".to_owned()));
    let prepared = prepare_structure(&request, &AtomicBool::new(false))
        .expect("the sparse cell edit is prepared on the worker");
    document
        .execute_prepared_structure(prepared)
        .expect("the root applies as one history command");
    let Some(SourceDocument::Markdown(source)) = document.source.as_deref() else {
        panic!("the document keeps Markdown as its editable source");
    };
    assert_eq!(
        source.source_bytes(),
        [original.as_slice(), b"| beta |  |\n"].concat()
    );
    assert_eq!(
        document.tables[0].cell(address).map(|cell| &cell.input),
        Some(&CellInput::Text("beta".to_owned()))
    );
    document.undo().expect("one Undo restores the old root");
    let Some(SourceDocument::Markdown(source)) = document.source.as_deref() else {
        panic!("Undo restores the Markdown source type");
    };
    assert_eq!(source.source_bytes(), original);
    assert!(document.tables[0].cell(address).is_none());
}

#[test]
// Verify a source structure change keeps the separator with a header-only table.
fn column_structure_on_header_only_table_keeps_one_separator() {
    let (mut document, mut request) = markdown_growth_request_from_bytes(
        b"| A |\n| --- |\n".to_vec(),
        CellAddress { row: 0, column: 0 },
        CellInput::Text("unused".to_owned()),
    );
    let table_id = document.tables[0].id;
    request.operation = Some(StructureOperation::Insert {
        table_id,
        axis: StructureAxis::Columns,
        index: 0,
        count: 1,
    });
    request.changes.clear();

    let prepared = prepare_structure(&request, &AtomicBool::new(false))
        .expect("the inserted source column is prepared for the minimal table");
    document
        .execute_prepared_structure(prepared)
        .expect("the source and column structure apply together");
    let Some(SourceDocument::Markdown(source)) = document.source.as_deref() else {
        panic!("the document keeps its Markdown source");
    };
    let reopened = crate::markdown_adapter::MarkdownDocument::parse(source.source_bytes().to_vec())
        .expect("the structure result reopens");
    assert_eq!(reopened.tables.len(), 1);
    assert_eq!(reopened.tables[0].rows.len(), 1);
    assert_eq!(reopened.parser_width(0), Some(2));
    assert_eq!(
        source.source_bytes().split(|byte| *byte == b'\n').count(),
        3
    );
}

#[test]
// Verify spilled sparse-edit history restores its original source and table root.
fn spilled_markdown_sparse_edit_undo_restores_the_original_root() {
    let long_cell = "v".repeat(3 * 1024 * 1024);
    let original = format!("# Notes\n\n| value |\n| --- |\n| {long_cell} |\n").into_bytes();
    let address = CellAddress { row: 2, column: 0 };
    let (mut document, request) = markdown_growth_request_from_bytes(
        original.clone(),
        address,
        CellInput::Text("added".to_owned()),
    );
    let prepared = prepare_structure(&request, &AtomicBool::new(false))
        .expect("large Markdown growth prepares");
    assert!(prepared.history_file.is_some(), "history spills to disk");
    document
        .execute_prepared_structure(prepared)
        .expect("the growth is one document command");

    let undo_request = document
        .structure_history_restore_request(false)
        .expect("the undo request uses spilled history");
    let restored =
        prepare_structure_restore(&undo_request).expect("the worker reads the before root");
    document
        .apply_prepared_structure_restore(restored)
        .expect("Undo restores the original Markdown root");
    let Some(SourceDocument::Markdown(source)) = document.source.as_deref() else {
        panic!("Undo restores Markdown source");
    };
    assert_eq!(source.source_bytes(), original);
    assert_eq!(source.tables[0].rows.len(), 2);
    assert_eq!(
        document.extent_end(document.tables[0].id),
        Some(CellAddress { row: 1, column: 0 })
    );

    assert!(document.tables[0].cell(address).is_none());
}

#[test]
// Checks a valid corner edit and rejects an endpoint beyond the Markdown read limit.
fn markdown_extends_to_a_readable_corner_and_rejects_an_oversized_rectangle() {
    let corner = CellAddress { row: 2, column: 2 };
    let (_, request) = markdown_growth_request(corner, CellInput::Text("corner".to_owned()));
    let prepared = prepare_structure(&request, &AtomicBool::new(false))
        .expect("the sparse table corner is accepted by the worker");
    let Some(SourceDocument::Markdown(source)) = prepared.source_after.as_deref() else {
        panic!("the prepared root contains Markdown source");
    };
    assert_eq!(source.tables[0].rows.len(), 3);
    assert_eq!(source.tables[0].rows[2][2].value, "corner");
    assert_eq!(source.tables[0].rows[2][0].value, "");
    assert!(prepared.tables[0].cell(corner).is_some());

    let distant = CellAddress {
        row: crate::document::MAX_ROWS - 1,
        column: crate::document::MAX_COLUMNS - 1,
    };
    let (_, request) = markdown_growth_request(distant, CellInput::Text("endpoint".to_owned()));
    let failure = prepare_structure(&request, &AtomicBool::new(false))
        .expect_err("the edit exceeds the reader's limits");
    assert_eq!(failure.key(), "status-markdown-resource-limit");
}

#[test]
// Verifies that a new field keeps the first row, active delimiter and CRLF intact.
fn csv_sparse_cell_edit_preserves_untouched_fields_and_line_endings() {
    let original = b"alpha;1\r\nbeta\r\n";
    let csv = crate::csv_adapter::CsvDocument::parse(
        original.to_vec(),
        crate::csv_adapter::EncodingChoice::Auto,
        ';',
    )
    .expect("CSV fixture parses");
    let source = Arc::new(SourceDocument::Csv(Arc::new(csv.clone())));
    let mut document = csv.to_document(302, "values.csv");
    document.set_source(Some(Arc::clone(&source)));
    let table = &document.tables[0];
    let view = document.view(table.id).expect("table view exists");
    let address = CellAddress { row: 1, column: 2 };
    let request = StructureRequest {
        document_id: document.id,
        revision: document.revision,
        calculation_revision: document.calculation_revision,
        source: Some(source),
        operation: Some(StructureOperation::SetTextCell {
            table_id: table.id,
            address,
            cell: Box::new(cell(CellInput::Text("added".to_owned()))),
        }),
        tables: vec![StructureTableMeta {
            id: table.id,
            name: table.name.clone(),
            rows: table.row_count(),
            columns: table.column_count(),
            string_cells: table.string_cells_shared(),
            ..StructureTableMeta::default()
        }],
        changes: Vec::new(),
        text_overlay_cells: document.text_overlay_cells(),
        text_overlay_revision: document.text_overlay_snapshot().revision,
        view_table_id: table.id,
        view_layout: Arc::clone(&view.layout),
        selection: CellRange::single(address),
        scroll: ScrollPosition::default(),
        cross_sheet_references: false,
        clipboard_paste: None,
        prepared_selection_after: None,
    };
    let prepared = prepare_structure(&request, &AtomicBool::new(false))
        .expect("a sparse CSV field edit is prepared");
    let Some(SourceDocument::Csv(updated)) = prepared.source_after.as_deref() else {
        panic!("the CSV document keeps its source type");
    };
    assert_eq!(updated.source_bytes(), b"alpha;1\r\nbeta;;added\r\n");
    assert_eq!(updated.records[0].fields.len(), 2);
    assert_eq!(updated.records[1].fields.len(), 3);
    assert_eq!(updated.records[1].fields[2].value, "added");
    assert!(prepared.tables[0].cell(address).is_some());
    document
        .execute_prepared_structure(prepared)
        .expect("commit the CSV edit");
    document.undo().expect("one Undo restores the ragged CSV");
    let Some(SourceDocument::Csv(restored)) = document.source.as_deref() else {
        panic!("Undo retains the CSV source");
    };
    assert_eq!(restored.source_bytes(), original);
}
