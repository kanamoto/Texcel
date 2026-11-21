use super::*;

// Format rich text as Markdown while checking that the text can round-trip.
pub(super) fn markdown_markup(
    context: &ExportBuildContext<'_, '_>,
    markdown_context: Option<&MarkdownExportContext>,
    cell: &Cell,
    value: &str,
    mode: ModifierOutput,
    entries: &mut Vec<ConversionEntry>,
    (table, address): (&ExportTable, CellAddress),
) -> String {
    let request = context.request;
    let original_style = source_xlsx_style(context, table, address);
    let cannot_represent_style = original_style.as_ref().is_some_and(|style| {
        style.font_name.is_some()
            || style.font_size.is_some()
            || style.fill_color.is_some()
            || style.number_format.is_some()
            || style.vertical_alignment.is_some()
            || style.wrap_text
            || style.top_border.is_some()
            || style.bottom_border.is_some()
            || style.left_border.is_some()
            || style.right_border.is_some()
    }) || table.rich_text(address).is_some_and(|runs| {
        runs.iter()
            .any(|run| run.font_name.is_some() || run.font_size.is_some())
    });
    if mode == ModifierOutput::Convert && cannot_represent_style {
        add_formatting_omission_with_message(
            entries,
            table,
            address,
            "サイズ・背景・配置・罫線・数値表示書式はMarkdownで表せないため省略しました",
            crate::i18n::Message::new("report-reason-markdown-cell-style"),
        );
    }
    if mode == ModifierOutput::TextOnly {
        let has_rich_style = table.rich_text(address).is_some_and(|runs| {
            runs.iter().any(|run| {
                run.bold
                    || run.italic
                    || run.strike
                    || run.font_name.is_some()
                    || run.font_size.is_some()
                    || run.font_color.is_some()
            })
        });
        let has_cell_style = original_style.as_ref().is_some_and(|style| {
            style.bold
                || style.italic
                || style.strike
                || style.font_color.is_some()
                || style.font_name.is_some()
                || style.font_size.is_some()
                || style.fill_color.is_some()
                || style.number_format.is_some()
                || style.vertical_alignment.is_some()
                || style.wrap_text
                || style.top_border.is_some()
                || style.bottom_border.is_some()
                || style.left_border.is_some()
                || style.right_border.is_some()
        });
        if has_rich_style || has_cell_style {
            add_omission_with_message(
                entries,
                table,
                address,
                "文字修飾を選択設定に従い出力せず省略しました",
                crate::i18n::Message::new("report-reason-modifier-text-only"),
            );
        }
        let (plain, _) = plain_output_text(
            request,
            markdown_context,
            value,
            cell,
            entries,
            table,
            address,
        );
        return escape_table_cell(&escape_markdown_text(&plain));
    }
    if let Some(runs) = table.rich_text(address) {
        let style = source_xlsx_style(context, table, address);
        let mut effective_runs = runs
            .iter()
            .map(|run| XlsxTextRun {
                text: run.text.clone(),
                bold: run.bold,
                italic: run.italic,
                strike: run.strike,
                code: run.code,
                hyperlink: run.hyperlink.clone(),
                font_name: run.font_name.clone(),
                font_size: run.font_size,
                font_color: run.font_color.clone(),
            })
            .collect::<Vec<_>>();
        let literal_value = match &cell.input {
            CellInput::Text(source) => {
                let value = crate::document::literal_text_value(source);
                (value != source).then_some(value)
            }
            _ => None,
        };
        let run_value = effective_runs
            .iter()
            .map(|run| run.text.as_str())
            .collect::<String>();
        if literal_value.is_some_and(|value| value == run_value)
            && let Some(value) = literal_value
        {
            let encoded = crate::document::encode_literal_text(value);
            if let Some(marker) = encoded.strip_suffix(value)
                && !marker.is_empty()
                && let Some(first) = effective_runs.first_mut()
            {
                first.text.insert_str(0, marker);
            }
        }
        for run in &mut effective_runs {
            run.bold |= style.as_ref().is_some_and(|style| style.bold);
            run.italic |= style.as_ref().is_some_and(|style| style.italic);
            run.strike |= style.as_ref().is_some_and(|style| style.strike);
        }
        let (output, omitted, retry_limit_reached) =
            markdown_runs_as_markup_checked(&effective_runs);
        if omitted {
            report_markdown_run_omission(entries, table, address, retry_limit_reached);
        }
        for run in runs {
            if run.font_color.is_some() {
                add_formatting_omission_with_message(
                    entries,
                    table,
                    address,
                    "文字色をMarkdownで表せないため省略しました",
                    crate::i18n::Message::new("report-reason-markdown-font-color"),
                );
            }
        }
        if style
            .as_ref()
            .is_some_and(|style| style.font_color.is_some())
        {
            add_formatting_omission_with_message(
                entries,
                table,
                address,
                "文字色をMarkdownで表せないため省略しました",
                crate::i18n::Message::new("report-reason-markdown-font-color"),
            );
        }
        let output = table.hyperlink(address).map_or(output.clone(), |link| {
            markdown_link_markup(link, &output, entries, table, address)
        });
        return escape_table_cell(&output);
    }
    let style = source_xlsx_style(context, table, address);
    let mut content = escape_markdown_text(value);
    if style.as_ref().is_some_and(|style| style.strike) {
        content = format!("~~{content}~~");
    }
    if style.as_ref().is_some_and(|style| style.italic) {
        content = format!("*{content}*");
    }
    if style.as_ref().is_some_and(|style| style.bold) {
        content = format!("**{content}**");
    }
    if style
        .as_ref()
        .is_some_and(|style| style.font_color.is_some())
    {
        add_formatting_omission_with_message(
            entries,
            table,
            address,
            "文字色をMarkdownで表せないため省略しました",
            crate::i18n::Message::new("report-reason-markdown-font-color"),
        );
    }
    let content = table.hyperlink(address).map_or(content.clone(), |link| {
        markdown_link_markup(link, &content, entries, table, address)
    });
    escape_table_cell(&content)
}

