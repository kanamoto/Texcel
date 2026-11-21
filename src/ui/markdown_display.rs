use super::*;

// Return full-document Markdown display data for a grid cell.
pub(super) fn markdown_display_for_cell<'a>(
    grid: GridTable<'a>,
    address: CellAddress,
    cell: &Cell,
    cache: &'a mut BTreeMap<MarkdownCellDisplayKey, CachedMarkdownCellDisplay>,
    requests: &mut Vec<(u64, CellAddress)>,
) -> Option<&'a MarkdownCellDisplay> {
    let source = grid.markdown_source;
    let table_index = grid.markdown_table_index;
    let document_id = grid.document.id;
    let table_id = grid.table.id;
    let cell_input_revision = grid.document.cell_input_revision(table_id, address);
    let source_cell_unchanged = !grid.document.cell_input_is_dirty(table_id, address);
    let CellInput::Text(text) = &cell.input else {
        return None;
    };
    if let Some(source) = source {
        let source_display = source.cell_display(table_index?, address.row, address.column);
        if (source_cell_unchanged
            || (text.len() <= MAX_MARKDOWN_CELL_PARSE_BYTES
                && source
                    .cell_display_for_source(table_index?, address.row, address.column, text)
                    .is_some()))
            && let Some(display) = source_display
        {
            return Some(display);
        }
    }
    let key = (document_id, table_id, address);
    if let Some(cached) = cache.get(&key).filter(|cached| {
        cached.cell_input_revision == Some(cell_input_revision)
            && cached.edit_session_id.is_none()
            && cached.raw_text.is_none()
    }) {
        return Some(&cached.display);
    }
    requests.push((table_id, address));
    None
}

// Build a bounded Markdown display for a search or preview result.
pub(super) fn markdown_display_for_preview<'a>(
    grid: GridTable<'_>,
    address: CellAddress,
    session_id: u64,
    text: &str,
    cache: &'a mut BTreeMap<MarkdownCellDisplayKey, CachedMarkdownCellDisplay>,
) -> Option<&'a MarkdownCellDisplay> {
    let document_id = grid.document.id;
    let table_id = grid.table.id;
    let document_revision = grid.document.revision;
    let calculation_revision = grid.document.calculation_revision;
    if text.len() > MAX_MARKDOWN_CELL_PARSE_BYTES {
        return None;
    }
    let key = (document_id, table_id, address);
    let stale = cache.get(&key).is_none_or(|cached| {
        cached.document_revision != document_revision
            || cached.calculation_revision != calculation_revision
            || cached.edit_session_id != Some(session_id)
            || cached.raw_text.as_deref() != Some(text)
    });
    if stale {
        if cache.len() >= MAX_CACHED_MARKDOWN_CELLS {
            cache.clear();
        }
        cache.insert(
            key,
            CachedMarkdownCellDisplay {
                document_revision,
                calculation_revision,
                cell_input_revision: None,
                raw_text: Some(text.to_owned()),
                edit_session_id: Some(session_id),
                display: MarkdownCellDisplay::parse_fragment(text),
            },
        );
    }
    cache.get(&key).map(|cached| &cached.display)
}
