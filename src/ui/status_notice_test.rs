use super::*;
use crate::ui::scenario_tests as scenario;
use egui_kittest::kittest::{By, Queryable};

/// Clears an old status message when the user starts another operation.
#[test]
fn status_notice_clears_on_the_next_user_action() {
    let directory = scenario::ScenarioDirectory::new();
    let mut harness = scenario::scenario_harness(directory.0.join("settings.json"));
    let notice = Message::new("status-save-conflict");
    harness.state_mut().status = Some(notice.clone());
    harness.step();
    let label = notice.resolve(Language::English);
    assert!(
        harness
            .query_all(By::new().label_contains(&label))
            .next()
            .is_some()
    );

    scenario::send_event(&mut harness, egui::Event::Text("x".to_owned()));
    assert!(
        harness
            .query_all(By::new().label_contains(&label))
            .next()
            .is_none(),
        "a new text action replaces the previous notice"
    );
}

/// Expires a status message after its five-second display window.
#[test]
fn status_notice_expires_at_its_deadline() {
    let directory = scenario::ScenarioDirectory::new();
    let mut harness = scenario::scenario_harness(directory.0.join("settings.json"));
    let notice = Message::new("status-save-conflict");
    harness.state_mut().status = Some(notice.clone());
    harness.step();
    harness.state_mut().status_lifetime =
        Some((notice.clone(), Instant::now() - STATUS_NOTICE_DURATION));

    harness.step();

    let label = notice.resolve(Language::English);
    assert!(
        harness
            .query_all(By::new().label_contains(&label))
            .next()
            .is_none()
    );
}

/// Keeps the import notice until the user closes it, even after other input.
#[test]
fn import_notice_stays_until_closed() {
    let directory = scenario::ScenarioDirectory::new();
    let mut harness = scenario::scenario_harness(directory.0.join("settings.json"));
    let document_id = harness.state().workspace.active_document.unwrap();
    let notice = Message::new("status-import-formatting-only");
    harness.state_mut().import_notice = Some((document_id, notice.clone()));
    harness.step();
    scenario::send_event(
        &mut harness,
        scenario::key_event(egui::Key::ArrowDown, egui::Modifiers::NONE),
    );
    assert!(harness.state().import_notice.is_some());

    let close = text(Language::English, "action-close-notice");
    harness.get_by_label(&close).click();
    harness.step();
    assert!(harness.state().import_notice.is_none());
}
