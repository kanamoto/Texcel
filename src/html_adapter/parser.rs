use super::finish::{close_row, finish_table, is_hidden_element, parse_span};
use super::grid::RowBuilder;
use super::tokenizer::{Token, TokenKind, tokenize};
use super::*;

#[path = "parser/inline.rs"]
mod inline;
use self::inline::*;

#[derive(Default)]
struct CellBuilder {
    tag_name: String,
    content_start: usize,
    element_start: usize,
    display_value: String,
    alignment: Option<String>,
    row_span: usize,
    column_span: usize,
    has_id: bool,
    rich_text: Vec<crate::document::CellTextRun>,
    format_stack: Vec<InlineFormat>,
    link_targets: std::collections::BTreeSet<String>,
    anchor_count: usize,
    link_tooltip: Option<String>,
    linked_characters: usize,
    has_unsupported_markup: bool,
}

#[derive(Clone, Default)]
struct InlineFormat {
    tag: String,
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    href: Option<String>,
}

#[derive(Default)]
pub(super) struct TableBuilder {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) opening_tag_range: std::ops::Range<usize>,
    pub(super) caption_start: Option<usize>,
    pub(super) caption_range: Option<std::ops::Range<usize>>,
    pub(super) caption_text: String,
    pub(super) caption_active: bool,
    pub(super) has_id: bool,
    pub(super) rows: Vec<RowBuilder>,
    pub(super) row: Option<RowBuilder>,
    cell: Option<CellBuilder>,
    group: usize,
    pub(super) nested: bool,
    pub(super) malformed: bool,
}

impl HtmlDocument {
    /// Tokenizes UTF-8 HTML while retaining every source byte between edits.
    pub fn parse(bytes: Vec<u8>) -> Result<Self, HtmlError> {
        if bytes.len() > MAX_HTML_BYTES {
            return Err(HtmlError::ResourceLimit);
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| HtmlError::InvalidUtf8)?;
        let tokens = tokenize(text)?;
        super::encoding::validate_charset_declarations(&tokens)?;
        let (tables, diagnostics) = parse_tables(&tokens, text)?;
        if tables.is_empty() {
            return Err(HtmlError::NoTables);
        }
        Ok(Self {
            bytes,
            tables,
            diagnostics,
        })
    }
}

// Builds table grids while leaving templates and foreign markup outside the editable model.
fn parse_tables(
    tokens: &[Token],
    source: &str,
) -> Result<(Vec<HtmlTable>, Vec<HtmlDiagnostic>), HtmlError> {
    parse_tables_with_cell_limit(tokens, source, MAX_HTML_CELLS)
}

