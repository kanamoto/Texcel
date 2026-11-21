use super::*;

impl FileWorker {
    /// Saves a complete CSV conversion and returns the parsed source in the saved acknowledgement.
    pub fn save_converted_csv(
        &self,
        token: JobToken,
        path: PathBuf,
        bytes: Vec<u8>,
        delimiter: char,
        expected: SaveExpectation,
    ) -> Result<(), SubmitError> {
        self.submit(
            token,
            WorkerMessage::SaveConvertedCsv {
                token,
                path,
                bytes,
                delimiter,
                expected,
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        )
    }

    // Safely saves a load report as UTF-8 text on a worker.
    pub fn save_report(
        &self,
        token: JobToken,
        path: PathBuf,
        source_path: PathBuf,
        report: Arc<ImportReport>,
        language: crate::i18n::Language,
    ) -> Result<(), SubmitError> {
        self.submit(
            token,
            WorkerMessage::SaveReport {
                token,
                path,
                source_path,
                report,
                language,
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        )
    }

    /// Saves prepared export files with their preflight-captured identities.
    /// Queue-full returns the artifact ownership so the UI can retry without cloning bytes.
    pub fn save_export(
        &self,
        token: JobToken,
        artifacts: Vec<ExportArtifact>,
    ) -> Result<(), (SubmitError, Vec<ExportArtifact>)> {
        let cancelled = Arc::new(AtomicBool::new(false));
        let message = WorkerMessage::Export {
            token,
            artifacts,
            cancelled: Arc::clone(&cancelled),
        };
        let Ok(mut jobs) = self.cancellation.lock() else {
            let WorkerMessage::Export { artifacts, .. } = message else {
                unreachable!()
            };
            return Err((SubmitError::Stopped, artifacts));
        };
        jobs.insert(token, cancelled);
        drop(jobs);
        match self.sender.try_send(message) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(WorkerMessage::Export { artifacts, .. })) => {
                if let Ok(mut jobs) = self.cancellation.lock() {
                    jobs.remove(&token);
                }
                Err((SubmitError::QueueFull, artifacts))
            }
            Err(TrySendError::Disconnected(WorkerMessage::Export { artifacts, .. })) => {
                if let Ok(mut jobs) = self.cancellation.lock() {
                    jobs.remove(&token);
                }
                Err((SubmitError::Stopped, artifacts))
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                unreachable!("export message was submitted")
            }
        }
    }

    /// Saves the already-rendered export report without touching document save state.
    pub fn save_export_report(
        &self,
        token: JobToken,
        path: PathBuf,
        source_path: Option<PathBuf>,
        report: Arc<crate::conversion_report::ExportReport>,
        language: crate::i18n::Language,
    ) -> Result<(), SubmitError> {
        self.submit(
            token,
            WorkerMessage::ExportReport {
                token,
                path,
                source_path,
                report,
                language,
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        )
    }

    // Sends the CSV source and changed cells for patching and safe replacement on a worker.
    pub fn save_csv(
        &self,
        token: JobToken,
        path: PathBuf,
        request: CsvSaveRequest,
        expected: SaveExpectation,
    ) -> Result<(), SubmitError> {
        self.submit(
            token,
            WorkerMessage::SaveCsv {
                token,
                path,
                request,
                expected,
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        )
    }

    // Sends the Markdown source and changed cells for range replacement and async saving.
    pub fn save_markdown(
        &self,
        token: JobToken,
        path: PathBuf,
        source: Arc<MarkdownDocument>,
        changes: Vec<MarkdownCellChange>,
        text_overlay: TextOverlaySaveRequest,
        expected: SaveExpectation,
    ) -> Result<(), SubmitError> {
        self.submit(
            token,
            WorkerMessage::SaveMarkdown {
                token,
                path,
                source,
                changes,
                text_overlay,
                expected,
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        )
    }
}
