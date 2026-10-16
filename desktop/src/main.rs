//! Main entry point for the cross-platform egui desktop application.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
pub(crate) use reader_core::github_sync;
#[cfg(feature = "performance-checks")]
mod performance;
mod self_update;
mod system_fonts;
mod ui;

use app::{ReaderApp, TxtImportState};
use eframe::egui;
use std::path::PathBuf;
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
        if let Some(font_data) = system_fonts::load_path(path) {
            fonts.font_data.insert("cjk_font".to_owned(), font_data);
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
    for path in &bold_font_paths {
        if let Some(font_data) = system_fonts::load_path(path) {
            fonts.font_data.insert("cjk_bold".to_owned(), font_data);
            break;
        }
    }
    // Register Serif font family
    let serif_font_paths = [
        "C:\\Windows\\Fonts\\simsun.ttc",
        "C:\\Windows\\Fonts\\times.ttf",
        "/usr/share/fonts/opentype/noto/NotoSerifCJK-Regular.ttc",
        "/System/Library/Fonts/Songti.ttc",
    ];
    for path in &serif_font_paths {
        if let Some(font_data) = system_fonts::load_path(path) {
            fonts.font_data.insert("serif_font".to_owned(), font_data);
            break;
        }
    }
    // Emoji / symbol fallback font
    let emoji_paths = [
        "C:\\Windows\\Fonts\\seguisym.ttf",
        "C:\\Windows\\Fonts\\seguiemj.ttf",
        "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
        "/System/Library/Fonts/Apple Color Emoji.ttc",
    ];
    for path in &emoji_paths {
        if let Some(data) = system_fonts::load_path(path) {
            fonts.font_data.insert("emoji_font".to_owned(), data);
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

    #[cfg(target_os = "macos")]
    system_fonts::register_defaults(&mut fonts);
    register_reader_font_families(&mut fonts);
    ctx.set_fonts(fonts);
}

fn register_reader_font_families(fonts: &mut egui::FontDefinitions) {
    let fallback = fonts.families[&egui::FontFamily::Proportional].clone();
    for (family, preferred) in [("Bold", "cjk_bold"), ("Serif", "serif_font")] {
        // System CJK fonts are optional, including on headless Linux runners.
        // Keep every family usable with egui's bundled fonts alone.
        let mut chain: Vec<_> = [preferred, "cjk_font"]
            .into_iter()
            .filter(|name| fonts.font_data.contains_key(*name))
            .map(str::to_owned)
            .collect();
        for name in &fallback {
            if !chain.contains(name) {
                chain.push(name.clone());
            }
        }
        fonts
            .families
            .insert(egui::FontFamily::Name(family.into()), chain);
    }
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

fn startup_open_path_from_args<I>(args: I) -> Option<PathBuf>
where
    I: IntoIterator<Item = std::ffi::OsString>,
{
    args.into_iter().map(PathBuf::from).find(|path| {
        path.is_file()
            && path
                .extension()
                .and_then(|e| e.to_str())
                .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "epub" | "txt"))
                .unwrap_or(false)
    })
}

fn startup_open_path_from_process_args() -> Option<PathBuf> {
    startup_open_path_from_args(std::env::args_os().skip(1))
}

fn main() -> eframe::Result {
    // Isolated UI diagnostics must never ask for access to real keychain entries.
    if cfg!(feature = "performance-checks") && std::env::args().any(|arg| arg == "--measure-reader")
        || cfg!(debug_assertions) && std::env::args().any(|arg| arg == "--ui-preview")
    {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
    }
    let debug_enabled = debug_enabled_from_args(std::env::args().skip(1));
    reader_core::sharing::set_debug_logging_enabled(debug_enabled);
    if debug_enabled {
        eprintln!("[APP-DBG] Debug console logging enabled via --debug");
    }

    let startup_open_path = startup_open_path_from_process_args();

    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([1200.0, 800.0])
        .with_min_inner_size([600.0, 400.0]);
    #[cfg(debug_assertions)]
    let viewport = if std::env::args().any(|arg| arg == "--ui-preview")
        && std::env::args().any(|arg| arg == "--ui-preview-small")
    {
        viewport.with_inner_size([600.0, 400.0])
    } else {
        viewport
    };
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
        Box::new(move |cc| {
            setup_fonts(&cc.egui_ctx);
            let mut app = ReaderApp::default();
            #[cfg(debug_assertions)]
            if std::env::args().any(|arg| arg == "--ui-preview") {
                if let Ok(scene) = std::env::var("READER_RECEIVE_PREVIEW") {
                    app.book_share = ui::book_share::ShareUI::receive_fixture(&scene);
                    app.i18n
                        .set_language(reader_core::i18n::Language::from_code("zh_cn"));
                    app.dark_mode = std::env::var("READER_RECEIVE_DARK").as_deref() == Ok("1");
                    app.appearance_linkage.set_app(app.dark_mode);
                }
            }

            #[cfg(debug_assertions)]
            if std::env::args().any(|a| a == "--ui-preview")
                && std::env::args().any(|a| {
                    matches!(
                        a.as_str(),
                        "--github-login-preview" | "--github-sync-preview"
                    )
                })
            {
                app.open_sync(0);
            }
            #[cfg(debug_assertions)]
            if std::env::args().any(|a| a == "--appearance-linkage-preview") {
                app.show_appearance_linkage = true;
                app.i18n
                    .set_language(reader_core::i18n::Language::from_code("zh_cn"));
                if std::env::args().any(|a| a == "--appearance-linkage-independent") {
                    app.appearance_linkage.set_app(true);
                    app.appearance_linkage.reading_linked = false;
                    app.appearance_linkage.reading_mode =
                        reader_core::appearance::DisplayChoice::Light;
                    app.appearance_selected_node = 2;
                }
            }
            #[cfg(feature = "performance-checks")]
            if std::env::args().any(|arg| arg == "--measure-reader") {
                let output = std::env::var("READER_PERFORMANCE_REPORT")
                    .expect("READER_PERFORMANCE_REPORT is required for the isolated benchmark");
                app.reader_introduction_seen = true;
                app.scroll_mode = false;
                app.reader_page_animation = "Realistic".into();
                app.reader_performance = Some(performance::ReaderPerformance::new(output.into()));
            }
            if let Some(path) = &startup_open_path {
                if path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|ext| ext.eq_ignore_ascii_case("txt"))
                    .unwrap_or(false)
                {
                    app.txt_import = Some(TxtImportState::new(path.clone()));
                } else {
                    let path_str = path.to_string_lossy().to_string();
                    app.open_book_from_path(&path_str, None);
                }
            }
            Ok(Box::new(app))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        debug_enabled_from_args, register_reader_font_families, startup_open_path_from_args,
    };
    use eframe::egui;
    use std::ffi::OsString;

    #[test]
    fn reader_font_families_render_with_missing_or_partial_system_fonts() {
        for available in 0..8 {
            let mut fonts = egui::FontDefinitions::default();
            let fallback = fonts.families[&egui::FontFamily::Proportional].clone();
            let data = fonts.font_data[&fallback[0]].clone();
            for (bit, name) in ["cjk_font", "cjk_bold", "serif_font"]
                .into_iter()
                .enumerate()
            {
                if available & (1 << bit) != 0 {
                    fonts.font_data.insert(name.into(), data.clone());
                }
            }
            register_reader_font_families(&mut fonts);
            for (family, preferred) in [("Bold", "cjk_bold"), ("Serif", "serif_font")] {
                let chain = &fonts.families[&egui::FontFamily::Name(family.into())];
                assert!(chain.iter().all(|name| fonts.font_data.contains_key(name)));
                assert!(fallback.iter().all(|name| chain.contains(name)));
                if fonts.font_data.contains_key(preferred) {
                    assert_eq!(chain.first().map(String::as_str), Some(preferred));
                }
            }
            let ctx = egui::Context::default();
            ctx.set_fonts(fonts);
            let _ = ctx.run(Default::default(), |ctx| {
                for family in ["Bold", "Serif"] {
                    ctx.fonts(|fonts| {
                        let galley = fonts.layout_no_wrap(
                            "Latin 中文，？！".into(),
                            egui::FontId::new(18., egui::FontFamily::Name(family.into())),
                            egui::Color32::WHITE,
                        );
                        assert_eq!(galley.text(), "Latin 中文，？！");
                        assert!(galley.rect.width() > 0.);
                    });
                }
            });
        }
    }

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

    #[test]
    fn startup_open_path_should_ignore_nonexistent_paths() {
        assert!(startup_open_path_from_args([OsString::from(r"C:\temp\missing.epub")]).is_none());
    }

    #[test]
    fn startup_open_path_should_pick_existing_epub_or_txt() {
        let tmp_dir = std::env::temp_dir().join("rust_epub_reader_startup_test");
        let _ = std::fs::create_dir_all(&tmp_dir);
        let epub = tmp_dir.join("sample.epub");
        std::fs::write(&epub, b"dummy").unwrap();

        let found = startup_open_path_from_args([epub.clone().into_os_string()]);
        assert_eq!(found.as_deref(), Some(epub.as_path()));

        let _ = std::fs::remove_file(&epub);
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
