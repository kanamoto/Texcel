use super::scenario_tests::{
    ScenarioDirectory, cell_text, click_cell, key_event, scenario_harness, select_cell_by_name_box,
    send_batch, send_event, wait_until,
};
use super::*;
use egui_kittest::kittest::{By, Queryable};

/// Finds the on-screen rectangle of one grid cell by its address label.
fn cell_rect(harness: &egui_kittest::Harness<'_, TexcelApp>, label: &str) -> Rect {
    harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains(label))
        .next()
        .unwrap_or_else(|| panic!("{label} is visible"))
        .rect()
}

#[test]
/// Drags the fill handle of 1 and 2 down two rows and gets the next numbers.
fn dragging_the_fill_handle_continues_a_number_sequence() {
    let directory = ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    click_cell(&mut harness, "A1");
    for value in ["1", "2"] {
        send_batch(&mut harness, [egui::Event::Text(value.to_owned())]);
        send_event(&mut harness, key_event(Key::Enter, egui::Modifiers::NONE));
    }
    select_cell_by_name_box(&mut harness, "A1:A2");
    let start = cell_rect(&harness, "A2").right_bottom() - Vec2::splat(3.0);
    let end = cell_rect(&harness, "A4").center();
    send_batch(&mut harness, [egui::Event::PointerMoved(start)]);
    send_batch(
        &mut harness,
        [egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    for step in 1..=4 {
        let point = start + (end - start) * (step as f32 / 4.0);
        send_batch(&mut harness, [egui::Event::PointerMoved(point)]);
    }
    send_batch(
        &mut harness,
        [egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    let document_id = harness.state().workspace.active_document.unwrap();
    wait_until(&mut harness, |app| {
        cell_text(app, document_id, CellAddress { row: 3, column: 0 }).is_some()
    });
    let values = (0..4)
        .map(|row| cell_text(harness.state(), document_id, CellAddress { row, column: 0 }))
        .collect::<Vec<_>>();
    assert_eq!(
        values,
        ["1", "2", "3", "4"].map(|value| Some(value.to_owned()))
    );
}
