use super::*;
use egui_kittest::{
    Harness,
    kittest::{By, Queryable},
};

// Builds a small rendered table for formula-reference pointer tests.
fn reference_harness() -> Harness<'static, TexcelApp> {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    install_test_markdown_grid(harness.state_mut(), 6, 5, "Formula references");
    harness.step();
    harness
}

// Sends one real input event through the app's normal frame processing.
fn send_reference_event(harness: &mut Harness<'_, TexcelApp>, event: egui::Event) {
    send_reference_events(harness, [event]);
}

// Runs one ordered event batch through the production raw-input hook.
fn send_reference_events(
    harness: &mut Harness<'_, TexcelApp>,
    events: impl IntoIterator<Item = egui::Event>,
) {
    harness.input_mut().events.extend(events);
    let ctx = harness.ctx.clone();
    let mut raw = std::mem::take(harness.input_mut());
    eframe::App::raw_input_hook(harness.state_mut(), &ctx, &mut raw);
    *harness.input_mut() = raw;
    harness.step();
}

// Sends a complete cell click with the requested keyboard modifiers.
fn click_reference_cell(
    harness: &mut Harness<'_, TexcelApp>,
    position: Pos2,
    modifiers: egui::Modifiers,
) {
    send_reference_event(harness, egui::Event::ModifiersChanged(modifiers));
    send_reference_event(harness, egui::Event::PointerMoved(position));
    send_reference_event(
        harness,
        egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers,
        },
    );
    send_reference_event(
        harness,
        egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers,
        },
    );
    send_reference_event(
        harness,
        egui::Event::ModifiersChanged(egui::Modifiers::NONE),
    );
}

// Finds a formula outline in the paint shapes produced for the latest frame.
fn shape_has_reference_outline(shape: &egui::epaint::Shape) -> bool {
    match shape {
        egui::epaint::Shape::LineSegment { stroke, .. } => {
            stroke.width >= 2.0 && stroke.color == formula_reference_color(0)
        }
        egui::epaint::Shape::Vec(shapes) => shapes.iter().any(shape_has_reference_outline),
        _ => false,
    }
}

// Keeps one raster review available when this host exposes a WGPU adapter.
#[test]
#[ignore = "requires a WGPU adapter for raster review"]
fn formula_reference_outline_raster_review() {
    let mut harness = reference_harness();
    harness
        .state_mut()
        .begin_selected_replacing("=A1:B2".to_owned());
    harness.step();
    let rendered = harness
        .render()
        .expect("the raster review needs a working WGPU adapter");
    let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("kittest-renders");
    std::fs::create_dir_all(&directory).expect("create raster review directory");
    rendered
        .save(directory.join("formula-reference.png"))
        .expect("save the reference outline review");
}

