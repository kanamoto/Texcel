use super::*;

#[derive(Clone, Debug)]
pub(super) struct Relationship {
    pub(super) id: String,
    pub(super) target: String,
    pub(super) kind: String,
    pub(super) external: bool,
}

pub(super) struct SheetScanResult {
    pub(super) sheet_cell_count: usize,
    pub(super) total_cells: usize,
    pub(super) entries: Vec<ConversionEntry>,
}

/// Builds the error for an XLSX package size limit.
pub(super) fn limit_error(limit: XlsxResourceLimit, maximum: u64) -> XlsxScanError {
    XlsxScanError::LimitExceeded { limit, maximum }
}

/// Stops scanning when the conversion report would exceed its limit.
pub(super) fn ensure_report_limit(
    entries: &[ConversionEntry],
    limits: XlsxPackageLimits,
) -> Result<(), XlsxScanError> {
    if entries.len() > limits.max_report_entries {
        return Err(limit_error(
            XlsxResourceLimit::ReportEntryCount,
            limits.max_report_entries as u64,
        ));
    }
    Ok(())
}

/// Adds a conversion item if the report remains within its limit.
pub(super) fn push_report_entry(
    entries: &mut Vec<ConversionEntry>,
    entry: ConversionEntry,
    limits: XlsxPackageLimits,
) -> Result<(), XlsxScanError> {
    let diagnostic_bytes = entry
        .reason
        .len()
        .saturating_add(entry.sheet_name.as_ref().map_or(0, String::len))
        .saturating_add(entry.range.as_ref().map_or(0, String::len));
    if diagnostic_bytes > limits.max_diagnostic_bytes {
        return Err(limit_error(
            XlsxResourceLimit::ReportTextBytes,
            limits.max_diagnostic_bytes as u64,
        ));
    }
    if entries.len() >= limits.max_report_entries {
        return Err(limit_error(
            XlsxResourceLimit::ReportEntryCount,
            limits.max_report_entries as u64,
        ));
    }
    entries.push(entry);
    Ok(())
}

/// Reads workbook relationship targets from the package.
pub(super) fn parse_relationships(
    bytes: &[u8],
    part: &str,
    entries: &mut Vec<ConversionEntry>,
    limits: XlsxPackageLimits,
) -> Result<Vec<Relationship>, XlsxScanError> {
    let mut relationships = Vec::new();
    for (name, attributes) in parse_xml_elements(bytes, part, limits)? {
        if name == "Relationships" {
            continue;
        }
        if name != "Relationship" {
            push_report_entry(
                entries,
                ConversionEntry::new(
                    ConversionClass::Omitted,
                    None,
                    None,
                    None,
                    format!("未分類の非対応relationship要素: {part}:{name}"),
                )
                .with_message(
                    Message::new("report-reason-xlsx-unknown-relationship-element")
                        .with_text("part", part)
                        .with_text("element", &name),
                ),
                limits,
            )?;
            continue;
        }
        let Some(id) = attributes.get("Id") else {
            return Err(XlsxScanError::InvalidPackage(
                crate::i18n::Message::new("status-xlsx-package-relationship-id-missing")
                    .with_text("part", part),
            ));
        };
        let Some(target) = attributes.get("Target") else {
            return Err(XlsxScanError::InvalidPackage(
                crate::i18n::Message::new("status-xlsx-package-relationship-target-missing")
                    .with_text("part", part),
            ));
        };
        let Some(kind) = attributes
            .get("Type")
            .and_then(|value| value.rsplit('/').next())
        else {
            return Err(XlsxScanError::InvalidPackage(
                crate::i18n::Message::new("status-xlsx-package-relationship-type-missing")
                    .with_text("part", part),
            ));
        };
        relationships.push(Relationship {
            id: id.clone(),
            target: target.clone(),
            kind: kind.to_owned(),
            external: attributes
                .get("TargetMode")
                .is_some_and(|value| value.eq_ignore_ascii_case("external")),
        });
    }
    Ok(relationships)
}

