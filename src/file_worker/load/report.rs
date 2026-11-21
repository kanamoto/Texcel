use super::*;

/// Reports each XLSX filter, input rule, and conditional format, which Markdown cannot keep.
pub(in crate::file_worker) fn xlsx_rule_report_entries(
    source: &XlsxDocument,
) -> Vec<ConversionEntry> {
    let mut entries = Vec::new();
    for (index, sheet) in source.sheets.iter().enumerate() {
        for loss in &sheet.rule_losses {
            let (reason, key) = match loss.kind {
                crate::xlsx_adapter::XlsxRuleKind::Filter => (
                    "XLSX filters cannot be kept in Markdown.",
                    "report-reason-xlsx-filter-not-text",
                ),
                crate::xlsx_adapter::XlsxRuleKind::Validation => (
                    "XLSX input rules cannot be kept in Markdown.",
                    "report-reason-xlsx-validation-not-text",
                ),
                crate::xlsx_adapter::XlsxRuleKind::ConditionalFormat => (
                    "XLSX conditional formats cannot be kept in Markdown.",
                    "report-reason-xlsx-conditional-not-text",
                ),
            };
            entries.push(
                ConversionEntry::new(
                    ConversionClass::Omitted,
                    Some(index + 1),
                    Some(sheet.name.clone()),
                    loss.range.map(rule_range),
                    reason,
                )
                .with_message(crate::i18n::Message::new(key)),
            );
        }
    }
    entries
}

/// Formats the range used by one imported rule.
pub(in crate::file_worker) fn rule_range(range: crate::document::CellRange) -> String {
    let (start, end) = range.normalized();
    if start == end {
        crate::xlsx_adapter::cell_reference(start)
    } else {
        format!(
            "{}:{}",
            crate::xlsx_adapter::cell_reference(start),
            crate::xlsx_adapter::cell_reference(end)
        )
    }
}
