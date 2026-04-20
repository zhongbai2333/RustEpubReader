//! Main entry point for the cross-platform egui desktop application.
// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod self_update;
mod ui;

use app::ReaderApp;
use eframe::egui;
use std::sync::Arc;

fn load_app_icon() -> Option<Arc<egui::viewport::IconData>> {
    fn decode_icon(bytes: &[u8]) -> Option<Arc<egui::viewport::IconData>> {
        let image = image::load_from_memory(bytes).ok()?;
        let rgba = image.into_rgba8();
        let (width, height) = rgba.dimensions();
        Some(Arc::new(egui::viewport::IconData {
            rgba: rgba.into_raw(),
            width,
            height,
        }))
    }

    decode_icon(include_bytes!("../../icon/ReaderIcon2.png"))
        .or_else(|| decode_icon(include_bytes!("../../icon/ReaderIcon.png")))
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let font_paths = [
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\simhei.ttf",
        "C:\\Windows\\Fonts\\simsun.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/System/Library/Fonts/PingFang.ttc",
    ];
    for path in &font_paths {
        if let Ok(font_data) = std::fs::read(path) {
            fonts.font_data.insert(
                "cjk_font".to_owned(),
                egui::FontData::from_owned(font_data).into(),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "cjk_font".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .insert(0, "cjk_font".to_owned());
            break;
        }
    }
    let bold_font_paths = [
        "C:\\Windows\\Fonts\\msyhbd.ttc",
        "C:\\Windows\\Fonts\\simhei.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc",
        "/System/Library/Fonts/PingFang.ttc",
    ];
    let mut bold_loaded = false;
    for path in &bold_font_paths {
        if let Ok(font_data) = std::fs::read(path) {
            fonts.font_data.insert(
                "cjk_bold".to_owned(),
                egui::FontData::from_owned(font_data).into(),
            );
            bold_loaded = true;
            break;
        }
    }
    let mut bold_family = Vec::new();
    if bold_loaded {
        bold_family.push("cjk_bold".to_owned());
    }
    bold_family.push("cjk_font".to_owned());
    fonts
        .families
        .insert(egui::FontFamily::Name("Bold".into()), bold_family);

    // Register Serif font family
    let serif_font_paths = [
        "C:\\Windows\\Fonts\\simsun.ttc",
        "C:\\Windows\\Fonts\\times.ttf",
        "/usr/share/fonts/opentype/noto/NotoSerifCJK-Regular.ttc",
        "/System/Library/Fonts/Songti.ttc",
    ];
    let mut serif_family = Vec::new();
    for path in &serif_font_paths {
        if let Ok(font_data) = std::fs::read(path) {
            fonts.font_data.insert(
                "serif_font".to_owned(),
                egui::FontData::from_owned(font_data).into(),
            );
            serif_family.push("serif_font".to_owned());
            break;
        }
    }
    serif_family.push("cjk_font".to_owned());
    fonts
        .families
        .insert(egui::FontFamily::Name("Serif".into()), serif_family);

    // Emoji / symbol fallback font
    let emoji_paths = [
        "C:\\Windows\\Fonts\\seguisym.ttf",
        "C:\\Windows\\Fonts\\seguiemj.ttf",
        "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
        "/System/Library/Fonts/Apple Color Emoji.ttc",
    ];
    for path in &emoji_paths {
        if let Ok(data) = std::fs::read(path) {
            fonts.font_data.insert(
                "emoji_font".to_owned(),
                egui::FontData::from_owned(data).into(),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push("emoji_font".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("emoji_font".to_owned());
            break;
        }
    }

    ctx.set_fonts(fonts);
}

fn debug_enabled_from_args<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .map(|arg| arg.as_ref().to_owned())
        .any(|arg| matches!(arg.as_str(), "--debug" | "-d"))
}

fn main() -> eframe::Result {
    let debug_enabled = debug_enabled_from_args(std::env::args().skip(1));
    reader_core::sharing::set_debug_logging_enabled(debug_enabled);
    if debug_enabled {
        eprintln!("[APP-DBG] Debug console logging enabled via --debug");
    }

    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([1200.0, 800.0])
        .with_min_inner_size([600.0, 400.0]);
    let viewport = if let Some(icon) = load_app_icon() {
        viewport.with_icon(icon)
    } else {
        viewport
    };

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Rust EPUB Reader",
        options,
        Box::new(|cc| {
            setup_fonts(&cc.egui_ctx);
            Ok(Box::new(ReaderApp::default()))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::debug_enabled_from_args;

    #[test]
    fn debug_flag_should_be_detected() {
        assert!(debug_enabled_from_args(["--debug"]));
        assert!(debug_enabled_from_args(["-d"]));
        assert!(debug_enabled_from_args(["--foo", "--debug"]));
    }

    #[test]
    fn debug_flag_should_be_disabled_by_default() {
        assert!(!debug_enabled_from_args(std::iter::empty::<&str>()));
        assert!(!debug_enabled_from_args(["--help"]));
    }
}
