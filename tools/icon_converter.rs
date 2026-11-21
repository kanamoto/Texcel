use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder, RgbaImage, imageops::FilterType};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(test)]
#[path = "icon_converter_test.rs"]
mod tests;

/// Builds Windows and macOS icons from one square PNG source.
pub fn convert_icons(source: &Path, output_dir: &Path) -> Result<(), Box<dyn Error>> {
    ensure_source_is_not_output(source, &icon_output_paths(output_dir))?;
    convert_icons_unchecked(source, output_dir)
}

/// Builds every icon and preview after checking all destinations first.
pub fn convert_all(
    source: &Path,
    output_dir: &Path,
    preview_dir: &Path,
) -> Result<(), Box<dyn Error>> {
    let mut outputs = icon_output_paths(output_dir);
    outputs.extend(preview_output_paths(preview_dir));
    ensure_source_is_not_output(source, &outputs)?;
    convert_icons(source, output_dir)?;
    write_previews(source, preview_dir)?;
    Ok(())
}

/// Writes only platform icon files after their paths pass the source check.
fn convert_icons_unchecked(source: &Path, output_dir: &Path) -> Result<(), Box<dyn Error>> {
    let source = image::open(source)?.into_rgba8();
    if source.width() != 1024 || source.height() != 1024 {
        return Err("the source icon must be 1024 by 1024 pixels".into());
    }
    fs::create_dir_all(output_dir)?;

    let windows_pngs = windows_images(&source)?;
    write_ico(&output_dir.join("app_icon.ico"), &windows_pngs)?;

    let mac_icon = padded_mac_image(&source)?;
    write_png(&output_dir.join("app_icon_macos.png"), &mac_icon)?;
    write_icns(&output_dir.join("app_icon.icns"), &mac_icon)?;

    let linux_icon = resize(&source, 256);
    write_png(&output_dir.join("app_icon_linux_256.png"), &linux_icon)?;
    Ok(())
}

/// Makes Windows images at common shell and taskbar sizes.
fn windows_images(source: &RgbaImage) -> Result<Vec<(u32, Vec<u8>)>, image::ImageError> {
    [16, 20, 24, 32, 40, 48, 64, 128, 256]
        .into_iter()
        .map(|size| png_bytes(&resize(source, size)).map(|data| (size, data)))
        .collect()
}

/// Adds clear transparent space around the macOS artwork.
fn padded_mac_image(source: &RgbaImage) -> Result<RgbaImage, Box<dyn Error>> {
    let Some((left, top, right, bottom)) = visible_bounds(source) else {
        return Err("the source icon has no visible pixels".into());
    };
    let crop_width = right - left;
    let crop_height = bottom - top;
    let scale = (source.width() as f32 * 0.8 / crop_width as f32)
        .min(source.height() as f32 * 0.8 / crop_height as f32);
    let width = (crop_width as f32 * scale).round().max(1.0) as u32;
    let height = (crop_height as f32 * scale).round().max(1.0) as u32;
    let crop = image::imageops::crop_imm(source, left, top, crop_width, crop_height).to_image();
    let content = image::imageops::resize(&crop, width, height, FilterType::Lanczos3);
    let mut canvas = RgbaImage::new(source.width(), source.height());
    let offset_x = (source.width() - width) / 2;
    let offset_y = (source.height() - height) / 2;
    image::imageops::overlay(
        &mut canvas,
        &content,
        i64::from(offset_x),
        i64::from(offset_y),
    );
    Ok(canvas)
}

/// Finds the smallest rectangle that contains visible source pixels.
fn visible_bounds(image: &RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let mut left = image.width();
    let mut top = image.height();
    let mut right = 0;
    let mut bottom = 0;
    for (index, pixel) in image.pixels().enumerate() {
        if pixel.0[3] == 0 {
            continue;
        }
        let x = index as u32 % image.width();
        let y = index as u32 / image.width();
        left = left.min(x);
        top = top.min(y);
        right = right.max(x + 1);
        bottom = bottom.max(y + 1);
    }
    (left < right && top < bottom).then_some((left, top, right, bottom))
}

/// Resizes an image with a filter suited to small icon edges.
fn resize(source: &RgbaImage, size: u32) -> RgbaImage {
    image::imageops::resize(source, size, size, FilterType::Lanczos3)
}

/// Encodes one RGBA image as PNG bytes for both icon containers.
fn png_bytes(image: &RgbaImage) -> Result<Vec<u8>, image::ImageError> {
    let mut bytes = Vec::new();
    PngEncoder::new(&mut bytes).write_image(
        image.as_raw(),
        image.width(),
        image.height(),
        ExtendedColorType::Rgba8,
    )?;
    Ok(bytes)
}

