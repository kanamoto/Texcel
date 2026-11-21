use super::{MenuAction, MenuCheck, MenuEntry, MenuShortcut, MenuSubmenu, action, submenu};
use MenuEntry::{Action as A, Separator as Sep, Submenu as S};
use MenuShortcut::{Command as Cmd, CommandShift as Shift, None as NoKey};

const FORMAT_BOLD: MenuAction = action("format.bold", "format-bold", Cmd('B'), MenuCheck::None);
const FORMAT_ITALIC: MenuAction =
    action("format.italic", "format-italic", Cmd('I'), MenuCheck::None);
const FORMAT_STRIKE: MenuAction = action(
    "format.strike",
    "format-strike",
    Shift('X'),
    MenuCheck::None,
);
const FORMAT_CODE: MenuAction = action("format.code", "format-code", Cmd('E'), MenuCheck::None);
const FORMAT_LINK_EDIT: MenuAction = action(
    "format.link_edit",
    "format-link-edit",
    NoKey,
    MenuCheck::None,
);
const FORMAT_LINK_OPEN: MenuAction = action(
    "format.link_open",
    "format-link-open",
    NoKey,
    MenuCheck::None,
);
const FORMAT_ALIGN_LEFT: MenuAction = action(
    "format.align_left",
    "format-align-markdown-left",
    NoKey,
    MenuCheck::None,
);
const FORMAT_ALIGN_CENTER: MenuAction = action(
    "format.align_center",
    "format-align-markdown-center",
    NoKey,
    MenuCheck::None,
);
const FORMAT_ALIGN_RIGHT: MenuAction = action(
    "format.align_right",
    "format-align-markdown-right",
    NoKey,
    MenuCheck::None,
);

const LINKS: MenuSubmenu = submenu(
    "menu.format.links",
    "menu-format-links",
    &[A(&FORMAT_LINK_EDIT), A(&FORMAT_LINK_OPEN)],
);
const ALIGNMENT: MenuSubmenu = submenu(
    "menu.format.alignment",
    "format-alignment",
    &[
        A(&FORMAT_ALIGN_LEFT),
        A(&FORMAT_ALIGN_CENTER),
        A(&FORMAT_ALIGN_RIGHT),
    ],
);

pub(super) const FORMAT: &[MenuEntry] = &[
    A(&FORMAT_BOLD),
    A(&FORMAT_ITALIC),
    A(&FORMAT_STRIKE),
    A(&FORMAT_CODE),
    Sep,
    S(&LINKS),
    S(&ALIGNMENT),
];
