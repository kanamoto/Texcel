use super::*;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::ui) enum SearchScope {
    #[default]
    Selection,
    Table,
    Document,
}

#[derive(Debug)]
pub(in crate::ui) struct SearchDialogState {
    pub(in crate::ui) document_id: DocumentId,
    pub(in crate::ui) table_id: u64,
    pub(in crate::ui) query: String,
    pub(in crate::ui) replacement: String,
    pub(in crate::ui) scope: SearchScope,
    pub(in crate::ui) representation: crate::structure::SearchRepresentation,
    pub(in crate::ui) case_sensitive: bool,
    pub(in crate::ui) regular_expression: bool,
    pub(in crate::ui) whole_cell: bool,
    pub(in crate::ui) replace_mode: bool,
    pub(in crate::ui) message: Option<Message>,
    pub(in crate::ui) preview: Option<crate::structure::PreparedSearch>,
    pub(in crate::ui) preview_key: Option<SearchJobKey>,
    pub(in crate::ui) focus_query: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::ui) struct SearchJobKey {
    pub(in crate::ui) document_id: DocumentId,
    pub(in crate::ui) document_revision: u64,
    pub(in crate::ui) calculation_revision: u64,
    pub(in crate::ui) table_id: u64,
    pub(in crate::ui) view_layout_revision: u64,
    pub(in crate::ui) query: String,
    pub(in crate::ui) replacement: String,
    pub(in crate::ui) scope: SearchScope,
    pub(in crate::ui) representation: crate::structure::SearchRepresentation,
    pub(in crate::ui) case_sensitive: bool,
    pub(in crate::ui) regular_expression: bool,
    pub(in crate::ui) whole_cell: bool,
    pub(in crate::ui) replace_mode: bool,
    pub(in crate::ui) ranges: Vec<(u64, CellRange)>,
}

#[derive(Clone, Debug)]
pub(in crate::ui) struct PendingSearch {
    pub(in crate::ui) token: JobToken,
    pub(in crate::ui) key: SearchJobKey,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) enum DataEditKind {
    TrimWhitespace,
    SplitColumn,
    JoinColumns,
    Transpose,
    SortRows,
    DetectDuplicates,
    RemoveDuplicates,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::ui) struct SortKeyEditor {
    pub(in crate::ui) column: String,
    pub(in crate::ui) kind: crate::structure::SortValueKind,
    pub(in crate::ui) descending: bool,
}

#[derive(Debug)]
pub(in crate::ui) struct DataEditDialogState {
    pub(in crate::ui) document_id: DocumentId,
    pub(in crate::ui) table_id: u64,
    pub(in crate::ui) kind: DataEditKind,
    pub(in crate::ui) range: CellRange,
    pub(in crate::ui) range_text: String,
    pub(in crate::ui) selected_ranges: Vec<CellRange>,
    pub(in crate::ui) delimiter: String,
    pub(in crate::ui) destination_column: String,
    pub(in crate::ui) overwrite: bool,
    pub(in crate::ui) sort_keys: Vec<SortKeyEditor>,
    pub(in crate::ui) preview: Option<crate::structure::PreparedDataEdit>,
    pub(in crate::ui) preview_key: Option<DataEditJobKey>,
    pub(in crate::ui) message: Option<Message>,
}

impl DataEditDialogState {
    /// Copies editable values while removing state tied to a prior worker preview.
    pub(in crate::ui) fn submission_snapshot(&self) -> Self {
        Self {
            document_id: self.document_id,
            table_id: self.table_id,
            kind: self.kind,
            range: self.range,
            range_text: self.range_text.clone(),
            selected_ranges: self.selected_ranges.clone(),
            delimiter: self.delimiter.clone(),
            destination_column: self.destination_column.clone(),
            overwrite: self.overwrite,
            sort_keys: self.sort_keys.clone(),
            preview: None,
            preview_key: None,
            message: self.message.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::ui) struct DataEditJobKey {
    pub(in crate::ui) document_id: DocumentId,
    pub(in crate::ui) document_revision: u64,
    pub(in crate::ui) calculation_revision: u64,
    pub(in crate::ui) table_id: u64,
    pub(in crate::ui) view_layout_revision: u64,
    pub(in crate::ui) current_selection: Vec<CellRange>,
    pub(in crate::ui) kind: DataEditKind,
    pub(in crate::ui) range: CellRange,
    pub(in crate::ui) range_text: String,
    pub(in crate::ui) selected_ranges: Vec<CellRange>,
    pub(in crate::ui) delimiter: String,
    pub(in crate::ui) destination_column: String,
    pub(in crate::ui) overwrite: bool,
    pub(in crate::ui) sort_keys: Vec<SortKeyEditor>,
}

#[derive(Clone, Debug)]
pub(in crate::ui) struct PendingDataEdit {
    pub(in crate::ui) token: JobToken,
    pub(in crate::ui) key: DataEditJobKey,
}
