use super::*;
use crate::document::DocumentDisplayMode;
use crate::ui::scenario_tests as scenario;
use egui::Key;
use std::fs;

/// Formats two selected Markdown cells once and undoes the batch once in both display modes.
#[test]
fn selected_cells_format_once_in_live_and_source_modes() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("format.md");
    fs::write(
        &source,
        "| first | second |\n| --- | --- |\n| alpha | beta |\n",
    )
    .expect("write a small Markdown table");
    let mut harness = scenario::scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source.clone());
    let (document_id, table_id) = active_ids(&harness);

    scenario::click_cell(&mut harness, "A2");
    select_through_right(&mut harness);
    let before_revision = harness.state().workspace.documents[&document_id].revision;
    let command = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::B, command));
    assert_cells(&harness, document_id, table_id, "**alpha**", "**beta**");
    assert_eq!(
        harness.state().workspace.documents[&document_id].revision,
        before_revision + 1,
        "both cells share one undo command"
    );

    scenario::send_event(&mut harness, scenario::key_event(Key::Z, command));
    assert_cells(&harness, document_id, table_id, "alpha", "beta");

    scenario::send_event(&mut harness, scenario::key_event(Key::A, command));
    scenario::send_event(&mut harness, scenario::key_event(Key::Slash, command));
    assert_eq!(
        harness.state().workspace.documents[&document_id].display_mode,
        DocumentDisplayMode::Source
    );
    scenario::send_event(&mut harness, scenario::key_event(Key::I, command));
    assert_cells(&harness, document_id, table_id, "*alpha*", "*beta*");
    assert_eq!(
        cell_text_at(&harness, document_id, 0, 0),
        "*first*",
        "whole-sheet formatting clips to stored table cells"
    );
}

/// Uses the active cell to choose one format state for every selected cell.
#[test]
fn mixed_selection_toggle_uses_the_active_cell_in_both_directions() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("mixed-format.md");
    fs::write(
        &source,
        "| first | second |\n| --- | --- |\n| **alpha** | beta |\n",
    )
    .expect("write a mixed-format row");
    let mut harness = scenario::scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source);
    let (document_id, _) = active_ids(&harness);
    let command = scenario::command_modifiers(&harness.ctx);

    scenario::click_cell(&mut harness, "A2");
    select_through_right(&mut harness);
    assert_eq!(
        active_cell(&harness, document_id),
        CellAddress { row: 1, column: 0 }
    );
    scenario::send_event(&mut harness, scenario::key_event(Key::B, command));
    assert_eq!(cell_text_at(&harness, document_id, 1, 0), "alpha");
    assert_eq!(cell_text_at(&harness, document_id, 1, 1), "beta");
    scenario::send_event(&mut harness, scenario::key_event(Key::Z, command));
    assert_eq!(cell_text_at(&harness, document_id, 1, 0), "**alpha**");
    assert_eq!(cell_text_at(&harness, document_id, 1, 1), "beta");

    scenario::click_cell(&mut harness, "B2");
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::ArrowLeft, egui::Modifiers::SHIFT),
    );
    assert_eq!(
        active_cell(&harness, document_id),
        CellAddress { row: 1, column: 1 }
    );
    scenario::send_event(&mut harness, scenario::key_event(Key::B, command));
    assert_eq!(cell_text_at(&harness, document_id, 1, 0), "**alpha**");
    assert_eq!(cell_text_at(&harness, document_id, 1, 1), "**beta**");
    scenario::send_event(&mut harness, scenario::key_event(Key::Z, command));
    assert_eq!(cell_text_at(&harness, document_id, 1, 0), "**alpha**");
    assert_eq!(cell_text_at(&harness, document_id, 1, 1), "beta");
}

/// Formats a replaced numeric result without changing another live formula.
#[test]
fn formatting_replaced_formula_result_keeps_the_value_and_live_formula() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("formatted-result.md");
    fs::write(
        &source,
        "| result | formula |\n| --- | --- |\n| =1+2 | =2+2 |\n",
    )
    .expect("write a result and a live formula");
    let mut harness = scenario::scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source);
    let (document_id, table_id) = active_ids(&harness);
    let result_address = CellAddress { row: 1, column: 0 };
    let formula_address = CellAddress { row: 1, column: 1 };
    scenario::wait_until(&mut harness, |app| {
        app.workspace.documents[&document_id]
            .calculation_result_for(table_id, result_address)
            .is_some_and(|result| result.status == crate::document::CalcStatus::Fresh)
    });
    harness
        .state_mut()
        .workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .replace_formula_results_in_ranges(table_id, &[CellRange::single(result_address)])
        .expect("the fresh formula result becomes a numeric constant");
    assert!(matches!(
        harness.state().workspace.documents[&document_id].tables[0]
            .cell(result_address)
            .unwrap()
            .input,
        CellInput::Number(3.0)
    ));

    scenario::click_cell(&mut harness, "A2");
    let command = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::B, command));
    assert_eq!(cell_text_at(&harness, document_id, 1, 0), "**3**");
    assert!(matches!(
        harness.state().workspace.documents[&document_id].tables[0]
            .cell(formula_address)
            .unwrap()
            .input,
        CellInput::Text(ref source) if source == "=2+2"
    ));

    scenario::send_event(&mut harness, scenario::key_event(Key::Z, command));
    scenario::wait_until(&mut harness, |app| {
        !app.pending_history_restores.contains_key(&document_id)
            && matches!(
                app.workspace.documents[&document_id].tables[0]
                    .cell(result_address)
                    .unwrap()
                    .input,
                CellInput::Number(3.0)
            )
    });
    let before_formula_shortcut = harness.state().workspace.documents[&document_id].revision;
    scenario::click_cell(&mut harness, "B2");
    scenario::send_event(&mut harness, scenario::key_event(Key::B, command));
    let document = &harness.state().workspace.documents[&document_id];
    assert_eq!(document.revision, before_formula_shortcut);
    assert!(matches!(
        document.tables[0].cell(formula_address).unwrap().input,
        CellInput::Text(ref source) if source == "=2+2"
    ));
}

