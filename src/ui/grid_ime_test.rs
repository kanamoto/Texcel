use super::scenario_tests::{
    ScenarioDirectory, click_cell, key_event, scenario_harness, send_batch, send_event,
};
use super::*;

/// Sends one IME event the way the platform delivers it after IME is enabled.
fn ime(harness: &mut egui_kittest::Harness<'_, TexcelApp>, event: egui::ImeEvent) {
    send_batch(harness, [egui::Event::Ime(event)]);
}

#[test]
/// Checks that a selected cell keeps the platform IME enabled before editing starts.
fn selected_cell_requests_platform_ime_before_editing() {
    let directory = ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    click_cell(&mut harness, "A1");
    send_batch(&mut harness, []);
    assert!(!harness.state().edit.is_editing());
    let ime = harness.output().platform_output.ime;
    assert!(
        ime.is_some(),
        "a selected cell must keep the platform IME enabled"
    );
}

#[test]
/// Checks that a composition started on a selected cell becomes the cell value.
fn first_composition_key_on_a_selected_cell_is_not_committed_as_ascii() {
    let directory = ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    click_cell(&mut harness, "A1");
    ime(
        &mut harness,
        egui::ImeEvent::Preedit {
            text: "k".to_owned(),
            active_range_chars: None,
        },
    );
    assert!(harness.state().edit.is_editing());
    ime(
        &mut harness,
        egui::ImeEvent::Preedit {
            text: "か".to_owned(),
            active_range_chars: None,
        },
    );
    ime(&mut harness, egui::ImeEvent::Commit("か".to_owned()));
    assert_eq!(harness.state().edit.buffer, "か");
    send_event(&mut harness, key_event(Key::Enter, egui::Modifiers::NONE));
    let document_id = harness.state().workspace.active_document.unwrap();
    assert_eq!(
        super::scenario_tests::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 0, column: 0 }
        )
        .as_deref(),
        Some("か")
    );
}

#[test]
/// Ignores a cursor key the platform forwards during a preedit so the commit is not duplicated.
fn forwarded_arrow_during_preedit_keeps_the_composition_in_place() {
    let directory = ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    click_cell(&mut harness, "A1");
    for text in ["k", "か", "かな"] {
        ime(
            &mut harness,
            egui::ImeEvent::Preedit {
                text: text.to_owned(),
                active_range_chars: Some(text.chars().count()..text.chars().count()),
            },
        );
    }
    send_event(
        &mut harness,
        key_event(Key::ArrowLeft, egui::Modifiers::NONE),
    );
    assert!(harness.state().edit.composing);
    send_batch(
        &mut harness,
        [
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: String::new(),
                active_range_chars: None,
            }),
            egui::Event::Ime(egui::ImeEvent::Commit("かな".to_owned())),
        ],
    );
    assert_eq!(harness.state().edit.buffer, "かな");
    assert!(!harness.state().edit.composing);

    // After the commit, arrows leave the cell again as in direct input.
    send_event(
        &mut harness,
        key_event(Key::ArrowRight, egui::Modifiers::NONE),
    );
    let document_id = harness.state().workspace.active_document.unwrap();
    assert_eq!(
        super::scenario_tests::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 0, column: 0 }
        )
        .as_deref(),
        Some("かな")
    );
    assert!(!harness.state().edit.is_editing());
}

#[test]
/// Starts a composition on a selected cell without asking the platform to interrupt it.
///
/// winit drops its own marked text on an interrupt while macOS keeps the composition, so the
/// platform's later commit would never reach Texcel and the cell would stay in preedit.
fn first_preedit_on_a_selected_cell_does_not_interrupt_the_platform_ime() {
    let directory = ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    click_cell(&mut harness, "A1");
    send_batch(&mut harness, []);
    ime(
        &mut harness,
        egui::ImeEvent::Preedit {
            text: "あ".to_owned(),
            active_range_chars: Some(1..1),
        },
    );
    let output = harness.output().platform_output.ime;
    assert!(
        output.is_some_and(|ime| !ime.should_interrupt_composition),
        "moving focus into the new cell editor must not interrupt the composition"
    );
    assert!(harness.state().edit.composing);
    send_batch(
        &mut harness,
        [
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: String::new(),
                active_range_chars: None,
            }),
            egui::Event::Ime(egui::ImeEvent::Commit("あ".to_owned())),
        ],
    );
    assert!(!harness.state().edit.composing);
    send_event(&mut harness, key_event(Key::Enter, egui::Modifiers::NONE));
    let document_id = harness.state().workspace.active_document.unwrap();
    assert_eq!(
        super::scenario_tests::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 0, column: 0 }
        )
        .as_deref(),
        Some("あ")
    );
}
