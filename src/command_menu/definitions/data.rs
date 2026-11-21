use super::{MenuAction, MenuCheck, MenuEntry, MenuShortcut, MenuSubmenu, action, submenu};
use MenuEntry::{Action as A, Separator as Sep, Submenu as S};
use MenuShortcut::None as NoKey;

const DATA_SORT: MenuAction = action("data.sort", "menu-data-sort", NoKey, MenuCheck::None);
const DATA_REPLACE: MenuAction = action(
    "data.replace_results",
    "top-replace-formula-results",
    NoKey,
    MenuCheck::None,
);
const DATA_TRIM: MenuAction = action("data.trim", "menu-data-trim", NoKey, MenuCheck::None);
const DATA_SPLIT: MenuAction = action("data.split", "menu-data-split", NoKey, MenuCheck::None);
const DATA_JOIN: MenuAction = action("data.join", "menu-data-join", NoKey, MenuCheck::None);
const DATA_TRANSPOSE: MenuAction = action(
    "data.transpose",
    "menu-data-transpose",
    NoKey,
    MenuCheck::None,
);
const DATA_DETECT_DUPLICATES: MenuAction = action(
    "data.detect_duplicates",
    "menu-data-detect-duplicates",
    NoKey,
    MenuCheck::None,
);
const DATA_REMOVE_DUPLICATES: MenuAction = action(
    "data.remove_duplicates",
    "menu-data-remove-duplicates",
    NoKey,
    MenuCheck::None,
);

const ORGANIZE: MenuSubmenu = submenu(
    "menu.data.organize",
    "menu-data-organize",
    &[
        A(&DATA_TRIM),
        A(&DATA_SPLIT),
        A(&DATA_JOIN),
        A(&DATA_TRANSPOSE),
    ],
);
const DUPLICATES: MenuSubmenu = submenu(
    "menu.data.duplicates",
    "menu-data-duplicates",
    &[A(&DATA_DETECT_DUPLICATES), A(&DATA_REMOVE_DUPLICATES)],
);

pub(super) const DATA: &[MenuEntry] = &[
    A(&DATA_SORT),
    A(&DATA_REPLACE),
    Sep,
    S(&ORGANIZE),
    S(&DUPLICATES),
];
