use super::*;

// Classify an XLSX package part and record any feature that needs attention.
pub(super) fn classify_part(name: &str, entries: &mut Vec<ConversionEntry>) {
    let supported = matches!(
        name,
        "xl/sharedStrings.xml" | "xl/theme/theme1.xml" | "docProps/core.xml" | "docProps/app.xml"
    ) || name.starts_with("xl/worksheets/_rels/sheet");
    if supported {
        return;
    }
    let known_unsupported = name.starts_with("xl/printerSettings/")
        || name.starts_with("xl/drawings/")
        || name.starts_with("xl/charts/")
        || name.starts_with("xl/tables/")
        || name.starts_with("xl/pivotTables/")
        || name.starts_with("xl/pivotCache/")
        || name.starts_with("xl/externalLinks/")
        || name.starts_with("xl/comments")
        || name.starts_with("xl/threadedComments/")
        || name.starts_with("customXml/")
        || name.starts_with("xl/activeX/")
        || name.starts_with("xl/embeddings/");
    let (reason, message_key) = if name.starts_with("xl/printerSettings/") {
        (
            format!("印刷設定の部品は出力へ引き継がれません: {name}"),
            "report-reason-xlsx-printer-part",
        )
    } else if known_unsupported {
        (
            format!("非対応XLSX部品は出力へ引き継がれません: {name}"),
            "report-reason-xlsx-unsupported-part",
        )
    } else {
        (
            format!("未分類の非対応OOXML部品: {name}"),
            "report-reason-xlsx-unknown-part",
        )
    };
    let entry = ConversionEntry::new(ConversionClass::Omitted, None, None, None, reason)
        .with_message(Message::new(message_key).with_text("part", name));
    entries.push(if name.starts_with("xl/printerSettings/") {
        entry.formatting_only()
    } else {
        entry
    });
}

// Inspect known XML parts for features that affect import fidelity.
pub(super) fn scan_known_xml(
    bytes: &[u8],
    part: &str,
    limits: XlsxPackageLimits,
) -> Result<Vec<ConversionEntry>, XlsxScanError> {
    let known_elements: &[&str] = if part == "xl/workbook.xml" {
        &[
            "workbook",
            "fileVersion",
            "fileSharing",
            "workbookPr",
            "workbookProtection",
            "bookViews",
            "workbookView",
            "sheets",
            "sheet",
            "functionGroups",
            "externalReferences",
            "definedNames",
            "calcPr",
            "oleSize",
            "customWorkbookViews",
            "pivotCaches",
            "smartTagPr",
            "smartTagTypes",
            "webPublishing",
            "fileRecoveryPr",
            "webPublishObjects",
            "extLst",
        ]
    } else if part == "xl/styles.xml" {
        &[
            "styleSheet",
            "numFmts",
            "numFmt",
            "fonts",
            "font",
            "fills",
            "b",
            "i",
            "strike",
            "color",
            "sz",
            "name",
            "charset",
            "family",
            "scheme",
            "u",
            "vertAlign",
            "outline",
            "shadow",
            "condense",
            "extend",
            "rFont",
            "fill",
            "patternFill",
            "fgColor",
            "bgColor",
            "borders",
            "border",
            "left",
            "right",
            "top",
            "bottom",
            "diagonal",
            "vertical",
            "horizontal",
            "start",
            "end",
            "alignment",
            "protection",
            "cellStyleXfs",
            "xf",
            "cellXfs",
            "cellStyles",
            "cellStyle",
            "dxfs",
            "tableStyles",
            "tableStyle",
            "tableStyleElement",
            "extLst",
        ]
    } else {
        &[
            "sst",
            "si",
            "t",
            "r",
            "rPr",
            "rFont",
            "b",
            "i",
            "strike",
            "color",
            "sz",
            "vertAlign",
            "u",
            "charset",
            "family",
            "scheme",
            "phoneticPr",
            "phoneticRun",
            "rPh",
            "extLst",
        ]
    };
    let mut entries = Vec::new();
    let mut reader = Reader::from_reader(bytes);
    let mut reported = BTreeSet::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) | Ok(Event::Empty(element)) => {
                let name = local_name(element.name().as_ref());
                let (reason, message_key) = if name == "workbookProtection" {
                    (
                        Some("ブック保護設定は出力へ引き継がれません".to_owned()),
                        Some("report-reason-xlsx-workbook-protection"),
                    )
                } else if name == "extLst" {
                    (
                        Some(format!(
                            "拡張XML要素は出力へ引き継がれません: {part}:{name}"
                        )),
                        Some("report-reason-xlsx-extension-element"),
                    )
                } else if !known_elements.contains(&name.as_str()) {
                    (
                        Some(format!("未分類の非対応XML要素: {part}:{name}")),
                        Some("report-reason-xlsx-unknown-element"),
                    )
                } else {
                    (None, None)
                };
                if let Some(reason) = reason
                    && reported.insert(reason.clone())
                {
                    push_report_entry(
                        &mut entries,
                        ConversionEntry::new(ConversionClass::Omitted, None, None, None, reason)
                            .with_message(
                                Message::new(message_key.expect("report reason has a key"))
                                    .with_text("part", part)
                                    .with_text("element", &name),
                            ),
                        limits,
                    )?;
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => (),
            Err(error) => {
                return Err(XlsxScanError::InvalidXml {
                    part: part.to_owned(),
                    reason: error.to_string(),
                });
            }
        }
    }
    Ok(entries)
}

