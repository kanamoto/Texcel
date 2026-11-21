use super::*;
use crate::ui::scenario_tests as scenario;
use egui_kittest::{
    Harness,
    kittest::{By, NodeT, Queryable},
};

/// Captures this harness's outgoing data before it reaches the user's clipboard.
pub(super) fn capture_if_requested(ctx: &egui::Context, html: &str, text: &str) -> bool {
    let id = Id::unique("capture_copy_options_test");
    ctx.data_mut(|data| {
        if data.get_temp::<bool>(id) != Some(true) {
            return false;
        }
        data.insert_temp(id, (html.to_owned(), text.to_owned()));
        true
    })
}

/// Copies a formula and bold text through every content and format choice in the Edit menu.
#[test]
fn copy_options_writes_all_eight_selected_formats() {
    let folder = scenario::ScenarioDirectory::new();
    let source = folder.0.join("copy.md");
    std::fs::write(
        &source,
        "| Formula | City |\n| ---: | :---: |\n| =1+2 | **Tokyo** Bay |\n",
    )
    .unwrap();
    let mut harness = scenario::scenario_harness(folder.0.join("settings.json"));
    super::super::top::force_shared_menu_bar_for_test(&harness.ctx);
    scenario::queue_open_and_wait(&mut harness, source);
    scenario::wait_until(&mut harness, |app| {
        let doc = app.workspace.active_document().unwrap();
        doc.calculation_result_for(doc.active_table, CellAddress { row: 1, column: 0 })
            .is_some_and(|result| result.status == CalcStatus::Fresh)
    });

    // Extend the active cell to the next column using the usual keyboard selection.
    scenario::click_cell(&mut harness, "A2");
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::ArrowRight, egui::Modifiers::SHIFT),
    );
    let id = Id::unique("capture_copy_options_test");
    harness.ctx.data_mut(|data| data.insert_temp(id, true));
    for source in [false, true] {
        for format in ["tsv", "csv", "markdown", "html"] {
            click_named(&mut harness, "Edit");
            click_named(&mut harness, "Copy with Options…");
            click_named(
                &mut harness,
                if source {
                    "Source (keeps formulas and formatting marks)"
                } else {
                    "Displayed values (results, no formatting marks)"
                },
            );
            let label =
                crate::i18n::context_text(&harness.ctx, &format!("dialog-copy-format-{format}"));
            click_named(&mut harness, &label);
            click_named(&mut harness, "Copy");

            // Read both ordinary paste formats and the data a Texcel paste would receive.
            let (html, text) = harness
                .ctx
                .data_mut(|data| data.remove_temp::<(String, String)>(id))
                .unwrap_or_else(|| panic!("copy wrote {format}, source={source}"));
            let (formula, city) = if source {
                ("=1+2", "**Tokyo** Bay")
            } else {
                ("3", "Tokyo Bay")
            };
            let expected = match format {
                "markdown" => format!("| {formula} | {city} |\n| ---: | :---: |\n"),
                "csv" => format!("{formula},{city}"),
                _ => format!("{formula}\t{city}"),
            };
            assert_eq!(text, expected, "{format}, source={source}");
            assert!(
                html.contains(&format!(">{formula}</td>")),
                "{format}, source={source}"
            );
            if format == "html" {
                // Another application reads the visible HTML table and cannot use Texcel's payload.
                let expected_city = if source {
                    "**Tokyo** Bay"
                } else {
                    "<b>Tokyo</b> Bay"
                };
                assert!(
                    html.contains(expected_city),
                    "HTML source={source} must contain {expected_city:?}"
                );
            }
            let cells = crate::clipboard::tests::read_owned_test_cells(&text, &html);
            assert_eq!(cells[0][0].formula_source, source, "{format}");
            assert_eq!(cells[0][0].input.source_text(), formula, "{format}");
            assert_eq!(cells[0][1].input.source_text(), city, "{format}");
            if format == "html" && !source {
                let runs = cells[0][1]
                    .rich_text
                    .as_ref()
                    .expect("Texcel receives the same rich text as other apps");
                assert!(runs.iter().any(|run| run.text == "Tokyo" && run.bold));
                assert!(runs.iter().any(|run| run.text == " Bay" && !run.bold));
            }
        }
    }
}

/// Clicks a labeled control after its popup or dialog has finished layout.
fn click_named(harness: &mut Harness<'_, TexcelApp>, label: &str) {
    for _ in 0..10 {
        let point = harness.query_all(By::new()).find_map(|node| {
            if node
                .accesskit_node()
                .label()
                .is_none_or(|name| name.trim_end() != label)
            {
                return None;
            }
            let rect = node.rect();
            (rect.is_positive() && harness.ctx.content_rect().contains(rect.center()))
                .then(|| rect.center())
        });
        if let Some(point) = point {
            scenario::click_point(harness, point);
            return;
        }
        scenario::send_batch(harness, []);
    }
    let buttons = harness
        .query_all(By::new())
        .filter_map(|node| node.accesskit_node().label())
        .filter(|name| !name.is_empty())
        .take(35)
        .collect::<Vec<_>>();
    panic!("the copy workflow shows {label:?} among {buttons:?}");
}
