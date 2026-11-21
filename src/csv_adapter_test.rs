use super::*;

#[test]
// Verify that CSV parsing preserves quoted delimiters, newlines, and empty records.
fn quoted_delimiters_newlines_and_empty_records_keep_their_meaning() {
    let source = b"a,\"b,c\"\r\n\r\n\"multi\nline\",\"\"\r\n".to_vec();
    let parsed =
        CsvDocument::parse(source.clone(), EncodingChoice::Auto, ',').expect("valid CSV parses");
    assert_eq!(parsed.source_bytes(), source);
    assert_eq!(parsed.newline, NewlineStyle::CrLf);
    assert_eq!(parsed.records.len(), 3);
    assert_eq!(parsed.records[0].fields[1].value, "b,c");
    assert!(parsed.records[1].fields.is_empty());
    assert_eq!(parsed.records[2].fields[0].value, "multi\nline");
    assert_eq!(parsed.records[2].fields[1].value, "");
}

#[test]
// Verify that editing one field leaves other source bytes and quote style unchanged.
fn replacing_one_field_preserves_other_bytes_and_quote_style() {
    let source = b"a,\"b,c\"\r\nx,y\r\n".to_vec();
    let parsed = CsvDocument::parse(source, EncodingChoice::Auto, ',').expect("valid CSV parses");
    let updated = parsed
        .replace_field(0, 1, "new, value")
        .expect("quoted field can be replaced");
    assert_eq!(updated.source_bytes(), b"a,\"new, value\"\r\nx,y\r\n");
    assert_eq!(updated.records[0].fields[1].value, "new, value");
}

#[test]
// Verify that sparse saves preserve untouched bytes and extend only edited rows.
fn saving_sparse_edits_preserves_original_bytes_and_extends_only_touched_rows() {
    let source = CsvDocument::parse(b"a,\"b,c\"\r\nx,y".to_vec(), EncodingChoice::Auto, ',')
        .expect("valid CSV parses");
    let updated = source
        .apply_changes(&[
            CsvCellChange {
                row: 0,
                column: 0,
                value: Some("A".to_owned()),
            },
            CsvCellChange {
                row: 1,
                column: 1,
                value: Some("Y".to_owned()),
            },
            CsvCellChange {
                row: 1,
                column: 3,
                value: Some("z".to_owned()),
            },
        ])
        .expect("sparse changes serialize");
    assert_eq!(updated.source_bytes(), b"A,\"b,c\"\r\nx,Y,,z");
    assert_eq!(updated.records[1].fields[3].value, "z");
}

#[test]
// Reject edits that would write more fields than the CSV reader can open.
fn wide_row_edits_check_the_output_limit_before_materializing_fields() {
    let source = CsvDocument::parse(b"a\n".to_vec(), EncodingChoice::Auto, ',')
        .expect("small source parses");

    // Each new row needs all fields up to its last value, including the empty fields before it.
    let changes = (0..=MAX_CSV_CELLS / MAX_CSV_COLUMNS)
        .map(|row| CsvCellChange {
            row,
            column: MAX_CSV_COLUMNS - 1,
            value: Some("end".to_owned()),
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        source.apply_changes(&changes),
        Err(CsvError::ResourceLimit(
            crate::i18n::ResourceLimit::CsvOutputCellCount
        ))
    ));
}

#[test]
// Count actual ragged CSV fields instead of the unused bounding rectangle.
fn ragged_csv_growth_counts_saved_fields_instead_of_the_bounding_rectangle() {
    let header = (0..501)
        .map(|column| format!("header-{column}"))
        .collect::<Vec<_>>()
        .join(",");
    let mut bytes = format!("{header}\n").into_bytes();
    for _ in 0..999 {
        bytes.extend_from_slice(b"left,right\n");
    }
    let source = CsvDocument::parse(bytes, EncodingChoice::Auto, ',')
        .expect("the ragged source is below its actual field limit");
    assert_eq!(source.parsed_cell_count, 2_499);
    assert!(source.records.len() * 501 > MAX_CSV_CELLS);

    let updated = source
        .apply_changes(&[CsvCellChange {
            row: 1,
            column: 2,
            value: Some("added".to_owned()),
        }])
        .expect("one real field can be added while actual field count is below the limit");
    assert_eq!(updated.records[1].fields[2].value, "added");
    assert_eq!(updated.parsed_cell_count, 2_500);

    let reopened = CsvDocument::parse(updated.source_bytes().to_vec(), EncodingChoice::Auto, ',')
        .expect("the exact output remains readable by the CSV reader");
    assert_eq!(reopened.parsed_cell_count, 2_500);
    assert_eq!(reopened.records[1].fields[2].value, "added");
}

