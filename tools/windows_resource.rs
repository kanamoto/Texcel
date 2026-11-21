/// Names the copied icon beside the generated resource script.
pub const ICON_RESOURCE_NAME: &str = "texcel-icon.ico";

/// Returns an ASCII resource line that is safe in legacy compiler code pages.
pub fn icon_resource_line() -> String {
    format!("1 ICON \"{ICON_RESOURCE_NAME}\"\r\n")
}
