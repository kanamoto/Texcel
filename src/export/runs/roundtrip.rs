use super::*;

const MARKDOWN_GFM_RETRY_LIMIT: usize = 32;

/// Builds markup and reports unsupported run details.
pub fn markdown_runs_as_markup_checked(runs: &[XlsxTextRun]) -> (String, bool, bool) {
    let normalized = merge_markdown_runs(runs);
    if markdown_cell_markup_matches(&normalized) {
        return (markdown_runs_as_markup(&normalized), false, false);
    }

    let mut retained = normalized
        .iter()
        .map(|run| XlsxTextRun {
            text: run.text.clone(),
            code: run.code,
            hyperlink: run.hyperlink.clone(),
            ..XlsxTextRun::default()
        })
        .collect::<Vec<_>>();
    let mut omitted = false;
    let mut retry_count = 0;
    let mut retry_limit_reached = false;
    for (index, run) in normalized.iter().enumerate() {
        if !(run.bold || run.italic || run.strike) {
            continue;
        }
        if retry_count >= MARKDOWN_GFM_RETRY_LIMIT {
            retry_limit_reached = true;
            omitted = true;
            break;
        }
        retry_count += 1;
        let mut trial = retained.clone();
        trial[index].bold = run.bold;
        trial[index].italic = run.italic;
        trial[index].strike = run.strike;
        if markdown_cell_markup_matches(&trial) {
            retained = trial;
        } else {
            omitted = true;
        }
    }

    let mut matches = if retry_limit_reached {
        false
    } else if retry_count < MARKDOWN_GFM_RETRY_LIMIT {
        retry_count += 1;
        markdown_cell_markup_matches(&retained)
    } else {
        retry_limit_reached = true;
        false
    };
    if retry_count == MARKDOWN_GFM_RETRY_LIMIT && !matches {
        retry_limit_reached = true;
    }
    while !matches && !retry_limit_reached {
        let Some(index) = retained
            .iter()
            .rposition(|run| run.bold || run.italic || run.strike)
        else {
            break;
        };
        retained[index].bold = false;
        retained[index].italic = false;
        retained[index].strike = false;
        omitted = true;
        if retry_count >= MARKDOWN_GFM_RETRY_LIMIT {
            retry_limit_reached = true;
            break;
        }
        retry_count += 1;
        matches = markdown_cell_markup_matches(&retained);
        if retry_count == MARKDOWN_GFM_RETRY_LIMIT && !matches {
            retry_limit_reached = true;
        }
    }
    if retry_limit_reached {
        for run in &mut retained {
            run.bold = false;
            run.italic = false;
            run.strike = false;
        }
        omitted = true;
    }
    (
        markdown_runs_as_markup(&retained),
        omitted,
        retry_limit_reached,
    )
}

/// Reports a Markdown run that cannot be represented.
pub fn report_markdown_run_omission(
    entries: &mut Vec<ConversionEntry>,
    table: &ExportTable,
    address: CellAddress,
    retry_limit_reached: bool,
) {
    let (reason, message_key) = if retry_limit_reached {
        (
            "セルの原文を文字列として出力し、部分修飾を省略しました",
            "report-reason-markdown-cell-source",
        )
    } else {
        (
            "GFMで元の修飾へ再解析できない文字runは表示文字列として出力し、修飾を省略しました",
            "report-reason-markdown-runs-text-only",
        )
    };
    add_omission_with_message(
        entries,
        table,
        address,
        reason,
        crate::i18n::Message::new(message_key),
    );
}

