use super::*;

/// Builds the text shown or saved for a conversion report.
pub(super) fn render_report(
    language: crate::i18n::Language,
    title_key: &str,
    file_name: &str,
    timestamp: &str,
    format: &str,
    export: Option<(&u64, &crate::i18n::Message)>,
    entries: &[ConversionEntry],
) -> String {
    let summary = summarize(entries);
    let mut text = format!("{}\n", crate::i18n::text(language, title_key));
    if !file_name.is_empty() {
        text.push_str(
            &crate::i18n::Message::new("report-file-line")
                .with_text("file", file_name)
                .resolve(language),
        );
        text.push('\n');
    }
    if !timestamp.is_empty() {
        text.push_str(
            &crate::i18n::Message::new("report-timestamp-line")
                .with_text("timestamp", timestamp)
                .resolve(language),
        );
        text.push('\n');
    }
    if let Some((revision, settings)) = export {
        text.push_str(
            &crate::i18n::Message::new("report-document-version-line")
                .with_number("revision", *revision as i64)
                .resolve(language),
        );
        text.push('\n');
        text.push_str(
            &crate::i18n::Message::new("report-settings-line")
                .with_message("settings", settings.clone())
                .resolve(language),
        );
        text.push('\n');
    }
    text.push_str(
        &crate::i18n::Message::new("report-format-summary")
            .with_text("format", format)
            .with_number("retained", summary.retained as i64)
            .with_number("converted", summary.converted as i64)
            .with_number("omitted", summary.omitted as i64)
            .with_number("unprocessable", summary.unprocessable as i64)
            .resolve(language),
    );
    text.push('\n');
    for entry in entries {
        let sheet = match (&entry.sheet_order, &entry.sheet_name) {
            (Some(order), Some(name)) => crate::i18n::Message::new("report-sheet-named")
                .with_number("order", *order as i64)
                .with_text("name", name)
                .resolve(language),
            (Some(order), None) => crate::i18n::Message::new("report-sheet")
                .with_number("order", *order as i64)
                .resolve(language),
            (None, Some(name)) => crate::i18n::Message::new("report-sheet-name-only")
                .with_text("name", name)
                .resolve(language),
            (None, None) => crate::i18n::text(language, "report-workbook"),
        };
        let range = entry
            .range
            .as_deref()
            .map_or(String::new(), |range| format!(" {range}"));
        let reason = entry
            .message
            .as_ref()
            .map_or_else(|| entry.reason.clone(), |message| message.resolve(language));
        text.push_str(
            &crate::i18n::Message::new("report-entry")
                .with_text("class", entry.class.label(language))
                .with_text("location", sheet)
                .with_text("range", range)
                .with_text("reason", reason)
                .resolve(language),
        );
        text.push('\n');
    }
    text
}
