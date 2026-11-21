use super::{MenuAction, MenuCheck, MenuEntry, MenuShortcut, action};
use MenuEntry::Action as A;
use MenuShortcut::None as NoKey;

const HELP_OPEN: MenuAction = action("help.open", "menu-help-open", NoKey, MenuCheck::None);

pub(super) const HELP: &[MenuEntry] = &[A(&HELP_OPEN)];
