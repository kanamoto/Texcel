use super::*;
use egui_kittest::Harness;
use std::fs;
use std::time::{Duration, Instant};

// Sends Enter through the normal app frame.
fn press_enter(harness: &mut Harness<'_, TexcelApp>) {
    harness.event(egui::Event::Key {
        key: Key::Enter,
        physical_key: Some(Key::Enter),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.step();
}

// Waits for the worker result to be adopted by a normal UI frame.
fn wait_for_preview(harness: &mut Harness<'_, TexcelApp>, ready: impl Fn(&TexcelApp) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !ready(harness.state()) && Instant::now() < deadline {
        harness.step();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        ready(harness.state()),
        "preview worker did not finish in time; status={:?}, search={:?}, data={:?}",
        harness.state().status,
        harness.state().pending_searches,
        harness.state().pending_data_edits
    );
}

// Creates one document with a selected A1 cell for dialog operations.
fn dialog_harness(value: &str) -> (Harness<'static, TexcelApp>, DocumentId, TableId) {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let (document_id, table_id) = {
        let document = harness.state().workspace.active_document().unwrap();
        (document.id, document.active_table().unwrap().id)
    };
    let document = harness
        .state_mut()
        .workspace
        .documents
        .get_mut(&document_id)
        .unwrap();
    document
        .set_cell(table_id, CellAddress::default(), Some(Cell::text(value)))
        .expect("A1 exists in the starter document");
    document.view_mut(table_id).unwrap().selection = CellRange::single(CellAddress::default());
    (harness, document_id, table_id)
}

// Loads a real Markdown table with room for a split preview destination.
fn split_dialog_harness() -> (Harness<'static, TexcelApp>, DocumentId, TableId) {
    let directory = crate::ui::tests::TestDirectory::new();
    let path = directory.0.join("split-preview.md");
    fs::write(
        &path,
        "| value | part 1 | part 2 |\n| --- | --- | --- |\n| alpha,beta |  |  |\n",
    )
    .expect("write split preview source");
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let document_id = harness
        .state_mut()
        .start_path_load(path)
        .expect("submit Markdown fixture load");
    let deadline = Instant::now() + Duration::from_secs(3);
    while !harness.state().file_backings.contains_key(&document_id) && Instant::now() < deadline {
        harness.step();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(harness.state().file_backings.contains_key(&document_id));
    let table_id = harness.state().workspace.documents[&document_id]
        .active_table()
        .unwrap()
        .id;
    harness
        .state_mut()
        .workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .view_mut(table_id)
        .unwrap()
        .set_selection(CellRange::single(CellAddress { row: 1, column: 0 }));
    (harness, document_id, table_id)
}

// Verify Enter previews first, applies the current result, and rejects changed search conditions.
#[test]
fn search_enter_previews_then_applies_and_rejects_stale_conditions() {
    let (mut harness, document_id, table_id) = dialog_harness("needle");
    let app = harness.state_mut();
    app.start_search_dialog();
    let dialog = app.search_dialog.as_mut().unwrap();
    dialog.query = "needle".to_owned();
    dialog.replacement = "replaced".to_owned();
    dialog.replace_mode = true;
    dialog.case_sensitive = true;
    dialog.whole_cell = true;
    harness.step();

    press_enter(&mut harness);
    assert!(harness.state().pending_searches.contains_key(&document_id));
    wait_for_preview(&mut harness, |app| {
        app.search_dialog
            .as_ref()
            .and_then(|dialog| dialog.preview.as_ref())
            .is_some_and(|preview| preview.prepared.is_some())
    });
    assert_eq!(
        harness.state().workspace.documents[&document_id].tables[0]
            .cell(CellAddress::default())
            .unwrap()
            .input,
        CellInput::Text("needle".to_owned()),
        "previewing must not edit the cell"
    );

    press_enter(&mut harness);
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .tables
            .iter()
            .find(|table| table.id == table_id)
            .unwrap()
            .cell(CellAddress::default())
            .unwrap()
            .input,
        CellInput::Text("replaced".to_owned()),
        "Enter applies the ready preview"
    );

    harness.state_mut().start_search_dialog();
    let dialog = harness.state_mut().search_dialog.as_mut().unwrap();
    dialog.query = "replaced".to_owned();
    dialog.replacement = "stale".to_owned();
    dialog.replace_mode = true;
    dialog.case_sensitive = true;
    dialog.whole_cell = true;
    harness.step();
    press_enter(&mut harness);
    wait_for_preview(&mut harness, |app| {
        app.search_dialog
            .as_ref()
            .and_then(|dialog| dialog.preview.as_ref())
            .is_some_and(|preview| preview.prepared.is_some())
    });
    harness.state_mut().search_dialog.as_mut().unwrap().query = "changed".to_owned();
    press_enter(&mut harness);
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .tables
            .iter()
            .find(|table| table.id == table_id)
            .unwrap()
            .cell(CellAddress::default())
            .unwrap()
            .input,
        CellInput::Text("replaced".to_owned()),
        "a preview for the old query must not be applied"
    );
}

// Verify Enter previews and applies a data edit through the same modal command path.
#[test]
fn data_edit_enter_previews_then_applies_current_result() {
    let (mut harness, document_id, table_id) = split_dialog_harness();
    harness
        .state_mut()
        .start_data_edit_dialog(DataEditKind::SplitColumn);
    harness
        .state_mut()
        .data_edit_dialog
        .as_mut()
        .unwrap()
        .overwrite = true;
    harness.step();

    press_enter(&mut harness);
    assert!(
        harness
            .state()
            .pending_data_edits
            .contains_key(&document_id)
    );
    wait_for_preview(&mut harness, |app| {
        app.data_edit_dialog
            .as_ref()
            .and_then(|dialog| dialog.preview.as_ref())
            .is_some_and(|preview| preview.prepared.is_some())
    });
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .tables
            .iter()
            .find(|table| table.id == table_id)
            .unwrap()
            .cell(CellAddress { row: 1, column: 0 })
            .unwrap()
            .input,
        CellInput::Text("alpha,beta".to_owned()),
        "previewing must not edit the cell"
    );

    press_enter(&mut harness);
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .tables
            .iter()
            .find(|table| table.id == table_id)
            .unwrap()
            .cell(CellAddress { row: 1, column: 0 })
            .unwrap()
            .input,
        CellInput::Text("alpha,beta".to_owned()),
        "Enter applies the ready data edit"
    );
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .tables
            .iter()
            .find(|table| table.id == table_id)
            .unwrap()
            .cell(CellAddress { row: 1, column: 1 })
            .unwrap()
            .input,
        CellInput::Text("alpha".to_owned())
    );
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .tables
            .iter()
            .find(|table| table.id == table_id)
            .unwrap()
            .cell(CellAddress { row: 1, column: 2 })
            .unwrap()
            .input,
        CellInput::Text("beta".to_owned())
    );
}