// Verifies that click, drag, arrows, and F2 keep a formula as one uncommitted edit.
#[test]
fn pointer_and_arrow_reference_picking_preserve_the_formula_edit() {
    let mut harness = reference_harness();
    let (document_id, table_id) = {
        let app = harness.state_mut();
        let document = app.workspace.active_document().unwrap();
        (document.id, document.active_table().unwrap().id)
    };
    let (source_revision, source_input) = {
        let document = &harness.state().workspace.documents[&document_id];
        (
            document.revision,
            document
                .active_table()
                .unwrap()
                .cell(CellAddress::default())
                .unwrap()
                .input
                .clone(),
        )
    };
    harness
        .state_mut()
        .begin_selected_replacing("=SUM(".to_owned());
    harness.step();
    let cell_at = |harness: &Harness<'_, TexcelApp>, label: &str| {
        harness
            .query_all(By::new().role(egui::Role::GridCell).label_contains(label))
            .next()
            .expect("the target grid cell is visible")
            .rect()
            .center()
    };
    let start = cell_at(&harness, "A2");
    send_reference_event(&mut harness, egui::Event::PointerMoved(start));
    send_reference_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    );
    send_reference_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    assert_eq!(harness.state().edit.buffer, "=SUM(A2");
    assert!(harness.state().edit.is_editing());
    assert_eq!(
        harness.state().edit.address,
        Some(CellAddress { row: 0, column: 0 })
    );
    assert_eq!(
        harness.state().edit.target,
        Some(crate::document::EditTarget::FormulaBar)
    );
    assert_eq!(harness.state().edit.mode, EditMode::FormulaReference);

    let start = cell_at(&harness, "A2");
    let end = cell_at(&harness, "C3");
    send_reference_event(&mut harness, egui::Event::PointerMoved(start));
    send_reference_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    );
    send_reference_event(&mut harness, egui::Event::PointerMoved(end));
    send_reference_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    assert_eq!(harness.state().edit.buffer, "=SUM(A2:C3");
    assert_eq!(harness.state().formula_references.overlays().len(), 1);
    assert!(
        harness
            .output()
            .shapes
            .iter()
            .any(|clipped| shape_has_reference_outline(&clipped.shape))
    );

    send_reference_event(
        &mut harness,
        egui::Event::Key {
            key: Key::ArrowRight,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                shift: true,
                ..egui::Modifiers::NONE
            },
        },
    );
    assert_eq!(harness.state().edit.buffer, "=SUM(A2:D3");
    send_reference_event(
        &mut harness,
        egui::Event::Key {
            key: Key::F2,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    assert_ne!(harness.state().edit.mode, EditMode::FormulaReference);
    let buffer = harness.state().edit.buffer.clone();
    send_reference_event(
        &mut harness,
        egui::Event::Key {
            key: Key::ArrowRight,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    assert_eq!(harness.state().edit.buffer, buffer);
    assert!(harness.state().edit.is_editing());
    let document = harness
        .state()
        .workspace
        .documents
        .get(&document_id)
        .unwrap();
    assert_eq!(
        &document
            .tables
            .iter()
            .find(|table| table.id == table_id)
            .unwrap()
            .cell(CellAddress::default())
            .unwrap()
            .input,
        &source_input
    );
    assert_eq!(document.revision, source_revision);
}

// Verifies formula-bar editing enters reference mode only after F2.
#[test]
fn formula_bar_reference_pick_stays_in_the_original_edit_session() {
    let mut harness = reference_harness();
    let (document_id, table_id) = {
        let document = harness.state().workspace.active_document().unwrap();
        (document.id, document.active_table().unwrap().id)
    };
    harness
        .state_mut()
        .workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .set_cell(
            table_id,
            CellAddress::default(),
            Some(Cell::formula("=A1", None)),
        )
        .expect("the formula source is stored");
    harness.step();
    let formula_preview = harness
        .query_all(By::new().role(egui::Role::Button).label_contains("=A1"))
        .next()
        .expect("the formula bar preview is exposed")
        .rect()
        .center();
    send_reference_event(&mut harness, egui::Event::PointerMoved(formula_preview));
    send_reference_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: formula_preview,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    );
    send_reference_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: formula_preview,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    assert!(harness.state().edit.is_editing());
    assert_eq!(harness.state().edit.buffer, "=A1");
    assert_eq!(harness.state().edit.mode, EditMode::Edit);
    send_reference_event(
        &mut harness,
        egui::Event::Key {
            key: Key::F2,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    assert_eq!(harness.state().edit.mode, EditMode::FormulaReference);
    let session_id = harness.state().edit.session_id;
    let (revision, input) = {
        let document = &harness.state().workspace.documents[&document_id];
        (
            document.revision,
            document
                .tables
                .iter()
                .find(|table| table.id == table_id)
                .unwrap()
                .cell(CellAddress::default())
                .unwrap()
                .input
                .clone(),
        )
    };
    let target = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("B2"))
        .next()
        .expect("the reference cell is visible")
        .rect()
        .center();
    send_reference_event(&mut harness, egui::Event::PointerMoved(target));
    send_reference_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    );
    send_reference_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    let app = harness.state();
    assert_eq!(app.edit.buffer, "=B2");
    assert!(app.edit.is_editing());
    assert_eq!(app.edit.session_id, session_id);
    let document = &app.workspace.documents[&document_id];
    assert_eq!(document.revision, revision);
    assert_eq!(
        document
            .tables
            .iter()
            .find(|table| table.id == table_id)
            .unwrap()
            .cell(CellAddress::default())
            .unwrap()
            .input,
        input
    );
    send_reference_event(
        &mut harness,
        egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    assert!(!harness.state().edit.is_editing());
    let document = &harness.state().workspace.documents[&document_id];
    assert!(document.revision > revision);
    assert_eq!(
        document
            .tables
            .iter()
            .find(|table| table.id == table_id)
            .unwrap()
            .cell(CellAddress::default())
            .unwrap()
            .input
            .source_text(),
        "=B2"
    );
}

// Verifies F4 uses the application keyboard path to cycle reference anchoring.
#[test]
fn f4_cycles_reference_anchoring_in_the_active_edit() {
    let mut harness = reference_harness();
    harness
        .state_mut()
        .begin_selected_replacing("=A1".to_owned());
    harness.step();
    for expected in ["=$A$1", "=A$1", "=$A1", "=A1"] {
        send_reference_event(
            &mut harness,
            egui::Event::Key {
                key: Key::F4,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        );
        assert_eq!(harness.state().edit.buffer, expected);
        assert!(harness.state().edit.is_editing());
    }
}

// Verifies reference arrows are removed before TextEdit can move its cursor.
#[test]
fn reference_arrow_keeps_the_cursor_after_a_trailing_operator() {
    let mut harness = reference_harness();
    let (document_id, table_id) = {
        let document = harness.state().workspace.active_document().unwrap();
        (document.id, document.active_table().unwrap().id)
    };
    let target = CellAddress { row: 3, column: 3 };
    if let Some(view) = harness
        .state_mut()
        .workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .view_mut(table_id)
    {
        view.set_selection(CellRange::single(target));
    }
    harness
        .state_mut()
        .begin_selected_replacing("=A1+".to_owned());
    harness.step();
    send_reference_event(
        &mut harness,
        egui::Event::Key {
            key: Key::ArrowLeft,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    assert_eq!(harness.state().edit.buffer, "=A1+C4");
    assert_eq!(harness.state().edit.cursor_byte, "=A1+C4".len());
    assert!(harness.state().edit.is_editing());
}

// Keeps earlier text around a deferred reference arrow after focus loss.
#[test]
fn ordered_reference_input_applies_pending_text_before_focus_loss() {
    let mut harness = reference_harness();
    let (document_id, table_id) = {
        let document = harness.state().workspace.active_document().unwrap();
        (document.id, document.active_table().unwrap().id)
    };
    let start = CellAddress { row: 3, column: 3 };
    harness
        .state_mut()
        .workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .view_mut(table_id)
        .unwrap()
        .set_selection(CellRange::single(start));
    harness
        .state_mut()
        .begin_selected_replacing("=B1".to_owned());
    harness.step();
    send_reference_events(
        &mut harness,
        [
            egui::Event::Text("+".to_owned()),
            egui::Event::Key {
                key: Key::ArrowLeft,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Text(")".to_owned()),
        ],
    );
    assert_eq!(harness.state().edit.buffer, "=B1+");
    assert!(harness.state().pending_edit_input.is_some());
    send_reference_events(
        &mut harness,
        [
            egui::Event::WindowFocused(false),
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "あ".to_owned(),
                active_range_chars: Some(1..1),
            }),
        ],
    );
    assert_eq!(harness.state().edit.buffer, "=B1+C4)");
    assert!(harness.state().edit.is_editing());
    assert!(!harness.state().edit.composing);
}

// Applies F2 before an arrow in the same raw input batch.
#[test]
fn f2_and_arrow_follow_their_same_batch_order() {
    let mut harness = reference_harness();
    harness
        .state_mut()
        .begin_selected_replacing("=A1".to_owned());
    harness.step();
    send_reference_event(
        &mut harness,
        egui::Event::Key {
            key: Key::F2,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    assert_ne!(harness.state().edit.mode, EditMode::FormulaReference);
    send_reference_events(
        &mut harness,
        [
            egui::Event::Key {
                key: Key::F2,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Key {
                key: Key::ArrowRight,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    assert_eq!(harness.state().edit.buffer, "=B1");
    assert_eq!(harness.state().edit.mode, EditMode::FormulaReference);
}

// Blocks formula reference shortcuts while a settings modal owns input.
#[test]
fn settings_modal_blocks_background_formula_reference_input() {
    let mut harness = reference_harness();
    harness
        .state_mut()
        .begin_selected_replacing("=A1".to_owned());
    harness.step();
    let original = harness.state().edit.buffer.clone();
    let original_mode = harness.state().edit.mode;
    harness.state_mut().settings_window_open = true;
    harness.step();

    send_reference_events(
        &mut harness,
        [
            egui::Event::Key {
                key: Key::ArrowRight,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Key {
                key: Key::F4,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );

    assert!(harness.state().modal_input_blocked());
    assert_eq!(harness.state().edit.buffer, original);
    assert_eq!(harness.state().edit.mode, original_mode);
}

// Verifies F2 resumes reference picking from the token under the live text cursor.
#[test]
fn f2_cursor_move_reanchors_reference_navigation() {
    let mut harness = reference_harness();
    harness
        .state_mut()
        .begin_selected_replacing("=A1+B2".to_owned());
    harness.state_mut().edit.anchor_byte = 3;
    harness.state_mut().edit.cursor_byte = 3;
    harness.step();
    assert_eq!(harness.state().edit.mode, EditMode::FormulaReference);
    send_reference_event(
        &mut harness,
        egui::Event::Key {
            key: Key::F2,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    send_reference_event(
        &mut harness,
        egui::Event::Key {
            key: Key::ArrowLeft,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    send_reference_event(
        &mut harness,
        egui::Event::Key {
            key: Key::F2,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    send_reference_event(
        &mut harness,
        egui::Event::Key {
            key: Key::ArrowRight,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    assert_eq!(harness.state().edit.buffer, "=B1+B2");
}

// Verifies a text selection spanning references is replaced as one range.
#[test]
fn picking_replaces_explicit_selection_across_multiple_references() {
    let mut harness = reference_harness();
    harness
        .state_mut()
        .begin_selected_replacing("=A1+B1".to_owned());
    harness.step();
    for event in [
        egui::Event::Key {
            key: Key::F2,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::Key {
            key: Key::Home,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::Key {
            key: Key::ArrowRight,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::Key {
            key: Key::End,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                shift: true,
                ..egui::Modifiers::NONE
            },
        },
    ] {
        send_reference_event(&mut harness, event);
    }
    send_reference_event(
        &mut harness,
        egui::Event::Key {
            key: Key::F2,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    assert_eq!(
        (
            harness.state().edit.anchor_byte,
            harness.state().edit.cursor_byte
        ),
        (1, "=A1+B1".len())
    );
    assert_eq!(
        harness
            .state()
            .text_selection
            .map(|selection| (selection.start, selection.end)),
        Some((1, "=A1+B1".len()))
    );
    let target = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("C2"))
        .next()
        .expect("the replacement cell is visible")
        .rect()
        .center();
    click_reference_cell(&mut harness, target, egui::Modifiers::NONE);
    assert_eq!(harness.state().edit.buffer, "=C2");
}

// Replaces the token at a collapsed cursor after a formerly selected span is cleared.
#[test]
fn cleared_text_selection_does_not_replace_the_old_selection_on_pick() {
    let mut harness = reference_harness();
    harness
        .state_mut()
        .begin_selected_replacing("=A1+B1".to_owned());
    harness.step();
    send_reference_event(
        &mut harness,
        egui::Event::Key {
            key: Key::F2,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    for event in [
        egui::Event::Key {
            key: Key::Home,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::Key {
            key: Key::ArrowRight,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::Key {
            key: Key::End,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                shift: true,
                ..egui::Modifiers::NONE
            },
        },
    ] {
        send_reference_event(&mut harness, event);
    }
    send_reference_event(
        &mut harness,
        egui::Event::Key {
            key: Key::End,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    assert_eq!(
        harness.state().edit.anchor_byte,
        harness.state().edit.cursor_byte
    );
    send_reference_event(
        &mut harness,
        egui::Event::Key {
            key: Key::F2,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    let target = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("C2"))
        .next()
        .expect("the replacement cell is visible")
        .rect()
        .center();
    click_reference_cell(&mut harness, target, egui::Modifiers::NONE);
    assert_eq!(harness.state().edit.buffer, "=A1+C2");
}

// Verifies Shift-click extends the last picked reference instead of using the formula cell.
#[test]
fn shift_click_extends_the_current_formula_reference() {
    let mut harness = reference_harness();
    harness
        .state_mut()
        .begin_selected_replacing("=A1".to_owned());
    harness.step();
    let cell_at = |harness: &Harness<'_, TexcelApp>, label: &str| {
        harness
            .query_all(By::new().role(egui::Role::GridCell).label_contains(label))
            .next()
            .expect("the target grid cell is visible")
            .rect()
            .center()
    };
    let first = cell_at(&harness, "B2");
    click_reference_cell(&mut harness, first, egui::Modifiers::NONE);
    let second = cell_at(&harness, "C3");
    click_reference_cell(
        &mut harness,
        second,
        egui::Modifiers {
            shift: true,
            ..egui::Modifiers::NONE
        },
    );
    assert_eq!(harness.state().edit.buffer, "=B2:C3");
}

// Verifies reference picking cannot use a stale detail-panel byte selection.
#[test]
fn detail_panel_text_is_not_a_formula_reference_target() {
    let mut harness = reference_harness();
    harness
        .state_mut()
        .begin_selected_replacing("=B1\n".to_owned());
    harness.state_mut().edit.cursor_byte = 3;
    harness.state_mut().edit.anchor_byte = 3;
    harness.step();
    let app = harness.state_mut();
    app.edit
        .switch_target(crate::document::EditTarget::DetailPanel);
    app.edit.buffer = "=あ".to_owned();
    app.edit.mark_buffer_changed();
    assert!(!app.formula_reference_edit_is_current());
    assert!(!app.insert_formula_reference(
        CellRange::single(CellAddress { row: 1, column: 1 }),
        &egui::Context::default(),
    ));
    assert_eq!(app.edit.buffer, "=あ");
}

// Verifies disabling reference mode cancels the active drag before it changes selection.
#[test]
fn f2_cancels_a_formula_reference_drag() {
    let mut harness = reference_harness();
    harness
        .state_mut()
        .begin_selected_replacing("=A1".to_owned());
    harness.step();
    let start = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("B2"))
        .next()
        .expect("the drag start cell is visible")
        .rect()
        .center();
    let end = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("C3"))
        .next()
        .expect("the drag end cell is visible")
        .rect()
        .center();
    send_reference_event(&mut harness, egui::Event::PointerMoved(start));
    send_reference_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    );
    send_reference_event(
        &mut harness,
        egui::Event::PointerMoved(start + Vec2::new(12.0, 0.0)),
    );
    assert!(harness.state().grid_selection_drag.is_some());
    send_reference_event(
        &mut harness,
        egui::Event::Key {
            key: Key::F2,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    let selection = {
        let document = harness.state().workspace.active_document().unwrap();
        document
            .view(document.active_table().unwrap().id)
            .unwrap()
            .selection
    };
    send_reference_event(&mut harness, egui::Event::PointerMoved(end));
    send_reference_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    assert!(harness.state().grid_selection_drag.is_none());
    assert_ne!(harness.state().edit.mode, EditMode::FormulaReference);
    let after = {
        let document = harness.state().workspace.active_document().unwrap();
        document
            .view(document.active_table().unwrap().id)
            .unwrap()
            .selection
    };
    assert_eq!(after, selection);
}

// Prevents a completed formula drag from becoming a normal selection drag after commit.
#[test]
fn committing_during_reference_drag_cancels_the_pointer_gesture() {
    let mut harness = reference_harness();
    harness
        .state_mut()
        .begin_selected_replacing("=A1".to_owned());
    harness.step();
    let get_cell = |harness: &Harness<'_, TexcelApp>, label: &str| {
        harness
            .query_all(By::new().role(egui::Role::GridCell).label_contains(label))
            .next()
            .expect("the grid cell is visible")
            .rect()
            .center()
    };
    let start = get_cell(&harness, "B2");
    let end = get_cell(&harness, "C3");
    send_reference_event(&mut harness, egui::Event::PointerMoved(start));
    send_reference_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    );
    send_reference_event(&mut harness, egui::Event::PointerMoved(end));
    assert!(harness.state().grid_selection_drag.is_some());
    send_reference_event(
        &mut harness,
        egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    assert!(!harness.state().edit.is_editing());
    assert!(harness.state().grid_selection_drag.is_none());
    let selection_after_commit = {
        let document = harness.state().workspace.active_document().unwrap();
        document
            .view(document.active_table().unwrap().id)
            .unwrap()
            .selection
    };
    send_reference_event(
        &mut harness,
        egui::Event::PointerMoved(end + Vec2::new(20.0, 0.0)),
    );
    send_reference_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    let selection_after = {
        let document = harness.state().workspace.active_document().unwrap();
        document
            .view(document.active_table().unwrap().id)
            .unwrap()
            .selection
    };
    assert_eq!(selection_after, selection_after_commit);
}
