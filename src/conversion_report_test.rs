use super::*;
use std::io::{Cursor, Read};
use std::sync::Arc;
use zip::ZipArchive;

// Create a package whose worksheet part has a renamed path.
fn renamed_sheet_sample() -> Vec<u8> {
    use std::io::Write;
    use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

    let mut archive = ZipArchive::new(Cursor::new(include_bytes!("../samples/workbook.xlsx")))
        .expect("open sample XLSX");
    let mut output = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).expect("read sample part");
        let source_name = file.name().to_owned();
        let output_name = if source_name == "xl/worksheets/sheet1.xml" {
            "xl/sales.xml".to_owned()
        } else {
            source_name.clone()
        };
        let mut content = Vec::new();
        file.read_to_end(&mut content).expect("inflate sample part");
        if source_name == "xl/_rels/workbook.xml.rels" {
            let xml = String::from_utf8(content).expect("relationships are UTF-8");
            content = xml
                .replace("worksheets/sheet1.xml", "sales.xml")
                .into_bytes();
        } else if source_name == "[Content_Types].xml" {
            let xml = String::from_utf8(content).expect("content types are UTF-8");
            content = xml
                .replace("/xl/worksheets/sheet1.xml", "/xl/sales.xml")
                .into_bytes();
        }
        output
            .start_file(output_name, options)
            .expect("write renamed sample part");
        output.write_all(&content).expect("write part content");
    }
    output.finish().expect("finish renamed sample").into_inner()
}

// Create a package with shared strings referenced by worksheet cells.
fn package_with_shared_string_references(
    reference_count: usize,
    shared_string_xml: &str,
) -> Vec<u8> {
    use std::io::Write;
    use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

    let mut archive = ZipArchive::new(Cursor::new(include_bytes!("../samples/workbook.xlsx")))
        .expect("open sample XLSX");
    let mut output = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).expect("read sample part");
        let name = file.name().to_owned();
        let mut content = Vec::new();
        file.read_to_end(&mut content).expect("inflate sample part");
        let mut xml = if name == "xl/worksheets/sheet1.xml" {
            Some(String::from_utf8(content.clone()).expect("sheet XML is UTF-8"))
        } else if name == "xl/_rels/workbook.xml.rels" {
            Some(String::from_utf8(content.clone()).expect("relationships are UTF-8"))
        } else if name == "[Content_Types].xml" {
            Some(String::from_utf8(content.clone()).expect("content types are UTF-8"))
        } else {
            None
        };
        if let Some(xml) = &mut xml {
            match name.as_str() {
                "xl/worksheets/sheet1.xml" => {
                    let mut extra_cells = String::new();
                    for column in 0..reference_count {
                        extra_cells.push_str(&format!(
                            "<c r=\"{}10\" t=\"s\"><v>0</v></c>",
                            test_column_name(column)
                        ));
                    }
                    *xml = xml
                        .replace(
                            "<c r=\"A1\" s=\"1\" t=\"inlineStr\"><is><t>単価</t></is></c>",
                            "<c r=\"A1\" s=\"1\" t=\"s\"><v>0</v></c>",
                        )
                        .replace("</sheetData>", &format!("{extra_cells}</sheetData>"));
                }
                "xl/_rels/workbook.xml.rels" => {
                    *xml = xml.replace(
                            "</Relationships>",
                            "<Relationship Id=\"rIdSharedStrings\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings\" Target=\"sharedStrings.xml\"/></Relationships>",
                        );
                }
                "[Content_Types].xml" => {
                    *xml = xml.replace(
                            "</Types>",
                            "<Override PartName=\"/xl/sharedStrings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml\"/></Types>",
                        );
                }
                _ => (),
            }
            content = xml.as_bytes().to_vec();
        }
        output.start_file(name, options).expect("write sample part");
        output.write_all(&content).expect("write part content");
    }
    output
        .start_file("xl/sharedStrings.xml", options)
        .expect("add shared strings");
    output
            .write_all(
                format!(
                    "<sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" count=\"1\" uniqueCount=\"1\"><si>{shared_string_xml}</si></sst>"
                )
                .as_bytes(),
            )
            .expect("write shared string");
    output
        .finish()
        .expect("finish shared-string sample")
        .into_inner()
}

