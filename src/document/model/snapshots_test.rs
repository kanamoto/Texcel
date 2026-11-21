use super::*;

/// Keeps column snapshots narrow and synchronized through undo and save resets.
#[test]
fn changed_cell_column_index_tracks_edits_undo_and_save() {
    let table_id = 1;
    let mut document = Document::new(1, "Grid", &["Table"]);
    for (address, value) in [
        (CellAddress { row: 0, column: 0 }, "a"),
        (CellAddress { row: 0, column: 1 }, "b"),
        (CellAddress { row: 1, column: 1 }, "c"),
    ] {
        document
            .set_cell(table_id, address, Some(Cell::text(value)))
            .expect("the test cell is stored");
    }

    let column = document.changed_cell_snapshot_for_column(table_id, 1);
    assert_eq!(column.len(), 2);

    document.undo().expect("the last edit can be undone");
    let column = document.changed_cell_snapshot_for_column(table_id, 1);
    assert_eq!(column.len(), 2);
    assert!(column.iter().any(|(_, address, cell)| {
        *address == CellAddress { row: 1, column: 1 } && cell.is_none()
    }));

    document.mark_saved(document.revision);
    assert!(
        document
            .changed_cell_snapshot_for_column(table_id, 1)
            .is_empty()
    );
}
