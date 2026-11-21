//!  Checks the XLSX package before the spreadsheet adapter reads its contents.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};

use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};
use zip::ZipArchive;

use super::{ConversionClass, ConversionEntry, parse_cell_reference};
use crate::document::{MAX_COLUMNS, MAX_ROWS};
use crate::i18n::Message;

mod classify;
mod error;
mod package;
mod shared;
mod worksheet;
mod xml;

use classify::*;
use package::*;
use shared::*;
use worksheet::*;
use xml::*;

pub use error::{XlsxResourceLimit, XlsxScanError};

/// Sets the allowed package sizes and counts before any XLSX content is loaded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XlsxPackageLimits {
    pub max_compressed_bytes: u64,
    pub max_expanded_bytes: u64,
    pub max_part_expanded_bytes: u64,
    pub max_parts: usize,
    pub max_cells: usize,
    pub max_sheets: usize,
    pub max_rows: usize,
    pub max_columns: usize,
    pub max_xml_events: usize,
    pub max_xml_attribute_bytes: usize,
    pub max_xml_attributes_per_element: usize,
    pub max_xml_element_attribute_bytes: usize,
    pub max_xml_name_bytes: usize,
    pub max_xml_text_bytes: usize,
    pub max_shared_string_bytes: u64,
    pub max_xml_metadata_elements: usize,
    pub max_report_entries: usize,
    pub max_diagnostic_bytes: usize,
    pub max_part_name_bytes: usize,
    pub max_relationships: usize,
    pub max_xml_metadata_bytes: usize,
}

// Account for per-cell string/vector headers and both reader/model rich-run allocations.
pub(super) const SHARED_STRING_VALUE_OVERHEAD: u64 = 64;

pub(super) const RICH_TEXT_RUN_OVERHEAD: u64 =
    (std::mem::size_of::<umya_spreadsheet::TextElement>()
        + std::mem::size_of::<crate::xlsx_adapter::XlsxTextRun>()
        + 16) as u64;

pub(super) const RICH_TEXT_PROPERTIES_OVERHEAD: u64 =
    std::mem::size_of::<umya_spreadsheet::Font>() as u64;

impl Default for XlsxPackageLimits {
    // Set conservative limits for compressed and expanded XLSX package data.
    fn default() -> Self {
        Self {
            max_compressed_bytes: 64 * 1024 * 1024,
            max_expanded_bytes: 256 * 1024 * 1024,
            max_part_expanded_bytes: 64 * 1024 * 1024,
            max_parts: 4096,
            max_cells: 500_000,
            max_sheets: 256,
            max_rows: MAX_ROWS,
            max_columns: MAX_COLUMNS,
            max_xml_events: 6_000_000,
            max_xml_attribute_bytes: 16 * 1024,
            max_xml_attributes_per_element: 128,
            max_xml_element_attribute_bytes: 64 * 1024,
            max_xml_name_bytes: 256,
            max_xml_text_bytes: 4 * 1024 * 1024,
            max_shared_string_bytes: 4 * 1024 * 1024,
            max_xml_metadata_elements: 100_000,
            max_report_entries: 10_000,
            max_diagnostic_bytes: 4 * 1024,
            max_part_name_bytes: 1_024,
            max_relationships: 100_000,
            max_xml_metadata_bytes: 32 * 1024 * 1024,
        }
    }
}

// Sheets, sizes, and conversion items found by the limited OOXML check.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XlsxPackageInspection {
    pub date_1904: bool,
    pub sheets: Vec<XlsxSheetInspection>,
    pub cell_count: usize,
    pub expanded_bytes: u64,
    pub entries: Vec<ConversionEntry>,
}

