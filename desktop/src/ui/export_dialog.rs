//! Export dialog interface for saving annotations and data.
use crate::app::ReaderApp;
use eframe::egui;
use reader_core::export::ExportMode;

impl ReaderApp {
    /// Render the export dialog window.
    pub fn render_export_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_export_dialog {
            return;
        }

        let mut open = true;
        egui::Window::new(self.i18n.t("export.title"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(360.0)
            .show(ctx, |ui| {
                ui.label(self.i18n.t("export.description"));
                ui.add_space(8.0);

                let modes = [
                    (
                        ExportMode::Original,
                        self.i18n.t("export.mode_original").to_string(),
                    ),
                    (
                        ExportMode::WithCorrections,
                        self.i18n.t("export.mode_corrections").to_string(),
                    ),
                    (
                        ExportMode::WithAnnotations,
                        self.i18n.t("export.mode_annotations").to_string(),
                    ),
                    (
                        ExportMode::Full,
                        self.i18n.t("export.mode_full").to_string(),
                    ),
                ];

                for (mode, label) in &modes {
                    if ui.button(label.as_str()).clicked() {
                        self.do_export(*mode);
                        self.show_export_dialog = false;
                        self.export_library_path = None;
                    }
                }
            });

        if !open {
            self.show_export_dialog = false;
            self.export_library_path = None;
        }
    }

    fn do_export(&self, mode: ExportMode) {
        // Determine which book path to use: library export or currently open book
        let book_path = self
            .export_library_path
            .as_deref()
            .or(self.book_path.as_deref());
        let Some(book_path) = book_path else { return };

        // Build a minimal config if none is loaded
        let fallback_cfg;
        let config = match &self.book_config {
            Some(c) => c,
            None => {
                fallback_cfg = reader_core::library::BookConfig {
                    id: String::new(),
                    title: String::new(),
                    epub_path: book_path.to_string(),
                    last_chapter: 0,
                    last_block: 0,
                    last_char_offset: 0,
                    last_chapter_title: None,
                    last_opened: 0,
                    created_at: 0,
                    updated_at: 0,
                    settings: Default::default(),
                    file_hash: None,
                    metadata: None,
                    bookmarks: Vec::new(),
                    highlights: Vec::new(),
                    notes: Vec::new(),
                    corrections: Vec::new(),
                    reading_stats: None,
                    last_contribute_prompt_count: 0,
                };
                &fallback_cfg
            }
        };

        if let Some(path) = rfd::FileDialog::new()
            .set_title("Export EPUB")
            .add_filter("EPUB", &["epub"])
            .save_file()
        {
            let path_str = path.to_string_lossy().to_string();
            let path_str = if !path_str.ends_with(".epub") {
                format!("{path_str}.epub")
            } else {
                path_str
            };
            let _ = reader_core::export::export_book(book_path, &path_str, config, mode);
        }
    }
}
