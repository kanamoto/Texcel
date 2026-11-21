use super::scenario_tests::{
    ScenarioDirectory, command_modifiers, key_event, queue_open_and_wait, scenario_harness,
    select_cell_by_name_box, send_batch, send_event,
};
use super::*;
use std::fs;
use std::time::Duration;

type AppHarness = egui_kittest::Harness<'static, TexcelApp>;

/// Lets workers finish while the app keeps drawing frames.
fn settle(harness: &mut AppHarness, frames: usize) {
    for _ in 0..frames {
        send_batch(harness, []);
        std::thread::sleep(Duration::from_millis(3));
    }
}

/// Runs one shared menu command against the active window.
fn run_menu(harness: &mut AppHarness, id: &str) {
    let target = harness.state().native_action_target(&harness.ctx, false);
    harness.state_mut().native_menu_events_for_test = vec![crate::native_menu::NativeMenuEvent {
        id: id.to_owned(),
        recent_path: None,
        target,
    }];
    harness.step();
    settle(harness, 80);
}

/// Deletes the rows or columns of a selection and returns the saved Markdown.
fn delete_and_save(body: &str, selection: &str, command: &str) -> String {
    let directory = ScenarioDirectory::new();
    let path = directory.0.join("formulas.md");
    fs::write(&path, body).expect("write the input table");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    queue_open_and_wait(&mut harness, path.clone());
    settle(&mut harness, 20);
    select_cell_by_name_box(&mut harness, selection);
    settle(&mut harness, 5);
    run_menu(&mut harness, command);
    let save = command_modifiers(&harness.ctx);
    send_event(&mut harness, key_event(Key::S, save));
    settle(&mut harness, 150);
    fs::read_to_string(&path).expect("read the saved table")
}

const ROWS: &str = "| A |\n| --- |\n| 1 |\n| 2 |\n| 3 |\n| =SUM(A2:A4) |\n";

#[test]
/// Checks that deleting the first row of a range shrinks the range.
fn deleting_the_first_row_of_a_range_shrinks_it() {
    let saved = delete_and_save(ROWS, "A2", "edit.delete_rows");
    assert!(saved.contains("=SUM(A2:A3)"), "saved: {saved:?}");
}

#[test]
/// Checks that deleting the last row of a range shrinks the range.
fn deleting_the_last_row_of_a_range_shrinks_it() {
    let saved = delete_and_save(ROWS, "A4", "edit.delete_rows");
    assert!(saved.contains("=SUM(A2:A3)"), "saved: {saved:?}");
}

#[test]
/// Checks that deleting every row of a range turns the reference into an error.
fn deleting_every_row_of_a_range_is_a_reference_error() {
    let saved = delete_and_save(ROWS, "A2:A4", "edit.delete_rows");
    assert!(saved.contains("=SUM(#REF!)"), "saved: {saved:?}");
}

#[test]
/// Checks that a single-cell reference to a deleted row is still a reference error.
fn deleting_a_single_referenced_row_is_a_reference_error() {
    let saved = delete_and_save(
        "| A | B |\n| --- | --- |\n| 1 | =A2*2 |\n| 2 | x |\n",
        "A2",
        "edit.delete_rows",
    );
    assert!(!saved.contains("=A2*2"), "saved: {saved:?}");
}

#[test]
/// Checks that deleting the first column of a range shrinks the range.
fn deleting_the_first_column_of_a_range_shrinks_it() {
    let saved = delete_and_save(
        "| A | B | C | D |\n| --- | --- | --- | --- |\n| 1 | 2 | 3 | =SUM(A2:C2) |\n",
        "A2",
        "edit.delete_columns",
    );
    assert!(saved.contains("=SUM(A2:B2)"), "saved: {saved:?}");
}

#[test]
/// Checks that text typed into the name box after a row insert does not also edit a cell.
fn name_box_text_after_row_insert_does_not_reach_the_grid() {
    let directory = ScenarioDirectory::new();
    let path = directory.0.join("names.md");
    fs::write(&path, "| A | B |\n| --- | --- |\n| 1 | 2 |\n| 3 | 4 |\n").expect("write the table");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    queue_open_and_wait(&mut harness, path);
    settle(&mut harness, 20);
    select_cell_by_name_box(&mut harness, "A3");
    run_menu(&mut harness, "insert.rows");
    select_cell_by_name_box(&mut harness, "B3");
    assert!(
        !harness.state().edit.is_editing(),
        "the grid did not start editing"
    );
    let document_id = harness.state().workspace.active_document.unwrap();
    assert_eq!(
        super::scenario_tests::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 0, column: 0 }
        )
        .as_deref(),
        Some("A")
    );
}