#[test]
// Verify that end-of-file column and row insertions keep their intended order.
fn eof_column_and_new_row_insertions_keep_their_coordinate_order() {
    let source = CsvDocument::parse(b"left,\nlast,".to_vec(), EncodingChoice::Auto, ',')
        .expect("ragged CSV parses");
    let updated = source
        .apply_changes(&[
            CsvCellChange {
                row: 1,
                column: 1,
                value: Some("EDIT".to_owned()),
            },
            CsvCellChange {
                row: 1,
                column: 2,
                value: Some("COL".to_owned()),
            },
            CsvCellChange {
                row: 2,
                column: 0,
                value: Some("ROW".to_owned()),
            },
        ])
        .expect("EOF additions serialize without overlapping insertions");
    assert_eq!(updated.source_bytes(), b"left,\nlast,EDIT,COL\nROW");
    assert_eq!(updated.records[1].fields[1].value, "EDIT");
    assert_eq!(updated.records[1].fields[2].value, "COL");
    assert_eq!(updated.records[2].fields[0].value, "ROW");
}

#[test]
// Verify that empty fields and later record BOM bytes remain explicit source values.
fn empty_single_field_and_later_record_bom_remain_explicit_values() {
    let single = CsvDocument::parse(b"abc\n".to_vec(), EncodingChoice::Auto, ',')
        .expect("single-field CSV parses");
    let emptied = single
        .replace_field(0, 0, "")
        .expect("empty single field stays representable");
    assert_eq!(emptied.source_bytes(), b"\"\"\n");
    assert_eq!(emptied.records[0].fields.len(), 1);
    assert_eq!(emptied.records[0].fields[0].value, "");

    let with_bom_data = CsvDocument::parse(
        "a,b\n\u{FEFF}x,y".as_bytes().to_vec(),
        EncodingChoice::Auto,
        ',',
    )
    .expect("later-row BOM is cell text");
    assert_eq!(with_bom_data.records[1].fields[0].value, "\u{FEFF}x");
}

#[test]
// Verify that empty and missing cells retain one-field CSV shape after save and undo.
fn empty_and_missing_cells_keep_one_field_across_save_and_undo() {
    let source = CsvDocument::parse(b"abc\n".to_vec(), EncodingChoice::Auto, ',')
        .expect("single-field CSV parses");
    let mut document = source.to_document(41, "one field");
    let address = CellAddress { row: 0, column: 0 };
    document
        .set_cell(1, address, Some(Cell::text("")))
        .expect("make an explicit empty string");
    let saved_empty = source
        .apply_changes(&document_csv_changes(&document))
        .expect("save explicit empty value");
    assert_eq!(saved_empty.source_bytes(), b"\"\"\n");
    document.mark_saved(document.revision);

    document.undo().expect("undo after saving the empty value");
    let restored = saved_empty
        .apply_changes(&document_csv_changes(&document))
        .expect("save the restored source after undo");
    assert_eq!(restored.source_bytes(), b"\"abc\"\n");
    assert_eq!(restored.records[0].fields[0].value, "abc");

    document
        .set_cell(1, address, None)
        .expect("remove the sparse cell value");
    let missing = restored
        .apply_changes(&document_csv_changes(&document))
        .expect("a missing cell clears its existing CSV field");
    assert_eq!(missing.source_bytes(), b"\"\"\n");
    assert_eq!(missing.records[0].fields.len(), 1);
    assert_eq!(missing.records[0].fields[0].value, "");
}

#[test]
// Verify that an absent trailing field remains absent after an edit, save, and undo.
fn absent_trailing_field_stays_absent_before_and_after_a_saved_edit_undo() {
    let source = CsvDocument::parse(b"a,b\nx\n".to_vec(), EncodingChoice::Auto, ',')
        .expect("ragged CSV parses");
    let mut document = source.to_document(42, "ragged source");
    let address = CellAddress { row: 1, column: 1 };
    document
        .set_cell(1, address, Some(Cell::text("y")))
        .expect("append the missing field");

    document.undo().expect("undo before saving");
    let undone_before_save = source
        .apply_changes(&document_csv_changes(&document))
        .expect("absence delta restores the original row shape");
    assert_eq!(undone_before_save.source_bytes(), b"a,b\nx\n");
    assert_eq!(undone_before_save.records[1].fields.len(), 1);

    document
        .set_cell(1, address, Some(Cell::text("y")))
        .expect("append the missing field again");
    let saved = source
        .apply_changes(&document_csv_changes(&document))
        .expect("save the appended field");
    assert_eq!(saved.source_bytes(), b"a,b\nx,y\n");
    document.mark_saved(document.revision);

    document.undo().expect("undo after the field was saved");
    let undone_after_save = saved
        .apply_changes(&document_csv_changes(&document))
        .expect("opening shape survives the intervening save");
    assert_eq!(undone_after_save.source_bytes(), b"a,b\nx\n");
    assert_eq!(undone_after_save.records[1].fields.len(), 1);
}