/// Checks whether an output cell preserves the source Markdown runs.
pub fn markdown_cell_markup_matches(runs: &[XlsxTextRun]) -> bool {
    let cell = escape_table_cell(&markdown_runs_as_markup(runs));
    let source = format!("| {cell} |\n| --- |\n");
    let mut builder = None::<MarkdownCellBuilder>;
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_GFM;
    for event in Parser::new_ext(&source, options) {
        match event {
            Event::Start(Tag::TableCell) => builder = Some(MarkdownCellBuilder::default()),
            Event::End(TagEnd::TableCell) => {
                if let Some(builder) = builder.take() {
                    return markdown_runs_match_expected(&builder.finish().runs, runs);
                }
            }
            Event::Start(Tag::Strong) => {
                if let Some(builder) = &mut builder {
                    builder.bold_depth = builder.bold_depth.saturating_add(1);
                }
            }
            Event::End(TagEnd::Strong) => {
                if let Some(builder) = &mut builder {
                    builder.bold_depth = builder.bold_depth.saturating_sub(1);
                }
            }
            Event::Start(Tag::Emphasis) => {
                if let Some(builder) = &mut builder {
                    builder.italic_depth = builder.italic_depth.saturating_add(1);
                }
            }
            Event::End(TagEnd::Emphasis) => {
                if let Some(builder) = &mut builder {
                    builder.italic_depth = builder.italic_depth.saturating_sub(1);
                }
            }
            Event::Start(Tag::Strikethrough) => {
                if let Some(builder) = &mut builder {
                    builder.strike_depth = builder.strike_depth.saturating_add(1);
                }
            }
            Event::End(TagEnd::Strikethrough) => {
                if let Some(builder) = &mut builder {
                    builder.strike_depth = builder.strike_depth.saturating_sub(1);
                }
            }
            Event::Start(Tag::Link {
                dest_url, title, ..
            }) => {
                if let Some(builder) = &mut builder {
                    let start = builder.content.plain_text().chars().count();
                    builder.active_links.push((
                        start,
                        crate::document::CellHyperlink {
                            target: dest_url.to_string(),
                            tooltip: (!title.is_empty()).then(|| title.to_string()),
                            location: false,
                        },
                    ));
                }
            }
            Event::End(TagEnd::Link) => {
                if let Some(builder) = &mut builder
                    && let Some((start, link)) = builder.active_links.pop()
                {
                    let end = builder.content.plain_text().chars().count();
                    builder.content.links.push((start, end, link));
                }
            }
            Event::Text(text) => {
                if let Some(builder) = &mut builder {
                    builder.push_text(&text);
                }
            }
            Event::Code(text) => {
                if let Some(builder) = &mut builder {
                    builder.push_code(&text);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some(builder) = &mut builder {
                    builder.push_text("\n");
                }
            }
            Event::InlineHtml(html) if is_html_line_break(&html) => {
                if let Some(builder) = &mut builder {
                    builder.push_text("\n");
                }
            }
            _ => (),
        }
    }
    false
}

/// Checks output runs against the expected text and styles.
pub(super) fn markdown_runs_match_expected(
    actual: &[XlsxTextRun],
    expected: &[XlsxTextRun],
) -> bool {
    let mut expected_expanded = Vec::new();
    for run in merge_markdown_runs(expected) {
        let start = run
            .text
            .char_indices()
            .find(|(_, character)| !character.is_whitespace())
            .map_or(run.text.len(), |(index, _)| index);
        let end = run
            .text
            .char_indices()
            .rev()
            .find(|(_, character)| !character.is_whitespace())
            .map_or(start, |(index, character)| index + character.len_utf8());
        if start == end {
            append_markdown_run(
                &mut expected_expanded,
                run.text,
                false,
                false,
                false,
                false,
                None,
            );
        } else if run.code && run.text[start..end].contains('\n') {
            append_markdown_run(
                &mut expected_expanded,
                run.text[..start].to_owned(),
                false,
                false,
                false,
                false,
                None,
            );
            let parts = run.text[start..end].split('\n').collect::<Vec<_>>();
            for (index, part) in parts.iter().enumerate() {
                append_markdown_run(
                    &mut expected_expanded,
                    (*part).to_owned(),
                    run.bold,
                    run.italic,
                    run.strike,
                    true,
                    run.hyperlink.clone(),
                );
                if index + 1 < parts.len() {
                    append_markdown_run(
                        &mut expected_expanded,
                        "\n".to_owned(),
                        false,
                        false,
                        false,
                        false,
                        run.hyperlink.clone(),
                    );
                }
            }
            append_markdown_run(
                &mut expected_expanded,
                run.text[end..].to_owned(),
                false,
                false,
                false,
                false,
                None,
            );
        } else {
            append_markdown_run(
                &mut expected_expanded,
                run.text[..start].to_owned(),
                false,
                false,
                false,
                false,
                None,
            );
            append_markdown_run(
                &mut expected_expanded,
                run.text[start..end].to_owned(),
                run.bold,
                run.italic,
                run.strike,
                run.code,
                run.hyperlink.clone(),
            );
            append_markdown_run(
                &mut expected_expanded,
                run.text[end..].to_owned(),
                false,
                false,
                false,
                false,
                None,
            );
        }
    }
    actual.len() == expected_expanded.len()
        && actual
            .iter()
            .zip(expected_expanded)
            .all(|(actual, expected)| {
                actual.text == expected.text
                    && actual.bold == expected.bold
                    && actual.italic == expected.italic
                    && actual.strike == expected.strike
                    && actual.code == expected.code
                    && actual.hyperlink == expected.hyperlink
            })
}

/// Adds a non-empty Markdown text run.
pub(super) fn append_markdown_run(
    runs: &mut Vec<XlsxTextRun>,
    text: String,
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    hyperlink: Option<crate::document::CellHyperlink>,
) {
    if text.is_empty() {
        return;
    }
    if let Some(previous) = runs.last_mut()
        && previous.bold == bold
        && previous.italic == italic
        && previous.strike == strike
        && previous.code == code
        && previous.hyperlink == hyperlink
    {
        previous.text.push_str(&text);
    } else {
        runs.push(XlsxTextRun {
            text,
            bold,
            italic,
            strike,
            code,
            hyperlink,
            ..XlsxTextRun::default()
        });
    }
}
