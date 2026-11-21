use super::*;
use crate::ui::tests::new_app;
use std::time::{Duration, Instant};

/// Limits a select-all alignment to the source table's separator columns.
#[test]
fn markdown_alignment_intersects_select_all_with_source_columns() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let document_id = app.workspace.active_document.unwrap();
    let table_id = app.workspace.documents[&document_id].active_table;
    let source = crate::markdown_adapter::MarkdownDocument::parse(
        b"| Name | Amount |\n| --- | --- |\n| Tea | 123 |\n".to_vec(),
    )
    .expect("the two-column Markdown table parses");
    let source = SourceDocument::Markdown(Arc::new(source));
    let document = crate::structure::source_to_document(&source, document_id, &[table_id])
        .expect("the parsed table adopts its stable ID");
    app.workspace.documents.insert(document_id, document);
    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .view_mut(table_id)
        .unwrap()
        .selection = CellRange {
        anchor: CellAddress::default(),
        head: CellAddress {
            row: MAX_ROWS - 1,
            column: MAX_COLUMNS - 1,
        },
    };

    app.request_table_management(StructureAction::MarkdownAlignCenter, None, None);
    assert!(app.pending_structures.contains_key(&document_id));
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.pending_structures.contains_key(&document_id) {
        app.process_file_results();
        assert!(Instant::now() < deadline, "alignment preparation completes");
        std::thread::sleep(Duration::from_millis(3));
    }
    let Some(SourceDocument::Markdown(markdown)) =
        app.workspace.documents[&document_id].source.as_deref()
    else {
        panic!("the Markdown source remains editable");
    };
    assert_eq!(markdown.parser_width(0), Some(2));
    assert_eq!(
        markdown.column_alignment(0, 0),
        Some(crate::markdown_adapter::MarkdownTableAlignment::Center)
    );
    assert_eq!(
        markdown.column_alignment(0, 1),
        Some(crate::markdown_adapter::MarkdownTableAlignment::Center)
    );
    let bytes_before_temporary = markdown.source_bytes().to_vec();

    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .view_mut(table_id)
        .unwrap()
        .selection = CellRange::single(CellAddress { row: 0, column: 2 });
    app.request_table_management(StructureAction::MarkdownAlignLeft, None, None);
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.pending_structures.contains_key(&document_id) {
        app.process_file_results();
        assert!(
            Instant::now() < deadline,
            "temporary alignment preparation completes"
        );
        std::thread::sleep(Duration::from_millis(3));
    }
    let Some(SourceDocument::Markdown(markdown)) =
        app.workspace.documents[&document_id].source.as_deref()
    else {
        panic!("temporary alignment keeps the Markdown source");
    };
    assert_eq!(markdown.parser_width(0), Some(2));
    assert_eq!(markdown.source_bytes(), bytes_before_temporary);
    assert_eq!(
        markdown.column_alignment(0, 2),
        Some(crate::markdown_adapter::MarkdownTableAlignment::Left)
    );
    assert_eq!(
        app.status.as_ref().map(Message::key),
        Some("status-markdown-column-alignment-temporary")
    );
}

/// Uses the promoted CSV width when a selection includes virtual growth columns.
#[test]
fn csv_alignment_uses_promoted_markdown_width() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let document_id = app.workspace.active_document.unwrap();
    let csv = crate::csv_adapter::CsvDocument::parse(
        b"Name,Amount\nTea,123\n".to_vec(),
        crate::csv_adapter::EncodingChoice::Auto,
        ',',
    )
    .expect("the two-column CSV parses");
    let mut document = csv.to_document(document_id, "CSV");
    document.set_source(Some(Arc::new(SourceDocument::Csv(Arc::new(csv)))));
    let table_id = document.active_table;
    app.workspace.documents.insert(document_id, document);
    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .view_mut(table_id)
        .unwrap()
        .selection = CellRange {
        anchor: CellAddress::default(),
        head: CellAddress {
            row: MAX_ROWS - 1,
            column: MAX_COLUMNS - 1,
        },
    };

    app.request_table_management(StructureAction::MarkdownAlignRight, None, None);
    assert!(app.pending_structures.contains_key(&document_id));
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.pending_structures.contains_key(&document_id) {
        app.process_file_results();
        assert!(
            Instant::now() < deadline,
            "CSV promotion and alignment complete"
        );
        std::thread::sleep(Duration::from_millis(3));
    }
    let Some(SourceDocument::Markdown(markdown)) =
        app.workspace.documents[&document_id].source.as_deref()
    else {
        panic!("CSV alignment promotes the current source to Markdown");
    };
    assert_eq!(markdown.parser_width(0), Some(2));
    assert_eq!(
        markdown.column_alignment(0, 0),
        Some(crate::markdown_adapter::MarkdownTableAlignment::Right)
    );
    assert_eq!(
        markdown.column_alignment(0, 1),
        Some(crate::markdown_adapter::MarkdownTableAlignment::Right)
    );
}
