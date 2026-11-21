use super::*;

#[test]
// Keeps whole-axis copies bounded to the source extent before building clipboard cells.
fn whole_axis_copy_uses_the_extent_and_preserves_small_blank_ranges() {
    let full_sheet = CellRange {
        anchor: CellAddress::default(),
        head: CellAddress {
            row: MAX_ROWS - 1,
            column: MAX_COLUMNS - 1,
        },
    };
    let extent = CellAddress { row: 2, column: 3 };
    let bounded = bounded_copy_ranges(
        &[full_sheet],
        extent,
        crate::clipboard::MAX_CLIPBOARD_CELLS,
        MAX_ROWS,
    )
    .expect("a small document fits the clipboard limit");
    assert_eq!(
        bounded,
        vec![CellRange {
            anchor: CellAddress::default(),
            head: extent,
        }]
    );

    let full_row = CellRange {
        anchor: CellAddress::default(),
        head: CellAddress {
            row: 0,
            column: MAX_COLUMNS - 1,
        },
    };
    assert_eq!(
        bounded_copy_ranges(
            &[full_row],
            extent,
            crate::clipboard::MAX_CLIPBOARD_CELLS,
            MAX_ROWS
        ),
        Some(vec![CellRange {
            anchor: CellAddress::default(),
            head: CellAddress { row: 0, column: 3 },
        }])
    );

    let full_column = CellRange {
        anchor: CellAddress { row: 0, column: 2 },
        head: CellAddress {
            row: MAX_ROWS - 1,
            column: 2,
        },
    };
    assert_eq!(
        bounded_copy_ranges(
            &[full_column],
            extent,
            crate::clipboard::MAX_CLIPBOARD_CELLS,
            MAX_ROWS
        ),
        Some(vec![CellRange {
            anchor: CellAddress { row: 0, column: 2 },
            head: CellAddress { row: 2, column: 2 },
        }])
    );

    let blank = CellRange {
        anchor: CellAddress {
            row: 50,
            column: 20,
        },
        head: CellAddress {
            row: 51,
            column: 21,
        },
    };
    assert_eq!(
        bounded_copy_ranges(
            &[blank],
            CellAddress::default(),
            crate::clipboard::MAX_CLIPBOARD_CELLS,
            MAX_ROWS
        ),
        Some(vec![blank])
    );
}

#[test]
// Keeps the source columns' alignment when a copied range starts after column A.
fn markdown_copy_uses_alignment_for_the_selected_columns() {
    let source = crate::markdown_adapter::MarkdownDocument::parse(
        b"| Name | Center | Amount |\n| --- | :---: | ---: |\n| a | b | 4 |\n".to_vec(),
    )
    .expect("Markdown alignment fixture parses");
    let alignments = copied_markdown_alignments(
        Some(&source),
        Some(0),
        CellRange {
            anchor: CellAddress { row: 0, column: 1 },
            head: CellAddress { row: 1, column: 2 },
        },
    );
    assert_eq!(
        alignments,
        [
            Some(crate::markdown_adapter::MarkdownTableAlignment::Center),
            Some(crate::markdown_adapter::MarkdownTableAlignment::Right),
        ]
    );
}
