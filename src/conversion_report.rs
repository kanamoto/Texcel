//!  Records which XLSX data is kept, converted, or dropped, and performs limited OOXML checks.
//!
//!  The checks only read ZIP part names, relationships, content types, and known sheet XML elements.
//!  They do not load unknown XML into edit state or rebuild the package.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
#[cfg(test)]
pub(crate) use xlsx_scan::XlsxResourceLimit;

/// A category that tells the user how converted information was handled.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ConversionClass {
    Retained,
    Converted,
    Omitted,
    Unprocessable,
}

/// Describes whether a reported loss can change imported table content.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum ConversionImpact {
    /// The change may affect a value, formula, or table structure.
    #[default]
    Content,
    /// The change affects presentation, visibility, or print-only details.
    Formatting,
}

impl ConversionClass {
    /// Returns this report category in the selected language.
    pub fn label(self, language: crate::i18n::Language) -> String {
        match self {
            Self::Retained => crate::i18n::text(language, "report-kept"),
            Self::Converted => crate::i18n::text(language, "report-converted"),
            Self::Omitted => crate::i18n::text(language, "report-omitted"),
            Self::Unprocessable => crate::i18n::text(language, "report-unprocessable"),
        }
    }
}

/// A conversion item. Omit sheet and range for workbook-wide data with neither known.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ConversionEntry {
    pub class: ConversionClass,
    pub impact: ConversionImpact,
    pub sheet_order: Option<usize>,
    pub sheet_name: Option<String>,
    pub range: Option<String>,
    pub reason: String,
    pub message: Option<crate::i18n::Message>,
}

impl ConversionEntry {
    /// Creates an item for a sheet cell or for the workbook as a whole.
    pub fn new(
        class: ConversionClass,
        sheet_order: Option<usize>,
        sheet_name: Option<String>,
        range: Option<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            class,
            impact: ConversionImpact::Content,
            sheet_order,
            sheet_name,
            range,
            reason: reason.into(),
            message: None,
        }
    }

    /// Marks a known loss that changes formatting or print details only.
    pub fn formatting_only(mut self) -> Self {
        self.impact = ConversionImpact::Formatting;
        self
    }

    /// Adds a message key so report views can translate this item later.
    pub fn with_message(mut self, message: crate::i18n::Message) -> Self {
        self.message = Some(message);
        self
    }
}

/// Report counts by category.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ConversionSummary {
    pub retained: usize,
    pub converted: usize,
    pub omitted: usize,
    pub unprocessable: usize,
}

impl ConversionSummary {
    /// Adds an item to the count for its conversion category.
    fn include(&mut self, class: ConversionClass) {
        match class {
            ConversionClass::Retained => self.retained += 1,
            ConversionClass::Converted => self.converted += 1,
            ConversionClass::Omitted => self.omitted += 1,
            ConversionClass::Unprocessable => self.unprocessable += 1,
        }
    }

    // Returns whether an unsupported item must stop the export.
    pub fn blocks_export(self) -> bool {
        self.unprocessable > 0
    }

    // Returns whether the user must confirm before export.
    pub fn requires_confirmation(self) -> bool {
        self.converted > 0 || self.omitted > 0
    }
}

// A display group that combines items with the same handling and reason into counts and ranges.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversionGroup {
    pub class: ConversionClass,
    pub impact: ConversionImpact,
    pub sheet_order: Option<usize>,
    pub sheet_name: Option<String>,
    pub reason: String,
    pub message: Option<crate::i18n::Message>,
    pub count: usize,
    pub ranges: Vec<String>,
}

// A conversion report fixed to the document after a successful load.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportReport {
    pub file_name: String,
    pub imported_at: DateTime<Utc>,
    pub input_format: String,
    pub entries: Vec<ConversionEntry>,
    summary: ConversionSummary,
    groups: Vec<ConversionGroup>,
}

impl ImportReport {
    // Builds a report in a stable order from confirmed load information.
    pub fn new(
        file_name: impl Into<String>,
        imported_at: DateTime<Utc>,
        input_format: impl Into<String>,
        mut entries: Vec<ConversionEntry>,
    ) -> Self {
        entries.sort_by(compare_entries);
        let file_name = file_name.into();
        let input_format = input_format.into();
        let summary = summarize(&entries);
        let groups = group_entries(&entries);
        Self {
            file_name,
            imported_at,
            input_format,
            entries,
            summary,
            groups,
        }
    }