// Keeps a small configurable limit for focused resource-boundary tests.
pub(super) fn parse_tables_with_cell_limit(
    tokens: &[Token],
    source: &str,
    cell_limit: usize,
) -> Result<(Vec<HtmlTable>, Vec<HtmlDiagnostic>), HtmlError> {
    let mut tables = Vec::new();
    let mut diagnostics = Vec::new();
    let mut active: Option<TableBuilder> = None;
    let mut nested_depth = 0_usize;
    let mut template_depth = 0_usize;
    let mut hidden_elements = Vec::<String>::new();
    let mut total_rows = 0_usize;
    let mut total_cells = 0_usize;
    let mut total_grid_cells = 0_usize;

    for token in tokens {
        if token.foreign || token.ignored {
            continue;
        }
        match &token.kind {
            TokenKind::Start { name, .. } if name == "template" => {
                template_depth = template_depth.saturating_add(1);
                continue;
            }
            TokenKind::End { name, .. } if name == "template" => {
                template_depth = template_depth.saturating_sub(1);
                continue;
            }
            _ if template_depth > 0 => continue,
            TokenKind::Start { name, .. } if is_hidden_element(name) => {
                if let Some(cell) = active.as_mut().and_then(|table| table.cell.as_mut()) {
                    cell.has_unsupported_markup = true;
                }
                hidden_elements.push(name.clone());
                continue;
            }
            TokenKind::End { name, .. } if hidden_elements.last() == Some(name) => {
                hidden_elements.pop();
                continue;
            }
            _ if !hidden_elements.is_empty() => continue,
            _ => {}
        }
        if let Some(table) = active.as_mut()
            && let TokenKind::Start { attrs, .. } = &token.kind
            && attrs.contains_key("id")
        {
            table.has_id = true;
        }
        match &token.kind {
            TokenKind::Start { name, attrs, .. }
                if is_inline_format(name) || name == "a" || name == "span" =>
            {
                if let Some(cell) = active.as_mut().and_then(|table| table.cell.as_mut()) {
                    if name == "a" {
                        let href = attrs.get("href").cloned();
                        cell.anchor_count = cell.anchor_count.saturating_add(1);
                        if cell.link_tooltip.is_none() {
                            cell.link_tooltip = attrs.get("title").cloned();
                        }
                        if let Some(href) = &href {
                            cell.link_targets.insert(href.clone());
                        } else {
                            cell.has_unsupported_markup = true;
                        }
                        cell.format_stack.push(InlineFormat {
                            tag: name.clone(),
                            href,
                            ..InlineFormat::default()
                        });
                    } else {
                        cell.format_stack.push(format_for_tag(name));
                    }
                    if attrs.contains_key("style") {
                        cell.has_unsupported_markup = true;
                    }
                }
            }
            TokenKind::Start { name, attrs, .. }
                if active
                    .as_ref()
                    .and_then(|table| table.cell.as_ref())
                    .is_some()
                    && !is_basic_inline_element(name)
                    && !is_table_structure_element(name) =>
            {
                if let Some(cell) = active.as_mut().and_then(|table| table.cell.as_mut()) {
                    cell.has_unsupported_markup = true;
                    if attrs.contains_key("style") || attrs.contains_key("class") {
                        cell.has_unsupported_markup = true;
                    }
                }
            }
            TokenKind::End { name, .. }
                if active
                    .as_ref()
                    .and_then(|table| table.cell.as_ref())
                    .is_some_and(|cell| {
                        cell.format_stack.iter().any(|format| format.tag == *name)
                    }) =>
            {
                if let Some(cell) = active.as_mut().and_then(|table| table.cell.as_mut())
                    && let Some(position) = cell
                        .format_stack
                        .iter()
                        .rposition(|format| format.tag == *name)
                {
                    cell.format_stack.truncate(position);
                }
            }
            TokenKind::Start {
                name, range, attrs, ..
            } if name == "table" => {
                if let Some(table) = &mut active {
                    table.nested = true;
                    table.has_id |= attrs.contains_key("id");
                    nested_depth = nested_depth.saturating_add(1);
                } else {
                    active = Some(TableBuilder {
                        start: range.start,
                        end: range.end,
                        opening_tag_range: range.clone(),
                        has_id: attrs.contains_key("id"),
                        ..TableBuilder::default()
                    });
                }
            }
            TokenKind::End { name, range } if name == "table" => {
                if nested_depth > 0 {
                    nested_depth -= 1;
                    continue;
                }
                if let Some(mut table) = active.take() {
                    close_cell(&mut table, source, range.start, range.start)?;
                    close_row(&mut table, &mut total_rows, range.start, range.start)?;
                    if table.caption_active {
                        table.malformed = true;
                        if let Some(start) = table.caption_start.take() {
                            table.caption_range = Some(start..range.start);
                        }
                    }
                    table.end = range.end;
                    finish_table(
                        table,
                        &mut tables,
                        &mut diagnostics,
                        &mut total_grid_cells,
                        cell_limit,
                    )?;
                }
            }
            _ if nested_depth > 0 => continue,
            TokenKind::Start { name, range, .. } if name == "caption" => {
                if let Some(table) = &mut active {
                    if table.caption_active || table.caption_range.is_some() {
                        table.malformed = true;
                    }
                    table.caption_active = true;
                    table.caption_start = Some(range.end);
                }
            }
            TokenKind::End { name, range } if name == "caption" => {
                if let Some(table) = &mut active
                    && let Some(start) = table.caption_start.take()
                {
                    table.caption_range = Some(start..range.start);
                    table.caption_active = false;
                }
            }
            TokenKind::Start { name, range, .. } | TokenKind::End { name, range }
                if matches!(name.as_str(), "thead" | "tbody" | "tfoot") =>
            {
                if let Some(table) = &mut active {
                    close_cell(table, source, range.start, range.start)?;
                    close_row(table, &mut total_rows, range.start, range.start)?;
                    table.group = table.group.saturating_add(1);
                }
            }
            TokenKind::Start {
                name, range, attrs, ..
            } if name == "tr" => {
                if let Some(table) = &mut active {
                    close_cell(table, source, range.start, range.start)?;
                    close_row(table, &mut total_rows, range.start, range.start)?;
                    table.row = Some(RowBuilder {
                        group: table.group,
                        range: range.clone(),
                        content_range: range.end..range.end,
                        has_id: attrs.contains_key("id"),
                        ..RowBuilder::default()
                    });
                }
            }
            TokenKind::End { name, range } if name == "tr" => {
                if let Some(table) = &mut active {
                    close_cell(table, source, range.start, range.start)?;
                    close_row(table, &mut total_rows, range.end, range.start)?;
                }
            }
            TokenKind::Start {
                name, range, attrs, ..
            } if name == "td" || name == "th" => {
                if let Some(table) = &mut active {
                    total_cells = total_cells.saturating_add(1);
                    if total_cells > cell_limit {
                        return Err(HtmlError::ResourceLimit);
                    }
                    close_cell(table, source, range.start, range.start)?;
                    if table.row.is_none() {
                        table.malformed = true;
                        table.row = Some(RowBuilder {
                            group: table.group,
                            range: range.clone(),
                            content_range: range.start..range.start,
                            ..RowBuilder::default()
                        });
                    }
                    let row_span = parse_span(attrs.get("rowspan")).unwrap_or(1);
                    let column_span = parse_span(attrs.get("colspan")).unwrap_or(1);
                    table.cell = Some(CellBuilder {
                        tag_name: name.clone(),
                        content_start: range.end,
                        element_start: range.start,
                        row_span,
                        column_span,
                        alignment: attrs
                            .get("align")
                            .map(|alignment| alignment.to_ascii_lowercase())
                            .filter(|alignment| {
                                matches!(alignment.as_str(), "left" | "center" | "right")
                            }),
                        has_unsupported_markup: attrs.get("align").is_some_and(|alignment| {
                            !matches!(
                                alignment.to_ascii_lowercase().as_str(),
                                "left" | "center" | "right"
                            )
                        }),
                        has_id: attrs.contains_key("id"),
                        ..CellBuilder::default()
                    });
                }
            }
            TokenKind::End { name, range } if name == "td" || name == "th" => {
                if let Some(table) = &mut active {
                    if table
                        .cell
                        .as_ref()
                        .is_some_and(|cell| cell.tag_name != *name)
                    {
                        table.malformed = true;
                    }
                    close_cell(table, source, range.start, range.end)?;
                }
            }
            TokenKind::Start { name, .. } if name == "br" => {
                if let Some(table) = &mut active
                    && let Some(cell) = &mut table.cell
                {
                    cell.display_value.push('\n');
                    push_inline_text(cell, "\n");
                }
            }
            TokenKind::Text(value) => {
                if let Some(table) = &mut active
                    && let Some(cell) = &mut table.cell
                {
                    cell.display_value.push_str(value);
                    push_inline_text(cell, value);
                } else if let Some(table) = &mut active
                    && table.caption_active
                {
                    table.caption_text.push_str(value);
                }
            }
            TokenKind::Error => {
                if let Some(table) = &mut active {
                    table.malformed = true;
                }
            }
            _ => {}
        }
    }

    if let Some(mut table) = active {
        table.malformed = true;
        close_cell(&mut table, source, source.len(), source.len())?;
        close_row(&mut table, &mut total_rows, source.len(), source.len())?;
        if table.caption_active
            && let Some(start) = table.caption_start.take()
        {
            table.caption_range = Some(start..source.len());
        }
        table.end = source.len();
        finish_table(
            table,
            &mut tables,
            &mut diagnostics,
            &mut total_grid_cells,
            cell_limit,
        )?;
    }
    Ok((tables, diagnostics))
}

