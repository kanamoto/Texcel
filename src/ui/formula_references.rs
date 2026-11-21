use super::*;

use ironcalc_base::expressions::{
    lexer::util::{cycle_reference, get_tokens},
    token::TokenType,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct FormulaReferenceToken {
    pub(super) bytes: std::ops::Range<usize>,
    pub(super) cells: CellRange,
    overlay: Option<CellRange>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FormulaReferenceOverlay {
    pub(super) cells: CellRange,
    pub(super) color_index: usize,
}

// Clips formula outlines to the sheet.
pub(super) fn visible_formula_reference_overlays(
    overlays: &[FormulaReferenceOverlay],
    rows: usize,
    columns: usize,
) -> Vec<FormulaReferenceOverlay> {
    overlays
        .iter()
        .filter_map(|overlay| {
            let (start, end) = overlay.cells.normalized();
            // Outlines stop at the sheet edge.
            let end = CellAddress {
                row: end.row.min(rows.saturating_sub(1)),
                column: end.column.min(columns.saturating_sub(1)),
            };
            (start.row <= end.row && start.column <= end.column).then_some(
                FormulaReferenceOverlay {
                    cells: CellRange {
                        anchor: start,
                        head: end,
                    },
                    color_index: overlay.color_index,
                },
            )
        })
        .collect()
}

// Chooses distinct colors for formula references on the grid.
pub(super) fn formula_reference_color(index: usize) -> Color32 {
    const COLORS: [Color32; 6] = [
        Color32::from_rgb(220, 55, 47),
        Color32::from_rgb(37, 112, 205),
        Color32::from_rgb(26, 142, 91),
        Color32::from_rgb(163, 82, 190),
        Color32::from_rgb(208, 126, 18),
        Color32::from_rgb(15, 142, 158),
    ];
    COLORS[index % COLORS.len()]
}

#[derive(Clone, Debug, Default)]
pub(super) struct FormulaReferenceState {
    document_id: Option<DocumentId>,
    table_id: Option<TableId>,
    address: Option<CellAddress>,
    session_id: Option<u64>,
    buffer_revision: Option<u64>,
    was_formula: bool,
    tokens: Vec<FormulaReferenceToken>,
    cursor: Option<(usize, usize)>,
    owned_inserted_span: Option<std::ops::Range<usize>>,
    owned_inserted_range: Option<CellRange>,
    pub(super) current_range: Option<CellRange>,
    pub(super) drag_anchor: Option<CellAddress>,
}

impl FormulaReferenceState {
    // Keeps parsed references tied to one active edit buffer.
    pub(super) fn refresh(
        &mut self,
        document_id: Option<DocumentId>,
        table_id: Option<TableId>,
        address: Option<CellAddress>,
        edit: &EditSession,
        row_count: usize,
        column_count: usize,
    ) {
        let formula = edit.is_editing() && edit.buffer.starts_with('=');
        let new_session = self.session_id != Some(edit.session_id)
            || self.document_id != document_id
            || self.table_id != table_id
            || self.address != address;
        if new_session {
            self.document_id = document_id;
            self.table_id = table_id;
            self.address = address;
            self.session_id = Some(edit.session_id);
            self.buffer_revision = None;
            self.was_formula = formula;
            self.current_range = None;
            self.drag_anchor = None;
            self.cursor = None;
            self.owned_inserted_span = None;
            self.owned_inserted_range = None;
        } else if formula != self.was_formula {
            self.was_formula = formula;
            self.current_range = None;
            self.drag_anchor = None;
            self.cursor = None;
            self.owned_inserted_span = None;
            self.owned_inserted_range = None;
        }
        if !formula {
            self.tokens.clear();
            self.buffer_revision = Some(edit.buffer_revision);
            self.cursor = Some((edit.anchor_byte, edit.cursor_byte));
            return;
        }
        if self.buffer_revision != Some(edit.buffer_revision) {
            self.tokens = parse_formula_references(&edit.buffer, row_count, column_count);
            self.buffer_revision = Some(edit.buffer_revision);
            self.owned_inserted_span = None;
            self.owned_inserted_range = None;
            self.current_range = self.token_at_cursor(edit).map(|token| token.cells);
            self.drag_anchor = None;
        } else if self.cursor != Some((edit.anchor_byte, edit.cursor_byte)) {
            self.owned_inserted_span = None;
            self.owned_inserted_range = None;
            self.current_range = self.token_at_cursor(edit).map(|token| token.cells);
            self.drag_anchor = None;
        }
        self.cursor = Some((edit.anchor_byte, edit.cursor_byte));
    }

    // Clears the current reference state when no cell formula is being edited.
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    // Returns the visible cell outlines for the current formula.
    pub(super) fn overlays(&self) -> Vec<FormulaReferenceOverlay> {
        self.tokens
            .iter()
            .enumerate()
            .filter_map(|(color_index, token)| {
                Some(FormulaReferenceOverlay {
                    cells: token.overlay?,
                    color_index,
                })
            })
            .collect()
    }

    // Finds the reference under the current text selection or cursor.
    pub(super) fn token_at_cursor(&self, edit: &EditSession) -> Option<&FormulaReferenceToken> {
        let start = edit.anchor_byte.min(edit.cursor_byte);
        let end = edit.anchor_byte.max(edit.cursor_byte);
        self.tokens.iter().find(|token| {
            if start == end {
                token.bytes.start <= start && start <= token.bytes.end
            } else {
                token.bytes.start < end && start < token.bytes.end
            }
        })
    }

    // Returns the original document and table targeted by this formula edit.
    pub(super) fn target(&self) -> Option<(DocumentId, TableId, CellAddress, u64)> {
        Some((
            self.document_id?,
            self.table_id?,
            self.address?,
            self.session_id?,
        ))
    }

    // Remembers the grid range currently represented by the edit buffer.
    pub(super) fn set_current_range(&mut self, range: Option<CellRange>) {
        self.current_range = range;
    }

    // Remembers the reference inserted by the current picker gesture.
    pub(super) fn set_owned_inserted_reference(
        &mut self,
        edit: &EditSession,
        span: std::ops::Range<usize>,
        range: CellRange,
    ) {
        self.owned_inserted_span = Some(span);
        self.owned_inserted_range = Some(range);
        self.current_range = Some(range);
        self.cursor = Some((edit.anchor_byte, edit.cursor_byte));
        self.buffer_revision = Some(edit.buffer_revision);
    }

    // Tracks cursor changes without reparsing an unchanged formula buffer.
    pub(super) fn sync_cursor(&mut self, edit: &EditSession) {
        if self.cursor != Some((edit.anchor_byte, edit.cursor_byte)) {
            self.owned_inserted_span = None;
            self.owned_inserted_range = None;
            self.current_range = self.token_at_cursor(edit).map(|token| token.cells);
            self.drag_anchor = None;
            self.cursor = Some((edit.anchor_byte, edit.cursor_byte));
        }
    }

    // Chooses the text selection when it spans beyond one reference token.
    pub(super) fn insertion_range(
        &self,
        edit: &EditSession,
        captured_selection: Option<std::ops::Range<usize>>,
    ) -> std::ops::Range<usize> {
        let start = captured_selection
            .as_ref()
            .map_or(edit.anchor_byte.min(edit.cursor_byte), |range| range.start);
        let end = captured_selection
            .as_ref()
            .map_or(edit.anchor_byte.max(edit.cursor_byte), |range| range.end);
        if start != end {
            let overlaps = self
                .tokens
                .iter()
                .filter(|token| token.bytes.start < end && start < token.bytes.end)
                .collect::<Vec<_>>();
            if overlaps.len() != 1 || overlaps[0].bytes.start > start || overlaps[0].bytes.end < end
            {
                return start..end;
            }
            return overlaps[0].bytes.clone();
        }
        self.owned_inserted_span
            .clone()
            .or_else(|| self.token_at_cursor(edit).map(|token| token.bytes.clone()))
            .unwrap_or(start..end)
    }

    // Uses IronCalc's formula-aware F4 cycle for the selected token.
    pub(super) fn cycle_reference(&self, edit: &mut EditSession) -> Option<CellRange> {
        let selected = self.token_at_cursor(edit)?.cells;
        let start = byte_to_char_index(&edit.buffer, edit.anchor_byte.min(edit.cursor_byte));
        let end = byte_to_char_index(&edit.buffer, edit.anchor_byte.max(edit.cursor_byte));
        let locale = ironcalc_base::locale::get_locale("en").ok()?;
        let language = ironcalc_base::language::get_language("en").ok()?;
        let (formula, anchor, cursor) =
            cycle_reference(&edit.buffer, start, end, locale, language).ok()?;
        edit.buffer = formula;
        edit.anchor_byte = byte_index_for_char(&edit.buffer, usize::try_from(anchor).ok()?);
        edit.cursor_byte = byte_index_for_char(&edit.buffer, usize::try_from(cursor).ok()?);
        edit.mark_buffer_changed();
        Some(selected)
    }
}

// Converts IronCalc's character positions into safe UTF-8 byte ranges.
fn parse_formula_references(
    formula: &str,
    row_count: usize,
    column_count: usize,
) -> Vec<FormulaReferenceToken> {
    let Some(body) = formula.strip_prefix('=') else {
        return Vec::new();
    };
    let mut references = Vec::new();
    for marked in get_tokens(body) {
        let (cells, visible) = match marked.token {
            TokenType::Reference {
                sheet, row, column, ..
            } => {
                let Some(cell) = coordinate_address(row, column) else {
                    continue;
                };
                let cells = CellRange::single(cell);
                let overlay = sheet
                    .is_none()
                    .then(|| visible_formula_range(cells, row_count, column_count))
                    .flatten();
                (cells, overlay)
            }
            TokenType::Range { sheet, left, right } => {
                let Some(start) = coordinate_address(left.row, left.column) else {
                    continue;
                };
                let Some(end) = coordinate_address(right.row, right.column) else {
                    continue;
                };
                let cells = CellRange {
                    anchor: start,
                    head: end,
                };
                let overlay = sheet
                    .is_none()
                    .then(|| visible_formula_range(cells, row_count, column_count))
                    .flatten();
                (cells, overlay)
            }
            _ => continue,
        };
        let Some(start_char) = usize::try_from(marked.start)
            .ok()
            .map(|start| start.saturating_add(1))
        else {
            continue;
        };
        let Some(end_char) = usize::try_from(marked.end)
            .ok()
            .map(|end| end.saturating_add(1))
        else {
            continue;
        };
        let bytes =
            byte_index_for_char(formula, start_char)..byte_index_for_char(formula, end_char);
        references.push(FormulaReferenceToken {
            bytes,
            cells,
            overlay: visible,
        });
    }
    references
}

// Keeps valid references outside the current table extent for F4 and cursor matching.
fn coordinate_address(row: i32, column: i32) -> Option<CellAddress> {
    let row = usize::try_from(row.checked_sub(1)?).ok()?;
    let column = usize::try_from(column.checked_sub(1)?).ok()?;
    if row >= MAX_ROWS || column >= MAX_COLUMNS {
        return None;
    }
    Some(CellAddress { row, column })
}

// Clips a valid reference range to cells currently drawn by the grid.
fn visible_formula_range(
    range: CellRange,
    row_count: usize,
    column_count: usize,
) -> Option<CellRange> {
    let (start, end) = range.normalized();
    if row_count == 0 || column_count == 0 || start.row >= row_count || start.column >= column_count
    {
        return None;
    }
    Some(CellRange {
        anchor: start,
        head: CellAddress {
            row: end.row.min(row_count - 1),
            column: end.column.min(column_count - 1),
        },
    })
}

// Converts a UTF-8 byte position to IronCalc's character cursor position.
fn byte_to_char_index(text: &str, byte: usize) -> usize {
    text.get(..byte).map_or(0, |prefix| prefix.chars().count())
}

#[cfg(test)]
#[path = "formula_references_test.rs"]
mod tests;
