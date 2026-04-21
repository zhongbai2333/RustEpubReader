//! Floating toolbar containing quick actions and menu options.
use crate::app::{AppView, ReaderApp};
use eframe::egui;

/// A button that looks like a normal `Button` when inactive,
/// but shows the theme's selection background when `active` is true.
fn toggle_btn(ui: &mut egui::Ui, active: bool, icon: &str, size: f32) -> egui::Response {
    let mut btn = egui::Button::new(egui::RichText::new(icon).size(size));
    if active {
        btn = btn.fill(ui.visuals().selection.bg_fill);
    }
    ui.add(btn)
}

impl ReaderApp {
    pub fn render_toolbar(&mut self, ui: &mut egui::Ui) {
        let btn_size = 16.0_f32;
        // Use the full available width to decide whether to collapse
        let toolbar_width = ui.available_width();
        // Threshold below which secondary buttons collapse into an overflow menu
        let compact = toolbar_width < 700.0;

        ui.horizontal(|ui| {
            // ── Left: navigation ──
            let in_library = self.view == AppView::Library;
            if toggle_btn(ui, in_library, "📚", btn_size)
                .on_hover_text(self.i18n.t("toolbar.library"))
                .clicked()
            {
                self.flush_reading_stats();
                self.view = AppView::Library;
                // Reset review panel state when returning to library
                self.show_review_panel = false;
                self.review_panel_chapter = None;
                self.review_panel_anchor = None;
                self.review_panel_just_opened = false;
            }
            ui.add_space(2.0);
            if ui
                .button(egui::RichText::new("📂").size(btn_size))
                .on_hover_text(self.i18n.t("toolbar.open"))
                .clicked()
            {
                self.open_file_dialog();
            }
            ui.add_space(2.0);
            if !compact {
                if toggle_btn(ui, self.show_sharing_panel, "📡", btn_size)
                    .on_hover_text(self.i18n.t("share.toolbar"))
                    .clicked()
                {
                    self.show_sharing_panel = !self.show_sharing_panel;
                }
            }

            if self.book.is_some() {
                ui.separator();

                // ── Reader controls (always visible) ──
                let toc_tip = if self.show_toc {
                    self.i18n.t("toolbar.hide_toc").to_string()
                } else {
                    self.i18n.t("toolbar.show_toc").to_string()
                };
                if toggle_btn(ui, self.show_toc, "☰", btn_size)
                    .on_hover_text(&toc_tip)
                    .clicked()
                {
                    self.show_toc = !self.show_toc;
                    if self.show_toc {
                        self.scroll_toc_to_current = true;
                    }
                }

                let mode_tip = if self.scroll_mode {
                    self.i18n.t("toolbar.scroll_mode").to_string()
                } else {
                    self.i18n.t("toolbar.page_mode").to_string()
                };
                let mode_label = if self.scroll_mode { "📜" } else { "📄" };
                if ui
                    .button(egui::RichText::new(mode_label).size(btn_size))
                    .on_hover_text(&mode_tip)
                    .clicked()
                {
                    self.scroll_mode = !self.scroll_mode;
                    self.pages_dirty = true;
                }

                if !compact {
                    let settings_tip = self.i18n.t("toolbar.reading_settings").to_string();
                    if toggle_btn(ui, self.show_settings, "⚙", btn_size)
                        .on_hover_text(&settings_tip)
                        .clicked()
                    {
                        self.show_settings = !self.show_settings;
                    }

                    ui.separator();

                    let search_tip = self.i18n.t("toolbar.search").to_string();
                    if toggle_btn(ui, self.show_search, "🔍", btn_size)
                        .on_hover_text(&search_tip)
                        .clicked()
                    {
                        self.show_search = !self.show_search;
                        if self.show_search {
                            self.show_annotations = false;
                        }
                    }

                    let annot_tip = self.i18n.t("toolbar.annotations").to_string();
                    if toggle_btn(ui, self.show_annotations, "📝", btn_size)
                        .on_hover_text(&annot_tip)
                        .clicked()
                    {
                        self.show_annotations = !self.show_annotations;
                        if self.show_annotations {
                            self.show_search = false;
                        }
                    }

                    let stats_tip = self.i18n.t("toolbar.stats").to_string();
                    if toggle_btn(ui, self.show_stats, "📊", btn_size)
                        .on_hover_text(&stats_tip)
                        .clicked()
                    {
                        self.show_stats = !self.show_stats;
                    }

                    // Bookmark current chapter
                    let ch_bookmarked = self.book_config.as_ref().is_some_and(|cfg| {
                        cfg.bookmarks
                            .iter()
                            .any(|b| b.chapter == self.current_chapter)
                    });
                    let bm_icon = if ch_bookmarked { "★" } else { "☆" };
                    let bm_tip = if ch_bookmarked {
                        "取消书签"
                    } else {
                        "添加章节书签"
                    };
                    if toggle_btn(ui, ch_bookmarked, bm_icon, btn_size)
                        .on_hover_text(bm_tip)
                        .clicked()
                    {
                        if let Some(cfg) = &mut self.book_config {
                            if ch_bookmarked {
                                cfg.bookmarks.retain(|b| b.chapter != self.current_chapter);
                            } else {
                                cfg.bookmarks.push(reader_core::library::Bookmark {
                                    chapter: self.current_chapter,
                                    block: 0,
                                    created_at: reader_core::now_secs(),
                                });
                            }
                            cfg.save(&self.data_dir);
                        }
                    }

                    // TTS button
                    let tts_tip = self.i18n.t("toolbar.tts").to_string();
                    if toggle_btn(ui, self.show_tts_panel, "🔊", btn_size)
                        .on_hover_text(&tts_tip)
                        .clicked()
                    {
                        self.show_tts_panel = !self.show_tts_panel;
                    }
                } else {
                    // ── Compact: overflow menu ⋮ ──
                    ui.separator();
                    ui.menu_button(egui::RichText::new("⋮").size(btn_size), |ui| {
                        // Sharing
                        let share_label = self.i18n.t("share.toolbar").to_string();
                        if ui
                            .selectable_label(self.show_sharing_panel, &share_label)
                            .clicked()
                        {
                            self.show_sharing_panel = !self.show_sharing_panel;
                            ui.close_menu();
                        }

                        ui.separator();

                        // Settings
                        let settings_label = self.i18n.t("toolbar.reading_settings").to_string();
                        if ui
                            .selectable_label(self.show_settings, &settings_label)
                            .clicked()
                        {
                            self.show_settings = !self.show_settings;
                            ui.close_menu();
                        }

                        // Search
                        let search_label = self.i18n.t("toolbar.search").to_string();
                        if ui
                            .selectable_label(self.show_search, &search_label)
                            .clicked()
                        {
                            self.show_search = !self.show_search;
                            if self.show_search {
                                self.show_annotations = false;
                            }
                            ui.close_menu();
                        }

                        // Annotations
                        let annot_label = self.i18n.t("toolbar.annotations").to_string();
                        if ui
                            .selectable_label(self.show_annotations, &annot_label)
                            .clicked()
                        {
                            self.show_annotations = !self.show_annotations;
                            if self.show_annotations {
                                self.show_search = false;
                            }
                            ui.close_menu();
                        }

                        // Stats
                        let stats_label = self.i18n.t("toolbar.stats").to_string();
                        if ui.selectable_label(self.show_stats, &stats_label).clicked() {
                            self.show_stats = !self.show_stats;
                            ui.close_menu();
                        }

                        // Bookmark
                        let ch_bookmarked = self.book_config.as_ref().is_some_and(|cfg| {
                            cfg.bookmarks
                                .iter()
                                .any(|b| b.chapter == self.current_chapter)
                        });
                        let bm_icon = if ch_bookmarked { "★" } else { "☆" };
                        let bm_text = if ch_bookmarked {
                            "取消书签"
                        } else {
                            "添加章节书签"
                        };
                        let bm_label = format!("{} {}", bm_icon, bm_text);
                        if ui.selectable_label(ch_bookmarked, &bm_label).clicked() {
                            if let Some(cfg) = &mut self.book_config {
                                if ch_bookmarked {
                                    cfg.bookmarks.retain(|b| b.chapter != self.current_chapter);
                                } else {
                                    cfg.bookmarks.push(reader_core::library::Bookmark {
                                        chapter: self.current_chapter,
                                        block: 0,
                                        created_at: reader_core::now_secs(),
                                    });
                                }
                                cfg.save(&self.data_dir);
                            }
                            ui.close_menu();
                        }

                        // TTS
                        let tts_label = self.i18n.t("toolbar.tts").to_string();
                        if ui
                            .selectable_label(self.show_tts_panel, &tts_label)
                            .clicked()
                        {
                            self.show_tts_panel = !self.show_tts_panel;
                            ui.close_menu();
                        }
                    });
                }

                if !compact {
                    ui.separator();

                    // ── Book title ──
                    if let Some(book) = &self.book {
                        let title_color = if self.dark_mode {
                            egui::Color32::from_gray(200)
                        } else {
                            egui::Color32::from_gray(60)
                        };
                        ui.label(
                            egui::RichText::new(&book.title)
                                .strong()
                                .size(14.0)
                                .color(title_color),
                        );
                    }
                }

                // ── Right: chapter nav + font + theme ──
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let theme_icon = if self.dark_mode { "☀" } else { "☾" };
                    let theme_tip = if self.dark_mode {
                        self.i18n.t("toolbar.light_mode").to_string()
                    } else {
                        self.i18n.t("toolbar.dark_mode").to_string()
                    };
                    if ui
                        .button(egui::RichText::new(theme_icon).size(btn_size))
                        .on_hover_text(&theme_tip)
                        .clicked()
                    {
                        self.dark_mode = !self.dark_mode;
                    }

                    ui.separator();

                    if ui.button("A+").clicked() {
                        self.font_size = (self.font_size + 2.0).min(40.0);
                        self.pages_dirty = true;
                    }
                    ui.label(format!("{:.0}", self.font_size));
                    if ui.button("A-").clicked() {
                        self.font_size = (self.font_size - 2.0).max(12.0);
                        self.pages_dirty = true;
                    }

                    ui.separator();

                    if ui.button("→").clicked() {
                        self.next_chapter();
                    }
                    let hint_color = if self.dark_mode {
                        egui::Color32::from_gray(140)
                    } else {
                        egui::Color32::from_gray(100)
                    };
                    ui.label(
                        egui::RichText::new(format!(
                            " {} / {} ",
                            self.current_chapter + 1,
                            self.total_chapters()
                        ))
                        .color(hint_color),
                    );
                    if ui.button("←").clicked() {
                        self.prev_chapter();
                    }
                });
            }
        });
    }
}
