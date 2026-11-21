use super::*;

#[test]
// Uses a dark code chip for bright text and a light chip for dark text.
fn inline_code_background_tracks_the_cell_text_color() {
    let mut dark_job = egui::text::LayoutJob::default();
    append_markdown_layout_text(
        &mut dark_job,
        "literal",
        Color32::WHITE,
        14.0,
        MarkdownInlineStyle {
            code: true,
            ..MarkdownInlineStyle::default()
        },
        false,
    );
    assert_eq!(
        dark_job.sections[0].format.background,
        Color32::from_gray(48)
    );

    let mut light_job = egui::text::LayoutJob::default();
    append_markdown_layout_text(
        &mut light_job,
        "literal",
        Color32::BLACK,
        14.0,
        MarkdownInlineStyle {
            code: true,
            ..MarkdownInlineStyle::default()
        },
        false,
    );
    assert_eq!(
        light_job.sections[0].format.background,
        Color32::from_gray(235)
    );
}