#[test]
// Verify that undoing an appended row restores the original blank records.
fn undo_of_an_appended_row_keeps_original_blank_records() {
    let source = CsvDocument::parse(b"a,b\n\nx\n".to_vec(), EncodingChoice::Auto, ',')
        .expect("CSV with an explicit blank record parses");
    let mut document = source.to_document(43, "blank record");
    let address = CellAddress { row: 3, column: 0 };
    document
        .set_cell(1, address, Some(Cell::text("new")))
        .expect("append a row after the original records");
    let saved = source
        .apply_changes(&document_csv_changes(&document))
        .expect("the new row is saved");
    assert_eq!(saved.source_bytes(), b"a,b\n\nx\nnew\n");
    document.mark_saved(document.revision);

    document.undo().expect("undo the appended row");
    let restored = saved
        .apply_changes(&document_csv_changes(&document))
        .expect("undo removes only the appended row");
    assert_eq!(restored.source_bytes(), b"a,b\n\nx\n");
    assert_eq!(restored.records.len(), 3);
    assert!(restored.records[1].fields.is_empty());
}

// Create a document with CSV-backed cells for edit tests.
fn document_csv_changes(document: &Document) -> Vec<CsvCellChange> {
    document
        .changed_cell_snapshot()
        .into_iter()
        .map(|(_, address, cell)| CsvCellChange {
            row: address.row,
            column: address.column,
            value: cell.map(|cell| cell.input.source_text()),
        })
        .collect()
}

#[test]
// Verify that an invalid quote candidate does not hide a valid delimiter.
fn delimiter_candidates_with_an_invalid_quote_do_not_hide_valid_candidates() {
    let analysis = analyze_delimiters(b"left;\",right\nfoo;bar\n", EncodingChoice::Auto)
        .expect("one malformed delimiter interpretation does not reject the source");
    assert_eq!(analysis.suggested, Some(','));
    assert_eq!(analysis.candidates.len(), 2);
    assert!(
        analysis
            .candidates
            .iter()
            .all(|candidate| candidate.delimiter != ';')
    );
    let unsupported = analyze_delimiters(b"\"unfinished\n", EncodingChoice::Auto)
        .expect("valid encoding still opens custom delimiter settings");
    assert!(unsupported.candidates.is_empty());
    assert_eq!(
        CsvDocument::parse(b"\"unfinished\n".to_vec(), EncodingChoice::Auto, ','),
        Err(CsvError::UnclosedQuote)
    );
}

#[test]
// Verify that invalid text after a closing quote is rejected without changing the selected encoding.
fn closed_quote_trailing_text_is_rejected_and_shift_jis_encoding_stays_fixed() {
    assert_eq!(
        CsvDocument::parse(b"\"ab\"cd,x".to_vec(), EncodingChoice::Auto, ','),
        Err(CsvError::InvalidQuoteSyntax)
    );

    let source = CsvDocument::parse(vec![0x82, 0xA0, b',', b'1'], EncodingChoice::ShiftJis, ',')
        .expect("selected Shift_JIS data parses");
    let updated = source
        .replace_field(0, 1, "2")
        .expect("ASCII edits retain the selected encoding");
    assert_eq!(updated.encoding, CsvEncoding::ShiftJis);
    assert_eq!(updated.source_bytes(), &[0x82, 0xA0, b',', b'2']);
    assert_eq!(
        updated.replace_field(0, 0, "¥"),
        Err(CsvError::UnrepresentableCharacter)
    );
}

#[test]
// Verify that later row edits preserve blank rows, BOM, and final newline policy.
fn adding_a_later_row_keeps_blank_rows_bom_and_final_newline_policy() {
    let source = CsvDocument::parse(b"\xEF\xBB\xBFa,b\n".to_vec(), EncodingChoice::Auto, ',')
        .expect("BOM CSV parses");
    let updated = source
        .apply_changes(&[CsvCellChange {
            row: 2,
            column: 2,
            value: Some("z".to_owned()),
        }])
        .expect("new row is serialized");
    assert_eq!(updated.source_bytes(), b"\xEF\xBB\xBFa,b\n\n,,z\n");

    let empty = CsvDocument::parse(b"\xEF\xBB\xBF".to_vec(), EncodingChoice::Auto, ',')
        .expect("BOM-only input is an empty document");
    let first_cell = empty
        .apply_changes(&[CsvCellChange {
            row: 0,
            column: 0,
            value: Some("a".to_owned()),
        }])
        .expect("the first row follows the BOM directly");
    assert_eq!(first_cell.source_bytes(), b"\xEF\xBB\xBFa\n");
}

