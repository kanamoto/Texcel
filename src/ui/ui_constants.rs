//! Shares chrome dimensions and grid limits across the screen modules.

pub(super) use super::chrome::{
    CHROME_CONTROL_HEIGHT, CHROME_EDGE_INSET, CHROME_STATUS_HEIGHT, CHROME_TAB_HEIGHT,
    FORMULA_BAR_CORNER_RADIUS,
};

pub(super) const ROW_HEADER_WIDTH: f32 = 52.0;
pub(super) const HEADER_HEIGHT: f32 = 28.0;
pub(super) const ROW_HEIGHT: f32 = 26.0;
pub(super) const MAX_WRAPPED_ROW_HEIGHT: f32 = 4_096.0;
pub(super) const MAX_GRID_TEXT_BYTES: usize = 4 * 1024;
pub(super) const MAX_MARKDOWN_CELL_PARSE_BYTES: usize = 16 * 1024;
pub(super) const MAX_CACHED_MARKDOWN_CELLS: usize = 4_096;
pub(super) const INLINE_EDIT_TEXT_BYTES: usize = 512;
pub(super) const MAX_CELL_EDIT_WINDOW_BYTES: usize = 16 * 1024;
pub(super) const ACCESSKIT_TABLE_TOKEN_MASK: u32 = (1 << 24) - 1;
pub(super) const ACCESSKIT_NODE_MARKER: u64 = 0xA;
