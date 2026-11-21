use super::*;

/// Keeps rendered text and formatting separate from raw Markdown source.
pub(super) struct MarkdownCopyContent {
    pub(super) plain_text: String,
    pub(super) has_features: bool,
    pub(super) rich_text: Vec<crate::document::CellTextRun>,
}

/// Collects the source and cache used to resolve one Markdown cell for copying.
pub(super) struct MarkdownCopyContext<'a> {
    pub(super) document: &'a Document,
    pub(super) source: &'a crate::markdown_adapter::MarkdownDocument,
    pub(super) table_index: Option<usize>,
    pub(super) table_id: TableId,
    pub(super) language: crate::i18n::Language,
    pub(super) cache: &'a BTreeMap<MarkdownCellDisplayKey, CachedMarkdownCellDisplay>,
}

/// Keeps rendered text separate while retaining the cell's raw Markdown source.
pub(super) fn attach_markdown_copy_content(
    cell: &mut crate::clipboard::ClipboardCell,
    content: &MarkdownCopyContent,
) {
    cell.raw_markdown_source = true;
    cell.markdown_plain_text = Some(content.plain_text.clone());
    cell.markdown_has_features = content.has_features;
}

/// Resolves source text and supported Markdown features for an owned clipboard cell.
pub(super) fn markdown_source_cell_copy_content(
    context: MarkdownCopyContext<'_>,
    address: CellAddress,
    raw: &str,
    pending: &mut Vec<(CellAddress, CellInputRevision, String)>,
) -> Option<MarkdownCopyContent> {
    let MarkdownCopyContext {
        document,
        source,
        table_index,
        table_id,
        language,
        cache,
    } = context;
    let table_index = table_index?;
    if !document.cell_input_is_dirty(table_id, address) {
        let Some(display) = source
            .cell_display_for_source(table_index, address.row, address.column, raw)
            .or_else(|| source.cell_display(table_index, address.row, address.column))
        else {
            return Some(MarkdownCopyContent {
                plain_text: crate::document::literal_text_value(raw).to_owned(),
                has_features: false,
                rich_text: Vec::new(),
            });
        };
        return Some(markdown_copy_content(display, source, language));
    }
    let revision = document.cell_input_revision(table_id, address);
    let key = (document.id, table_id, address);
    if let Some(cached) = cache.get(&key).filter(|cached| {
        cached.cell_input_revision == Some(revision)
            && cached.edit_session_id.is_none()
            && cached.raw_text.is_none()
    }) {
        return Some(markdown_copy_content(&cached.display, source, language));
    }
    pending.push((address, revision, raw.to_owned()));
    None
}

/// Joins parsed runs and records whether the source uses supported formatting.
fn markdown_copy_content(
    display: &crate::markdown_adapter::MarkdownCellDisplay,
    source: &crate::markdown_adapter::MarkdownDocument,
    language: crate::i18n::Language,
) -> MarkdownCopyContent {
    MarkdownCopyContent {
        plain_text: markdown_display_plain_text(display, source, language),
        has_features: display
            .runs
            .iter()
            .any(|run| run.bold || run.italic || run.strike || run.code || run.link),
        rich_text: display
            .runs
            .iter()
            .filter_map(|run| {
                let text = run.text_for_language(Some(source), language)?.into_owned();
                Some(crate::document::CellTextRun {
                    text,
                    bold: run.bold,
                    italic: run.italic,
                    strike: run.strike,
                    code: run.code,
                    hyperlink: run
                        .link
                        .then(|| run.link_target.clone())
                        .flatten()
                        .map(|target| crate::document::CellHyperlink {
                            target,
                            tooltip: run.link_tooltip.clone(),
                            location: false,
                        }),
                    font_name: None,
                    font_size: None,
                    font_color: None,
                })
            })
            .collect(),
    }
}

/// Joins visible runs without copying their Markdown delimiters.
fn markdown_display_plain_text(
    display: &crate::markdown_adapter::MarkdownCellDisplay,
    source: &crate::markdown_adapter::MarkdownDocument,
    language: crate::i18n::Language,
) -> String {
    display
        .runs
        .iter()
        .filter_map(|run| run.text_for_language(Some(source), language))
        .map(|text| text.into_owned())
        .collect()
}