#[test]
// Verify that ambiguous delimiter detection asks the user to choose.
fn delimiter_detection_requests_choice_on_a_tie() {
    let analysis = analyze_delimiters(b"one\ntwo\n", EncodingChoice::Auto)
        .expect("valid UTF-8 can be analyzed");
    assert_eq!(analysis.suggested, None);
    assert_eq!(analysis.candidates.len(), 3);
}

#[test]
// Verify that non-UTF-8 input needs an encoding choice and cannot be saved lossily.
fn non_utf8_input_requires_an_explicit_choice_and_rejects_lossy_save() {
    let bytes = vec![0x82, 0xA0, b',', b'1'];
    assert_eq!(
        CsvDocument::parse(bytes.clone(), EncodingChoice::Auto, ','),
        Err(CsvError::EncodingChoiceRequired)
    );
    let parsed = CsvDocument::parse(bytes, EncodingChoice::ShiftJis, ',')
        .expect("selected Shift_JIS data parses");
    assert_eq!(parsed.records[0].fields[0].value, "あ");
    assert_eq!(
        parsed.replace_field(0, 0, "𠮷"),
        Err(CsvError::UnrepresentableCharacter)
    );
}

#[test]
// Verify that explicit CP932 wins over valid UTF-8 and keeps a custom delimiter on save.
fn explicit_cp932_and_custom_delimiter_round_trip() {
    let bytes = vec![0xC2, 0xA2, b';', b'1', b'\n'];
    let as_utf8 = CsvDocument::parse(bytes.clone(), EncodingChoice::Utf8, ';')
        .expect("explicit UTF-8 parses the same valid byte sequence");
    assert_eq!(as_utf8.records[0].fields[0].value, "¢");

    let as_cp932 = CsvDocument::parse(bytes, EncodingChoice::ShiftJis, ';')
        .expect("explicit CP932 uses its decoder for every byte");
    assert_eq!(as_cp932.records[0].fields[0].value, "ﾂ｢");
    let saved = as_cp932
        .replace_field(0, 0, "あ")
        .expect("the replacement is representable in CP932");
    assert_eq!(saved.source_bytes(), &[0x82, 0xA0, b';', b'1', b'\n']);
}

#[test]
// Verify that CSV progress contains complete rows and cancellation returns no partial source.
fn csv_progress_stops_only_after_a_complete_record() {
    let mut preview_rows = Vec::new();
    let result = CsvDocument::parse_with_progress(
        b"a,b\n1,2\n3,4\n5,6\n".to_vec(),
        EncodingChoice::Auto,
        ',',
        |row, fields| {
            preview_rows.push((
                row,
                fields
                    .iter()
                    .map(|field| field.value.clone())
                    .collect::<Vec<_>>(),
            ));
            row < 2
        },
    )
    .expect("the source is syntactically valid");

    assert!(result.is_none());
    assert_eq!(
        preview_rows,
        vec![
            (0, vec!["a".to_owned(), "b".to_owned()]),
            (1, vec!["1".to_owned(), "2".to_owned()]),
            (2, vec!["3".to_owned(), "4".to_owned()]),
        ]
    );
}

#[test]
// Verify that a UTF-8 BOM and trailing newline survive parsing and saving.
fn utf8_bom_and_trailing_newline_are_preserved() {
    let source = b"\xEF\xBB\xBFx,y\n".to_vec();
    let parsed = CsvDocument::parse(source.clone(), EncodingChoice::Auto, ',')
        .expect("BOM-marked UTF-8 parses");
    assert_eq!(parsed.encoding, CsvEncoding::Utf8Bom);
    assert_eq!(parsed.records.len(), 1);
    assert_eq!(parsed.source_bytes(), source);
}

#[test]
// Verify that CSV samples retain uneven rows, empty records, and formula-like text.
fn repository_samples_preserve_ragged_rows_empty_records_and_formula_text() {
    let strings = CsvDocument::parse(
        include_bytes!("../samples/strings.csv").to_vec(),
        EncodingChoice::Auto,
        ',',
    )
    .expect("strings sample parses");
    assert_eq!(strings.records.len(), 6);
    assert_eq!(strings.records[1].fields[0].value, "00123");
    assert_eq!(strings.records[2].fields[1].value, "He said \"Hello\"");
    assert_eq!(strings.records[3].fields[1].value, "line 1\nline 2");
    assert_eq!(strings.records[4].fields.len(), 2);
    assert!(strings.records[5].fields.is_empty());

    let formulas = CsvDocument::parse(
        include_bytes!("../samples/formulas.csv").to_vec(),
        EncodingChoice::Auto,
        ',',
    )
    .expect("formula sample parses as text");
    assert_eq!(formulas.records.len(), 10);
    assert_eq!(formulas.records[0].fields[2].value, "=A1*B1");
    assert_eq!(
        formulas.records[7].fields[2].value,
        "=TEXCEL_UNSUPPORTED(1)"
    );
    assert_eq!(formulas.records[8].fields[0].value, "00123");
}
