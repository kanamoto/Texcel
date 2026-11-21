use super::*;
use image::GenericImageView;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

struct TestDirectory(PathBuf);

impl TestDirectory {
    /// Creates an isolated directory for generated icon files.
    fn new() -> Self {
        let id = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("texcel-icon-{}-{id}", std::process::id()));
        fs::create_dir_all(&path).expect("test directory is created");
        Self(path)
    }
}

impl Drop for TestDirectory {
    /// Removes generated test files after each check.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
// Verify ICO entries contain readable PNG images at every required size.
fn ico_contains_readable_multi_size_pngs() {
    let directory = TestDirectory::new();
    let source_path = directory.0.join("source.png");
    let source = RgbaImage::from_pixel(1024, 1024, image::Rgba([32, 48, 64, 255]));
    write_png(&source_path, &source).expect("source PNG is written");
    let original = fs::read(&source_path).expect("source bytes are read");
    convert_icons(&source_path, &directory.0).expect("icons are generated");

    assert_eq!(fs::read(&source_path).unwrap(), original);
    let ico = fs::read(directory.0.join("app_icon.ico")).expect("ICO is written");
    assert_eq!(&ico[0..4], &[0, 0, 1, 0]);
    let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
    assert_eq!(count, 9);
    let mut sizes = Vec::new();
    for index in 0..count {
        let entry = 6 + index * 16;
        let width = if ico[entry] == 0 {
            256
        } else {
            u32::from(ico[entry])
        };
        let length = u32::from_le_bytes(ico[entry + 8..entry + 12].try_into().unwrap()) as usize;
        let offset = u32::from_le_bytes(ico[entry + 12..entry + 16].try_into().unwrap()) as usize;
        let image = image::load_from_memory(&ico[offset..offset + length]).unwrap();
        assert_eq!(image.dimensions(), (width, width));
        sizes.push(width);
    }
    assert_eq!(sizes, [16, 20, 24, 32, 40, 48, 64, 128, 256]);
}

#[test]
// Verify ICNS blocks read back and macOS margins stay separate from Windows.
fn icns_has_all_sizes_and_a_padded_macos_image() {
    let directory = TestDirectory::new();
    let source_path = directory.0.join("source.png");
    let mut source = RgbaImage::new(1024, 1024);
    for y in 48..976 {
        for x in 48..976 {
            source.put_pixel(x, y, image::Rgba([96, 80, 112, 255]));
        }
    }
    write_png(&source_path, &source).expect("source PNG is written");
    convert_icons(&source_path, &directory.0).expect("icons are generated");

    let icns = fs::read(directory.0.join("app_icon.icns")).expect("ICNS is written");
    assert_eq!(&icns[0..4], b"icns");
    assert_eq!(
        u32::from_be_bytes(icns[4..8].try_into().unwrap()) as usize,
        icns.len()
    );
    let mut offset = 8;
    let mut sizes = Vec::new();
    while offset < icns.len() {
        let kind = &icns[offset..offset + 4];
        let length = u32::from_be_bytes(icns[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let image = image::load_from_memory(&icns[offset + 8..offset + length]).unwrap();
        assert_eq!(image.width(), image.height());
        sizes.push(image.width());
        assert_eq!(kind.len(), 4);
        offset += length;
    }
    assert_eq!(sizes, [16, 32, 64, 128, 256, 512, 1024]);

    let mac = image::open(directory.0.join("app_icon_macos.png"))
        .unwrap()
        .into_rgba8();
    assert_eq!(mac.dimensions(), (1024, 1024));
    assert_eq!(mac.get_pixel(0, 0).0[3], 0);
    assert_eq!(mac.get_pixel(512, 512).0[3], 255);
    let (left, top, right, bottom) = visible_bounds(&mac).unwrap();
    assert!((right - left).abs_diff(819) <= 1);
    assert!((bottom - top).abs_diff(819) <= 1);
    let linux = image::open(directory.0.join("app_icon_linux_256.png")).unwrap();
    assert_eq!(linux.dimensions(), (256, 256));
}

#[test]
// Verify source aliases are rejected before any generated file is changed.
fn conversion_rejects_source_and_preview_aliases_before_writing() {
    let directory = TestDirectory::new();
    let source_path = directory.0.join("app_icon_macos.png");
    let source = RgbaImage::from_pixel(1024, 1024, image::Rgba([10, 20, 30, 255]));
    write_png(&source_path, &source).expect("source PNG is written");
    let original = fs::read(&source_path).expect("source bytes are read");

    assert!(convert_all(&source_path, &directory.0, &directory.0).is_err());
    assert_eq!(fs::read(&source_path).unwrap(), original);
    assert!(!directory.0.join("app_icon.ico").exists());

    let preview_dir = directory.0.join("previews");
    fs::create_dir_all(&preview_dir).expect("preview folder is created");
    let preview_source = preview_dir.join("windows-16.png");
    write_png(&preview_source, &source).expect("preview-named source is written");
    let preview_original = fs::read(&preview_source).expect("preview source bytes are read");
    let output_dir = directory.0.join("icons");
    assert!(convert_all(&preview_source, &output_dir, &preview_dir).is_err());
    assert_eq!(fs::read(&preview_source).unwrap(), preview_original);
    assert!(!output_dir.join("app_icon.ico").exists());
}

#[cfg(unix)]
#[test]
// Verify a symlink to the source is treated as the same output path.
fn conversion_rejects_a_symlinked_source_output() {
    use std::os::unix::fs::symlink;

    let directory = TestDirectory::new();
    let source_path = directory.0.join("source.png");
    let output_alias = directory.0.join("app_icon_macos.png");
    let source = RgbaImage::from_pixel(1024, 1024, image::Rgba([40, 50, 60, 255]));
    write_png(&source_path, &source).expect("source PNG is written");
    symlink(&source_path, &output_alias).expect("output alias is linked to source");
    let original = fs::read(&source_path).expect("source bytes are read");

    assert!(convert_all(&source_path, &directory.0, &directory.0).is_err());
    assert_eq!(fs::read(&source_path).unwrap(), original);
}

#[cfg(unix)]
#[test]
// Verify a hard link to the source is not truncated by conversion.
fn conversion_rejects_a_hard_linked_source_output() {
    use std::fs::hard_link;

    let directory = TestDirectory::new();
    let source_path = directory.0.join("source.png");
    let output_alias = directory.0.join("app_icon_macos.png");
    let source = RgbaImage::from_pixel(1024, 1024, image::Rgba([70, 80, 90, 255]));
    write_png(&source_path, &source).expect("source PNG is written");
    hard_link(&source_path, &output_alias).expect("output is linked to source");
    let original = fs::read(&source_path).expect("source bytes are read");

    assert!(convert_all(&source_path, &directory.0, &directory.0).is_err());
    assert_eq!(fs::read(&source_path).unwrap(), original);
}
