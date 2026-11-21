use super::*;

/// Saves a conversion report to its chosen path.
pub(super) fn run_save_report(
    token: JobToken,
    path: PathBuf,
    source_path: PathBuf,
    report: Arc<ImportReport>,
    language: crate::i18n::Language,
    cancelled: Arc<AtomicBool>,
) -> FileResult {
    let valid_extension = path
        .extension()
        .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("txt"));
    // Report saving can replace an existing file, so reject the normalized source path before saving.
    let result = if !valid_extension {
        Err(user_io_error(
            io::ErrorKind::InvalidInput,
            Message::new("status-import-report-txt-required"),
        ))
    } else if same_normalized_path(&path, &source_path) {
        Err(user_io_error(
            io::ErrorKind::InvalidInput,
            Message::new("status-report-source-overwrite-blocked"),
        ))
    } else {
        let text = report.to_text(language);
        atomic_save(
            &path,
            text.as_bytes(),
            &SaveExpectation::CurrentAtStart,
            token.job_id,
            &cancelled,
        )
    };
    let kind = match result {
        Ok(_) => FileResultKind::ReportSaved,
        Err(error) if error.kind() == io::ErrorKind::Interrupted => FileResultKind::Cancelled,
        Err(error) => FileResultKind::failed(error),
    };
    FileResult {
        token,
        operation: FileOperation::SaveReport,
        path,
        kind,
    }
}

/// Saves an export report for the confirmed document version.
pub(super) fn run_save_export_report(
    token: JobToken,
    path: PathBuf,
    source_path: Option<PathBuf>,
    report: Arc<crate::conversion_report::ExportReport>,
    language: crate::i18n::Language,
    cancelled: Arc<AtomicBool>,
) -> FileResult {
    let result = if !path
        .extension()
        .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("txt"))
    {
        Err(user_io_error(
            io::ErrorKind::InvalidInput,
            Message::new("status-export-report-txt-required"),
        ))
    } else if source_path
        .as_deref()
        .is_some_and(|source| same_normalized_path(&path, source))
    {
        Err(user_io_error(
            io::ErrorKind::InvalidInput,
            Message::new("status-report-source-overwrite-blocked"),
        ))
    } else {
        atomic_save(
            &path,
            report.to_text(language).as_bytes(),
            &SaveExpectation::CurrentAtStart,
            token.job_id,
            &cancelled,
        )
    };
    let kind = match result {
        Ok(_) => FileResultKind::ReportSaved,
        Err(error) if error.kind() == io::ErrorKind::Interrupted => FileResultKind::Cancelled,
        Err(error) => FileResultKind::failed(error),
    };
    FileResult {
        token,
        operation: FileOperation::ExportReport,
        path,
        kind,
    }
}

/// Patches changed cells into the original CSV bytes and saves the result.
pub(super) fn run_save_csv(
    token: JobToken,
    path: PathBuf,
    request: CsvSaveRequest,
    expected: SaveExpectation,
    cancelled: Arc<AtomicBool>,
) -> FileResult {
    let CsvSaveRequest {
        source,
        table_id,
        changes,
        text_overlay,
    } = request;
    let source_changes =
        super::save_text_overlay::csv_changes(&source, table_id, changes, &text_overlay);
    let result = source
        .apply_changes(&source_changes)
        .map_err(|error| {
            user_io_error(
                io::ErrorKind::InvalidData,
                crate::i18n::csv_error_message(error),
            )
        })
        .and_then(|updated| {
            let mut document = updated.to_document(token.document_id, "CSV");
            if let Some(table) = document.tables.first_mut() {
                let source_id = table.id;
                table.id = table_id;
                document.active_table = table_id;
                if let Some(view) = document.views.remove(&source_id) {
                    document.views.insert(table_id, view);
                }
            }
            let cells = crate::structure::apply_requested_text_overlay(
                &mut document,
                text_overlay.base.revision,
                token.revision,
                &text_overlay.base.cells,
                &text_overlay.changes,
                true,
            )
            .map_err(|error| user_io_error(io::ErrorKind::InvalidData, error))?;
            let text_overlay = TextOverlaySnapshot {
                revision: token.revision,
                cells,
            };
            let identity = atomic_save(
                &path,
                updated.source_bytes(),
                &expected,
                token.job_id,
                &cancelled,
            )?;
            Ok((identity, Arc::new(updated), text_overlay))
        });
    let kind = match result {
        Ok((identity, source, text_overlay)) => FileResultKind::Saved {
            identity,
            csv_source: Some(source),
            markdown_source: None,
            text_overlay: Some(text_overlay),
        },
        Err(error) if error.kind() == io::ErrorKind::Interrupted => FileResultKind::Cancelled,
        Err(error) => save_failure_kind(error, &path, &expected),
    };
    FileResult {
        token,
        operation: FileOperation::Save,
        path,
        kind,
    }
}

