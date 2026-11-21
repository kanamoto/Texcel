use super::*;

/// Rebuilds the bundled sample while changing selected parts for narrow tests.
pub(crate) fn rewrite_sample_parts(mut transform: impl FnMut(&str, &mut Vec<u8>)) -> Vec<u8> {
    rewrite_package_parts(include_bytes!("../samples/workbook.xlsx"), &mut transform)
}

/// Rewrites package parts while keeping the workbook as a valid ZIP archive.
pub(crate) fn rewrite_package_parts(
    source: &[u8],
    mut transform: impl FnMut(&str, &mut Vec<u8>),
) -> Vec<u8> {
    use std::io::{Read, Write};
    use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

    let mut archive = ZipArchive::new(Cursor::new(source)).expect("open sample package");
    let mut output = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).expect("read sample part");
        let name = file.name().to_owned();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).expect("inflate sample part");
        transform(&name, &mut bytes);
        output.start_file(name, options).expect("create test part");
        output.write_all(&bytes).expect("write test part");
    }
    output.finish().expect("finish test package").into_inner()
}

#[test]
// Verify that the sample workbook's values, rich text, and layout are read correctly.
fn sample_xlsx_values_rich_text_and_layout_are_read() {
    let source_bytes = include_bytes!("../samples/workbook.xlsx");
    let source = XlsxDocument::parse(source_bytes, 1).unwrap();
    assert_eq!(
        source
            .sheets
            .iter()
            .map(|sheet| sheet.name.as_str())
            .collect::<Vec<_>>(),
        ["売上", "型と書式"]
    );
    assert_eq!(
        source.sheets[1].cells[&CellAddress { row: 0, column: 0 }].value,
        XlsxValue::String("00123".to_owned())
    );
    assert_eq!(
        source.sheets[1].cells[&CellAddress { row: 3, column: 1 }].value,
        XlsxValue::Number(45292.0)
    );
    assert_eq!(
        source.sheets[1].cells[&CellAddress { row: 3, column: 0 }].value,
        XlsxValue::RichText(vec![
            XlsxTextRun {
                text: "重要".to_owned(),
                bold: true,
                ..XlsxTextRun::default()
            },
            XlsxTextRun {
                text: " ABC".to_owned(),
                ..XlsxTextRun::default()
            },
        ])
    );
    let document = source.to_document(9, "sample.xlsx");
    assert_eq!(document.tables.len(), 2);
    assert_eq!(document.tables[0].name, "売上");
    assert_eq!(
        document.tables[0]
            .cell(CellAddress { row: 1, column: 2 })
            .unwrap()
            .input,
        CellInput::Formula("=A2*B2".to_owned())
    );
    assert_eq!(
        document.tables[0]
            .cell(CellAddress { row: 1, column: 2 })
            .unwrap()
            .calculation
            .as_ref()
            .unwrap()
            .status,
        CalcStatus::Imported
    );
    assert_eq!(
        document.tables[0]
            .cell(CellAddress { row: 1, column: 2 })
            .unwrap()
            .imported_result,
        Some(CalcValue::Number(3600.0))
    );
}

#[test]
// Verify that XLSX progress begins after workbook parsing and can cancel sheet conversion.
fn xlsx_progress_cancels_after_the_first_converted_row() {
    let mut preview_rows = Vec::new();
    let result = XlsxDocument::parse_with_progress(
        include_bytes!("../samples/workbook.xlsx"),
        101,
        |table, name, row, cells| {
            preview_rows.push((table, name.to_owned(), row, cells.len()));
            false
        },
    )
    .expect("the sample workbook parses before preview cancellation");

    assert!(result.is_none());
    assert_eq!(preview_rows.len(), 1);
    assert_eq!(preview_rows[0].0, 0);
    assert_eq!(preview_rows[0].1, "売上");
    assert!(preview_rows[0].3 > 0);
}

#[test]
// Verify that an omitted freeze top-left cell uses the pane split boundary.
fn omitted_frozen_pane_top_left_uses_the_split_boundary() {
    let bytes = rewrite_sample_parts(|name, xml| {
        if name == "xl/worksheets/sheet1.xml" {
            let text = String::from_utf8(xml.clone()).expect("sample XML is UTF-8");
            let changed = text.replace(" topLeftCell=\"A2\"", "");
            assert_ne!(changed, text, "sample pane has an explicit top-left cell");
            *xml = changed.into_bytes();
        }
    });

    let parsed = XlsxDocument::parse(&bytes, 7).expect("omitted topLeftCell is valid");
    assert_eq!(parsed.sheets[0].frozen_rows, 1);
    assert_eq!(parsed.sheets[0].freeze_top_left, "A2");

    let explicit_inside = rewrite_sample_parts(|name, xml| {
        if name == "xl/worksheets/sheet1.xml" {
            let text = String::from_utf8(xml.clone()).expect("sample XML is UTF-8");
            let changed = text.replace("topLeftCell=\"A2\"", "topLeftCell=\"A1\"");
            assert_ne!(changed, text, "sample pane has an explicit top-left cell");
            *xml = changed.into_bytes();
        }
    });
    assert!(XlsxDocument::parse(&explicit_inside, 12).is_err());
}

