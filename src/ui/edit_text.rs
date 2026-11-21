use super::*;

#[path = "edit_text/widget.rs"]
mod widget;
use self::widget::show_cell_edit_widget;

/// Returns the source text for a cell input.
pub(super) fn cell_input_source(input: &CellInput) -> std::borrow::Cow<'_, str> {
    match input {
        CellInput::Empty => std::borrow::Cow::Borrowed(""),
        CellInput::Text(text) | CellInput::Formula(text) | CellInput::Error(text) => {
            std::borrow::Cow::Borrowed(text)
        }
        CellInput::Number(number) => std::borrow::Cow::Owned(number.to_string()),
        CellInput::Boolean(value) => {
            std::borrow::Cow::Borrowed(if *value { "TRUE" } else { "FALSE" })
        }
    }
}

/// Checks whether the cell input needs the detail editor.
pub(super) fn cell_input_needs_detail(input: &CellInput) -> bool {
    let text = match input {
        CellInput::Text(text) | CellInput::Formula(text) | CellInput::Error(text) => text.as_str(),
        CellInput::Empty | CellInput::Number(_) | CellInput::Boolean(_) => return false,
    };
    text.len() > INLINE_EDIT_TEXT_BYTES || text.contains('\n')
}

/// Builds display text for the selected cell view.
pub(crate) fn cell_view_text<'a>(
    cell: &'a Cell,
    calculation: Option<&'a crate::document::CalcResult>,
) -> std::borrow::Cow<'a, str> {
    if let CellInput::Text(source) = &cell.input {
        let value = crate::document::literal_text_value(source);
        if value != source {
            return std::borrow::Cow::Borrowed(value);
        }
    }
    if let Some(value) = calculation.and_then(|result| result.value.as_ref()) {
        return match value {
            crate::document::CalcValue::Text(text) | crate::document::CalcValue::Error(text) => {
                std::borrow::Cow::Borrowed(text)
            }
            crate::document::CalcValue::Number(number) => {
                std::borrow::Cow::Owned(number.to_string())
            }
            crate::document::CalcValue::Boolean(value) => {
                std::borrow::Cow::Borrowed(if *value { "TRUE" } else { "FALSE" })
            }
        };
    }
    cell_input_source(&cell.input)
}

