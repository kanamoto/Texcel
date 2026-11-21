//! Immutable source documents shared by file backing and structural undo.

use std::sync::Arc;

use crate::csv_adapter::CsvDocument;
use crate::markdown_adapter::MarkdownDocument;

#[derive(Clone, Debug)]
pub enum SourceDocument {
    Csv(Arc<CsvDocument>),
    Markdown(Arc<MarkdownDocument>),
}
