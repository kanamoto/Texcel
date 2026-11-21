use super::*;
use crate::i18n::ResourceLimit;

/// Checks exact Markdown escape growth and keeps the tracked budget aligned through undo.
#[test]
fn markdown_cell_budget_tracks_encoded_bytes_and_undo() {
    let bytes = b"| value |\n| --- |\n| old |\n".to_vec();
    let source =
        crate::markdown_adapter::MarkdownDocument::parse(bytes).expect("Markdown source parses");
    let mut document = source.to_document(1, "table.md");
    document.set_source(Some(Arc::new(SourceDocument::Markdown(Arc::new(source)))));
    let address = CellAddress { row: 1, column: 0 };
    let edited = Cell::text("new|value\nnext");

    assert_eq!(
        document.check_text_cell_resource_limit(1, address, Some(&edited)),
        TextCellResourceCheck::WithinLimit
    );
    document
        .set_cell(1, address, Some(edited))
        .expect("the in-limit edit is recorded");
    let expected = match document.source.as_deref().unwrap() {
        SourceDocument::Markdown(source) => source
            .apply_changes(&[crate::markdown_adapter::MarkdownCellChange {
                table: 0,
                row: address.row,
                column: address.column,
                value: "new|value\nnext".to_owned(),
            }])
            .expect("the writer accepts the same edit"),
        _ => unreachable!(),
    };
    let base_len = match document.source.as_deref().unwrap() {
        SourceDocument::Markdown(source) => source.source_bytes().len(),
        _ => unreachable!(),
    };
    assert_eq!(
        document.source_byte_delta_total,
        expected.source_bytes().len() as i128 - base_len as i128
    );

    document.undo().expect("the edit can be undone");
    assert_eq!(document.source_byte_delta_total, 0);
    assert_eq!(document.source_byte_delta_unknown, 0);
}

/// Checks CP932 field sizing without re-encoding the whole CSV document.
#[test]
fn csv_cell_budget_uses_encoded_field_and_quote_lengths() {
    let source = crate::csv_adapter::CsvDocument::parse(
        "a,\"old\"\r\n".as_bytes().to_vec(),
        crate::csv_adapter::EncodingChoice::ShiftJis,
        ',',
    )
    .expect("CSV source parses");
    let mut document = source.to_document(2, "table.csv");
    document.set_source(Some(Arc::new(SourceDocument::Csv(Arc::new(source)))));
    let address = CellAddress { row: 0, column: 1 };
    let edited = Cell::text("日本,語\"");
    assert_eq!(
        document.check_text_cell_resource_limit(1, address, Some(&edited)),
        TextCellResourceCheck::WithinLimit
    );
    document
        .set_cell(1, address, Some(edited))
        .expect("the in-limit edit is recorded");

    let (expected, original_len) = match document.source.as_deref().unwrap() {
        SourceDocument::Csv(source) => (
            source
                .apply_changes(&[crate::csv_adapter::CsvCellChange {
                    row: 0,
                    column: 1,
                    value: Some("日本,語\"".to_owned()),
                }])
                .expect("the CP932 writer accepts the same edit"),
            source.source_bytes().len(),
        ),
        _ => unreachable!(),
    };
    assert_eq!(
        document.source_byte_delta_total,
        expected.source_bytes().len() as i128 - original_len as i128
    );
}

/// Routes a missing ragged field to worker validation instead of the ordinary fast path.
#[test]
fn missing_markdown_field_requires_worker_validation() {
    let source = crate::markdown_adapter::MarkdownDocument::parse(
        b"| a | b |\n| --- | --- |\n| x |\n".to_vec(),
    )
    .expect("ragged Markdown source parses");
    let mut document = source.to_document(3, "ragged.md");
    document.set_source(Some(Arc::new(SourceDocument::Markdown(Arc::new(source)))));
    let address = CellAddress { row: 1, column: 1 };
    assert_eq!(
        document.check_text_cell_resource_limit(1, address, Some(&Cell::text("fill"))),
        TextCellResourceCheck::NeedsWorker
    );
}

