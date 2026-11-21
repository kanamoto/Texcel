//! Connects HTML table cells to source byte ranges without rewriting surrounding markup.

use std::ops::Range;

use crate::document::{
    Cell, CellAddress, CellHyperlink, CellInput, CellTextRun, Document, DocumentId, MergedRange,
};

mod display;
mod document;
mod encoding;
mod finish;
mod grid;
mod parser;
mod tokenizer;

const MAX_HTML_BYTES: usize = 64 * 1024 * 1024;
const MAX_HTML_ROWS: usize = 1_048_576;
const MAX_HTML_COLUMNS: usize = 16_384;
const MAX_HTML_CELLS: usize = 500_000;
const MAX_HTML_TOKENS: usize = 1_000_000;
pub(crate) const MAX_HTML_DISPLAY_BYTES: usize = 64 * 1024;

/// A table cell and its original editable text ranges.
#[derive(Clone, Debug, PartialEq)]
pub struct HtmlCell {
    /// Original inner HTML exposed as the cell's source text.
    pub value: String,
    /// Text derived from inner HTML for the normal grid view.
    pub display_value: String,
    /// Simple horizontal alignment from the HTML cell's `align` attribute.
    pub(crate) alignment: Option<String>,
    pub source_range: Range<usize>,
    pub(crate) rich_text: Option<Vec<CellTextRun>>,
    pub(crate) hyperlink: Option<CellHyperlink>,
    pub(crate) has_partial_link: bool,
    pub(crate) has_unsupported_markup: bool,
    pub(crate) element_range: Range<usize>,
    pub(crate) tag_name: String,
    pub(crate) has_id: bool,
    /// Number of logical table rows covered by this source cell.
    pub row_span: usize,
    /// Number of logical table columns covered by this source cell.
    pub column_span: usize,
    /// Whether this logical coordinate is covered by another source cell.
    pub placeholder: bool,
}

/// A source table in document order.
#[derive(Clone, Debug, PartialEq)]
pub struct HtmlTable {
    pub name: String,
    pub(crate) name_generated: bool,
    pub rows: Vec<Vec<HtmlCell>>,
    pub merges: Vec<MergedRange>,
    pub source_range: Range<usize>,
    pub(crate) opening_tag_range: Range<usize>,
    pub(crate) caption_range: Option<Range<usize>>,
    pub(crate) has_id: bool,
    pub(crate) source_rows: Vec<HtmlSourceRow>,
    nested: bool,
    malformed: bool,
}

impl HtmlTable {
    /// Returns the authored caption or the localized default table name.
    pub(crate) fn name_with_language(
        &self,
        index: usize,
        language: crate::i18n::Language,
    ) -> String {
        if self.name_generated {
            crate::i18n::Message::new("table-default-name")
                .with_number("number", index as i64 + 1)
                .resolve_generated_name(language)
        } else {
            self.name.clone()
        }
    }
}

// Physical row data needed by source-preserving structural edits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HtmlSourceRow {
    pub(crate) range: Range<usize>,
    pub(crate) content_range: Range<usize>,
    pub(crate) group: usize,
    pub(crate) has_id: bool,
    pub(crate) cells: Vec<HtmlSourceCell>,
}

// Original cell tags remain available when a column operation moves or duplicates cells.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HtmlSourceCell {
    pub(crate) element_range: Range<usize>,
    pub(crate) tag_name: String,
    pub(crate) has_id: bool,
    pub(crate) row_span: usize,
    pub(crate) column_span: usize,
}

/// Display text and inline metadata derived from one bounded raw cell fragment.
#[derive(Clone, Debug, PartialEq)]
pub struct HtmlCellDisplayData {
    pub display: String,
    pub rich_text: Option<Vec<CellTextRun>>,
    pub hyperlink: Option<CellHyperlink>,
    pub has_partial_link: bool,
    pub has_unsupported_markup: bool,
}

/// A non-destructive issue found while reading an HTML document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HtmlDiagnostic {
    NestedTable { table: usize },
    MalformedTable { table: usize },
}

/// A source span or table cell could not be handled safely.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HtmlError {
    InvalidUtf8,
    UnsupportedEncoding(String),
    ResourceLimit,
    NoTables,
    OutputMismatch,
}

impl std::fmt::Display for HtmlError {
    // Keeps adapter errors readable when a worker reports a failed source edit.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(
            &crate::i18n::html_error_message(self.clone()).resolve(crate::i18n::Language::English),
        )
    }
}

impl std::error::Error for HtmlError {}

impl HtmlDocument {}

/// An HTML document whose original bytes remain authoritative between edits.
#[derive(Clone, Debug, PartialEq)]
pub struct HtmlDocument {
    bytes: Vec<u8>,
    pub tables: Vec<HtmlTable>,
    pub diagnostics: Vec<HtmlDiagnostic>,
}

#[cfg(test)]
#[path = "html_adapter_test.rs"]
mod tests;
