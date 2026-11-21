#[path = "icon_converter.rs"]
mod converter;

use std::env;
use std::path::PathBuf;

// Build icon files and small preview images from the repository's master PNG.
fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut args = env::args_os().skip(1);
    let source = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("assets/icons/app_icon.png"));
    let output = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("assets/icons"));
    let previews = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/icon-preview"));
    if args.next().is_some() {
        eprintln!("usage: texcel-icon-convert [source.png] [output-dir] [preview-dir]");
        std::process::exit(2);
    }

    if let Err(error) = converter::convert_all(&source, &output, &previews) {
        eprintln!("icon conversion failed: {error}");
        std::process::exit(1);
    }
    println!("wrote ICO, ICNS, platform PNGs, and previews");
}
