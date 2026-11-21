//! Converts parsed source into a sparse live document.

use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

/// Builds source tables with stable IDs and localized defaults.
pub(crate) fn source_to_document(
    source: &SourceDocument,
    document_id: u64,
    stable_table_ids: &[TableId],
) -> Result<Document, Message> {
    source_to_document_with_language(
        source,
        document_id,
        stable_table_ids,
        crate::i18n::Language::English,
    )
}

/// Builds source tables with stable IDs and localized defaults.
pub(crate) fn source_to_document_with_language(
    source: &SourceDocument,
    document_id: u64,
    stable_table_ids: &[TableId],
    language: crate::i18n::Language,
) -> Result<Document, Message> {
    build_source_document(source, document_id, stable_table_ids, language, None)
}

/// Builds a source model that can stop during large cell conversions.
pub(crate) fn source_to_document_with_cancel(
    source: &SourceDocument,
    document_id: u64,
    stable_table_ids: &[TableId],
    cancelled: &AtomicBool,
) -> Result<Document, Message> {
    build_source_document(
        source,
        document_id,
        stable_table_ids,
        crate::i18n::Language::English,
        Some(cancelled),
    )
}

// Builds a document from an imported source and keeps stable table IDs.
fn build_source_document(
    source: &SourceDocument,
    document_id: u64,
    stable_table_ids: &[TableId],
    language: crate::i18n::Language,
    cancelled: Option<&AtomicBool>,
) -> Result<Document, Message> {
    check_cancelled(cancelled)?;
    let mut document = match source {
        SourceDocument::Csv(source) => match cancelled {
            Some(cancelled) => source.to_document_with_cancel(document_id, "CSV", cancelled)?,
            None => source.to_document(document_id, "CSV"),
        },
        SourceDocument::Markdown(source) => match cancelled {
            Some(cancelled) => {
                source.to_document_with_cancel(document_id, "Markdown", language, cancelled)?
            }
            None => source.to_document_with_language(document_id, "Markdown", language),
        },
    };
    if document.tables.len() != stable_table_ids.len() {
        return Err(Message::new("status-table-count-changed-after-parse"));
    }
    let mut seen = BTreeSet::new();
    for (table, table_id) in document.tables.iter_mut().zip(stable_table_ids) {
        check_cancelled(cancelled)?;
        if *table_id == 0 || !seen.insert(*table_id) {
            return Err(Message::new("status-table-id-duplicate-or-invalid")
                .with_number("table_id", *table_id as i64));
        }
        table.id = *table_id;
    }
    document.views = stable_table_ids
        .iter()
        .map(|table_id| (*table_id, crate::document::TableViewState::default()))
        .collect();
    if let Some(table_id) = stable_table_ids.first() {
        document.active_table = *table_id;
    }
    document.set_source(Some(Arc::new(source.clone())));
    Ok(document)
}

// Stops an import when its worker receives a cancellation request.
fn check_cancelled(cancelled: Option<&AtomicBool>) -> Result<(), Message> {
    if cancelled.is_some_and(|cancelled| cancelled.load(Ordering::Acquire)) {
        Err(Message::new("status-operation-cancelled"))
    } else {
        Ok(())
    }
}
