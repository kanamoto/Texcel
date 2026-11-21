use super::*;
use egui_kittest::{
    Harness,
    kittest::{By, NodeT, Queryable},
};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::scenario_tests as scenario;

#[test]
/// Keeps a Shift-click rectangle when modifier events pass through the app hook.
fn shift_click_keeps_the_selected_cell_rectangle() {
    let directory = scenario::ScenarioDirectory::new();
    let mut harness = scenario::scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(
        &mut harness,
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("samples")
            .join("interaction-workflow.md"),
    );
    select_shift_click_range(&mut harness);
    let view = harness
        .state()
        .workspace
        .active_document()
        .unwrap()
        .view(
            harness
                .state()
                .workspace
                .active_document()
                .unwrap()
                .active_table()
                .unwrap()
                .id,
        )
        .unwrap();
    assert_eq!(
        view.selection.normalized(),
        (
            CellAddress { row: 1, column: 1 },
            CellAddress { row: 2, column: 2 }
        )
    );
}

#[test]
/// Resizes the visible grid and edits its lower-right displayed cell.
fn sheet_resizing_changes_visible_cells_and_keeps_the_edge_editable() {
    let directory = scenario::ScenarioDirectory::new();
    let mut harness = scenario::scenario_harness(directory.0.join("resize-settings.json"));
    let document_id = harness.state().workspace.active_document.unwrap();
    let initial = visible_grid_cells(&harness);
    let initial_bounds = visible_cell_bounds(&initial);
    assert!(!initial.is_empty(), "the initial window shows grid cells");

    harness.set_size(Vec2::new(1300.0, 900.0)).step();
    let wider = wait_for_visible_grid_cells(&mut harness, |visible| {
        let bounds = visible_cell_bounds(visible);
        visible.len() > initial.len() && bounds.0 > initial_bounds.0 && bounds.1 > initial_bounds.1
    });

    harness.set_size(Vec2::new(640.0, 420.0)).step();
    let narrower = wait_for_visible_grid_cells(&mut harness, |visible| visible.len() < wider.len());
    let (address, rect) = narrower
        .iter()
        .filter(|(_, rect)| rect.width() >= 40.0 && rect.height() >= 16.0)
        .max_by(|(_, left), (_, right)| {
            left.center()
                .y
                .total_cmp(&right.center().y)
                .then_with(|| left.center().x.total_cmp(&right.center().x))
        })
        .copied()
        .expect("the resized window still shows an editable cell");
    scenario::click_point(&mut harness, rect.center());
    let document = &harness.state().workspace.documents[&document_id];
    let table_id = document.active_table().unwrap().id;
    let selected = document.view(table_id).unwrap().active_cell();
    assert_eq!(
        selected,
        address,
        "clicking {address:?} at {rect:?} with page {:?} keeps the actual selection",
        harness.state().grid_page_extent
    );
    scenario::send_event(&mut harness, egui::Event::Text("resize".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    // The checked edit may need a worker to grow the original table to this cell.
    scenario::wait_until(&mut harness, |app| {
        scenario::cell_text(app, document_id, address).as_deref() == Some("resize")
    });
    assert_eq!(
        scenario::cell_text(harness.state(), document_id, address).as_deref(),
        Some("resize"),
        "the lowest and rightmost visible cell accepts normal input"
    );
}

/// Selects B2, then extends it to C3 through a real Shift-pointer sequence.
fn select_shift_click_range(harness: &mut Harness<'_, TexcelApp>) {
    scenario::click_cell(harness, "B2");
    let end = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("C3"))
        .next()
        .expect("the range end cell is visible")
        .rect()
        .center();
    let mut shift = egui::Modifiers::NONE;
    shift.shift = true;
    scenario::send_batch(
        harness,
        [
            egui::Event::ModifiersChanged(shift),
            egui::Event::PointerMoved(end),
        ],
    );
    scenario::send_batch(
        harness,
        [egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: shift,
        }],
    );
    scenario::send_batch(
        harness,
        [
            egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: shift,
            },
            egui::Event::ModifiersChanged(egui::Modifiers::NONE),
        ],
    );
    scenario::send_batch(harness, []);
}

/// Reads visible cell coordinates and rectangles from the current AccessKit tree.
fn visible_grid_cells(harness: &Harness<'_, TexcelApp>) -> Vec<(CellAddress, egui::Rect)> {
    harness
        .query_all(By::new().role(egui::Role::GridCell))
        .filter_map(|node| {
            let label = node.accesskit_node().label()?;
            let address = label
                .split(',')
                .find_map(|part| super::parse_cell_address(part.trim()))?;
            Some((address, node.rect()))
        })
        .collect()
}

/// Waits for a resize to change the visible cell set through normal frames.
fn wait_for_visible_grid_cells(
    harness: &mut Harness<'_, TexcelApp>,
    ready: impl Fn(&[(CellAddress, egui::Rect)]) -> bool,
) -> Vec<(CellAddress, egui::Rect)> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let visible = visible_grid_cells(harness);
        if ready(&visible) {
            return visible;
        }
        scenario::send_batch(harness, []);
        assert!(Instant::now() < deadline, "the resized grid settles");
    }
}

/// Returns the greatest visible column and row indices.
fn visible_cell_bounds(cells: &[(CellAddress, egui::Rect)]) -> (usize, usize) {
    cells.iter().fold((0, 0), |(column, row), (address, _)| {
        (column.max(address.column), row.max(address.row))
    })
}