impl XlsxPackageInspection {
    /// Check ZIP data, known OOXML parts, and cell ranges before XLSX parsing.
    /// Bound each part by size, discard its XML after scanning, and keep unknown data out of the model.
    pub fn scan(bytes: &[u8], limits: XlsxPackageLimits) -> Result<Self, XlsxScanError> {
        if bytes.len() as u64 > limits.max_compressed_bytes {
            return Err(limit_error(
                XlsxResourceLimit::CompressedBytes,
                limits.max_compressed_bytes,
            ));
        }
        let mut archive = ZipArchive::new(Cursor::new(bytes))
            .map_err(|error| XlsxScanError::InvalidArchive(error.to_string()))?;
        if archive.len() > limits.max_parts {
            return Err(limit_error(
                XlsxResourceLimit::ZipParts,
                limits.max_parts as u64,
            ));
        }

        let mut expanded_bytes = 0_u64;
        let mut names = BTreeSet::new();
        let mut entries = vec![
            ConversionEntry::new(
                ConversionClass::Retained,
                None,
                None,
                None,
                "対応範囲のセル値・式・保存済み結果・書式を中間モデルへ変換します",
            )
            .with_message(Message::new("report-reason-xlsx-supported-data")),
        ];
        let mut core_xml = BTreeMap::<String, Vec<u8>>::new();
        let mut relationship_sets = BTreeMap::<String, Vec<Relationship>>::new();
        let mut relationship_count = 0_usize;
        let mut worksheet_parts = BTreeSet::new();
        let mut deferred_parts = Vec::new();
        let mut shared_string_lengths = Vec::new();

        for index in 0..archive.len() {
            let mut part = archive
                .by_index(index)
                .map_err(|error| XlsxScanError::InvalidArchive(error.to_string()))?;
            let name = part.name().to_owned();
            if name.len() > limits.max_part_name_bytes {
                return Err(limit_error(
                    XlsxResourceLimit::PartNameBytes,
                    limits.max_part_name_bytes as u64,
                ));
            }
            if !names.insert(name.clone()) {
                return Err(XlsxScanError::InvalidPackage(
                    crate::i18n::Message::new("status-xlsx-package-duplicate-part")
                        .with_text("part", &name),
                ));
            }
            let declared_size = part.size();
            if declared_size > limits.max_part_expanded_bytes {
                return Err(limit_error(
                    XlsxResourceLimit::PartExpandedBytes,
                    limits.max_part_expanded_bytes,
                ));
            }
            expanded_bytes = expanded_bytes.checked_add(declared_size).ok_or_else(|| {
                limit_error(XlsxResourceLimit::ExpandedBytes, limits.max_expanded_bytes)
            })?;
            if expanded_bytes > limits.max_expanded_bytes {
                return Err(limit_error(
                    XlsxResourceLimit::ExpandedBytes,
                    limits.max_expanded_bytes,
                ));
            }

            let mut content = Vec::new();
            part.by_ref()
                .take(limits.max_part_expanded_bytes.saturating_add(1))
                .read_to_end(&mut content)
                .map_err(|error| XlsxScanError::InvalidArchive(error.to_string()))?;
            if content.len() as u64 > limits.max_part_expanded_bytes {
                return Err(limit_error(
                    XlsxResourceLimit::ActualPartExpandedBytes,
                    limits.max_part_expanded_bytes,
                ));
            }
            if content.len() as u64 != declared_size {
                return Err(XlsxScanError::InvalidPackage(
                    crate::i18n::Message::new("status-xlsx-package-part-size-mismatch")
                        .with_text("part", &name),
                ));
            }
            if part.is_dir() {
                continue;
            }

            if name.ends_with(".xml") || name.ends_with(".rels") {
                validate_xml_limits(&content, &name, limits)?;
            }

            if name.ends_with(".rels") {
                let relationships = parse_relationships(&content, &name, &mut entries, limits)?;
                relationship_count = relationship_count.saturating_add(relationships.len());
                if relationship_count > limits.max_relationships {
                    return Err(limit_error(
                        XlsxResourceLimit::RelationshipCount,
                        limits.max_relationships as u64,
                    ));
                }
                add_relationship_issues(&relationships, &name, &mut entries, limits)?;
                relationship_sets.insert(name.clone(), relationships);
            } else if name == "[Content_Types].xml" {
                core_xml.insert(name.clone(), content.clone());
                entries.extend(parse_content_types(&content, limits)?);
            } else if name == "xl/workbook.xml" {
                core_xml.insert(name.clone(), content.clone());
                entries.extend(scan_known_xml(&content, &name, limits)?);
            } else if name == "xl/styles.xml" {
                entries.extend(scan_known_xml(&content, &name, limits)?);
            } else if name == "xl/sharedStrings.xml" {
                let result = scan_shared_strings(&content, &name, limits)?;
                shared_string_lengths = result.lengths;
                entries.extend(result.entries);
            } else if is_worksheet_candidate(&name) {
                worksheet_parts.insert(name.clone());
            } else {
                // Delay unknown part classification until workbook relationships identify sheets.
                deferred_parts.push(name.clone());
            }
            ensure_report_limit(&entries, limits)?;
        }

        if !core_xml.contains_key("[Content_Types].xml") {
            return Err(XlsxScanError::InvalidPackage(crate::i18n::Message::new(
                "status-xlsx-package-content-types-missing",
            )));
        }
        let workbook_xml = core_xml.get("xl/workbook.xml").ok_or_else(|| {
            XlsxScanError::InvalidPackage(crate::i18n::Message::new(
                "status-xlsx-package-workbook-missing",
            ))
        })?;
        let sheets = parse_workbook_sheets(workbook_xml, limits)?;
        if sheets.is_empty() {
            return Err(XlsxScanError::InvalidPackage(crate::i18n::Message::new(
                "status-xlsx-package-no-sheets",
            )));
        }
        if sheets.len() > limits.max_sheets {
            return Err(limit_error(
                XlsxResourceLimit::SheetCount,
                limits.max_sheets as u64,
            ));
        }
        let workbook_relationships = relationship_sets
            .get("xl/_rels/workbook.xml.rels")
            .ok_or_else(|| {
                XlsxScanError::InvalidPackage(crate::i18n::Message::new(
                    "status-xlsx-package-workbook-relationships-missing",
                ))
            })?;
        let targets = workbook_relationships
            .iter()
            .filter(|relationship| relationship.kind == "worksheet")
            .map(|relationship| (relationship.id.clone(), relationship.target.clone()))
            .collect::<BTreeMap<_, _>>();
        let all_relationship_targets = relationship_sets
            .iter()
            .flat_map(|(relationship_part, relationships)| {
                relationships.iter().filter_map(move |relationship| {
                    if relationship.external {
                        return None;
                    }
                    Some((
                        normalize_relationship_target(relationship_part, &relationship.target),
                        relationship_part.clone(),
                    ))
                })
            })
            .collect::<Vec<_>>();
        for (target, source) in all_relationship_targets {
            if !names.contains(&target) {
                return Err(XlsxScanError::InvalidPackage(
                    crate::i18n::Message::new("status-xlsx-package-target-missing")
                        .with_text("source", &source)
                        .with_text("target", &target),
                ));
            }
        }

        let resolved_sheets = sheets
            .into_iter()
            .enumerate()
            .map(|(index, sheet)| {
                let target = targets.get(&sheet.relationship_id).ok_or_else(|| {
                    XlsxScanError::InvalidPackage(
                        crate::i18n::Message::new("status-xlsx-package-sheet-relationship-missing")
                            .with_text("sheet", &sheet.name),
                    )
                })?;
                let part_name = normalize_relationship_target("xl/_rels/workbook.xml.rels", target);
                if !names.contains(&part_name) {
                    return Err(XlsxScanError::InvalidPackage(
                        crate::i18n::Message::new("status-xlsx-package-sheet-part-missing")
                            .with_text("sheet", &sheet.name)
                            .with_text("part", &part_name),
                    ));
                }
                Ok((index + 1, sheet, part_name))
            })
            .collect::<Result<Vec<_>, XlsxScanError>>()?;

        let referenced_worksheet_parts = resolved_sheets
            .iter()
            .map(|(_, _, part_name)| part_name.as_str())
            .collect::<BTreeSet<_>>();
        for name in deferred_parts {
            if !referenced_worksheet_parts.contains(name.as_str()) {
                classify_part(&name, &mut entries);
                ensure_report_limit(&entries, limits)?;
            }
        }
        for orphan in worksheet_parts
            .iter()
            .filter(|part| !referenced_worksheet_parts.contains(part.as_str()))
        {
            classify_part(orphan, &mut entries);
        }

        let mut inspected_sheets = Vec::with_capacity(resolved_sheets.len());
        let mut cell_count = 0_usize;
        let mut effective_expanded_bytes = expanded_bytes;
        for (order, sheet, part_name) in resolved_sheets {
            if sheet.state != "visible" {
                let entry = ConversionEntry::new(
                    ConversionClass::Omitted,
                    Some(order),
                    Some(sheet.name.clone()),
                    None,
                    format!("非表示シート状態は出力で保持されません: {}", sheet.state),
                )
                .formatting_only()
                .with_message(Message::new("report-reason-xlsx-hidden-sheet-state"));
                push_report_entry(&mut entries, entry, limits)?;
            }
            let mut sheet_limits = limits;
            sheet_limits.max_report_entries =
                limits.max_report_entries.saturating_sub(entries.len());
            let mut part = archive
                .by_name(&part_name)
                .map_err(|error| XlsxScanError::InvalidArchive(error.to_string()))?;
            let declared_size = part.size();
            if declared_size > limits.max_part_expanded_bytes {
                return Err(limit_error(
                    XlsxResourceLimit::PartExpandedBytes,
                    limits.max_part_expanded_bytes,
                ));
            }
            let mut content = Vec::new();
            part.by_ref()
                .take(limits.max_part_expanded_bytes.saturating_add(1))
                .read_to_end(&mut content)
                .map_err(|error| XlsxScanError::InvalidArchive(error.to_string()))?;
            if content.len() as u64 > limits.max_part_expanded_bytes {
                return Err(limit_error(
                    XlsxResourceLimit::ActualPartExpandedBytes,
                    limits.max_part_expanded_bytes,
                ));
            }
            if content.len() as u64 != declared_size {
                return Err(XlsxScanError::InvalidPackage(
                    crate::i18n::Message::new("status-xlsx-package-part-size-mismatch")
                        .with_text("part", &part_name),
                ));
            }
            validate_xml_limits(&content, &part_name, limits)?;
            let result = scan_worksheet(
                &content,
                WorksheetScanMetadata {
                    part: &part_name,
                    sheet_name: &sheet.name,
                    sheet_order: order,
                    limits: sheet_limits,
                },
                cell_count,
                &shared_string_lengths,
                &mut effective_expanded_bytes,
            )?;
            cell_count = result.total_cells;
            entries.extend(result.entries);
            ensure_report_limit(&entries, limits)?;
            inspected_sheets.push(XlsxSheetInspection {
                order,
                name: sheet.name,
                part_name,
                cell_count: result.sheet_cell_count,
            });
        }

        let date_1904 = parse_date_1904(workbook_xml, limits)?;
        Ok(Self {
            date_1904,
            sheets: inspected_sheets,
            cell_count,
            expanded_bytes: effective_expanded_bytes,
            entries,
        })
    }
}

// Sheet information in workbook order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XlsxSheetInspection {
    pub order: usize,
    pub name: String,
    pub part_name: String,
    pub cell_count: usize,
}
