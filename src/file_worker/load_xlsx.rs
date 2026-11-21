use super::*;
use crate::file_worker::load_progress::LoadProgressSender;

type ImportedXlsx = (
    Arc<MarkdownDocument>,
    crate::document::Document,
    CalculationSnapshot,
    FileIdentity,
    Arc<ImportReport>,
);

/// Loads a workbook and previews converted sheets after the full book parse.
pub(super) fn run_import_xlsx(
    token: JobToken,
    path: PathBuf,
    language: crate::i18n::Language,
    cancelled: Arc<AtomicBool>,
    progress_queue: Option<&SyncSender<LoadProgress>>,
    shutdown: &AtomicBool,
) -> FileResult {
    let path = super::canonical_load_path(path);
    let mut progress = LoadProgressSender::new(
        token,
        FileOperation::LoadXlsx,
        &path,
        progress_queue,
        &cancelled,
        shutdown,
    );
    if !progress.stage(LoadStage::ReadingSource) {
        return super::load::cancelled_load_result(token, path, FileOperation::LoadXlsx);
    }
    let result: io::Result<ImportedXlsx> = (|| {
        let (bytes, identity) = super::load::read_source(&path, &cancelled)?;
        if !progress.stage(LoadStage::ParsingWorkbook) {
            return Err(cancelled_error());
        }
        let inspection = XlsxPackageInspection::scan(&bytes, XlsxPackageLimits::default())
            .map_err(|error| {
                user_io_error(
                    io::ErrorKind::InvalidData,
                    crate::i18n::xlsx_scan_error_message(error),
                )
            })?;
        if cancelled.load(Ordering::Acquire) || shutdown.load(Ordering::Acquire) {
            return Err(cancelled_error());
        }
        let file_name = path.file_stem().map_or_else(
            || "XLSX".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        let source = XlsxDocument::parse_with_progress(
            &bytes,
            token.document_id,
            |table, name, row, cells| {
                if !progress.enabled() {
                    return true;
                }
                progress.push_row(
                    table,
                    name,
                    row,
                    cells
                        .iter()
                        .map(|(column, text)| LoadPreviewCell {
                            column: *column,
                            text: text.clone(),
                        })
                        .collect(),
                )
            },
        )
        .map_err(|error| {
            user_io_error(
                io::ErrorKind::InvalidData,
                crate::i18n::xlsx_error_message(error),
            )
        })?
        .ok_or_else(cancelled_error)?;
        let source = Arc::new(source);
        let (markdown, document, report) = super::import_markdown::xlsx_to_markdown(
            Arc::clone(&source),
            token.document_id,
            file_name,
            language,
            inspection.entries,
            &cancelled,
        )?;
        let calculation = CalculationSnapshot::from_document(&document, false);
        if !progress.finish() {
            return Err(cancelled_error());
        }
        Ok((markdown, document, calculation, identity, report))
    })();
    let mut kind = match result {
        Ok((source, document, calculation, identity, report)) => FileResultKind::ImportedMarkdown {
            source,
            document,
            calculation,
            identity,
            report,
            input_format: ImportedFormat::Xlsx,
        },
        Err(_error) if cancelled.load(Ordering::Acquire) => FileResultKind::Cancelled,
        Err(error) => FileResultKind::failed(error),
    };
    if cancelled.load(Ordering::Acquire) {
        kind = FileResultKind::Cancelled;
    }
    FileResult {
        token,
        operation: FileOperation::LoadXlsx,
        path,
        kind,
    }
}