/// Builds a report item for an invalid relationship.
pub(super) fn relationship_issue(
    relationship: &Relationship,
    part: &str,
) -> Option<ConversionEntry> {
    let known = matches!(
        relationship.kind.as_str(),
        "officeDocument"
            | "worksheet"
            | "styles"
            | "sharedStrings"
            | "theme"
            | "core-properties"
            | "extended-properties"
            | "hyperlink"
            | "calcChain"
            | "comments"
            | "drawing"
            | "table"
            | "printerSettings"
            | "pivotTable"
            | "pivotCacheDefinition"
            | "pivotCacheRecords"
            | "externalLink"
            | "vmlDrawing"
            | "customXml"
            | "oleObject"
            | "package"
            | "queryTable"
            | "ctrlProp"
            | "person"
            | "threadedComment"
    );
    let (issue, message_key) = match relationship.kind.as_str() {
        "officeDocument"
        | "worksheet"
        | "styles"
        | "sharedStrings"
        | "theme"
        | "core-properties"
        | "extended-properties" => return None,
        "hyperlink" => return None,
        "printerSettings" => (
            "印刷設定は出力へ引き継がれません",
            "report-reason-xlsx-relationship-printer",
        ),
        "comments" | "vmlDrawing" | "threadedComment" => (
            "コメント・注釈は出力へ引き継がれません",
            "report-reason-xlsx-relationship-comments",
        ),
        "drawing" | "table" | "pivotTable" | "pivotCacheDefinition" | "pivotCacheRecords" => (
            "図形・テーブル・ピボットは出力へ引き継がれません",
            "report-reason-xlsx-relationship-drawing",
        ),
        "externalLink" | "package" => (
            "外部ブック・埋め込みファイルは出力へ引き継がれません",
            "report-reason-xlsx-relationship-external",
        ),
        "calcChain" => (
            "計算チェーンは出力時に再生成されます",
            "report-reason-xlsx-relationship-calc-chain",
        ),
        "customXml" | "oleObject" | "queryTable" | "ctrlProp" | "person" => (
            "このXLSX部品は出力へ引き継がれません",
            "report-reason-xlsx-relationship-unsupported",
        ),
        _ if !known => (
            "未分類の非対応relationshipです",
            "report-reason-xlsx-relationship-unknown",
        ),
        _ => (
            "このXLSX relationshipは出力へ引き継がれません",
            "report-reason-xlsx-relationship-other",
        ),
    };
    let entry = ConversionEntry::new(
        ConversionClass::Omitted,
        None,
        None,
        None,
        format!("{part}: {} ({})", issue, relationship.kind),
    )
    .with_message(
        Message::new(message_key)
            .with_text("part", part)
            .with_text("relationship", &relationship.kind),
    );
    Some(if relationship.kind == "printerSettings" {
        entry.formatting_only()
    } else {
        entry
    })
}

/// Records workbook relationships that cannot be followed.
pub(super) fn add_relationship_issues(
    relationships: &[Relationship],
    part: &str,
    entries: &mut Vec<ConversionEntry>,
    limits: XlsxPackageLimits,
) -> Result<(), XlsxScanError> {
    let mut seen = BTreeSet::new();
    for relationship in relationships {
        if let Some(entry) = relationship_issue(relationship, part)
            && seen.insert((entry.class, entry.reason.clone()))
        {
            push_report_entry(entries, entry, limits)?;
        }
    }
    Ok(())
}