/// Keeps long cell text inside the bounded grid edit window.
pub(super) fn bounded_grid_text(text: std::borrow::Cow<'_, str>) -> std::borrow::Cow<'_, str> {
    if text.len() <= MAX_GRID_TEXT_BYTES {
        return text;
    }
    std::borrow::Cow::Owned(format!("{}…", text_preview(&text, MAX_GRID_TEXT_BYTES - 3)))
}

/// Builds a short preview without cutting a UTF-8 character.
pub(super) fn text_preview(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Finds the byte range around the edit cursor.
pub(super) fn edit_window_range(text: &str, cursor: usize) -> std::ops::Range<usize> {
    if text.len() <= MAX_CELL_EDIT_WINDOW_BYTES {
        return 0..text.len();
    }
    let cursor = clamp_byte_boundary(text, cursor.min(text.len()));
    let mut start = cursor.saturating_sub(MAX_CELL_EDIT_WINDOW_BYTES / 2);
    while !text.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = start
        .saturating_add(MAX_CELL_EDIT_WINDOW_BYTES)
        .min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    if cursor > end {
        end = cursor;
        start = end.saturating_sub(MAX_CELL_EDIT_WINDOW_BYTES);
        while !text.is_char_boundary(start) {
            start -= 1;
        }
    }
    start..end
}

/// Returns the visible text range for the active edit session.
pub(super) fn edit_session_window_range(edit: &EditSession) -> std::ops::Range<usize> {
    if edit.composing
        && let Some(window_start) = edit.edit_window_start
        && edit.buffer.len() > MAX_CELL_EDIT_WINDOW_BYTES
    {
        let start = clamp_byte_boundary(&edit.buffer, window_start.min(edit.buffer.len()));
        let mut end = start
            .saturating_add(MAX_CELL_EDIT_WINDOW_BYTES)
            .min(edit.buffer.len());
        while !edit.buffer.is_char_boundary(end) {
            end -= 1;
        }
        start..end
    } else {
        edit_window_range(&edit.buffer, edit.cursor_byte)
    }
}

/// Moves a byte offset to a valid UTF-8 boundary.
pub(super) fn clamp_byte_boundary(text: &str, mut byte: usize) -> usize {
    byte = byte.min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

/// Finds the changed byte range between two strings.
pub(super) fn text_diff_range(before: &str, after: &str) -> (usize, usize, usize) {
    let mut prefix = 0;
    for ((before_index, before_char), (_, after_char)) in
        before.char_indices().zip(after.char_indices())
    {
        if before_char != after_char {
            break;
        }
        prefix = before_index + before_char.len_utf8();
    }
    let before_tail = &before[prefix..];
    let after_tail = &after[prefix..];
    let mut common_suffix = 0;
    for (before_char, after_char) in before_tail.chars().rev().zip(after_tail.chars().rev()) {
        if before_char != after_char {
            break;
        }
        common_suffix += before_char.len_utf8();
    }
    (
        prefix,
        before.len().saturating_sub(common_suffix),
        after.len().saturating_sub(common_suffix),
    )
}

/// Shows an editor for a bounded section of long text.
pub(super) fn show_bounded_edit_text(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    id: Id,
    edit: &mut EditSession,
    widget: CellEditWidget,
) -> BoundedEditOutput {
    let initial_range = edit_session_window_range(edit);
    if initial_range.len() == edit.buffer.len() {
        if edit.edit_window_start.take().is_some()
            && let Some(mut state) = egui::TextEdit::load_state(ctx, id)
        {
            state.clear_undoer();
            egui::TextEdit::store_state(ctx, id, state);
        }
        // Seeds a fresh editor with the UTF-8-safe selection from the recovered buffer.
        if egui::TextEdit::load_state(ctx, id).is_none() {
            let mut state = egui::widgets::text_edit::TextEditState::default();
            let anchor = local_char_offset(
                &edit.buffer,
                clamp_byte_boundary(&edit.buffer, edit.anchor_byte),
            );
            let cursor = local_char_offset(
                &edit.buffer,
                clamp_byte_boundary(&edit.buffer, edit.cursor_byte),
            );
            state.cursor.set_char_range(Some(CCursorRange {
                primary: CCursor::new(cursor),
                secondary: CCursor::new(anchor),
                h_pos: None,
            }));
            egui::TextEdit::store_state(ctx, id, state);
        }
        let output = show_cell_edit_widget(ui, &mut edit.buffer, id, widget);
        if output.response.changed() {
            edit.mark_buffer_changed();
        }
        if let Some(range) = output.cursor_range {
            edit.cursor_byte = byte_index_for_char(&edit.buffer, range.primary.index.0);
            if !keep_recovered_anchor(ctx, edit, &output.response) {
                edit.anchor_byte = byte_index_for_char(&edit.buffer, range.secondary.index.0);
                edit.preserve_recovered_anchor = false;
            }
        }
        let selection_bytes = output.cursor_range.map(|range| {
            let sorted = range.as_sorted_char_range();
            let start = byte_index_for_char(&edit.buffer, usize::from(sorted.start));
            let end = byte_index_for_char(&edit.buffer, usize::from(sorted.end));
            start..end
        });
        return BoundedEditOutput {
            response: output.response,
            selection_bytes,
        };
    }

    handle_bounded_edit_events(ctx, id, edit, &initial_range, widget);
    let range = edit_session_window_range(edit);
    let external_anchor = edit.anchor_byte < range.start || edit.anchor_byte > range.end;
    let mut window = edit.buffer[range.clone()].to_owned();
    let before = window.clone();
    let local_anchor = local_char_offset(
        &window,
        clamp_byte_boundary(&edit.buffer, edit.anchor_byte.clamp(range.start, range.end))
            - range.start,
    );
    let local_cursor = local_char_offset(
        &window,
        clamp_byte_boundary(&edit.buffer, edit.cursor_byte.clamp(range.start, range.end))
            - range.start,
    );
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    if edit.edit_window_start != Some(range.start) {
        state.clear_undoer();
        edit.edit_window_start = Some(range.start);
    }
    state.cursor.set_char_range(Some(CCursorRange {
        primary: CCursor::new(local_cursor),
        secondary: CCursor::new(local_anchor),
        h_pos: None,
    }));
    egui::TextEdit::store_state(ctx, id, state);
    let output = show_cell_edit_widget(ui, &mut window, id, widget);
    if window != before {
        let (prefix, old_end, new_end) = text_diff_range(&before, &window);
        edit.buffer.replace_range(
            range.start + prefix..range.start + old_end,
            &window[prefix..new_end],
        );
        edit.mark_buffer_changed();
    }
    if let Some(cursor_range) = output.cursor_range {
        let primary = range.start + byte_index_for_char(&window, cursor_range.primary.index.0);
        let secondary = range.start + byte_index_for_char(&window, cursor_range.secondary.index.0);
        edit.cursor_byte = clamp_byte_boundary(&edit.buffer, primary.min(edit.buffer.len()));
        if keep_recovered_anchor(ctx, edit, &output.response) {
            // Keep the saved selection anchor while egui seeds a fresh text state.
        } else if !external_anchor || output.response.clicked() || output.response.dragged() {
            edit.anchor_byte = clamp_byte_boundary(&edit.buffer, secondary.min(edit.buffer.len()));
            edit.preserve_recovered_anchor = false;
        }
    }
    let selection_bytes = output.cursor_range.map(|cursor_range| {
        let sorted = cursor_range.as_sorted_char_range();
        let start = range.start + byte_index_for_char(&window, usize::from(sorted.start));
        let end = range.start + byte_index_for_char(&window, usize::from(sorted.end));
        start..end
    });
    BoundedEditOutput {
        response: output.response,
        selection_bytes,
    }
}

/// Keeps a recovered anchor until a keyboard or pointer event changes the editor.
fn keep_recovered_anchor(
    ctx: &egui::Context,
    edit: &EditSession,
    response: &egui::Response,
) -> bool {
    edit.preserve_recovered_anchor
        && !response.clicked()
        && !response.dragged()
        && !ctx.input(|input| {
            response.has_focus()
                && input.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::Key { pressed: true, .. }
                            | egui::Event::Text(_)
                            | egui::Event::Paste(_)
                    )
                })
        })
}

