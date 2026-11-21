use super::{MenuAction, MenuCheck, MenuEntry, MenuShortcut, action};
use MenuEntry::{Action as A, RecentFiles as Recent, Separator as Sep};
use MenuShortcut::{Command as Cmd, CommandShift as Shift, None as NoKey};

const FILE_NEW: MenuAction = action("file.new", "menu-file-new", Cmd('N'), MenuCheck::None);
const FILE_OPEN: MenuAction = action("file.open", "menu-file-open", Cmd('O'), MenuCheck::None);
const FILE_SAVE: MenuAction = action("file.save", "menu-file-save", Cmd('S'), MenuCheck::None);
const FILE_SAVE_AS: MenuAction = action(
    "file.save_as",
    "menu-file-save-as",
    Shift('S'),
    MenuCheck::None,
);
const FILE_CLOSE: MenuAction = action("file.close", "menu-file-close", Cmd('W'), MenuCheck::None);
const APP_SETTINGS: MenuAction = action(
    "app.settings",
    "action-settings",
    MenuShortcut::Settings,
    MenuCheck::None,
);
const APP_QUIT: MenuAction = action("app.quit", "menu-file-quit", NoKey, MenuCheck::None);

pub(super) const FILE: &[MenuEntry] = &[
    A(&FILE_NEW),
    A(&FILE_OPEN),
    Recent,
    Sep,
    A(&FILE_SAVE),
    A(&FILE_SAVE_AS),
    A(&FILE_CLOSE),
    Sep,
    A(&APP_SETTINGS),
    A(&APP_QUIT),
];

/// The settings item that macOS moves into its application menu.
#[cfg(target_os = "macos")]
pub(super) const APP_SETTINGS_ACTION: &MenuAction = &APP_SETTINGS;