/// Completes a selected-cell format through the worker when source byte deltas need rebuilding.
#[test]
fn selected_cell_format_completes_worker_validation_for_an_unmapped_overlay() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("format-overlay.md");
    fs::write(&source, "| value |\n| --- |\n| alpha |\n").expect("write a small Markdown table");
    let mut harness = scenario::scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source.clone());
    let (document_id, table_id) = active_ids(&harness);
    let overlay_address = CellAddress { row: 2, column: 0 };
    {
        let document = harness
            .state_mut()
            .workspace
            .documents
            .get_mut(&document_id)
            .expect("the loaded document exists");
        let table = document
            .tables
            .iter_mut()
            .find(|table| table.id == table_id)
            .expect("the loaded table exists");
        table.set_dimensions(3, 1);
        table.set_loaded_cell(overlay_address, Cell::text("staged"));
        document.record_changed_cells([(table_id, overlay_address)]);
    }

    scenario::click_cell(&mut harness, "A2");
    let command = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::B, command));
    scenario::wait_until(&mut harness, |app| {
        app.pending_structures.is_empty()
            && scenario::cell_text(app, document_id, CellAddress { row: 1, column: 0 }).as_deref()
                == Some("**alpha**")
    });
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 2, column: 0 },
        )
        .as_deref(),
        Some("staged")
    );

    scenario::send_event(&mut harness, scenario::key_event(Key::S, command));
    scenario::wait_until(&mut harness, |app| {
        app.pending_saves.is_empty() && !app.workspace.documents[&document_id].is_dirty()
    });
    let reopened = crate::markdown_adapter::MarkdownDocument::parse(
        fs::read(source).expect("the formatted source is saved"),
    )
    .expect("the saved source can be reopened");
    assert_eq!(reopened.tables[0].rows[1][0].value, "**alpha**");
    assert_eq!(reopened.tables[0].rows[2][0].value, "staged");
}

/// Clips whole-column and whole-row formatting to cells that exist in the saved table extent.
#[test]
fn full_axis_formatting_touches_only_stored_cells_inside_the_table() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("axes.md");
    fs::write(
        &source,
        "| first | second |\n| --- | --- |\n| alpha | beta |\n",
    )
    .expect("write a small Markdown table");
    let mut harness = scenario::scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source);
    let (document_id, _) = active_ids(&harness);
    let command = scenario::command_modifiers(&harness.ctx);

    scenario::click_cell(&mut harness, "A2");
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Space, egui::Modifiers::CTRL),
    );
    scenario::send_event(&mut harness, scenario::key_event(Key::B, command));
    assert_eq!(cell_text_at(&harness, document_id, 0, 0), "**first**");
    assert_eq!(cell_text_at(&harness, document_id, 1, 0), "**alpha**");
    assert_eq!(cell_text_at(&harness, document_id, 1, 1), "beta");
    scenario::send_event(&mut harness, scenario::key_event(Key::Z, command));

    scenario::click_cell(&mut harness, "A2");
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Space, egui::Modifiers::SHIFT),
    );
    scenario::send_event(&mut harness, scenario::key_event(Key::I, command));
    assert_eq!(cell_text_at(&harness, document_id, 0, 0), "first");
    assert_eq!(cell_text_at(&harness, document_id, 1, 0), "*alpha*");
    assert_eq!(cell_text_at(&harness, document_id, 1, 1), "*beta*");
    scenario::send_event(&mut harness, scenario::key_event(Key::I, command));
    assert_eq!(cell_text_at(&harness, document_id, 1, 0), "alpha");
    assert_eq!(cell_text_at(&harness, document_id, 1, 1), "beta");
}