/// Checks exact-limit warnings and rejects an overflowing result batch atomically.
#[test]
fn formula_result_command_rejects_source_overflow_atomically() {
    let base = b"| value |\n| --- |\n| x |\n";
    let prefix = b"<!--";
    let suffix = b"-->";
    let limit = crate::markdown_adapter::MAX_MARKDOWN_BYTES;
    let mut bytes = Vec::with_capacity(limit - 1);
    bytes.extend_from_slice(base);
    bytes.extend_from_slice(prefix);
    bytes.resize(limit - 1 - suffix.len(), b'x');
    bytes.extend_from_slice(suffix);
    let source = crate::markdown_adapter::MarkdownDocument::parse(bytes)
        .expect("the source fits just below the byte limit");
    let mut document = source.to_document(4, "formula.md");
    document.set_source(Some(Arc::new(SourceDocument::Markdown(Arc::new(source)))));
    let address = CellAddress { row: 1, column: 0 };
    let before = document.tables[0]
        .cell(address)
        .expect("the source cell is loaded")
        .clone();
    let after = Cell::text("xx");
    let revision = document.revision;
    let state_id = document.history_state_id();
    let source_bytes = match document.source.as_deref().unwrap() {
        SourceDocument::Markdown(source) => source.source_bytes().len(),
        _ => unreachable!(),
    };

    document
        .execute(EditCommand::SetFormulaResults {
            table_id: 1,
            changes: vec![CellChange {
                address,
                before: Some(before.clone()),
                after: Some(after.clone()),
            }],
            string_overrides: Vec::new(),
        })
        .expect("the result exactly at the limit is committed");
    assert_eq!(
        document.take_resource_limit_notice(),
        Some(ResourceLimit::FileSize)
    );
    let revision_at_limit = document.revision;
    let state_at_limit = document.history_state_id();
    let result = document.execute(EditCommand::SetFormulaResults {
        table_id: 1,
        changes: vec![CellChange {
            address,
            before: Some(after.clone()),
            after: Some(Cell::text("xxx")),
        }],
        string_overrides: Vec::new(),
    });

    assert_eq!(
        result,
        Err(DocumentError::ResourceLimit(ResourceLimit::FileSize))
    );
    assert_eq!(document.revision, revision_at_limit);
    assert_eq!(document.history_state_id(), state_at_limit);
    assert_eq!(document.tables[0].cell(address), Some(&after));
    assert!(document.revision > revision);
    assert_ne!(document.history_state_id(), state_id);
    assert_eq!(
        match document.source.as_deref().unwrap() {
            SourceDocument::Markdown(source) => source.source_bytes().len(),
            _ => unreachable!(),
        },
        source_bytes
    );
}

/// Checks that deleting a one-character CSV field cannot grow beyond the byte limit.
#[test]
fn clearing_a_single_field_at_the_csv_byte_limit_is_rejected_without_undo() {
    let mut bytes = Vec::with_capacity(crate::csv_adapter::MAX_CSV_SOURCE_BYTES);
    bytes.extend_from_slice(b"x\n");
    bytes.resize(crate::csv_adapter::MAX_CSV_SOURCE_BYTES, b'a');
    let source = crate::csv_adapter::CsvDocument::parse(
        bytes,
        crate::csv_adapter::EncodingChoice::Utf8,
        ',',
    )
    .expect("the CSV source fits exactly at the byte limit");
    let mut document = source.to_document(5, "delete.csv");
    document.set_source(Some(Arc::new(SourceDocument::Csv(Arc::new(source)))));
    let address = CellAddress::default();
    let before = document.tables[0]
        .cell(address)
        .expect("the first CSV field is loaded")
        .clone();
    let revision = document.revision;
    let state_id = document.history_state_id();

    let result = document.clear_range(
        1,
        CellRange {
            anchor: address,
            head: address,
        },
    );

    assert_eq!(
        result,
        Err(DocumentError::ResourceLimit(ResourceLimit::FileSize))
    );
    assert_eq!(document.revision, revision);
    assert_eq!(document.history_state_id(), state_id);
    assert_eq!(document.tables[0].cell(address), Some(&before));
    assert_eq!(
        match document.source.as_deref().unwrap() {
            SourceDocument::Csv(source) => source.source_bytes().len(),
            _ => unreachable!(),
        },
        crate::csv_adapter::MAX_CSV_SOURCE_BYTES
    );
}