#[test]
// Verify that a freeze top-left cell extends a sparse sheet.
fn freeze_top_left_extends_sparse_sheet() {
    let source = XlsxDocument {
        date_1904: false,
        sheets: vec![XlsxSheet {
            table_id: 1,
            name: "Top left".to_owned(),
            cells: BTreeMap::from([(
                CellAddress { row: 0, column: 0 },
                XlsxCell {
                    value: XlsxValue::String("start".to_owned()),
                    formula: None,
                    style: XlsxStyle::default(),
                    hyperlink: None,
                },
            )]),
            columns: Vec::new(),
            rows: Vec::new(),
            merges: Vec::new(),
            frozen_rows: 1,
            frozen_columns: 0,
            freeze_top_left: "A40".to_owned(),

            rule_losses: Vec::new(),
        }],
    };
    let document = source.to_document(1, "top-left.xlsx");
    assert_eq!(document.tables[0].row_count(), 40);
}

#[test]
// Verify that imported row heights are only recorded for the import report.
fn imported_row_heights_are_recorded_for_the_report() {
    let source = XlsxDocument {
        date_1904: false,
        sheets: vec![XlsxSheet {
            table_id: 1,
            name: "Rows".to_owned(),
            cells: BTreeMap::new(),
            columns: Vec::new(),
            rows: vec![XlsxRow {
                index: 5,
                height: 40.0,
                height_present: true,
                custom_height: true,
                custom_height_present: true,
                hidden: false,
            }],
            merges: Vec::new(),
            frozen_rows: 0,
            frozen_columns: 0,
            freeze_top_left: String::new(),

            rule_losses: Vec::new(),
        }],
    };
    let document = source.to_document(1, "rows.xlsx");
    assert_eq!(document.tables[0].row_count(), 5);
    assert!(document.tables[0].layout.has_imported_row_layout());
}

#[test]
// Verify that a missing column minimum is reported as an adapter error.
fn missing_column_min_is_reported_as_an_adapter_error() {
    let bytes = rewrite_sample_parts(|name, xml| {
        if name == "xl/worksheets/sheet1.xml" {
            let text = String::from_utf8(xml.clone()).expect("sample XML is UTF-8");
            let changed = text.replace("<col min=\"1\"", "<col");
            assert_ne!(changed, text, "sample contains a column min attribute");
            *xml = changed.into_bytes();
        }
    });
    assert!(XlsxDocument::parse(&bytes, 4).is_err());
}

#[test]
// Verify that theme colors resolve for cell styles and rich text runs.
fn theme_colors_are_resolved_for_cells_backgrounds_and_text_runs() {
    let bytes = rewrite_sample_parts(|name, xml| match name {
        "xl/styles.xml" => {
            let text = String::from_utf8(xml.clone()).expect("sample XML is UTF-8");
            let changed = text
                .replace("<color rgb=\"FFFFFFFF\"/>", "<color theme=\"1\"/>")
                .replace("<fgColor rgb=\"FF245580\"", "<fgColor theme=\"4\"");
            assert_ne!(
                changed, text,
                "sample contains explicit font and fill colors"
            );
            *xml = changed.into_bytes();
        }
        "xl/worksheets/sheet2.xml" => {
            let text = String::from_utf8(xml.clone()).expect("sample XML is UTF-8");
            let changed = text.replace("<rPr><b/></rPr>", "<rPr><color theme=\"5\"/><b/></rPr>");
            assert_ne!(changed, text, "sample contains a bold rich-text run");
            *xml = changed.into_bytes();
        }
        _ => (),
    });
    let source = XlsxDocument::parse(&bytes, 5).expect("parse theme color sample");
    assert!(
        source.sheets[0].cells[&CellAddress { row: 0, column: 0 }]
            .style
            .font_color
            .is_some()
    );
    assert!(
        source.sheets[0].cells[&CellAddress { row: 0, column: 0 }]
            .style
            .fill_color
            .is_some()
    );
    let XlsxValue::RichText(runs) =
        &source.sheets[1].cells[&CellAddress { row: 3, column: 0 }].value
    else {
        panic!("rich-text sample stays rich text");
    };
    assert!(runs[0].font_color.is_some());
}
