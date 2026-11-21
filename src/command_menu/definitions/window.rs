use super::{MenuAction, MenuCheck, MenuEntry, MenuShortcut, action};
use MenuEntry::{Action as A, Separator as Sep, WindowList};
use MenuShortcut::{Command as Cmd, None as NoKey};

const MINIMIZE: MenuAction = action(
    "window.minimize",
    "window-minimize",
    Cmd('M'),
    MenuCheck::None,
);
const ZOOM: MenuAction = action("window.zoom", "window-zoom", NoKey, MenuCheck::None);

pub(super) const WINDOW: &[MenuEntry] = &[A(&MINIMIZE), A(&ZOOM), Sep, WindowList];
