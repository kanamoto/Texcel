//!  Worker-side operations for history.

use super::*;

#[path = "history_size.rs"]
mod size;
pub(crate) use size::serialized_size;

#[derive(Serialize)]
#[serde(tag = "format", content = "source")]
enum HistorySourceRef<'a> {
    None,
    Csv {
        bytes: Base64BytesRef<'a>,
        encoding: CsvEncoding,
        delimiter: char,
        metadata: crate::csv_adapter::CsvHistoryMetadata,
    },
    Markdown {
        bytes: Base64BytesRef<'a>,
    },
}

#[derive(Deserialize)]
#[serde(tag = "format", content = "source")]
enum HistorySource {
    None,
    Csv {
        bytes: String,
        encoding: CsvEncoding,
        delimiter: char,
        metadata: crate::csv_adapter::CsvHistoryMetadata,
    },
    Markdown {
        bytes: String,
    },
}

#[derive(Serialize)]
pub(crate) struct StructureHistoryRootRef<'a> {
    version: u32,
    document_id: u64,
    tables: &'a [crate::document::Table],
    source: HistorySourceRef<'a>,
    view_table_id: TableId,
    view_layout: &'a TableLayout,
    selection: CellRange,
    scroll: ScrollPosition,
    target: (TableId, CellAddress),
    cross_sheet_references: bool,
    text_overlay: &'a [crate::document::TextOverlayCell],
}

#[derive(Clone, Copy)]
pub(crate) struct StructureHistoryMetadata {
    pub(crate) document_id: u64,
    pub(crate) view_table_id: TableId,
    pub(crate) selection: CellRange,
    pub(crate) scroll: ScrollPosition,
    pub(crate) target: (TableId, CellAddress),
    pub(crate) cross_sheet_references: bool,
}

#[derive(Deserialize)]
pub(crate) struct StructureHistoryRoot {
    version: u32,
    document_id: u64,
    tables: Vec<crate::document::Table>,
    source: HistorySource,
    view_table_id: TableId,
    view_layout: TableLayout,
    selection: CellRange,
    scroll: ScrollPosition,
    target: (TableId, CellAddress),
    cross_sheet_references: bool,
    #[serde(default)]
    text_overlay: Vec<crate::document::TextOverlayCell>,
}

struct Base64BytesRef<'a>(&'a [u8]);

impl Serialize for Base64BytesRef<'_> {
    /// Writes the structure history payload to its stored form.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(&Base64Display::new(self.0, &BASE64))
    }
}

impl HistorySource {
    /// Returns the source data needed to restore a table.
    fn into_source(self) -> Result<Option<SourceDocument>, Message> {
        match self {
            Self::None => Ok(None),
            Self::Csv {
                bytes,
                encoding,
                delimiter,
                metadata,
            } => {
                let bytes = BASE64.decode(bytes).map_err(|error| {
                    Message::new("status-history-source-decode-failed").with_text("detail", error)
                })?;
                let choice = if encoding == CsvEncoding::ShiftJis {
                    EncodingChoice::ShiftJis
                } else {
                    EncodingChoice::Auto
                };
                let mut source = crate::csv_adapter::CsvDocument::parse(bytes, choice, delimiter)
                    .map_err(|error| {
                    Message::new("status-history-csv-source-invalid")
                        .with_message("detail", crate::i18n::csv_error_message(error))
                })?;
                source.restore_history_metadata(metadata)?;
                Ok(Some(SourceDocument::Csv(Arc::new(source))))
            }
            Self::Markdown { bytes } => {
                let bytes = BASE64.decode(bytes).map_err(|error| {
                    Message::new("status-history-source-decode-failed").with_text("detail", error)
                })?;
                let source = MarkdownDocument::parse(bytes).map_err(|error| {
                    Message::new("status-history-markdown-source-invalid")
                        .with_message("detail", crate::i18n::markdown_error_message(error))
                })?;
                Ok(Some(SourceDocument::Markdown(Arc::new(source))))
            }
        }
    }
}

/// Returns the history source associated with a table.
fn history_source_ref(source: Option<&SourceDocument>) -> HistorySourceRef<'_> {
    match source {
        None => HistorySourceRef::None,
        Some(SourceDocument::Csv(source)) => HistorySourceRef::Csv {
            bytes: Base64BytesRef(source.source_bytes()),
            encoding: source.encoding,
            delimiter: source.delimiter,
            metadata: source.history_metadata(),
        },
        Some(SourceDocument::Markdown(source)) => HistorySourceRef::Markdown {
            bytes: Base64BytesRef(source.source_bytes()),
        },
    }
}

/// Returns the root record for a history file.
pub(crate) fn history_root_ref<'a>(
    tables: &'a [crate::document::Table],
    source: Option<&'a SourceDocument>,
    view_layout: &'a TableLayout,
    metadata: StructureHistoryMetadata,
) -> StructureHistoryRootRef<'a> {
    history_root_ref_with_text_overlay(tables, source, view_layout, metadata, &[])
}

/// Adds sparse source cells to history roots so spilled Undo keeps the same values.
pub(crate) fn history_root_ref_with_text_overlay<'a>(
    tables: &'a [crate::document::Table],
    source: Option<&'a SourceDocument>,
    view_layout: &'a TableLayout,
    metadata: StructureHistoryMetadata,
    text_overlay: &'a [crate::document::TextOverlayCell],
) -> StructureHistoryRootRef<'a> {
    StructureHistoryRootRef {
        version: 1,
        document_id: metadata.document_id,
        tables,
        source: history_source_ref(source),
        view_table_id: metadata.view_table_id,
        view_layout,
        selection: metadata.selection,
        scroll: metadata.scroll,
        target: metadata.target,
        cross_sheet_references: metadata.cross_sheet_references,
        text_overlay,
    }
}

