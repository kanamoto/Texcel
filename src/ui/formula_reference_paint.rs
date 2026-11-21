use super::*;

// Paints the source ranges visible in the active formula edit.
pub(super) fn paint(
    painter: &egui::Painter,
    rect: Rect,
    address: CellAddress,
    overlays: &[FormulaReferenceOverlay],
) {
    for reference in overlays {
        let (start, end) = reference.cells.normalized();
        if address.row < start.row
            || address.row > end.row
            || address.column < start.column
            || address.column > end.column
        {
            continue;
        }
        let stroke = Stroke::new(2.2, formula_reference_color(reference.color_index));
        if address.row == start.row {
            painter.line_segment([rect.left_top(), rect.right_top()], stroke);
        }
        if address.row == end.row {
            painter.line_segment([rect.left_bottom(), rect.right_bottom()], stroke);
        }
        if address.column == start.column {
            painter.line_segment([rect.left_top(), rect.left_bottom()], stroke);
        }
        if address.column == end.column {
            painter.line_segment([rect.right_top(), rect.right_bottom()], stroke);
        }
    }
}
