use super::*;

/// Parses a complete CSV conversion before safely saving and acknowledging its source.
pub(super) fn run_save_converted_csv(
    token: JobToken,
    path: PathBuf,
    bytes: Vec<u8>,
    delimiter: char,
    expected: SaveExpectation,
    cancelled: Arc<AtomicBool>,
) -> FileResult {
    let source = match CsvDocument::parse(bytes.clone(), EncodingChoice::Auto, delimiter) {
        Ok(source) => Arc::new(source),
        Err(error) => {
            return FileResult {
                token,
                operation: FileOperation::Save,
                path,
                kind: FileResultKind::Failed(crate::i18n::csv_error_message(error)),
            };
        }
    };
    if cancelled.load(Ordering::Acquire) {
        return FileResult {
            token,
            operation: FileOperation::Save,
            path,
            kind: FileResultKind::Cancelled,
        };
    }
    let kind = match atomic_save(&path, &bytes, &expected, token.job_id, &cancelled) {
        Ok(identity) => FileResultKind::Saved {
            identity,
            csv_source: Some(source),
            markdown_source: None,
            text_overlay: None,
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
