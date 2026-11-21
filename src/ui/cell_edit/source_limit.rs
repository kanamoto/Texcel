use crate::document::Document;
use crate::i18n::Message;
use crate::source::SourceDocument;

/// Reads the saved text extent without using the grid's extra input row or column.
pub(super) fn text_source_extent(document: &Document, table_id: u64) -> Option<(usize, usize)> {
    let source = document.source.as_deref()?;
    match source {
        SourceDocument::Markdown(markdown) => document
            .tables
            .iter()
            .position(|table| table.id == table_id)
            .and_then(|index| markdown.source_extent(index)),
        SourceDocument::Csv(csv) => Some((
            csv.records.len(),
            csv.records
                .iter()
                .map(|record| record.fields.len())
                .max()
                .unwrap_or(0),
        )),
    }
}

/// Builds the localized rejection message for the active text-source format.
pub(super) fn source_resource_limit_error(
    document: Option<&Document>,
    limit: crate::i18n::ResourceLimit,
) -> Message {
    match document.and_then(|document| document.source.as_deref()) {
        Some(SourceDocument::Csv(_)) => {
            crate::i18n::csv_error_message(crate::csv_adapter::CsvError::ResourceLimit(limit))
        }
        _ => crate::i18n::markdown_error_message(
            crate::markdown_adapter::MarkdownError::ResourceLimit(limit),
        ),
    }
}
