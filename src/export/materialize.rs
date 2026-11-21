use super::*;

// Build the export document from the current snapshot and stop when cancelled.
pub(super) fn materialize_document_cancellable(
    request: &ExportRequest,
    cancelled: Option<&AtomicBool>,
) -> Result<Document, crate::i18n::Message> {
    let mut document = match &request.source {
        Some(ExportSource::Csv(source)) => {
            let source = crate::source::SourceDocument::Csv(Arc::clone(source));
            let stable_table_ids = request
                .tables
                .iter()
                .map(|table| table.id)
                .collect::<Vec<_>>();
            match cancelled {
                Some(cancelled) => crate::structure::source_to_document_with_cancel(
                    &source,
                    request.document_id,
                    &stable_table_ids,
                    cancelled,
                )?,
                None => crate::structure::source_to_document(
                    &source,
                    request.document_id,
                    &stable_table_ids,
                )?,
            }
        }
        Some(ExportSource::Markdown(source)) => crate::structure::source_to_document(
            &crate::source::SourceDocument::Markdown(Arc::clone(source)),
            request.document_id,
            &request
                .tables
                .iter()
                .map(|table| table.id)
                .collect::<Vec<_>>(),
        )?,
        Some(ExportSource::Xlsx(source)) => source.to_document(request.document_id, "XLSX"),
        Some(ExportSource::Html(source)) => source.to_document(request.document_id, "HTML"),
        None => Document::new(
            request.document_id,
            "Export",
            &request
                .tables
                .iter()
                .map(|table| table.name.as_str())
                .collect::<Vec<_>>(),
        ),
    };
    for meta in &request.tables {
        if let Some(table) = document.tables.iter_mut().find(|table| table.id == meta.id) {
            table.set_dimensions(meta.rows.max(1), meta.columns.max(1));
        }
    }
    for (table_id, address, cell) in &request.changes {
        check_cancelled(cancelled)?;
        let Some(table) = document
            .tables
            .iter_mut()
            .find(|table| table.id == *table_id)
        else {
            return Err(crate::i18n::Message::new("status-table-id-unavailable")
                .with_number("table_id", *table_id as i64));
        };
        table.apply_snapshot_cell(*address, cell.clone());
        // A changed cell drops its imported formatting, because runs and links describe old text.
        table.set_loaded_format(*address, CellFormat::default());
    }
    // An import that rewrites a cell passes the formatting it wants to keep.
    for (table_id, address, format) in &request.format_changes {
        check_cancelled(cancelled)?;
        let Some(table) = document
            .tables
            .iter_mut()
            .find(|table| table.id == *table_id)
        else {
            return Err(crate::i18n::Message::new("status-table-id-unavailable")
                .with_number("table_id", *table_id as i64));
        };
        table.set_loaded_format(*address, format.clone());
    }
    Ok(document)
}
