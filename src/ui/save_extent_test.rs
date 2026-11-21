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

/// Types one value into a cell chosen through the name box.
fn put(harness: &mut AppHarness, cell: &str, value: &str) {
    select_cell_by_name_box(harness, cell);
    send_event(harness, egui::Event::Text(value.to_owned()));
    send_event(harness, key_event(Key::Enter, egui::Modifiers::NONE));
    settle(harness, 60);
}

/// Clears one cell chosen through the name box.
fn clear(harness: &mut AppHarness, cell: &str) {
    select_cell_by_name_box(harness, cell);
    send_event(harness, key_event(Key::Delete, egui::Modifiers::NONE));
    settle(harness, 60);
}

/// Opens a file, runs the edit steps, saves in place, and returns the saved text.
fn edit_and_save(name: &str, body: &str, steps: impl FnOnce(&mut AppHarness)) -> String {
    let directory = ScenarioDirectory::new();
    let path = directory.0.join(name);
    fs::write(&path, body).expect("write the input file");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    queue_open_and_wait(&mut harness, path.clone());
    settle(&mut harness, 20);
    steps(&mut harness);
    let command = command_modifiers(&harness.ctx);
    send_event(&mut harness, key_event(Key::S, command));
    settle(&mut harness, 150);
    fs::read_to_string(&path).expect("read the saved file")
}

#[test]
/// Checks that a far CSV cell typed and cleared again leaves no empty records behind.
fn csv_far_cell_typed_then_cleared_is_not_saved() {
    let saved = edit_and_save("far.csv", "a,b\n1,2\n", |harness| {
        put(harness, "A10", "x");
        clear(harness, "A10");
    });
    assert_eq!(saved, "a,b\n1,2\n");
}

#[test]
/// Checks that a CSV column typed and cleared again leaves no empty fields behind.
fn csv_far_column_typed_then_cleared_is_not_saved() {
    let saved = edit_and_save("column.csv", "a,b\n1,2\n", |harness| {
        put(harness, "E2", "x");
        clear(harness, "E2");
    });
    assert_eq!(saved, "a,b\n1,2\n");
}

#[test]
/// Checks that clearing the farther of two new CSV cells shrinks to the nearer one.
fn csv_range_shrinks_to_the_remaining_value() {
    let saved = edit_and_save("both.csv", "a,b\n1,2\n", |harness| {
        put(harness, "D6", "x");
        put(harness, "B4", "y");
        clear(harness, "D6");
    });
    let rows = saved.lines().collect::<Vec<_>>();
    assert_eq!(rows.len(), 4, "saved: {saved:?}");
    assert_eq!(rows[3], ",y");
}

#[test]
/// Checks that new Markdown rows use the same cell padding as the existing rows.
fn markdown_new_rows_match_existing_padding() {
    let saved = edit_and_save(
        "rows.md",
        "| A | B |\n| --- | --- |\n| 1 | 2 |\n",
        |harness| {
            put(harness, "B4", "y");
        },
    );
    assert_eq!(
        saved,
        "| A | B |\n| --- | --- |\n| 1 | 2 |\n|  |  |\n|  | y |\n"
    );
}

#[test]
/// Checks that cells in a new Markdown column use the same padding as the existing cells.
fn markdown_new_column_matches_existing_padding() {
    let saved = edit_and_save(
        "column.md",
        "| A | B |\n| --- | --- |\n| 1 | 2 |\n",
        |harness| {
            put(harness, "C2", "x");
        },
    );
    assert_eq!(saved, "| A | B |  |\n| --- | --- | --- |\n| 1 | 2 | x |\n");
}

#[test]
/// Checks that untouched trailing empty fields and rows from the original file are kept.
fn original_trailing_empty_cells_are_kept() {
    let saved = edit_and_save("keep.csv", "a,b,\n1,2,\n,,\n", |harness| {
        put(harness, "A2", "z");
    });
    assert_eq!(saved, "a,b,\nz,2,\n,,\n");
}
