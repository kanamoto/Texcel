use super::*;

// Prepare source data needed to keep Markdown table alignment during export.
pub(super) fn markdown_export_context(
    request: &ExportRequest,
    cancelled: Option<&AtomicBool>,
) -> Result<Option<MarkdownExportContext>, crate::i18n::Message> {
    let Some(ExportSource::Markdown(source)) = &request.source else {
        return Ok(None);
    };
    let mut changes = Vec::new();
    for (table_id, address, cell) in &request.changes {
        let Some(table_index) = request
            .tables
            .iter()
            .position(|table| table.id == *table_id)
        else {
            continue;
        };
        changes.push(crate::markdown_adapter::MarkdownCellChange {
            table: table_index,
            row: address.row,
            column: address.column,
            value: cell
                .as_ref()
                .map_or_else(String::new, |cell| cell.input.source_text()),
        });
    }
    let current_source = if changes.is_empty() {
        Arc::clone(source)
    } else {
        Arc::new(
            source
                .apply_changes(&changes)
                .map_err(crate::i18n::markdown_error_message)?,
        )
    };
    let text = std::str::from_utf8(current_source.source_bytes())
        .map_err(|_| crate::i18n::Message::new("status-markdown-utf8-invalid"))?;
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM;
    let mut context = MarkdownExportContext::default();
    let mut next_table = 0_usize;
    let mut active_table = None;
    let mut active_row = None;
    let mut next_row = 0_usize;
    let mut next_column = 0_usize;
    let mut active_column = None;
    let mut active_cell = None::<MarkdownCellBuilder>;

    for event in Parser::new_ext(text, options) {
        check_cancelled(cancelled)?;
        match event {
            Event::Start(Tag::Table(alignments)) => {
                let table_id = request
                    .tables
                    .get(next_table)
                    .ok_or_else(|| {
                        crate::i18n::Message::new("status-markdown-source-table-list-invalid")
                    })?
                    .id;
                next_table = next_table.saturating_add(1);
                active_table = Some(table_id);
                next_row = 0;
                context.alignments.insert(table_id, alignments);
            }
            Event::End(TagEnd::Table) => {
                active_table = None;
                active_row = None;
            }
            Event::Start(Tag::TableHead) => {
                active_row = Some(0);
                next_row = 1;
                next_column = 0;
            }
            Event::Start(Tag::TableRow) => {
                active_row = Some(next_row);
                next_row = next_row.saturating_add(1);
                next_column = 0;
            }
            Event::End(TagEnd::TableHead | TagEnd::TableRow) => active_row = None,
            Event::Start(Tag::TableCell) => {
                active_column = Some(next_column);
                next_column = next_column.saturating_add(1);
                active_cell = Some(MarkdownCellBuilder::default());
            }
            Event::End(TagEnd::TableCell) => {
                if let (Some(table_id), Some(row), Some(column), Some(builder)) = (
                    active_table,
                    active_row,
                    active_column.take(),
                    active_cell.take(),
                ) {
                    context
                        .cells
                        .insert((table_id, CellAddress { row, column }), builder.finish());
                }
            }
            Event::Start(Tag::Strong) => {
                if let Some(cell) = &mut active_cell {
                    cell.bold_depth = cell.bold_depth.saturating_add(1);
                }
            }
            Event::End(TagEnd::Strong) => {
                if let Some(cell) = &mut active_cell {
                    cell.bold_depth = cell.bold_depth.saturating_sub(1);
                }
            }
            Event::Start(Tag::Emphasis) => {
                if let Some(cell) = &mut active_cell {
                    cell.italic_depth = cell.italic_depth.saturating_add(1);
                }
            }
            Event::End(TagEnd::Emphasis) => {
                if let Some(cell) = &mut active_cell {
                    cell.italic_depth = cell.italic_depth.saturating_sub(1);
                }
            }
            Event::Start(Tag::Strikethrough) => {
                if let Some(cell) = &mut active_cell {
                    cell.strike_depth = cell.strike_depth.saturating_add(1);
                }
            }
            Event::End(TagEnd::Strikethrough) => {
                if let Some(cell) = &mut active_cell {
                    cell.strike_depth = cell.strike_depth.saturating_sub(1);
                }
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                ..
            }) => {
                if let Some(cell) = &mut active_cell {
                    cell.content.has_link = true;
                    cell.content.has_reference_link |= matches!(
                        link_type,
                        pulldown_cmark::LinkType::Reference
                            | pulldown_cmark::LinkType::Collapsed
                            | pulldown_cmark::LinkType::Shortcut
                            | pulldown_cmark::LinkType::ReferenceUnknown
                            | pulldown_cmark::LinkType::CollapsedUnknown
                            | pulldown_cmark::LinkType::ShortcutUnknown
                    );
                    cell.active_links.push((
                        cell.content.plain_text().chars().count(),
                        crate::document::CellHyperlink {
                            target: dest_url.to_string(),
                            tooltip: (!title.is_empty()).then(|| title.to_string()),
                            location: false,
                        },
                    ));
                }
            }
            Event::End(TagEnd::Link) => {
                if let Some(cell) = &mut active_cell
                    && let Some((start, link)) = cell.active_links.pop()
                {
                    let end = cell.content.plain_text().chars().count();
                    cell.content.links.push((start, end, link));
                }
            }
            Event::Start(Tag::Image { .. }) => {
                if let Some(cell) = &mut active_cell {
                    cell.content.has_image = true;
                }
            }
            Event::Text(text) => {
                if let Some(cell) = &mut active_cell {
                    cell.push_text(&text);
                }
            }
            Event::Code(text) => {
                if let Some(cell) = &mut active_cell {
                    cell.content.has_code = true;
                    cell.push_code(&text);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some(cell) = &mut active_cell {
                    cell.push_text("\n");
                }
            }
            Event::Html(text) | Event::InlineHtml(text) => {
                if let Some(cell) = &mut active_cell {
                    if is_html_line_break(&text) {
                        cell.push_text("\n");
                    } else {
                        cell.content.has_html = true;
                        cell.push_text(&text);
                    }
                }
            }
            Event::TaskListMarker(checked) => {
                if let Some(cell) = &mut active_cell {
                    cell.push_text(if checked { "[x]" } else { "[ ]" });
                }
            }
            _ => (),
        }
    }
    if next_table != request.tables.len() {
        return Err(crate::i18n::Message::new(
            "status-markdown-source-table-list-invalid",
        ));
    }
    Ok(Some(context))
}

// Check whether the Markdown source contains content outside its tables.
pub(super) fn markdown_has_non_table_content(source: &MarkdownDocument) -> bool {
    let Ok(text) = std::str::from_utf8(source.source_bytes()) else {
        return true;
    };
    let mut tables = source.tables.iter().collect::<Vec<_>>();
    tables.sort_by_key(|table| table.source_range.start);
    let mut cursor = 0_usize;
    for table in tables {
        if text
            .get(cursor..table.source_range.start)
            .is_some_and(|gap| !gap.trim().is_empty())
        {
            return true;
        }
        cursor = table.source_range.end;
    }
    text.get(cursor..)
        .is_some_and(|tail| !tail.trim().is_empty())
}

// Check whether a source line is an HTML line break.
pub(super) fn is_html_line_break(source: &str) -> bool {
    let normalized = source.trim().to_ascii_lowercase().replace(' ', "");
    matches!(normalized.as_str(), "<br>" | "<br/>")
}
