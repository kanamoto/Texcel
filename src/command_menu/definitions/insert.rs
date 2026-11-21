use super::{MenuAction, MenuCheck, MenuEntry, MenuShortcut, action};
use MenuEntry::Action as A;
use MenuShortcut::None as NoKey;

const INSERT_ROWS: MenuAction = action("insert.rows", "insert-row", NoKey, MenuCheck::None);
const INSERT_COLUMNS: MenuAction =
    action("insert.columns", "insert-column", NoKey, MenuCheck::None);

pub(super) const INSERT: &[MenuEntry] = &[A(&INSERT_ROWS), A(&INSERT_COLUMNS)];