// Closes one cell at an explicit or implied HTML boundary.
fn close_cell(
    table: &mut TableBuilder,
    source: &str,
    content_end: usize,
    element_end: usize,
) -> Result<(), HtmlError> {
    let Some(cell) = table.cell.take() else {
        return Ok(());
    };
    let range = cell.content_start..content_end;
    let value = source
        .get(range.clone())
        .ok_or(HtmlError::OutputMismatch)?
        .to_owned();
    let display_character_count = cell.display_value.chars().count();
    let html_cell = HtmlCell {
        value,
        display_value: cell.display_value,
        alignment: cell.alignment,
        source_range: range,
        element_range: cell.element_start..element_end,
        tag_name: cell.tag_name,
        has_id: cell.has_id,
        rich_text: has_rich_style(&cell.rich_text).then_some(cell.rich_text),
        hyperlink: (cell.anchor_count == 1
            && cell.link_targets.len() == 1
            && cell.linked_characters == display_character_count)
            .then(|| crate::document::CellHyperlink {
                target: cell.link_targets.iter().next().cloned().unwrap_or_default(),
                tooltip: cell.link_tooltip.clone(),
                location: false,
            }),
        has_partial_link: cell.anchor_count > 0
            && !(cell.anchor_count == 1
                && cell.link_targets.len() == 1
                && cell.linked_characters == display_character_count),
        has_unsupported_markup: cell.has_unsupported_markup,
        row_span: cell.row_span,
        column_span: cell.column_span,
        placeholder: false,
    };
    if let Some(row) = &mut table.row {
        row.cells.push(html_cell);
    }
    Ok(())
}
