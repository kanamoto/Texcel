use super::*;
use egui_kittest::Harness;
use std::time::{Duration, Instant};

#[test]
// Uses the Bold shortcut to promote and format a CSV cell.
fn csv_bold_shortcut_promotes_the_selected_text() {
    let mut harness = Harness::builder()
        .with_size([1000.0, 680.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let document_id = harness.state().workspace.active_document.unwrap();
    let csv = crate::csv_adapter::CsvDocument::parse(
        b"literal **word**\n".to_vec(),
        crate::csv_adapter::EncodingChoice::Auto,
        ',',
    )
    .expect("the source CSV parses");
    let mut document = csv.to_document(document_id, "CSV formatting");
    document.set_source(Some(Arc::new(SourceDocument::Csv(Arc::new(csv)))));
    let table_id = document.active_table;
    let calculation = crate::calc::CalculationSnapshot::from_document(&document, false);
    {
        let app = harness.state_mut();
        app.clear_calculation_jobs_for_reset(document_id);
        app.workspace.documents.insert(document_id, document);
        let revision = app.workspace.documents[&document_id].calculation_revision;
        let token = app.next_job_token(document_id, revision);
        app.initialize_calculation(token, calculation);
    }
    let address = CellAddress::default();
    let ctx = harness.ctx.clone();
    {
        let app = harness.state_mut();
        app.begin_cell_edit(table_id, address);
        app.edit.buffer = "literal **word**".to_owned();
        app.text_selection = Some(TextSelection {
            document_id,
            table_id,
            address,
            session_id: app.edit.session_id,
            buffer_revision: app.edit.buffer_revision,
            start: 10,
            end: 14,
        });
        app.language = Language::English;
        crate::i18n::set_context_language(&ctx, Language::English);
    }
    harness.step();

    let mut modifiers = egui::Modifiers::COMMAND;
    if harness.ctx.os().is_mac() {
        modifiers.mac_cmd = true;
    } else {
        modifiers.ctrl = true;
    }
    harness.event_modifiers(
        egui::Event::Key {
            key: egui::Key::B,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        },
        modifiers,
    );
    harness.step();
    // Wait for the promoted document even if its worker finished in the shortcut frame.
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(
        harness.state().workspace.documents[&document_id]
            .source
            .as_deref(),
        Some(SourceDocument::Markdown(_))
    ) {
        harness.step();
        assert!(Instant::now() < deadline, "promotion worker did not settle");
        std::thread::sleep(Duration::from_millis(3));
    }
    harness.step();
    let document = &harness.state().workspace.documents[&document_id];
    assert!(matches!(
        document.source.as_deref(),
        Some(SourceDocument::Markdown(_))
    ));
    assert!(
        document.tables[0]
            .cell(CellAddress { row: 0, column: 0 })
            .is_some_and(|cell| cell.input.source_text().contains("\\*\\*"))
    );
}
