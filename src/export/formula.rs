use super::*;

// Return the formula text that should be written for one cell.
pub(super) fn cell_formula_source(
    table: &ExportTable,
    address: CellAddress,
    cell: &Cell,
) -> Option<String> {
    formula_source(&cell.input, table.string_cells.contains(&address))
}

// Convert an exported value to plain text without changing its type.
pub(super) fn plain_output_text(
    request: &ExportRequest,
    markdown_context: Option<&MarkdownExportContext>,
    value: &str,
    cell: &Cell,
    entries: &mut Vec<ConversionEntry>,
    table: &ExportTable,
    address: CellAddress,
) -> (String, bool) {
    if table.hyperlink(address).is_some()
        && !(request.options.format == ExportFormat::Markdown
            && request.options.modifiers == ModifierOutput::Convert)
    {
        add_omission_with_message(
            entries,
            table,
            address,
            "セルリンクを出力形式で保持できないため表示文字列だけを出力しました",
            crate::i18n::Message::new("report-reason-cell-link-text"),
        );
    }
    if cell_formula_source(table, address, cell).is_some() {
        return (value.to_owned(), false);
    }
    if matches!(request.source, Some(ExportSource::Markdown(_))) {
        let Some(content) =
            markdown_context.and_then(|context| context.cells.get(&(table.id, address)))
        else {
            return (value.to_owned(), false);
        };
        let plain = content.plain_text();
        let changed = content.has_link
            || content.has_image
            || content.has_code
            || content.has_html
            || plain != value;
        if changed {
            report_markdown_text_conversion(entries, table, address, content);
            add_conversion_with_message(
                entries,
                table,
                address,
                "Markdown修飾を表示文字列に変換しました",
                crate::i18n::Message::new("report-reason-markdown-modifier-text"),
            );
        }
        return (plain, changed);
    }
    if let Some(runs) = table.rich_text(address) {
        return (runs.iter().map(|run| run.text.as_str()).collect(), true);
    }
    (value.to_owned(), false)
}
