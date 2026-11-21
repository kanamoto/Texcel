use crate::windows_resource::{ICON_RESOURCE_NAME, icon_resource_line};

#[test]
// Verify the resource script contains only an ASCII relative filename.
fn icon_resource_line_uses_an_ascii_relative_icon_name() {
    let line = icon_resource_line();
    assert_eq!(line, format!("1 ICON \"{ICON_RESOURCE_NAME}\"\r\n"));
    assert!(line.is_ascii());
}