    // Records the current UTC time and builds a report for a successful load.
    pub fn now(
        file_name: impl Into<String>,
        input_format: impl Into<String>,
        entries: Vec<ConversionEntry>,
    ) -> Self {
        Self::new(file_name, Utc::now(), input_format, entries)
    }

    // Returns counts by category.
    pub fn summary(&self) -> ConversionSummary {
        self.summary
    }

    // Returns display rows that combine items with the same category and reason.
    pub fn groups(&self) -> &[ConversionGroup] {
        &self.groups
    }

    /// Returns whether a content loss requires an automatic report.
    pub fn requires_dialog(&self) -> bool {
        self.entries.iter().any(|entry| {
            entry.impact == ConversionImpact::Content && entry.class != ConversionClass::Retained
        })
    }

    /// Returns whether formatting-only losses can be shown as a short notice.
    pub fn has_formatting_notes(&self) -> bool {
        self.entries.iter().any(|entry| {
            entry.impact == ConversionImpact::Formatting && entry.class != ConversionClass::Retained
        })
    }

    /// Formats the retained instant using the current OS region and time zone.
    pub fn formatted_imported_at(&self) -> String {
        import_time::format_local(self.imported_at)
    }

    // Renders one complete report in the language captured when saving starts.
    pub fn to_text(&self, language: crate::i18n::Language) -> String {
        render::render_report(
            language,
            "report-title-import",
            &self.file_name,
            &self.formatted_imported_at(),
            &self.input_format,
            None,
            &self.entries,
        )
    }
}

// State that allows one automatic display and can show the same report again after it is closed.
#[derive(Clone, Debug)]
pub struct ImportReportState {
    report: Arc<ImportReport>,
    source_path: PathBuf,
    automatic_shown: bool,
}

impl ImportReportState {
    // Attaches the confirmed report to the document after a successful load.
    pub fn new(report: Arc<ImportReport>, source_path: PathBuf) -> Self {
        Self {
            report,
            source_path,
            automatic_shown: false,
        }
    }

    // Returns the same report only when it has notes and has not been shown automatically.
    pub fn take_automatic_dialog(&mut self) -> Option<Arc<ImportReport>> {
        if self.automatic_shown || !self.report.requires_dialog() {
            return None;
        }
        self.automatic_shown = true;
        Some(Arc::clone(&self.report))
    }

    // Shows the report from the same load again from the menu.
    pub fn reopen(&self) -> Arc<ImportReport> {
        Arc::clone(&self.report)
    }

    // The source path used to avoid overwriting the original XLSX.
    pub fn source_path(&self) -> &Path {
        &self.source_path
    }
}

// An export report tied to a document version and output settings, separate from the load report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportReport {
    pub document_revision: u64,
    pub output_format: String,
    pub settings_summary: crate::i18n::Message,
    pub entries: Vec<ConversionEntry>,
    summary: ConversionSummary,
    groups: Vec<ConversionGroup>,
}

impl ExportReport {
    // Fixes the output version and settings and gives details a stable order.
    pub fn new(
        document_revision: u64,
        output_format: impl Into<String>,
        settings_summary: crate::i18n::Message,
        mut entries: Vec<ConversionEntry>,
    ) -> Self {
        entries.sort_by(compare_entries);
        let output_format = output_format.into();
        let summary = summarize(&entries);
        let groups = group_entries(&entries);
        Self {
            document_revision,
            output_format,
            settings_summary,
            entries,
            summary,
            groups,
        }
    }

    // Stops output when an unsupported item is present.
    pub fn can_export(&self) -> bool {
        !self.summary().blocks_export()
    }

    // Asks the user to continue only when data is converted or dropped.
    pub fn requires_confirmation(&self) -> bool {
        self.summary().requires_confirmation()
    }

    // Returns counts by category.
    pub fn summary(&self) -> ConversionSummary {
        self.summary
    }

    // Combines items with the same category, sheet, and reason for list display.
    pub fn groups(&self) -> &[ConversionGroup] {
        &self.groups
    }

    // Renders one complete report in the language captured when saving starts.
    pub fn to_text(&self, language: crate::i18n::Language) -> String {
        render::render_report(
            language,
            "report-title-export",
            "",
            "",
            &self.output_format,
            Some((&self.document_revision, &self.settings_summary)),
            &self.entries,
        )
    }
}

