use super::*;
use crate::file_worker::LoadPreviewCell;

#[test]
// Limit the rendered rows and columns to the nonzero viewport ranges.
fn preview_virtualization_indexes_both_scrolled_axes() {
    let rows = visible_range(
        288.0,
        444.0,
        PREVIEW_HEADER_HEIGHT,
        PREVIEW_ROW_HEIGHT,
        1_000_000,
    );
    let columns = visible_range(
        430.0,
        790.0,
        PREVIEW_ROW_HEADER_WIDTH,
        PREVIEW_COLUMN_WIDTH,
        16_384,
    );

    assert_eq!(rows, 10..16);
    assert_eq!(columns, 2..5);
}

#[test]
// Find sparse rows by their source indices without copying their cell contents.
fn preview_table_looks_up_rows_by_sparse_source_index() {
    let row = LoadPreviewRow {
        index: 7,
        cells: vec![
            LoadPreviewCell {
                column: 0,
                text: "first".to_owned(),
            },
            LoadPreviewCell {
                column: 3,
                text: "last".to_owned(),
            },
        ]
        .into(),
    };
    let chunk = Arc::new(LoadPreviewChunk {
        table_index: 0,
        table_name: "Sheet".to_owned(),
        first_row: 7,
        last_row: 7,
        column_count: 4,
        rows: vec![row].into(),
    });
    let table = LoadPreviewTable {
        name: "Sheet".to_owned(),
        row_count: 8,
        column_count: 4,
        chunks: BTreeMap::from([(7, chunk)]),
    };

    assert_eq!(table.row(7).unwrap().cells[1].column, 3);
    assert!(table.row(6).is_none());
}