// Explain why an XML element is outside the supported import range.
pub(super) fn unsupported_element_reason(name: &str) -> &'static str {
    match name {
        "pageMargins" | "pageSetup" | "printOptions" | "headerFooter" | "rowBreaks"
        | "colBreaks" | "brk" | "oddHeader" | "oddFooter" | "evenHeader" | "evenFooter"
        | "firstHeader" | "firstFooter" => "印刷設定は出力へ引き継がれません",
        "drawing" | "picture" | "legacyDrawing" | "legacyDrawingHF" => {
            "図形・画像・注釈は出力へ引き継がれません"
        }
        "tableParts" | "tablePart" => "Excelテーブルは出力へ引き継がれません",
        "extLst" => "拡張XML要素は出力へ引き継がれません",
        _ => "このシート要素は出力へ引き継がれません",
    }
}

/// Builds a translated reason for an unsupported sheet element.
pub(super) fn unsupported_element_message(name: &str) -> Message {
    Message::new(match name {
        "pageMargins" | "pageSetup" | "printOptions" | "headerFooter" | "rowBreaks"
        | "colBreaks" | "brk" | "oddHeader" | "oddFooter" | "evenHeader" | "evenFooter"
        | "firstHeader" | "firstFooter" => "report-reason-xlsx-print-settings",
        "drawing" | "picture" | "legacyDrawing" | "legacyDrawingHF" => "report-reason-xlsx-drawing",
        "tableParts" | "tablePart" => "report-reason-xlsx-table",
        "extLst" => "report-reason-xlsx-extension-element",
        _ => "report-reason-xlsx-unsupported-element",
    })
    .with_text("element", name)
}

// Check that an XLSX cell range is valid and within configured limits.
pub(super) fn validate_range(range: &str, limits: XlsxPackageLimits) -> Result<(), XlsxScanError> {
    let (first, last) = range.split_once(':').unwrap_or((range, range));
    for cell in [first, last] {
        let (row, column) = parse_cell_reference(cell).ok_or_else(|| {
            XlsxScanError::InvalidPackage(
                crate::i18n::Message::new("status-xlsx-package-cell-range-invalid")
                    .with_text("range", range),
            )
        })?;
        if row >= limits.max_rows {
            return Err(limit_error(XlsxResourceLimit::Rows, limits.max_rows as u64));
        }
        if column >= limits.max_columns {
            return Err(limit_error(
                XlsxResourceLimit::Columns,
                limits.max_columns as u64,
            ));
        }
    }
    Ok(())
}
