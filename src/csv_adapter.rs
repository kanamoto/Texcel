//!  Reads CSV/TSV files and updates cells while preserving the source bytes.
//!
//!  The `csv` crate interprets quoting rules. A separate scan records the original record and field
//!  byte ranges and quote state so unchanged regions can be written as they were.

use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::document::{
    Cell, CellAddress, CellInput, Document, DocumentId, StructureAxis, StructureOperation,
    StructurePlan,
};

mod codec;
mod document;
mod edits;
mod read;
mod structure;

pub use codec::analyze_delimiters;
use codec::{
    combine_newlines, decode_text, desired_new_width, desired_record_width, encode_new_record,
    encode_replacement, encode_text, mapped_record_width, parse_record, resolve_encoding,
    scan_records, validate_delimiter,
};

/// Checks whether one chosen character can be used as a CSV delimiter.
pub fn validate_delimiter_choice(delimiter: char) -> Result<(), CsvError> {
    validate_delimiter(delimiter)
}

pub(crate) const MAX_CSV_SOURCE_BYTES: usize = 64 * 1024 * 1024;
const MAX_CSV_ROWS: usize = 1_048_576;
const MAX_CSV_COLUMNS: usize = 16_384;
pub(crate) const MAX_CSV_ROWS_READER: usize = MAX_CSV_ROWS;
pub(crate) const MAX_CSV_COLUMNS_READER: usize = MAX_CSV_COLUMNS;
pub(crate) const MAX_CSV_CELLS: usize = 500_000;

// The input encoding. A UTF-8 BOM is also kept as the first source bytes.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CsvEncoding {
    Utf8,
    Utf8Bom,
    ShiftJis,
}

// The encoding selected when reading a file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncodingChoice {
    // Guess BOM-marked UTF-8 or strict UTF-8, and ask before choosing another encoding.
    Auto,
    // Decode strict UTF-8, while keeping a UTF-8 BOM when present.
    Utf8,
    // The user selected strict Shift_JIS compatible with CP932.
    ShiftJis,
}

// The line ending style. Mixed line endings are kept as they were.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NewlineStyle {
    Lf,
    CrLf,
    Mixed,
    None,
}

// Record width statistics that account for delimiter candidates and quoting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DelimiterCandidate {
    pub delimiter: char,
    pub record_count: usize,
    pub most_common_width: usize,
    pub consistent_record_count: usize,
    pub total_fields: usize,
}

// Returns the delimiter when the result is unique, or sends tied candidates to the UI.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DelimiterAnalysis {
    pub candidates: Vec<DelimiterCandidate>,
    pub suggested: Option<char>,
    /// Small decoded samples for candidate delimiters that need a user choice.
    pub previews: std::collections::BTreeMap<char, Vec<Vec<String>>>,
}

// One CSV field and the byte range to replace in the input file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CsvField {
    pub value: String,
    pub raw_range: Range<usize>,
    pub was_quoted: bool,
}

// One record and its fields. An empty record has zero fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CsvRecord {
    pub raw_range: Range<usize>,
    pub fields: Vec<CsvField>,
}

// The final value of a CSV cell included in a save request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CsvCellChange {
    pub row: usize,
    pub column: usize,
    // `None` means no cell; `Some("")` means an explicit empty field.
    pub value: Option<String>,
}

// Why a CSV file could not be read or safely updated.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CsvError {
    EncodingChoiceRequired,
    InvalidEncoding,
    UnrepresentableCharacter,
    InvalidDelimiter,
    UnclosedQuote,
    InvalidQuoteSyntax,
    InvalidNewline,
    CsvParse(String),
    ParserMismatch,
    NoSafeDelimiterMarker,
    ResourceLimit(crate::i18n::ResourceLimit),
    OutputMismatch,
}

impl std::fmt::Display for CsvError {
    // Show the CSV import or export failure in a short message.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = crate::i18n::csv_error_message(self.clone());
        formatter.write_str(&message.resolve(crate::i18n::Language::English))
    }
}

impl std::error::Error for CsvError {}

// A CSV/TSV document that keeps its original text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CsvDocument {
    bytes: Vec<u8>,
    pub encoding: CsvEncoding,
    pub delimiter: char,
    pub newline: NewlineStyle,
    pub records: Vec<CsvRecord>,
    parsed_cell_count: usize,
    parsed_max_column_count: usize,
    // Keep the row and column shape from when the file was opened, even after saving, so cell undo can remove a newly added tail.
    // When structural row or column commands are added, update this baseline shape with the command.
    origin_record_count: usize,
    origin_field_counts: Vec<usize>,
    origin_ends_with_newline: bool,
    introduced_fields: std::collections::BTreeMap<usize, std::collections::BTreeSet<usize>>,
}

/// CSV edit-origin metadata needed to undo appended rows and fields after restoring a structure root.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct CsvHistoryMetadata {
    origin_record_count: usize,
    origin_field_counts: Vec<usize>,
    origin_ends_with_newline: bool,
    introduced_fields: Vec<(usize, Vec<usize>)>,
}

#[derive(Clone, Debug)]
struct FieldSpan {
    range: Range<usize>,
    was_quoted: bool,
}

#[derive(Clone, Debug)]
struct RecordSpan {
    range: Range<usize>,
    content: Range<usize>,
    fields: Vec<FieldSpan>,
    newline: Option<NewlineStyle>,
}

#[cfg(test)]
#[path = "csv_adapter_test.rs"]
mod tests;
