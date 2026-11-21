use super::*;

#[test]
// Restore a menu check after muda toggles it while the app value stays unchanged.
fn check_item_sync_uses_the_real_native_state() {
    let item = CheckMenuItem::new("Live", true, false, None);
    item.set_checked(true);

    sync_checked_item(&item, false);

    assert!(!item.is_checked());
}

#[test]
// Updates shared rows from background windows without moving the active-window mark.
fn shared_menu_lists_update_without_stealing_the_active_window() {
    let first = vec![
        (1, "north.md".to_owned(), true),
        (2, "south.md".to_owned(), false),
    ];
    let background = vec![
        (1, "north-renamed.md".to_owned(), false),
        (2, "south.md".to_owned(), true),
        (3, "west.md".to_owned(), false),
    ];
    assert_eq!(
        shared_window_rows(&background, &first, false),
        vec![
            (1, "north-renamed.md".to_owned(), true),
            (2, "south.md".to_owned(), false),
            (3, "west.md".to_owned(), false),
        ]
    );
    assert_eq!(shared_window_rows(&background, &first, true), background);
}

#[test]
// Keeps a command for one document until its own window polls the shared queue.
fn native_events_wait_for_their_document_window() {
    let event = |document_id| NativeMenuEvent {
        id: "edit.undo".to_owned(),
        recent_path: None,
        target: NativeActionTarget {
            document_id,
            ..NativeActionTarget::default()
        },
    };
    let mut queue = VecDeque::from([event(Some(11)), event(Some(22))]);

    let first = take_events_for_document(&mut queue, Some(11));
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].target.document_id, Some(11));
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].target.document_id, Some(22));
    let second = take_events_for_document(&mut queue, Some(22));
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].target.document_id, Some(22));
    assert!(queue.is_empty());
}
