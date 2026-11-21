use super::*;

/// Builds an export confirmation from an immutable document snapshot.
pub(super) fn run_prepare_export(
    token: JobToken,
    request: ExportRequest,
    cancelled: Arc<AtomicBool>,
) -> FileResult {
    let path = request.destination.clone();
    let kind = if cancelled.load(Ordering::Acquire) {
        FileResultKind::Cancelled
    } else {
        match crate::export::prepare_export_cancellable(
            &request,
            request.formula_results.as_ref(),
            &cancelled,
        ) {
            Ok(_prepared) if cancelled.load(Ordering::Acquire) => FileResultKind::Cancelled,
            Ok(prepared) => FileResultKind::ExportPrepared(prepared),
            Err(_error) if cancelled.load(Ordering::Acquire) => FileResultKind::Cancelled,
            Err(error) => FileResultKind::Failed(error),
        }
    };
    FileResult {
        token,
        operation: FileOperation::PrepareExport,
        path,
        kind,
    }
}

/// Searches the requested document version for matching cells.
pub(super) fn run_search(
    token: JobToken,
    request: crate::structure::SearchRequest,
    cancelled: Arc<AtomicBool>,
) -> FileResult {
    let kind = if cancelled.load(Ordering::Acquire) {
        FileResultKind::Cancelled
    } else {
        match crate::structure::prepare_search(&request, &cancelled) {
            Ok(_prepared) if cancelled.load(Ordering::Acquire) => FileResultKind::Cancelled,
            Ok(prepared) => FileResultKind::SearchPrepared(prepared),
            Err(_error) if cancelled.load(Ordering::Acquire) => FileResultKind::Cancelled,
            Err(error) => FileResultKind::Failed(error),
        }
    };
    FileResult {
        token,
        operation: FileOperation::Search,
        path: PathBuf::new(),
        kind,
    }
}

/// Prepares the requested data edit and its preview.
pub(super) fn run_data_edit(
    token: JobToken,
    request: crate::structure::DataEditRequest,
    cancelled: Arc<AtomicBool>,
) -> FileResult {
    let kind = if cancelled.load(Ordering::Acquire) {
        FileResultKind::Cancelled
    } else {
        match crate::structure::prepare_data_edit(&request, &cancelled) {
            Ok(_prepared) if cancelled.load(Ordering::Acquire) => FileResultKind::Cancelled,
            Ok(prepared) => FileResultKind::DataEditPrepared(prepared),
            Err(_error) if cancelled.load(Ordering::Acquire) => FileResultKind::Cancelled,
            Err(error) => FileResultKind::Failed(error),
        }
    };
    FileResult {
        token,
        operation: FileOperation::DataEdit,
        path: PathBuf::new(),
        kind,
    }
}

/// Prepares a structural operation for the current table snapshot.
pub(super) fn run_prepare_structure(
    token: JobToken,
    request: crate::structure::StructureRequest,
    cancelled: Arc<AtomicBool>,
) -> FileResult {
    let kind = if cancelled.load(Ordering::Acquire) {
        FileResultKind::Cancelled
    } else {
        match crate::structure::prepare_structure(&request, &cancelled) {
            Ok(_prepared) if cancelled.load(Ordering::Acquire) => FileResultKind::Cancelled,
            Ok(prepared) => FileResultKind::StructurePrepared(prepared),
            Err(_error) if cancelled.load(Ordering::Acquire) => FileResultKind::Cancelled,
            Err(error) => FileResultKind::Failed(error),
        }
    };
    FileResult {
        token,
        operation: FileOperation::PrepareStructure,
        path: PathBuf::new(),
        kind,
    }
}

/// Prepares table metadata for a structural operation.
pub(super) fn run_prepare_table_structure(
    token: JobToken,
    request: crate::structure::StructureRequest,
    action: crate::structure::TableAction,
    cancelled: Arc<AtomicBool>,
) -> FileResult {
    let kind = if cancelled.load(Ordering::Acquire) {
        FileResultKind::Cancelled
    } else {
        match crate::structure::prepare_table_structure(&request, &action, &cancelled) {
            Ok(_prepared) if cancelled.load(Ordering::Acquire) => FileResultKind::Cancelled,
            Ok(prepared) => FileResultKind::StructurePrepared(prepared),
            Err(_error) if cancelled.load(Ordering::Acquire) => FileResultKind::Cancelled,
            Err(error) => FileResultKind::Failed(error),
        }
    };
    FileResult {
        token,
        operation: FileOperation::PrepareStructure,
        path: PathBuf::new(),
        kind,
    }
}

/// Restores a prior structure snapshot from worker storage.
pub(super) fn run_restore_structure_history(
    token: JobToken,
    request: crate::structure::StructureHistoryRestoreRequest,
    cancelled: Arc<AtomicBool>,
) -> FileResult {
    let kind = if cancelled.load(Ordering::Acquire) {
        FileResultKind::Cancelled
    } else {
        match crate::structure::prepare_structure_restore(&request) {
            Ok(_prepared) if cancelled.load(Ordering::Acquire) => FileResultKind::Cancelled,
            Ok(prepared) => FileResultKind::StructureHistoryRestored(prepared),
            Err(_error) if cancelled.load(Ordering::Acquire) => FileResultKind::Cancelled,
            Err(error) => FileResultKind::Failed(error),
        }
    };
    FileResult {
        token,
        operation: FileOperation::RestoreStructureHistory,
        path: PathBuf::new(),
        kind,
    }
}

/// Builds export bytes and its conversion report.
pub(super) fn run_export(
    token: JobToken,
    artifacts: Vec<ExportArtifact>,
    cancelled: Arc<AtomicBool>,
) -> FileResult {
    let paths = artifacts
        .iter()
        .map(|artifact| artifact.path.clone())
        .collect::<Vec<_>>();
    let path = paths.first().cloned().unwrap_or_default();
    let mut written = Vec::new();
    let mut failed = None;
    for (index, artifact) in artifacts.into_iter().enumerate() {
        if cancelled.load(Ordering::Acquire) {
            if written.is_empty() {
                return FileResult {
                    token,
                    operation: FileOperation::Export,
                    path,
                    kind: FileResultKind::Cancelled,
                };
            }
            failed = Some((
                artifact.path.clone(),
                crate::i18n::Message::new("status-export-cancelled"),
            ));
            break;
        }
        let expected = match artifact.expected_identity {
            Some(identity) if artifact.replace_existing => SaveExpectation::Matches(identity),
            _ => SaveExpectation::Absent,
        };
        match atomic_save(
            &artifact.path,
            &artifact.bytes,
            &expected,
            token.job_id.saturating_add(index as u64),
            &cancelled,
        ) {
            Ok(_) => written.push(artifact.path),
            Err(error) => {
                if error.kind() == io::ErrorKind::Interrupted && written.is_empty() {
                    return FileResult {
                        token,
                        operation: FileOperation::Export,
                        path,
                        kind: FileResultKind::Cancelled,
                    };
                }
                let message = if error.kind() == io::ErrorKind::Interrupted {
                    crate::i18n::Message::new("status-export-cancelled")
                } else {
                    file_error_message(error)
                };
                failed = Some((artifact.path, message));
                break;
            }
        }
    }
    let first_unwritten = written.len() + usize::from(failed.is_some());
    let not_written = paths.into_iter().skip(first_unwritten).collect();
    FileResult {
        token,
        operation: FileOperation::Export,
        path,
        kind: FileResultKind::Exported {
            written,
            failed,
            not_written,
        },
    }
}
