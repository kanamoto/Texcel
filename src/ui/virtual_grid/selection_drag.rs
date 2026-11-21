use super::super::*;

// Updates an active rectangle drag in the same frozen and filtered coordinate space as cells.
pub(super) fn update_selection_drag(
    ui: &egui::Ui,
    outcome: &mut GridOutcome,
    drag: Option<GridSelectionDrag>,
    geometry: GridPointerGeometry<'_>,
    document_id: DocumentId,
    table_id: u64,
) {
    let (position, released, primary_down, cancelled) = ui.input(|input| {
        (
            input.pointer.interact_pos(),
            input.pointer.button_released(egui::PointerButton::Primary),
            input.pointer.button_down(egui::PointerButton::Primary),
            input.key_pressed(Key::Escape),
        )
    });
    let Some(_) = drag.filter(|drag| drag.document_id == document_id && drag.table_id == table_id)
    else {
        return;
    };
    if cancelled {
        outcome.selection_drag_finished = true;
        return;
    }
    let finish_after_update = released || !primary_down;
    let Some(position) = position else {
        if finish_after_update {
            outcome.selection_drag_finished = true;
        }
        return;
    };
    let endpoint_rect = Rect::from_min_max(
        Pos2::new(
            (geometry.body_min.x + geometry.frozen_width).min(geometry.viewport.max.x),
            (geometry.body_min.y + geometry.frozen_height).min(geometry.viewport.max.y),
        ),
        geometry.viewport.max - Vec2::splat(1.0),
    );
    if !endpoint_rect.is_negative() {
        let edge = 24.0;
        let mut scroll = Vec2::ZERO;
        if position.x >= endpoint_rect.left() && position.x < endpoint_rect.left() + edge {
            scroll.x = 18.0;
        } else if position.x <= endpoint_rect.right() && position.x > endpoint_rect.right() - edge {
            scroll.x = -18.0;
        }
        if position.y >= endpoint_rect.top() && position.y < endpoint_rect.top() + edge {
            scroll.y = 18.0;
        } else if position.y <= endpoint_rect.bottom() && position.y > endpoint_rect.bottom() - edge
        {
            scroll.y = -18.0;
        }
        if scroll != Vec2::ZERO {
            ui.scroll_with_delta(scroll);
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }
    }
    let clamped = Pos2::new(
        position
            .x
            .clamp(geometry.body_min.x, geometry.viewport.max.x - 1.0),
        position
            .y
            .clamp(geometry.body_min.y, geometry.viewport.max.y - 1.0),
    );
    if let Some(address) = grid_address_at_pointer(clamped, geometry) {
        outcome.selection_drag_update = Some(address);
    }
    if finish_after_update {
        outcome.selection_drag_finished = true;
    }
}