// Convert a zero-based column index to spreadsheet letters.
fn test_column_name(mut index: usize) -> String {
    let mut letters = Vec::new();
    loop {
        letters.push((b'A' + (index % 26) as u8) as char);
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    letters.iter().rev().collect()
}

// Create a workbook package containing features used by report tests.
fn package_with_reported_features() -> Vec<u8> {
    crate::xlsx_adapter::tests::rewrite_sample_parts(|name, bytes| match name {
        "xl/workbook.xml" => {
            let mut xml = String::from_utf8(bytes.clone()).expect("workbook XML");
            xml = xml.replace(
                    "<sheets><sheet name=\"売上\" sheetId=\"1\" r:id=\"rId1\"/>",
                    "<workbookProtection lockStructure=\"1\"/><sheets><sheet name=\"売上\" sheetId=\"1\" state=\"hidden\" r:id=\"rId1\"/>",
                );
            *bytes = xml.into_bytes();
        }
        "xl/worksheets/sheet1.xml" => {
            let mut xml = String::from_utf8(bytes.clone()).expect("worksheet XML");
            xml = xml.replace(
                    "</sheetData>",
                    "</sheetData><autoFilter ref=\"A1:C4\"/><dataValidations><dataValidation sqref=\"A2:A4\"/></dataValidations><hyperlinks><hyperlink ref=\"B2\"/></hyperlinks>",
                );
            *bytes = xml.into_bytes();
        }
        _ => (),
    })
}

#[test]
// Verify that print settings and worksheet ranges appear in the import report.
fn sample_xlsx_scan_reports_print_settings_and_sheet_ranges() {
    let inspection = XlsxPackageInspection::scan(
        include_bytes!("../samples/workbook.xlsx"),
        XlsxPackageLimits::default(),
    )
    .expect("scan sample XLSX");

    assert_eq!(inspection.sheets.len(), 2);
    assert_eq!(inspection.sheets[0].name, "売上");
    assert!(inspection.cell_count > 0);
    assert!(inspection.entries.iter().any(|entry| {
        entry.class == ConversionClass::Omitted
            && entry.sheet_order == Some(1)
            && entry.sheet_name.as_deref() == Some("売上")
            && entry.reason.contains("印刷設定")
            && entry.impact == ConversionImpact::Formatting
    }));
}

#[test]
// Keeps known print-only worksheet nodes out of content-impact reports.
fn worksheet_print_children_are_formatting_only_but_unknown_children_are_content() {
    let bytes = crate::xlsx_adapter::tests::rewrite_sample_parts(|name, bytes| {
        if name == "xl/worksheets/sheet1.xml" {
            let xml = String::from_utf8(bytes.clone()).expect("worksheet XML");
            *bytes = xml
                .replace(
                    "</sheetData>",
                    "</sheetData><rowBreaks><brk id=\"1\" max=\"1048575\"/></rowBreaks><colBreaks><brk id=\"2\" max=\"16383\"/></colBreaks><headerFooter><oddHeader>&amp;Ctitle</oddHeader><firstFooter>page</firstFooter><unknownHeaderItem>lost</unknownHeaderItem></headerFooter>",
                )
                .into_bytes();
        }
    });
    let inspection =
        XlsxPackageInspection::scan(&bytes, XlsxPackageLimits::default()).expect("scan fixture");

    let formatting_print_entries = inspection
        .entries
        .iter()
        .filter(|entry| {
            entry.class == ConversionClass::Omitted
                && entry.impact == ConversionImpact::Formatting
                && entry.reason.contains("印刷")
        })
        .count();
    assert!(formatting_print_entries >= 7);
    assert!(inspection.entries.iter().any(|entry| {
        entry.reason.contains("unknownHeaderItem")
            && entry.class == ConversionClass::Omitted
            && entry.impact == ConversionImpact::Content
    }));
}

#[test]
// Verify that worksheet scanning follows workbook relationships to renamed parts.
fn xlsx_scan_follows_workbook_relationship_to_renamed_sheet_part() {
    let inspection =
        XlsxPackageInspection::scan(&renamed_sheet_sample(), XlsxPackageLimits::default())
            .expect("scan XLSX with a renamed sheet part");

    assert_eq!(inspection.sheets[0].part_name, "xl/sales.xml");
    assert!(
        !inspection
            .entries
            .iter()
            .any(|entry| entry.reason.contains("xl/sales.xml"))
    );
    assert!(inspection.entries.iter().any(|entry| {
        entry.sheet_order == Some(1)
            && entry.sheet_name.as_deref() == Some("売上")
            && entry.reason.contains("印刷設定")
    }));
}

#[test]
// Verify that sheet protection, hidden state, and feature ranges are reported.
fn xlsx_scan_reports_protection_hidden_state_and_feature_ranges() {
    let inspection = XlsxPackageInspection::scan(
        &package_with_reported_features(),
        XlsxPackageLimits::default(),
    )
    .expect("scan workbook protection and unsupported worksheet features");

    assert!(
        inspection
            .entries
            .iter()
            .any(|entry| { entry.reason.contains("ブック保護設定") })
    );
    assert!(inspection.entries.iter().any(|entry| {
        entry.sheet_order == Some(1)
            && entry.reason.contains("非表示シート状態")
            && entry.impact == ConversionImpact::Formatting
    }));
    assert!(inspection.entries.iter().any(|entry| {
        entry.reason.contains("ブック保護設定") && entry.impact == ConversionImpact::Content
    }));
    // The package scanner recognizes these nodes but leaves supported-rule
    // classification to the adapter, which can inspect their actual contents.
    assert!(!inspection.entries.iter().any(|entry| {
        entry.class == ConversionClass::Omitted
            && (entry.range.as_deref() == Some("A1:C4") || entry.range.as_deref() == Some("A2:A4"))
    }));
    assert!(
        !inspection
            .entries
            .iter()
            .any(|entry| entry.reason.contains("セルリンク"))
    );
    assert!(
        !inspection
            .entries
            .iter()
            .any(|entry| entry.reason.contains("fills"))
    );
}

#[test]
// Verify that shared string expansion is limited before workbook parsing.
fn xlsx_scan_limits_shared_string_expansion_before_adapter_parse() {
    let bytes = package_with_shared_string_references(100, "<t>ab</t>");
    let mut archive = ZipArchive::new(Cursor::new(&bytes)).expect("open shared-string package");
    let expanded_bytes = (0..archive.len())
        .map(|index| archive.by_index(index).expect("read part size").size())
        .sum::<u64>();
    let limits = XlsxPackageLimits {
        max_expanded_bytes: expanded_bytes + 100,
        ..XlsxPackageLimits::default()
    };
    assert!(matches!(
        XlsxPackageInspection::scan(&bytes, limits),
        Err(XlsxScanError::LimitExceeded { limit, .. })
            if limit == XlsxResourceLimit::SharedStringExpandedBytes
    ));

    let limits = XlsxPackageLimits {
        max_shared_string_bytes: 1,
        ..XlsxPackageLimits::default()
    };
    assert!(matches!(
        XlsxPackageInspection::scan(&bytes, limits),
        Err(XlsxScanError::LimitExceeded { limit, .. })
            if limit == XlsxResourceLimit::SharedStringBytes
    ));

    let references = "&#65;".repeat(17);
    let bytes = package_with_shared_string_references(0, &format!("<t>{references}</t>"));
    let limits = XlsxPackageLimits {
        max_shared_string_bytes: SHARED_STRING_VALUE_OVERHEAD + 16,
        ..XlsxPackageLimits::default()
    };
    assert!(matches!(
        XlsxPackageInspection::scan(&bytes, limits),
        Err(XlsxScanError::LimitExceeded { limit, .. })
            if limit == XlsxResourceLimit::SharedStringBytes
    ));

    let rich_text = "<r><t>a</t></r>";
    let bytes = package_with_shared_string_references(1, rich_text);
    let single_run_size = SHARED_STRING_VALUE_OVERHEAD + RICH_TEXT_RUN_OVERHEAD + "a".len() as u64;
    let limits = XlsxPackageLimits {
        max_shared_string_bytes: single_run_size - 1,
        ..XlsxPackageLimits::default()
    };
    assert!(matches!(
        XlsxPackageInspection::scan(&bytes, limits),
        Err(XlsxScanError::LimitExceeded { limit, .. })
            if limit == XlsxResourceLimit::SharedStringBytes
    ));

    let rich_text_with_attributes = "<r><rPr><b val=\"1\"/></rPr><t>a</t></r>";
    let bytes = package_with_shared_string_references(1, rich_text_with_attributes);
    let run_with_attributes_size =
        single_run_size + RICH_TEXT_PROPERTIES_OVERHEAD + "val".len() as u64 + "1".len() as u64;
    let limits = XlsxPackageLimits {
        max_shared_string_bytes: run_with_attributes_size - 1,
        ..XlsxPackageLimits::default()
    };
    assert!(matches!(
        XlsxPackageInspection::scan(&bytes, limits),
        Err(XlsxScanError::LimitExceeded { limit, .. })
            if limit == XlsxResourceLimit::SharedStringBytes
    ));

    let mut archive = ZipArchive::new(Cursor::new(&bytes)).expect("open rich shared strings");
    let expanded_bytes = (0..archive.len())
        .map(|index| archive.by_index(index).expect("read part size").size())
        .sum::<u64>();
    let limits = XlsxPackageLimits {
        max_shared_string_bytes: run_with_attributes_size,
        max_expanded_bytes: expanded_bytes + run_with_attributes_size * 3 / 2,
        ..XlsxPackageLimits::default()
    };
    assert!(matches!(
        XlsxPackageInspection::scan(&bytes, limits),
        Err(XlsxScanError::LimitExceeded { limit, .. })
            if limit == XlsxResourceLimit::SharedStringExpandedBytes
    ));

    let bytes = package_with_shared_string_references(0, "<t>&unknown;</t>");
    assert!(matches!(
        XlsxPackageInspection::scan(&bytes, XlsxPackageLimits::default()),
        Err(XlsxScanError::InvalidPackage(message))
            if message.resolve(crate::i18n::Language::Japanese).contains("未定義")
    ));
}

#[test]
// Verify that worksheet scans stop when cell or dimension limits are exceeded.
fn xlsx_scan_stops_on_cell_or_dimension_limits() {
    let limits = XlsxPackageLimits {
        max_cells: 1,
        ..XlsxPackageLimits::default()
    };
    assert!(matches!(
        XlsxPackageInspection::scan(include_bytes!("../samples/workbook.xlsx"), limits),
        Err(XlsxScanError::LimitExceeded { limit, .. }) if limit == XlsxResourceLimit::Cells
    ));

    let limits = XlsxPackageLimits {
        max_rows: 3,
        ..XlsxPackageLimits::default()
    };
    assert!(matches!(
        XlsxPackageInspection::scan(include_bytes!("../samples/workbook.xlsx"), limits),
        Err(XlsxScanError::LimitExceeded { limit, .. }) if limit == XlsxResourceLimit::Rows
    ));

    let limits = XlsxPackageLimits {
        max_columns: 2,
        ..XlsxPackageLimits::default()
    };
    assert!(matches!(
        XlsxPackageInspection::scan(include_bytes!("../samples/workbook.xlsx"), limits),
        Err(XlsxScanError::LimitExceeded { limit, .. }) if limit == XlsxResourceLimit::Columns
    ));

    let limits = XlsxPackageLimits {
        max_compressed_bytes: 1,
        ..XlsxPackageLimits::default()
    };
    assert!(matches!(
        XlsxPackageInspection::scan(include_bytes!("../samples/workbook.xlsx"), limits),
        Err(XlsxScanError::LimitExceeded { limit, .. }) if limit == XlsxResourceLimit::CompressedBytes
    ));

    let limits = XlsxPackageLimits {
        max_parts: 1,
        ..XlsxPackageLimits::default()
    };
    assert!(matches!(
        XlsxPackageInspection::scan(include_bytes!("../samples/workbook.xlsx"), limits),
        Err(XlsxScanError::LimitExceeded { limit, .. }) if limit == XlsxResourceLimit::ZipParts
    ));
}

#[test]
// Verify that XML metadata and report text are bounded while scanning.
fn xlsx_scan_caps_xml_metadata_and_report_strings_before_collecting_them() {
    let limits = XlsxPackageLimits {
        max_xml_metadata_bytes: 1,
        ..XlsxPackageLimits::default()
    };
    assert!(matches!(
        XlsxPackageInspection::scan(include_bytes!("../samples/workbook.xlsx"), limits),
        Err(XlsxScanError::LimitExceeded { limit, .. })
            if limit == XlsxResourceLimit::XmlMetadataBytes
    ));

    let limits = XlsxPackageLimits {
        max_xml_attributes_per_element: 0,
        ..XlsxPackageLimits::default()
    };
    assert!(matches!(
        XlsxPackageInspection::scan(include_bytes!("../samples/workbook.xlsx"), limits),
        Err(XlsxScanError::LimitExceeded { limit, .. })
            if limit == XlsxResourceLimit::XmlAttributeCount
    ));

    let limits = XlsxPackageLimits {
        max_diagnostic_bytes: 1,
        ..XlsxPackageLimits::default()
    };
    assert!(matches!(
        XlsxPackageInspection::scan(include_bytes!("../samples/workbook.xlsx"), limits),
        Err(XlsxScanError::LimitExceeded { limit, .. })
            if limit == XlsxResourceLimit::ReportTextBytes
    ));
}

#[test]
// Verify that unknown content types and relationships are reported.
fn xlsx_scan_detects_unknown_xml_content_type_and_relationships() {
    let bytes = crate::xlsx_adapter::tests::rewrite_sample_parts(|name, bytes| match name {
        "xl/worksheets/sheet1.xml" => {
            let mut xml = String::from_utf8(bytes.clone()).expect("sample sheet XML");
            xml = xml.replace(
                "</worksheet>",
                "<futureThing/><autoFilter ref=\"A1:C4\"/></worksheet>",
            );
            *bytes = xml.into_bytes();
        }
        "xl/_rels/workbook.xml.rels" => {
            let mut xml = String::from_utf8(bytes.clone()).expect("sample relationships XML");
            xml = xml.replace(
                    "</Relationships>",
                    "<Relationship Id=\"rIdFuture\" Type=\"urn:texcel:future\" Target=\"worksheets/sheet1.xml\"/></Relationships>",
                );
            *bytes = xml.into_bytes();
        }
        "[Content_Types].xml" => {
            let mut xml = String::from_utf8(bytes.clone()).expect("sample content types XML");
            xml = xml.replace(
                    "</Types>",
                    "<Override PartName=\"/xl/future.xml\" ContentType=\"application/x-texcel-future\"/></Types>",
                );
            *bytes = xml.into_bytes();
        }
        _ => (),
    });
    let inspection = XlsxPackageInspection::scan(&bytes, XlsxPackageLimits::default())
        .expect("scan package with unsupported items");
    assert!(inspection.entries.iter().any(|entry| {
        entry.sheet_order == Some(1) && entry.reason.contains("未分類の非対応XML要素: futureThing")
    }));
    assert!(!inspection.entries.iter().any(|entry| {
        entry.class == ConversionClass::Omitted && entry.reason.contains("フィルター")
    }));
    assert!(
        inspection
            .entries
            .iter()
            .any(|entry| entry.reason.contains("未分類の非対応relationship"))
    );
    assert!(
        inspection
            .entries
            .iter()
            .any(|entry| entry.reason.contains("application/x-texcel-future"))
    );
}

#[test]
// Verify that the import report opens once automatically and can be reopened later.
fn import_report_shows_attention_once_and_can_be_reopened() {
    let retained = ImportReport::new(
        "ok.xlsx",
        chrono::Utc::now(),
        "XLSX",
        vec![ConversionEntry::new(
            ConversionClass::Retained,
            None,
            None,
            None,
            "対応範囲",
        )],
    );
    let mut retained_state = ImportReportState::new(Arc::new(retained), "ok.xlsx".into());
    assert!(retained_state.take_automatic_dialog().is_none());

    let formatting_report = ImportReport::new(
        "input.xlsx",
        chrono::Utc::now(),
        "XLSX",
        vec![
            ConversionEntry::new(
                ConversionClass::Omitted,
                Some(1),
                Some("売上".into()),
                Some("A1".into()),
                "印刷設定",
            )
            .formatting_only(),
        ],
    );
    assert!(!formatting_report.requires_dialog());
    assert!(formatting_report.has_formatting_notes());
    let mut formatting_state =
        ImportReportState::new(Arc::new(formatting_report), "input.xlsx".into());
    assert!(formatting_state.take_automatic_dialog().is_none());
    assert!(
        formatting_state
            .reopen()
            .to_text(crate::i18n::Language::Japanese)
            .contains("印刷設定")
    );

    let report = ImportReport::new(
        "content.xlsx",
        chrono::Utc::now(),
        "XLSX",
        vec![ConversionEntry::new(
            ConversionClass::Omitted,
            Some(1),
            Some("売上".into()),
            Some("B2".into()),
            "値が失われます",
        )],
    );
    let mut state = ImportReportState::new(Arc::new(report), "input.xlsx".into());
    let first = state.take_automatic_dialog().expect("one automatic report");
    assert!(state.take_automatic_dialog().is_none());
    let reopened = state.reopen();
    assert!(Arc::ptr_eq(&first, &reopened));
    assert!(
        reopened
            .to_text(crate::i18n::Language::Japanese)
            .contains("値が失われます")
    );
}

#[test]
// Verify that reported ranges are stable and contain no duplicates.
fn import_report_groups_ranges_deterministically_without_duplicates() {
    let report = ImportReport::new(
        "input.xlsx",
        chrono::Utc::now(),
        "XLSX",
        ["B2", "A1", "A1"]
            .into_iter()
            .map(|range| {
                ConversionEntry::new(
                    ConversionClass::Omitted,
                    Some(1),
                    Some("売上".into()),
                    Some(range.into()),
                    "印刷設定",
                )
            })
            .collect(),
    );

    assert_eq!(report.groups().len(), 1);
    assert_eq!(report.groups()[0].count, 3);
    assert_eq!(report.groups()[0].ranges, ["A1", "B2"]);
}

#[test]
// Verify that report reasons and export settings use the requested language at render time.
fn report_text_resolves_typed_reasons_and_settings_for_each_language() {
    let settings = crate::i18n::Message::new("report-settings-summary")
        .with_text("path", "report.csv")
        .with_message("scope", crate::i18n::Message::new("report-scope-all"))
        .with_message(
            "formulas",
            crate::i18n::Message::new("report-formula-source"),
        )
        .with_message(
            "modifiers",
            crate::i18n::Message::new("report-modifiers-convert"),
        )
        .with_message(
            "header",
            crate::i18n::Message::new("report-markdown-header-first"),
        );
    let report = ExportReport::new(
        2,
        "CSV",
        settings,
        vec![
            ConversionEntry::new(
                ConversionClass::Converted,
                Some(1),
                Some("Sales".to_owned()),
                Some("A1".to_owned()),
                "型付きの値をCSV文字列に変換しました",
            )
            .with_message(crate::i18n::Message::new("report-reason-csv-typed-value")),
        ],
    );

    let english = report.to_text(crate::i18n::Language::English);
    let japanese = report.to_text(crate::i18n::Language::Japanese);
    assert!(!english.contains('\u{2068}') && !english.contains('\u{2069}'));
    assert!(!japanese.contains('\u{2068}') && !japanese.contains('\u{2069}'));
    assert!(english.contains("Output: report.csv"));
    assert!(english.contains("A typed value was converted to CSV text."));
    assert!(!english.contains("型付きの値をCSV文字列に変換しました"));
    assert!(japanese.contains("出力先: report.csv"));
    assert!(japanese.contains("型付きの値をCSVの文字列に変換しました。"));
}

#[test]
// Retains the UTC instant while both UI languages use the same OS-local timestamp.
fn import_timestamp_keeps_its_instant_across_language_changes() {
    use chrono::TimeZone;

    let instant = chrono::Utc.with_ymd_and_hms(2026, 11, 18, 3, 4, 5).unwrap();
    let report = ImportReport::new("sample.xlsx", instant, "XLSX", Vec::new());
    let formatted = report.formatted_imported_at();
    let english = report.to_text(crate::i18n::Language::English);
    let japanese = report.to_text(crate::i18n::Language::Japanese);

    assert_eq!(report.imported_at, instant);
    assert!(english.contains(&formatted));
    assert!(japanese.contains(&formatted));
}