/// Counts conversion items and their categories.
fn summarize(entries: &[ConversionEntry]) -> ConversionSummary {
    let mut summary = ConversionSummary::default();
    for entry in entries {
        summary.include(entry.class);
    }
    summary
}

/// Combines report items with matching display details.
fn group_entries(entries: &[ConversionEntry]) -> Vec<ConversionGroup> {
    let mut groups = BTreeMap::<
        (
            ConversionImpact,
            ConversionClass,
            Option<usize>,
            Option<String>,
            String,
            Option<crate::i18n::Message>,
        ),
        (ConversionGroup, BTreeSet<String>),
    >::new();
    for entry in entries {
        let key = (
            entry.impact,
            entry.class,
            entry.sheet_order,
            entry.sheet_name.clone(),
            entry.reason.clone(),
            entry.message.clone(),
        );
        let (group, seen_ranges) = groups.entry(key).or_insert_with(|| {
            (
                ConversionGroup {
                    class: entry.class,
                    impact: entry.impact,
                    sheet_order: entry.sheet_order,
                    sheet_name: entry.sheet_name.clone(),
                    reason: entry.reason.clone(),
                    message: entry.message.clone(),
                    count: 0,
                    ranges: Vec::new(),
                },
                BTreeSet::new(),
            )
        });
        group.count += 1;
        if let Some(range) = &entry.range
            && seen_ranges.insert(range.clone())
        {
            group.ranges.push(range.clone());
        }
    }
    groups.into_values().map(|(group, _)| group).collect()
}

/// Sorts report entries by sheet, range, and reason.
fn compare_entries(left: &ConversionEntry, right: &ConversionEntry) -> std::cmp::Ordering {
    left.impact
        .cmp(&right.impact)
        .then_with(|| {
            left.sheet_order
                .unwrap_or(usize::MAX)
                .cmp(&right.sheet_order.unwrap_or(usize::MAX))
        })
        .then_with(|| compare_ranges(left.range.as_deref(), right.range.as_deref()))
        .then_with(|| left.class.cmp(&right.class))
        .then_with(|| left.reason.cmp(&right.reason))
        .then_with(|| left.sheet_name.cmp(&right.sheet_name))
}

/// Orders optional cell ranges for report display.
fn compare_ranges(left: Option<&str>, right: Option<&str>) -> std::cmp::Ordering {
    match (left.and_then(range_order), right.and_then(range_order)) {
        (Some(left_key), Some(right_key)) => left_key.cmp(&right_key),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => left.cmp(&right),
    }
}

/// Parses a cell range into sortable row and column bounds.
fn range_order(range: &str) -> Option<(usize, usize, usize, usize)> {
    let (first, last) = range.split_once(':').unwrap_or((range, range));
    let (first_row, first_column) = parse_cell_reference(first)?;
    let (last_row, last_column) = parse_cell_reference(last)?;
    Some((first_row, first_column, last_row, last_column))
}

/// Parses a spreadsheet cell reference.
fn parse_cell_reference(source: &str) -> Option<(usize, usize)> {
    let source = source.trim().trim_start_matches('$');
    let mut column = 0_usize;
    let mut index = 0;
    for byte in source.bytes() {
        if byte.is_ascii_alphabetic() {
            column = column
                .checked_mul(26)?
                .checked_add(usize::from(byte.to_ascii_uppercase() - b'A') + 1)?;
            index += 1;
        } else {
            break;
        }
    }
    let row = source
        .get(index..)?
        .trim_start_matches('$')
        .parse::<usize>()
        .ok()?;
    if row == 0 || column == 0 {
        None
    } else {
        Some((row - 1, column - 1))
    }
}

mod import_time;
mod render;

mod xlsx_scan;
#[cfg(test)]
use xlsx_scan::{
    RICH_TEXT_PROPERTIES_OVERHEAD, RICH_TEXT_RUN_OVERHEAD, SHARED_STRING_VALUE_OVERHEAD,
};
pub use xlsx_scan::{XlsxPackageInspection, XlsxPackageLimits, XlsxScanError};

#[cfg(test)]
#[path = "conversion_report_test.rs"]
mod tests;