/// Patches changed cells into the original Markdown source and saves it.
pub(super) fn run_save_markdown(
    token: JobToken,
    path: PathBuf,
    source: Arc<MarkdownDocument>,
    changes: &[MarkdownCellChange],
    text_overlay: TextOverlaySaveRequest,
    expected: SaveExpectation,
    cancelled: Arc<AtomicBool>,
) -> FileResult {
    let source_changes = super::save_text_overlay::markdown_changes(
        &source,
        token.document_id,
        changes.to_vec(),
        &text_overlay,
    );
    let result = source
        .apply_changes(&source_changes)
        .map_err(|error| {
            user_io_error(
                io::ErrorKind::InvalidData,
                crate::i18n::markdown_error_message(error),
            )
        })
        .and_then(|updated| {
            let mut document = updated.to_document(token.document_id, "Markdown");
            let cells = crate::structure::apply_requested_text_overlay(
                &mut document,
                text_overlay.base.revision,
                token.revision,
                &text_overlay.base.cells,
                &text_overlay.changes,
                true,
            )
            .map_err(|error| user_io_error(io::ErrorKind::InvalidData, error))?;
            let text_overlay = TextOverlaySnapshot {
                revision: token.revision,
                cells,
            };
            let identity = atomic_save(
                &path,
                updated.source_bytes(),
                &expected,
                token.job_id,
                &cancelled,
            )?;
            Ok((identity, Arc::new(updated), text_overlay))
        });
    let kind = match result {
        Ok((identity, source, text_overlay)) => FileResultKind::Saved {
            identity,
            csv_source: None,
            markdown_source: Some(source),
            text_overlay: Some(text_overlay),
        },
        Err(error) if error.kind() == io::ErrorKind::Interrupted => FileResultKind::Cancelled,
        Err(error) => save_failure_kind(error, &path, &expected),
    };
    FileResult {
        token,
        operation: FileOperation::Save,
        path,
        kind,
    }
}

/// Turns a same-source identity race into an immediate external-change choice.
pub(super) fn save_failure_kind(
    error: io::Error,
    path: &Path,
    expected: &SaveExpectation,
) -> FileResultKind {
    let missing_destination_reappeared = error.kind() == io::ErrorKind::AlreadyExists
        && error
            .get_ref()
            .and_then(|cause| cause.downcast_ref::<Message>())
            .is_some_and(|message| message == &Message::new("status-save-destination-appeared"))
        && matches!(expected, SaveExpectation::Absent);
    if missing_destination_reappeared {
        return FileResultKind::SaveConflict;
    }
    let same_known_source = matches!(expected, SaveExpectation::KnownPath { source_path, .. }
        if same_normalized_path(path, source_path));
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent_is_missing =
        fs::metadata(parent).is_err_and(|error| error.kind() == io::ErrorKind::NotFound);
    if error.kind() == io::ErrorKind::NotFound
        && (matches!(expected, SaveExpectation::Absent) || same_known_source)
        && parent_is_missing
    {
        return FileResultKind::Failed(Message::new("status-save-parent-folder-missing"));
    }
    let changed_source = error
        .get_ref()
        .and_then(|cause| cause.downcast_ref::<Message>())
        .is_some_and(|message| message == &Message::new("status-save-destination-changed"))
        && matches!(expected, SaveExpectation::KnownPath { source_path, .. }
            if same_normalized_path(path, source_path));
    if changed_source {
        FileResultKind::SaveConflict
    } else {
        FileResultKind::failed(error)
    }
}
