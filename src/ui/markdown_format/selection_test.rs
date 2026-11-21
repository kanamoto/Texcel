use super::*;

/// Keeps one active-cell state for each whole-cell modifier across a mixed range.
#[test]
fn mixed_ranges_apply_and_remove_all_four_modifiers_from_active_state() {
    let actions = [
        (CellFormatAction::Bold, "**value**"),
        (CellFormatAction::Italic, "*value*"),
        (CellFormatAction::Strike, "~~value~~"),
        (CellFormatAction::Code, "`value`"),
    ];
    for (action, decorated) in actions {
        let mut table = crate::document::Table::new(1, "Format test");
        let active = CellAddress { row: 1, column: 0 };
        let peer = CellAddress { row: 1, column: 1 };
        table.set_loaded_cell(active, Cell::text(decorated));
        table.set_loaded_cell(peer, Cell::text("plain"));
        let selected = vec![
            (active, table.cell(active).unwrap().clone()),
            (peer, table.cell(peer).unwrap().clone()),
        ];

        let removed = format_selected_cells(&table, selected.clone(), action, false, false)
            .expect("a mixed range can remove the active format");
        assert_eq!(
            changed_text(&removed.cells, active).as_deref(),
            Some("value")
        );
        assert_eq!(changed_text(&removed.cells, peer), None);

        let applied = format_selected_cells(&table, selected, action, false, true)
            .expect("a mixed range can apply the active format");
        assert_eq!(changed_text(&applied.cells, active), None);
        let plain_decorated = decorated.replace("value", "plain");
        assert_eq!(
            changed_text(&applied.cells, peer).as_deref(),
            Some(plain_decorated.as_str())
        );
    }
}

/// Converts numeric and Boolean constants through the same visible Markdown spelling.
#[test]
fn typed_constants_can_receive_markdown_formatting() {
    let table = crate::document::Table::new(2, "Typed values");
    let selected = vec![
        (CellAddress { row: 0, column: 0 }, Cell::number(3.0)),
        (
            CellAddress { row: 0, column: 1 },
            Cell {
                input: CellInput::Boolean(true),
                ..Cell::empty()
            },
        ),
    ];
    let formatted = format_selected_cells(&table, selected, CellFormatAction::Bold, false, true)
        .expect("constant number and Boolean cells can be formatted");
    assert_eq!(
        changed_text(&formatted.cells, CellAddress { row: 0, column: 0 }).as_deref(),
        Some("**3**")
    );
    assert_eq!(
        changed_text(&formatted.cells, CellAddress { row: 0, column: 1 }).as_deref(),
        Some("**TRUE**")
    );
}

/// Removes both GFM marker spellings without treating intraword underscores as formatting.
#[test]
fn alternate_emphasis_markers_toggle_without_changing_ordinary_underscores() {
    for (action, formatted) in [
        (CellFormatAction::Bold, "__strong__"),
        (CellFormatAction::Italic, "_emphasis_"),
    ] {
        let mut table = crate::document::Table::new(1, "Alternate markers");
        let active = CellAddress { row: 1, column: 0 };
        let peer = CellAddress { row: 1, column: 1 };
        table.set_loaded_cell(active, Cell::text(formatted));
        table.set_loaded_cell(peer, Cell::text("snake_case_"));
        let selected = vec![
            (active, table.cell(active).unwrap().clone()),
            (peer, table.cell(peer).unwrap().clone()),
        ];
        let removed = format_selected_cells(&table, selected, action, false, false)
            .expect("valid GFM emphasis can be removed from a mixed range");
        let expected = if action == CellFormatAction::Bold {
            "strong"
        } else {
            "emphasis"
        };
        assert_eq!(
            changed_text(&removed.cells, active).as_deref(),
            Some(expected)
        );
        assert_eq!(changed_text(&removed.cells, peer), None);
    }
}

/// Formats converted spreadsheet error values with the same visible value as text input.
#[test]
fn error_constants_support_all_four_cell_modifiers() {
    for (action, formatted) in [
        (CellFormatAction::Bold, "**#DIV/0!**"),
        (CellFormatAction::Italic, "*#DIV/0!*"),
        (CellFormatAction::Strike, "~~#DIV/0!~~"),
        (CellFormatAction::Code, "`#DIV/0!`"),
    ] {
        let table = crate::document::Table::new(1, "Error values");
        let address = CellAddress { row: 0, column: 0 };
        let selected = vec![(
            address,
            Cell {
                input: CellInput::Error("#DIV/0!".to_owned()),
                ..Cell::empty()
            },
        )];
        let changes = format_selected_cells(&table, selected, action, false, true)
            .expect("a formula result error is a formattable constant");
        assert_eq!(
            changed_text(&changes.cells, address).as_deref(),
            Some(formatted)
        );
    }
}

/// Reads the source spelling of one planned change.
fn changed_text(changes: &[(CellAddress, Option<Cell>)], address: CellAddress) -> Option<String> {
    changes
        .iter()
        .find(|(changed_address, _)| *changed_address == address)
        .and_then(|(_, cell)| cell.as_ref())
        .map(|cell| cell.input.source_text())
}
