use super::*;

impl HtmlDocument {
    /// Derives safe text and inline formatting from one bounded raw cell fragment.
    pub(crate) fn display_fragment(fragment: &str) -> Option<HtmlCellDisplayData> {
        if fragment.len() > MAX_HTML_DISPLAY_BYTES {
            return None;
        }
        let mut wrapped = String::with_capacity(fragment.len() + 40);
        wrapped.push_str("<table><tr><td>");
        wrapped.push_str(fragment);
        wrapped.push_str("</td></tr></table>");
        let parsed = Self::parse(wrapped.into_bytes()).ok()?;
        let cell = parsed.tables.first()?.rows.first()?.first()?;
        Some(HtmlCellDisplayData {
            display: cell.display_value.clone(),
            rich_text: cell.rich_text.clone(),
            hyperlink: cell.hyperlink.clone(),
            has_partial_link: cell.has_partial_link,
            has_unsupported_markup: cell.has_unsupported_markup,
        })
    }
}