/// Creates a new history file with restricted permissions.
pub(crate) fn create_history_file() -> Result<Arc<StructureHistoryFile>, Message> {
    let root = std::env::temp_dir().join("texcel-recovery");
    fs::create_dir_all(&root).map_err(|error| {
        Message::new("status-history-storage-create-failed").with_text("detail", error)
    })?;
    set_private_permissions(&root)?;
    for _ in 0..32 {
        let sequence = NEXT_RECOVERY_ID.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory = root.join(format!("{}-{timestamp}-{sequence}", std::process::id()));
        match fs::create_dir(&directory) {
            Ok(()) => {
                set_private_permissions(&directory)?;
                return Ok(Arc::new(StructureHistoryFile { directory }));
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(
                    Message::new("status-history-storage-create-failed").with_text("detail", error)
                );
            }
        }
    }
    Err(Message::new("status-history-temp-name-unavailable").with_number("attempts", 32))
}

#[cfg(unix)]
/// Restricts history file access to the current user.
fn set_private_permissions(path: &Path) -> Result<(), Message> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| {
        Message::new("status-history-permissions-failed").with_text("detail", error)
    })
}

#[cfg(not(unix))]
/// Restricts history file access to the current user.
fn set_private_permissions(_path: &Path) -> Result<(), Message> {
    Ok(())
}

/// Writes the history root record.
pub(crate) fn write_history_root(
    history_file: &StructureHistoryFile,
    side: &str,
    root: &StructureHistoryRootRef<'_>,
) -> Result<(), Message> {
    let path = history_file.directory.join(format!("{side}.json"));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(&path)
        .map_err(|error| Message::new("status-history-write-failed").with_text("detail", error))?;
    let mut writer = io::BufWriter::new(file);
    serde_json::to_writer(&mut writer, root)
        .map_err(|error| Message::new("status-history-write-failed").with_text("detail", error))?;
    writer
        .flush()
        .and_then(|()| writer.get_ref().sync_all())
        .map_err(|error| Message::new("status-history-sync-failed").with_text("detail", error))
}

/// Reads and validates the history root record.
pub(crate) fn read_history_root(
    history_file: &StructureHistoryFile,
    side: &str,
) -> Result<StructureHistoryRoot, Message> {
    let path = history_file.directory.join(format!("{side}.json"));
    let file = File::open(&path)
        .map_err(|error| Message::new("status-history-read-failed").with_text("detail", error))?;
    let mut stream = serde_json::Deserializer::from_reader(BufReader::new(file))
        .into_iter::<StructureHistoryRoot>();
    let root = stream
        .next()
        .transpose()
        .map_err(|error| Message::new("status-history-read-failed").with_text("detail", error))?
        .ok_or_else(|| Message::new("status-history-empty"))?;
    if stream.next().is_some() {
        return Err(Message::new("status-history-trailing-data"));
    }
    if root.version != 1 {
        return Err(Message::new("status-history-version-unsupported")
            .with_number("version", root.version as i64));
    }
    Ok(root)
}

/// Builds the document snapshot needed to restore a structure change.
pub(crate) fn prepare_structure_restore(
    request: &StructureHistoryRestoreRequest,
) -> Result<PreparedStructureRestore, Message> {
    let (
        document_id,
        mut tables,
        root_source,
        view_table_id,
        view_layout,
        selection,
        scroll,
        target,
        cross_sheet_references,
        root_text_overlay,
    ) = {
        let side = if request.redo { "after" } else { "before" };
        let mut root = read_history_root(&request.history_file, side)?;
        if root.document_id != request.document_id {
            return Err(Message::new("status-history-document-mismatch"));
        }
        if !root
            .tables
            .iter()
            .any(|table| table.id == root.view_table_id)
            || !root.tables.iter().any(|table| table.id == root.target.0)
        {
            return Err(Message::new("status-history-table-missing"));
        }
        (
            root.document_id,
            std::mem::take(&mut root.tables),
            root.source.into_source()?.map(Arc::new),
            root.view_table_id,
            Arc::new(root.view_layout),
            root.selection,
            root.scroll,
            root.target,
            root.cross_sheet_references,
            std::mem::take(&mut root.text_overlay),
        )
    };
    for table in &mut tables {
        table.rebuild_axis_index();
    }
    let names = tables
        .iter()
        .map(|table| table.name.as_str())
        .collect::<Vec<_>>();
    let mut document = Document::new(document_id, "履歴復元", &names);
    document.tables = tables;
    document.active_table = view_table_id;
    document.views = document
        .tables
        .iter()
        .map(|table| (table.id, crate::document::TableViewState::default()))
        .collect();
    if let Some(view) = document.views.get_mut(&view_table_id) {
        view.layout = Arc::clone(&view_layout);
        view.selection = selection;
        view.scroll = scroll;
    }
    let source = root_source;
    document.source = source.clone();
    let keep_text_overlay = true;
    let text_overlay_cells = apply_requested_text_overlay(
        &mut document,
        0,
        0,
        &root_text_overlay,
        &[],
        keep_text_overlay,
    )?;
    let calculation = Arc::new(CalculationSnapshot::from_document(
        &document,
        cross_sheet_references,
    ));
    Ok(PreparedStructureRestore {
        document_id,
        calculation_revision: request.calculation_revision,
        history_file: request.history_file.clone(),
        redo: request.redo,
        replace_table_set: request.replace_table_set,
        tables: std::mem::take(&mut document.tables),
        text_overlay_cells,
        source,
        view_table_id,
        view_layout,
        selection,
        scroll,
        target,
        calculation,
    })
}
