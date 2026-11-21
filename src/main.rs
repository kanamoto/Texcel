// Run as a GUI app on Windows so no console window opens beside Texcel.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use eframe::egui;
use std::path::PathBuf;

/// Loads a small native icon from the generated platform artwork.
fn window_icon() -> Option<egui::IconData> {
    #[cfg(target_os = "macos")]
    let bytes = include_bytes!("../assets/icons/app_icon_macos.png");
    #[cfg(not(target_os = "macos"))]
    let bytes = include_bytes!("../assets/icons/app_icon.png");
    let image = image::load_from_memory(bytes).ok()?.into_rgba8();
    let image = image::imageops::resize(&image, 256, 256, image::imageops::FilterType::Lanczos3);
    Some(egui::IconData {
        rgba: image.into_raw(),
        width: 256,
        height: 256,
    })
}

// Start the native Texcel window with its default size and renderer.
fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1120.0, 760.0])
        .with_min_inner_size([760.0, 500.0])
        .with_drag_and_drop(true)
        .with_app_id("net.homeunix.hio.app.Texcel");
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }
    let mut options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport,
        ..Default::default()
    };
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::EventLoopBuilderExtMacOS;
        texcel::prepare_macos_file_open();
        options.event_loop_builder = Some(Box::new(|event_loop| {
            event_loop.with_default_menu(false);
        }));
    }
    #[cfg(target_os = "windows")]
    {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        options.event_loop_builder = Some(Box::new(|event_loop| {
            event_loop.with_msg_hook(texcel::translate_windows_menu_message);
        }));
    }
    let startup_paths = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    eframe::run_native(
        "Texcel",
        options,
        Box::new(move |creation| {
            let mut app = texcel::ui::TexcelApp::new(creation);
            app.queue_open_paths(startup_paths);
            let host = texcel::ui::TexcelAppHost::new(app, creation.egui_ctx.viewport_id());
            Ok(Box::new(host))
        }),
    )
}
