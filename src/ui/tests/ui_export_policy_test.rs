use super::*;
use egui_kittest::Harness;

#[test]
// Normalizes a CSV export to the active table before the settings dialog is drawn.
fn csv_export_dialog_requires_one_existing_table() {
    let directory = TestDirectory::new();
    let settings_path = directory.0.join("settings.json");
    let mut harness = Harness::builder().build_eframe(move |creation| {
        TexcelApp::new_with_settings_path(creation, Some(settings_path))
    });
    let (document_id, table_id) = {
        let document = harness.state().workspace.active_document().unwrap();
        (document.id, document.active_table().unwrap().id)
    };
    harness.state_mut().export_dialog = Some(ExportDialogState {
        document_id,
        options: ExportOptions {
            format: ExportFormat::Csv,
            scope: ExportScope::AllTables,
            formulas: FormulaOutput::Formula,
            modifiers: ModifierOutput::Convert,
            markdown_header: MarkdownHeader::UseFirstRow,
        },
        save_intent: None,
    });

    harness.step();

    assert_eq!(
        harness
            .state()
            .export_dialog
            .as_ref()
            .expect("export settings stay open until dismissed")
            .options
            .scope,
        ExportScope::Selected(table_id)
    );
}