/// Reads declared content types from the package.
pub(super) fn parse_content_types(
    bytes: &[u8],
    limits: XlsxPackageLimits,
) -> Result<Vec<ConversionEntry>, XlsxScanError> {
    let known_types = [
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml",
        "application/vnd.openxmlformats-officedocument.theme+xml",
        "application/vnd.openxmlformats-package.core-properties+xml",
        "application/vnd.openxmlformats-officedocument.extended-properties+xml",
        "application/vnd.openxmlformats-package.relationships+xml",
        "application/xml",
    ];
    let mut entries = Vec::new();
    for (name, attributes) in parse_xml_elements(bytes, "[Content_Types].xml", limits)? {
        if name == "Types" {
            continue;
        }
        if name != "Override" && name != "Default" {
            push_report_entry(
                &mut entries,
                ConversionEntry::new(
                    ConversionClass::Omitted,
                    None,
                    None,
                    None,
                    format!("未分類の非対応Content Types要素: {name}"),
                )
                .with_message(
                    Message::new("report-reason-xlsx-unknown-content-element")
                        .with_text("element", &name),
                ),
                limits,
            )?;
            continue;
        }
        let Some(content_type) = attributes.get("ContentType") else {
            continue;
        };
        if !known_types.contains(&content_type.as_str()) {
            let part = attributes
                .get("PartName")
                .or_else(|| attributes.get("Extension"))
                .map_or("Content Types".to_owned(), |value| {
                    value.trim_start_matches('/').to_owned()
                });
            push_report_entry(
                &mut entries,
                ConversionEntry::new(
                    ConversionClass::Omitted,
                    None,
                    None,
                    None,
                    format!("未分類のContent Typeです: {part} ({content_type})"),
                )
                .with_message(
                    Message::new("report-reason-xlsx-unknown-content-type")
                        .with_text("part", &part)
                        .with_text("content_type", content_type),
                ),
                limits,
            )?;
        }
    }
    Ok(entries)
}

pub(super) struct WorkbookSheet {
    pub(super) name: String,
    pub(super) relationship_id: String,
    pub(super) state: String,
}

/// Reads sheet names and relationship IDs from the workbook part.
pub(super) fn parse_workbook_sheets(
    bytes: &[u8],
    limits: XlsxPackageLimits,
) -> Result<Vec<WorkbookSheet>, XlsxScanError> {
    let mut sheets = Vec::new();
    for (name, attributes) in parse_xml_elements(bytes, "xl/workbook.xml", limits)? {
        if name != "sheet" {
            continue;
        }
        let sheet_name = attributes.get("name").cloned().ok_or_else(|| {
            XlsxScanError::InvalidPackage(crate::i18n::Message::new(
                "status-xlsx-package-sheet-name-missing",
            ))
        })?;
        let relationship_id = attributes
            .get("r:id")
            .or_else(|| attributes.get("id"))
            .cloned()
            .ok_or_else(|| {
                XlsxScanError::InvalidPackage(
                    crate::i18n::Message::new("status-xlsx-package-sheet-relationship-id-missing")
                        .with_text("sheet", &sheet_name),
                )
            })?;
        if sheets.len() >= limits.max_sheets {
            return Err(limit_error(
                XlsxResourceLimit::SheetCount,
                limits.max_sheets as u64,
            ));
        }
        let state = attributes
            .get("state")
            .cloned()
            .unwrap_or_else(|| "visible".to_owned());
        if !matches!(state.as_str(), "visible" | "hidden" | "veryHidden") {
            return Err(XlsxScanError::InvalidPackage(
                crate::i18n::Message::new("status-xlsx-package-sheet-state-invalid")
                    .with_text("sheet", &sheet_name)
                    .with_text("state", state),
            ));
        }
        sheets.push(WorkbookSheet {
            name: sheet_name,
            relationship_id,
            state,
        });
    }
    Ok(sheets)
}

/// Reads the workbook date system setting.
pub(super) fn parse_date_1904(
    bytes: &[u8],
    limits: XlsxPackageLimits,
) -> Result<bool, XlsxScanError> {
    Ok(parse_xml_elements(bytes, "xl/workbook.xml", limits)?
        .into_iter()
        .find(|(name, _)| name == "workbookPr")
        .and_then(|(_, attributes)| attributes.get("date1904").cloned())
        .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true")))
}

/// Resolves a relationship target within the package.
pub(super) fn normalize_relationship_target(relationship_part: &str, target: &str) -> String {
    let base = if relationship_part == "_rels/.rels" {
        ""
    } else if let Some((directory, _)) = relationship_part.rsplit_once("/_rels/") {
        directory
    } else {
        ""
    };
    let combined = if target.starts_with('/') {
        target.trim_start_matches('/').to_owned()
    } else if base.is_empty() {
        target.to_owned()
    } else {
        format!("{base}/{target}")
    };
    let mut components = Vec::<&str>::new();
    for component in combined.split('/') {
        match component {
            "" | "." => (),
            ".." => {
                components.pop();
            }
            value => components.push(value),
        }
    }
    components.join("/")
}
