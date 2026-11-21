use super::*;

/// Builds the accessible label for one grid cell.
pub(super) fn grid_cell_accessibility_label(
    ctx: &egui::Context,
    table_name: &str,
    address: CellAddress,
    column_header: &str,
    row_header: &str,
    selected: bool,
    editing: bool,
) -> String {
    let column_header = if column_header.is_empty() {
        Message::new("dialog-empty-tail")
    } else {
        Message::new("grid-cell-a11y-column-header").with_text("detail", column_header)
    };
    let row_header = if row_header.is_empty() || address.column == 0 {
        Message::new("dialog-empty-tail")
    } else {
        Message::new("grid-cell-a11y-row-header").with_text("detail", row_header)
    };
    let selection = Message::new(if selected {
        "grid-cell-a11y-selected"
    } else {
        "grid-cell-a11y-unselected"
    });
    let editing = Message::new(if editing {
        "grid-cell-a11y-editing"
    } else {
        "dialog-empty-tail"
    });
    context_message(
        ctx,
        &Message::new("grid-cell-a11y")
            .with_text("table", table_name)
            .with_text("coordinate", cell_coordinate(address))
            .with_number("row", (address.row + 1) as i64)
            .with_number("column", (address.column + 1) as i64)
            .with_message("column_header", column_header)
            .with_message("row_header", row_header)
            .with_message("selection", selection)
            .with_message("editing", editing),
    )
}

/// Registers visible cell text and actions with AccessKit.
pub(super) fn register_grid_cell_accessibility(
    ctx: &egui::Context,
    response: &egui::Response,
    label: &str,
    value: &str,
    address: CellAddress,
    selected: bool,
) {
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::GridCell, true, selected, label.to_owned())
    });
    ctx.accesskit_node_builder(response.id, |node| {
        use egui::accesskit::Action;
        node.set_role(egui::Role::GridCell);
        node.set_label(label);
        node.set_value(value);
        node.set_row_index(address.row);
        node.set_column_index(address.column);
        node.set_selected(selected);
        node.add_action(Action::ScrollIntoView);
    });
}

/// Returns the namespace used for AccessKit grid node IDs.
pub(super) fn accesskit_session_namespace() -> u32 {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    (Id::unique(("texcel-accesskit-session", std::process::id(), nonce)).value() as u32)
        & ACCESSKIT_TABLE_TOKEN_MASK
}

/// Builds a stable AccessKit node ID for a grid cell.
pub(super) fn grid_accesskit_cell_id(
    table_token: u32,
    address: CellAddress,
    segment_index: usize,
) -> Option<Id> {
    if table_token > ACCESSKIT_TABLE_TOKEN_MASK
        || address.row >= MAX_ROWS
        || address.column >= MAX_COLUMNS
        || segment_index >= 4
    {
        return None;
    }
    let packed = (ACCESSKIT_NODE_MARKER << 60)
        | ((table_token as u64) << 36)
        | ((address.row as u64) << 16)
        | ((address.column as u64) << 2)
        | segment_index as u64;
    let mixed = mix_grid_node_id(packed);
    debug_assert_ne!(mixed, 0, "the nonzero marker reserves egui's null ID");
    // The reversible full-width mixer diffuses the packed table/coordinate bits before using
    // egui's high-entropy ID constructor. The table token is unique for this app lifetime.
    // SAFETY: `mixed` is a bijective transform of a nonzero packed ID; uniqueness follows from
    // the unique table token and disjoint row/column/segment fields.
    Some(unsafe { Id::from_high_entropy_bits(mixed) })
}

/// Decodes an AccessKit node ID to its grid address.
pub(super) fn decode_grid_cell_node_id(
    node_id: egui::accesskit::NodeId,
) -> Option<(u32, CellAddress, usize)> {
    let packed = unmix_grid_node_id(u64::from(node_id));
    if packed >> 60 != ACCESSKIT_NODE_MARKER {
        return None;
    }
    let table_token = ((packed >> 36) & ACCESSKIT_TABLE_TOKEN_MASK as u64) as u32;
    let row = ((packed >> 16) & ((1 << 20) - 1)) as usize;
    let column = ((packed >> 2) & ((1 << 14) - 1)) as usize;
    let segment = (packed & 0b11) as usize;
    Some((table_token, CellAddress { row, column }, segment))
}

/// Mixes a grid address into an AccessKit node ID.
pub(super) fn mix_grid_node_id(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

/// Reverses the grid node ID mixing step.
pub(super) fn unmix_grid_node_id(value: u64) -> u64 {
    /// Reverses one right-shift XOR used in the ID mix.
    fn unxorshift_right(value: u64, shift: u32) -> u64 {
        let mut result = value;
        let mut bit_shift = shift;
        while bit_shift < 64 {
            result ^= value >> bit_shift;
            bit_shift += shift;
        }
        result
    }

    let value = unxorshift_right(value, 31);
    let value = value.wrapping_mul(0x3196_42b2_d24d_8ec3);
    let value = unxorshift_right(value, 27);
    let value = value.wrapping_mul(0x96de_1b17_3f11_9089);
    unxorshift_right(value, 30)
}

/// Registers accessibility data for the active cell editor.
pub(super) fn register_editing_cell_accessibility(
    ctx: &egui::Context,
    response: &egui::Response,
    label: &str,
    address: CellAddress,
    selected: bool,
) {
    ctx.accesskit_node_builder(response.id, |node| {
        node.set_label(label);
        node.set_row_index(address.row);
        node.set_column_index(address.column);
        node.set_selected(selected);
        node.add_action(egui::accesskit::Action::ScrollIntoView);
    });
}