/// Applies character formatting only to the editor selection and commits through the grid.
#[test]
fn format_shortcut_keeps_the_text_editor_selection_path() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("edit-selection.md");
    fs::write(&source, "| value |\n| --- |\n| alpha beta |\n")
        .expect("write a Markdown cell for text selection");
    let mut harness = scenario::scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source);
    let (document_id, table_id) = active_ids(&harness);
    scenario::click_cell(&mut harness, "A2");
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::F2, egui::Modifiers::NONE),
    );
    let command = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::A, command));
    scenario::send_event(&mut harness, egui::Event::Text("alpha beta".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Home, egui::Modifiers::NONE),
    );
    let shift = egui::Modifiers::SHIFT;
    scenario::send_event(&mut harness, scenario::key_event(Key::ArrowRight, shift));
    for _ in 0..4 {
        scenario::send_event(&mut harness, scenario::key_event(Key::ArrowRight, shift));
    }
    scenario::send_event(&mut harness, scenario::key_event(Key::B, command));
    assert!(harness.state().edit.is_editing());
    assert_eq!(harness.state().edit.buffer, "**alpha** beta");
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 0 }
        )
        .as_deref(),
        Some("**alpha** beta")
    );
    assert!(harness.state().workspace.documents[&document_id].can_undo());
    assert_eq!(
        harness.state().workspace.documents[&document_id].active_table,
        table_id
    );
}

/// Promotes a CSV range and records the source conversion and formatting as one undo step.
#[test]
fn selected_csv_cells_promote_and_undo_as_one_operation() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("format.csv");
    fs::write(&source, "first,second\none,two\n").expect("write a two-cell CSV table");
    let mut harness = scenario::scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source.clone());
    let (document_id, table_id) = active_ids(&harness);
    scenario::click_cell(&mut harness, "A2");
    select_through_right(&mut harness);
    let command = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::E, command));
    scenario::wait_until(&mut harness, |app| {
        !app.pending_structures.contains_key(&document_id)
            && matches!(
                app.workspace.documents[&document_id].source.as_deref(),
                Some(SourceDocument::Markdown(_))
            )
    });
    assert_cells(&harness, document_id, table_id, "`one`", "`two`");

    scenario::send_event(&mut harness, scenario::key_event(Key::Z, command));
    scenario::wait_until(&mut harness, |app| {
        app.pending_history_restores.is_empty()
            && matches!(
                app.workspace.documents[&document_id].source.as_deref(),
                Some(SourceDocument::Csv(_))
            )
    });
    assert_eq!(
        fs::read_to_string(&source).expect("the original CSV remains unchanged"),
        "first,second\none,two\n"
    );
    assert_eq!(cell_text_at(&harness, document_id, 1, 0), "one");
    assert_eq!(cell_text_at(&harness, document_id, 1, 1), "two");
}

/// Sends a real shifted grid movement to select the next cell.
fn select_through_right(harness: &mut egui_kittest::Harness<'_, TexcelApp>) {
    scenario::send_event(
        harness,
        scenario::key_event(Key::ArrowRight, egui::Modifiers::SHIFT),
    );
    let document = harness.state().workspace.active_document().unwrap();
    let view = document.view(document.active_table().unwrap().id).unwrap();
    assert_eq!(
        view.selection.normalized(),
        (
            CellAddress { row: 1, column: 0 },
            CellAddress { row: 1, column: 1 }
        ),
        "Shift+Right selects both data cells"
    );
}

/// Returns the current document and sheet without replacing their loaded state.
fn active_ids(harness: &egui_kittest::Harness<'_, TexcelApp>) -> (DocumentId, TableId) {
    let document = harness.state().workspace.active_document().unwrap();
    (document.id, document.active_table().unwrap().id)
}

/// Reads the current table input cell used for range-format decisions.
fn active_cell(
    harness: &egui_kittest::Harness<'_, TexcelApp>,
    document_id: DocumentId,
) -> CellAddress {
    let document = &harness.state().workspace.documents[&document_id];
    document
        .view(document.active_table().unwrap().id)
        .unwrap()
        .active_cell()
}

/// Checks the raw source values after one formatting operation.
fn assert_cells(
    harness: &egui_kittest::Harness<'_, TexcelApp>,
    document_id: DocumentId,
    table_id: TableId,
    first: &str,
    second: &str,
) {
    let document = &harness.state().workspace.documents[&document_id];
    let table = document
        .tables
        .iter()
        .find(|table| table.id == table_id)
        .unwrap();
    assert_eq!(
        table
            .cell(CellAddress { row: 1, column: 0 })
            .unwrap()
            .input
            .source_text(),
        first
    );
    assert_eq!(
        table
            .cell(CellAddress { row: 1, column: 1 })
            .unwrap()
            .input
            .source_text(),
        second
    );
}

/// Reads one cell's raw text from the active table.
fn cell_text_at(
    harness: &egui_kittest::Harness<'_, TexcelApp>,
    document_id: DocumentId,
    row: usize,
    column: usize,
) -> String {
    scenario::cell_text(harness.state(), document_id, CellAddress { row, column })
        .expect("the source cell remains present")
}
