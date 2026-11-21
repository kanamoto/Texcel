use super::*;

impl FileWorker {
    /// Builds an immutable output plan and its export report off the UI thread.
    pub fn prepare_export(
        &self,
        token: JobToken,
        request: ExportRequest,
    ) -> Result<(), SubmitError> {
        self.submit(
            token,
            WorkerMessage::PrepareExport {
                token,
                request,
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        )
    }

    /// Searches a revision-pinned source snapshot and optionally prepares a confirmable edit.
    pub fn search(
        &self,
        token: JobToken,
        request: crate::structure::SearchRequest,
    ) -> Result<(), SubmitError> {
        self.submit(
            token,
            WorkerMessage::Search {
                token,
                request,
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        )
    }

    /// Prepares one data transformation and its confirmation diff away from the UI thread.
    pub fn prepare_data_edit(
        &self,
        token: JobToken,
        request: crate::structure::DataEditRequest,
    ) -> Result<(), SubmitError> {
        self.submit(
            token,
            WorkerMessage::DataEdit {
                token,
                request: Box::new(request),
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        )
    }

    /// Prepares an atomic structural root replacement on the file worker.
    pub fn prepare_structure(
        &self,
        token: JobToken,
        request: crate::structure::StructureRequest,
    ) -> Result<(), SubmitError> {
        self.submit(
            token,
            WorkerMessage::PrepareStructure {
                token,
                request,
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        )
    }

    /// Parses one large dirty Markdown cell on the worker so visible styling stays responsive.
    pub fn prepare_markdown_cell_display(
        &self,
        token: JobToken,
        table_id: u64,
        address: crate::document::CellAddress,
        cell_input_revision: (u64, u64),
        source: Arc<MarkdownDocument>,
        change: MarkdownCellChange,
    ) -> Result<(), SubmitError> {
        self.submit(
            token,
            WorkerMessage::PrepareMarkdownCellDisplay {
                token,
                table_id,
                address,
                cell_input_revision,
                source,
                change,
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        )
    }

    /// Prepares table and worksheet add, duplicate, or delete operations off the UI thread.
    pub fn prepare_table_structure(
        &self,
        token: JobToken,
        request: crate::structure::StructureRequest,
        action: crate::structure::TableAction,
    ) -> Result<(), SubmitError> {
        self.submit(
            token,
            WorkerMessage::PrepareTableStructure {
                token,
                request,
                action,
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        )
    }

    /// Restores one spilled structural history root away from the UI thread.
    pub fn restore_structure_history(
        &self,
        token: JobToken,
        request: crate::structure::StructureHistoryRestoreRequest,
    ) -> Result<(), SubmitError> {
        self.submit(
            token,
            WorkerMessage::RestoreStructureHistory {
                token,
                request,
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        )
    }

    /// Drops a retired table root on the file worker after an atomic swap.
    pub fn discard_structure_tables(
        &self,
        tables: Vec<crate::document::Table>,
    ) -> Result<(), Vec<crate::document::Table>> {
        match self
            .sender
            .try_send(WorkerMessage::DiscardStructureTables { tables })
        {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(WorkerMessage::DiscardStructureTables { tables }))
            | Err(TrySendError::Disconnected(WorkerMessage::DiscardStructureTables { tables })) => {
                Err(tables)
            }
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => unreachable!(),
        }
    }
}