// Choose the XLSX style that best represents a cell’s source formatting.
pub(super) fn source_xlsx_style(
    context: &ExportBuildContext<'_, '_>,
    table: &ExportTable,
    address: CellAddress,
) -> Option<XlsxStyle> {
    if let Some(cell) = context.cell_changes.get(&(table.id, address)) {
        return cell.map(|_| crate::xlsx_adapter::xlsx_style(&table.style(address)));
    }
    let Some(ExportSource::Xlsx(source)) = &context.request.source else {
        return None;
    };
    source
        .sheets
        .iter()
        .find(|sheet| sheet.table_id == table.id)?
        .cells
        .get(&address)
        .map(|cell| cell.style.clone())
}

// Format an external link using Markdown link syntax.
pub(super) fn markdown_link_markup(
    link: &crate::document::CellHyperlink,
    text: &str,
    entries: &mut Vec<ConversionEntry>,
    table: &ExportTable,
    address: CellAddress,
) -> String {
    if link.location {
        add_omission_with_message(
            entries,
            table,
            address,
            "内部セルリンクをMarkdown URLへ変換できないため表示文字列だけを出力しました",
            crate::i18n::Message::new("report-reason-markdown-internal-link"),
        );
        return text.to_owned();
    }
    let target = link
        .target
        .replace('(', "%28")
        .replace(')', "%29")
        .replace(' ', "%20");
    match link.tooltip.as_deref() {
        Some(title) if !title.is_empty() => {
            let title = title.replace('\\', "\\\\").replace('"', "\\\"");
            format!("[{text}]({target} \"{title}\")")
        }
        _ => format!("[{text}]({target})"),
    }
}

// Record formatting and link details omitted for formula cells.
pub(super) fn report_formula_markdown_losses(
    context: &ExportBuildContext<'_, '_>,
    table: &ExportTable,
    address: CellAddress,
    entries: &mut Vec<ConversionEntry>,
) {
    let original_style = source_xlsx_style(context, table, address);
    if table.rich_text(address).is_some()
        || table.style(address) != crate::document::CellStyle::default()
        || original_style.is_some_and(|style| style != XlsxStyle::default())
    {
        add_formatting_omission_with_message(
            entries,
            table,
            address,
            "数式原文を出力するためセルの文字書式を省略しました",
            crate::i18n::Message::new("report-reason-formula-style-omitted"),
        );
    }
    if table.hyperlink(address).is_some() {
        add_omission_with_message(
            entries,
            table,
            address,
            "数式原文を出力するためセルリンクを省略しました",
            crate::i18n::Message::new("report-reason-formula-link-omitted"),
        );
    }
}