/// Applies cursor and selection events to the bounded text window.
pub(super) fn handle_bounded_edit_events(
    ctx: &egui::Context,
    id: Id,
    edit: &mut EditSession,
    window: &std::ops::Range<usize>,
    widget: CellEditWidget,
) {
    if edit.composing || !ctx.memory(|memory| memory.has_focus(id)) {
        return;
    }
    let multiline = matches!(widget, CellEditWidget::Multiline { .. });
    let events = ctx.input(|input| input.events.clone());
    let mut retained = Vec::with_capacity(events.len());
    let mut defer_remaining = false;
    let mut buffer_changed = false;
    for event in events {
        if defer_remaining {
            retained.push(event);
            continue;
        }
        let selection =
            edit.anchor_byte.min(edit.cursor_byte)..edit.anchor_byte.max(edit.cursor_byte);
        let selection_outside_window = selection.start < window.start || selection.end > window.end;
        let handled = match &event {
            egui::Event::Copy => {
                if !selection.is_empty() {
                    ctx.copy_text(edit.buffer[selection.clone()].to_owned());
                }
                true
            }
            egui::Event::Cut => {
                if !selection.is_empty() {
                    ctx.copy_text(edit.buffer[selection.clone()].to_owned());
                    edit.buffer.replace_range(selection.clone(), "");
                    edit.anchor_byte = selection.start;
                    edit.cursor_byte = selection.start;
                    buffer_changed = true;
                }
                true
            }
            egui::Event::Paste(text) => {
                edit.buffer.replace_range(selection.clone(), text);
                edit.cursor_byte = selection.start + text.len();
                edit.anchor_byte = edit.cursor_byte;
                buffer_changed = true;
                true
            }
            egui::Event::Text(text) if selection_outside_window && !text.is_empty() => {
                edit.buffer.replace_range(selection.clone(), text);
                edit.cursor_byte = selection.start + text.len();
                edit.anchor_byte = edit.cursor_byte;
                buffer_changed = true;
                true
            }
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => {
                if modifiers.command && *key == Key::A {
                    edit.anchor_byte = 0;
                    edit.cursor_byte = edit.buffer.len();
                    true
                } else if matches!(*key, Key::Backspace | Key::Delete) && !selection.is_empty() {
                    edit.buffer.replace_range(selection.clone(), "");
                    edit.anchor_byte = selection.start;
                    edit.cursor_byte = selection.start;
                    buffer_changed = true;
                    true
                } else if *key == Key::Backspace && !modifiers.command && !modifiers.alt {
                    if !selection.is_empty() {
                        edit.buffer.replace_range(selection.clone(), "");
                        edit.cursor_byte = selection.start;
                        buffer_changed = true;
                    } else if edit.cursor_byte > 0 {
                        let previous = previous_char_boundary(&edit.buffer, edit.cursor_byte);
                        edit.buffer.replace_range(previous..edit.cursor_byte, "");
                        edit.cursor_byte = previous;
                        buffer_changed = true;
                    }
                    edit.anchor_byte = edit.cursor_byte;
                    true
                } else if *key == Key::Delete && !modifiers.command && !modifiers.alt {
                    if !selection.is_empty() {
                        edit.buffer.replace_range(selection.clone(), "");
                        edit.cursor_byte = selection.start;
                        edit.anchor_byte = edit.cursor_byte;
                        buffer_changed = true;
                    } else if edit.cursor_byte < edit.buffer.len() {
                        let next = next_char_boundary(&edit.buffer, edit.cursor_byte);
                        edit.buffer.replace_range(edit.cursor_byte..next, "");
                        buffer_changed = true;
                    }
                    true
                } else if *key == Key::ArrowLeft && !modifiers.command && !modifiers.alt {
                    let cursor = if modifiers.shift || selection.is_empty() {
                        previous_char_boundary(&edit.buffer, edit.cursor_byte)
                    } else {
                        selection.start
                    };
                    if !modifiers.shift {
                        edit.anchor_byte = cursor;
                    }
                    edit.cursor_byte = cursor;
                    true
                } else if *key == Key::ArrowRight && !modifiers.command && !modifiers.alt {
                    let cursor = if modifiers.shift || selection.is_empty() {
                        next_char_boundary(&edit.buffer, edit.cursor_byte)
                    } else {
                        selection.end
                    };
                    if !modifiers.shift {
                        edit.anchor_byte = cursor;
                    }
                    edit.cursor_byte = cursor;
                    true
                } else if selection_outside_window
                    && multiline
                    && (*key == Key::Enter || *key == Key::Tab)
                {
                    let inserted = if *key == Key::Enter { "\n" } else { "\t" };
                    edit.buffer.replace_range(selection.clone(), inserted);
                    edit.cursor_byte = selection.start + inserted.len();
                    edit.anchor_byte = edit.cursor_byte;
                    buffer_changed = true;
                    true
                } else {
                    false
                }
            }
            _ => false,
        };
        if !handled {
            retained.push(event);
            defer_remaining = true;
        }
    }
    if buffer_changed {
        edit.mark_buffer_changed();
    }
    ctx.input_mut(|input| input.events = retained);
}

/// Finds the previous UTF-8 character boundary.
pub(super) fn previous_char_boundary(text: &str, offset: usize) -> usize {
    let offset = clamp_byte_boundary(text, offset.min(text.len()));
    if offset == 0 {
        0
    } else {
        text[..offset]
            .char_indices()
            .next_back()
            .map_or(0, |(index, _)| index)
    }
}

/// Finds the next UTF-8 character boundary.
pub(super) fn next_char_boundary(text: &str, offset: usize) -> usize {
    let offset = clamp_byte_boundary(text, offset.min(text.len()));
    text.get(offset..)
        .and_then(|tail| tail.chars().next())
        .map_or(text.len(), |character| offset + character.len_utf8())
}