/// Writes one standalone PNG derived from the master icon.
fn write_png(path: &Path, image: &RgbaImage) -> Result<(), Box<dyn Error>> {
    fs::write(path, png_bytes(image)?)?;
    Ok(())
}

/// Writes PNG-compressed icon images into a Windows ICO container.
fn write_ico(path: &Path, images: &[(u32, Vec<u8>)]) -> Result<(), Box<dyn Error>> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&[0, 0, 1, 0]);
    bytes.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + images.len() * 16;
    for (size, data) in images {
        let dimension = if *size == 256 { 0 } else { *size as u8 };
        bytes.extend_from_slice(&[dimension, dimension, 0, 0]);
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&32u16.to_le_bytes());
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += data.len();
    }
    for (_, data) in images {
        bytes.extend_from_slice(data);
    }
    fs::write(path, bytes)?;
    Ok(())
}

/// Writes PNG image blocks into a multi-size ICNS container.
fn write_icns(path: &Path, source: &RgbaImage) -> Result<(), Box<dyn Error>> {
    let mut chunks = Vec::new();
    for (kind, size) in [
        (*b"icp4", 16),
        (*b"icp5", 32),
        (*b"icp6", 64),
        (*b"ic07", 128),
        (*b"ic08", 256),
        (*b"ic09", 512),
        (*b"ic10", 1024),
    ] {
        let data = png_bytes(&resize(source, size))?;
        chunks.extend_from_slice(&kind);
        chunks.extend_from_slice(&((data.len() + 8) as u32).to_be_bytes());
        chunks.extend_from_slice(&data);
    }

    let mut bytes = Vec::with_capacity(chunks.len() + 8);
    bytes.extend_from_slice(b"icns");
    bytes.extend_from_slice(&((chunks.len() + 8) as u32).to_be_bytes());
    bytes.extend_from_slice(&chunks);
    fs::write(path, bytes)?;
    Ok(())
}

/// Creates size previews for a quick visual comparison of both margins.
pub fn write_previews(source: &Path, output_dir: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    ensure_source_is_not_output(source, &preview_output_paths(output_dir))?;
    write_previews_unchecked(source, output_dir)
}

/// Writes preview images after the caller checks all output paths.
fn write_previews_unchecked(
    source: &Path,
    output_dir: &Path,
) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let original = image::open(source)?.into_rgba8();
    let mac = padded_mac_image(&original)?;
    fs::create_dir_all(output_dir)?;
    let mut outputs = Vec::new();
    for (name, image) in [("windows", &original), ("macos", &mac)] {
        for size in [16, 32, 64, 128, 256] {
            let path = output_dir.join(format!("{name}-{size}.png"));
            write_png(&path, &resize(image, size))?;
            outputs.push(path);
        }
    }
    Ok(outputs)
}

/// Lists every generated platform icon path before conversion starts.
fn icon_output_paths(output_dir: &Path) -> Vec<PathBuf> {
    [
        "app_icon.ico",
        "app_icon_macos.png",
        "app_icon.icns",
        "app_icon_linux_256.png",
    ]
    .into_iter()
    .map(|name| output_dir.join(name))
    .collect()
}

/// Lists every generated comparison image before conversion starts.
fn preview_output_paths(output_dir: &Path) -> Vec<PathBuf> {
    ["windows", "macos"]
        .into_iter()
        .flat_map(|name| {
            [16, 32, 64, 128, 256].map(move |size| output_dir.join(format!("{name}-{size}.png")))
        })
        .collect()
}

/// Refuses any output path that resolves to the master source file.
fn ensure_source_is_not_output(source: &Path, outputs: &[PathBuf]) -> Result<(), Box<dyn Error>> {
    let source = resolved_path(source)?;
    for output in outputs {
        if resolved_path(output)? == source || same_existing_file(&source, output)? {
            return Err(format!(
                "icon output would overwrite its source: {}",
                output.display()
            )
            .into());
        }
    }
    Ok(())
}

/// Checks whether an existing output is another name for the source file.
fn same_existing_file(source: &Path, output: &Path) -> Result<bool, Box<dyn Error>> {
    match same_file::is_same_file(source, output) {
        Ok(same) => Ok(same),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// Resolves existing aliases and normalizes missing destination components.
fn resolved_path(path: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut missing = Vec::new();
    let mut existing = absolute.as_path();
    while !existing.exists() {
        let Some(name) = existing.file_name() else {
            return Err(format!("cannot resolve output path: {}", path.display()).into());
        };
        missing.push(name.to_os_string());
        existing = existing
            .parent()
            .ok_or_else(|| format!("cannot resolve output path: {}", path.display()))?;
    }
    let mut resolved = fs::canonicalize(existing)?;
    for name in missing.iter().rev() {
        resolved.push(name);
    }
    Ok(resolved)
}
