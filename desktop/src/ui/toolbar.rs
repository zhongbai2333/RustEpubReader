use super::design::{icon_button, Icon, Palette};
use crate::app::{AppView, ReaderApp};
use eframe::egui;
impl ReaderApp {
    pub fn render_toolbar(&mut self, ui: &mut egui::Ui) {
        let p = Palette::new(self.dark_mode);
        let compact = ui.available_width() < 650.;
        ui.horizontal(|ui| {
            if icon_button(ui, Icon::Back, self.i18n.t("toolbar.library"), false).clicked() {
                self.flush_reading_stats();
                self.view = AppView::Library;
                self.show_review_panel = false;
                self.review_panel_chapter = None;
                self.review_panel_anchor = None;
                self.review_panel_just_opened = false;
                self.review_panel_scroll_offset = None;
                self.show_settings = false;
                self.show_search = false;
                self.show_annotations = false;
            }
            if let Some(book) = &self.book {
                ui.allocate_ui_with_layout(
                    egui::vec2(
                        (ui.available_width() - if compact { 230. } else { 320. }).max(70.),
                        40.,
                    ),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new(&book.title).size(16.).strong())
                                .truncate(),
                        );
                        ui.small(
                            egui::RichText::new(self.i18n.tf2(
                                "reader.chapter_indicator",
                                &(self.current_chapter + 1).to_string(),
                                &self.total_chapters().to_string(),
                            ))
                            .color(p.muted),
                        );
                    },
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                super::design::menu_button(ui, egui::Button::new("⋯"), |ui| {
                    super::design::menu_style(ui);
                    if super::design::menu_item(ui, self.i18n.t("sync.multiplatform"), false, false)
                        .clicked()
                    {
                        self.open_sync(0);
                        ui.memory_mut(|m| m.close_popup());
                    }
                    if super::design::menu_item(ui, self.i18n.t("ui.import"), false, false)
                        .clicked()
                    {
                        self.open_file_dialog();
                        ui.memory_mut(|m| m.close_popup());
                    }
                    if super::design::menu_item(
                        ui,
                        self.i18n.t("toolbar.stats"),
                        self.show_stats,
                        false,
                    )
                    .clicked()
                    {
                        self.show_stats = !self.show_stats;
                        ui.memory_mut(|m| m.close_popup());
                    }
                    if compact {
                        if super::design::menu_item(
                            ui,
                            self.i18n.t("toolbar.annotations"),
                            self.show_annotations,
                            false,
                        )
                        .clicked()
                        {
                            self.show_annotations = !self.show_annotations;
                            self.show_search = false;
                            self.show_settings = false;
                            ui.memory_mut(|m| m.close_popup());
                        }
                        if super::design::menu_item(
                            ui,
                            self.i18n.t("toolbar.tts"),
                            self.show_tts_panel,
                            false,
                        )
                        .clicked()
                        {
                            self.show_tts_panel = !self.show_tts_panel;
                            ui.memory_mut(|m| m.close_popup());
                        }
                    }
                    let bookmarked = self.book_config.as_ref().is_some_and(|cfg| {
                        cfg.bookmarks
                            .iter()
                            .any(|b| b.chapter == self.current_chapter)
                    });
                    if super::design::menu_item(
                        ui,
                        self.i18n.t("annotations.add_bookmark"),
                        bookmarked,
                        false,
                    )
                    .clicked()
                    {
                        if let Some(cfg) = &mut self.book_config {
                            if bookmarked {
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
                        ui.memory_mut(|m| m.close_popup());
                    }
                    ui.separator();
                    if super::design::menu_item(
                        ui,
                        self.i18n.t(if self.scroll_mode {
                            "toolbar.scroll_mode"
                        } else {
                            "toolbar.page_mode"
                        }),
                        false,
                        false,
                    )
                    .clicked()
                    {
                        self.scroll_mode = !self.scroll_mode;
                        self.pages_dirty = true;
                        ui.memory_mut(|m| m.close_popup());
                    }
                    ui.horizontal(|ui| {
                        if ui.button("A−").clicked() {
                            self.font_size = (self.font_size - 2.).max(12.);
                            self.pages_dirty = true;
                        }
                        ui.label(format!("{:.0}", self.font_size));
                        if ui.button("A+").clicked() {
                            self.font_size = (self.font_size + 2.).min(40.);
                            self.pages_dirty = true;
                        }
                    });
                    if super::design::menu_item(
                        ui,
                        self.i18n.t(if self.dark_mode {
                            "toolbar.light_mode"
                        } else {
                            "toolbar.dark_mode"
                        }),
                        false,
                        false,
                    )
                    .clicked()
                    {
                        self.dark_mode = !self.dark_mode;
                        ui.memory_mut(|m| m.close_popup());
                    }
                    ui.horizontal(|ui| {
                        if ui.button("←").clicked() {
                            self.prev_chapter();
                        }
                        if ui.button("→").clicked() {
                            self.next_chapter();
                        }
                    });
                });
                if icon_button(
                    ui,
                    Icon::Settings,
                    self.i18n.t("toolbar.reading_settings"),
                    self.show_settings,
                )
                .clicked()
                {
                    self.show_settings = !self.show_settings;
                    self.show_search = false;
                    self.show_annotations = false;
                }
                if !compact {
                    if icon_button(
                        ui,
                        Icon::Audio,
                        self.i18n.t("toolbar.tts"),
                        self.show_tts_panel,
                    )
                    .clicked()
                    {
                        self.show_tts_panel = !self.show_tts_panel;
                    }
                    if icon_button(
                        ui,
                        Icon::Notes,
                        self.i18n.t("toolbar.annotations"),
                        self.show_annotations,
                    )
                    .clicked()
                    {
                        self.show_annotations = !self.show_annotations;
                        self.show_settings = false;
                        self.show_search = false;
                    }
                }
                if icon_button(
                    ui,
                    Icon::Search,
                    self.i18n.t("toolbar.search"),
                    self.show_search,
                )
                .clicked()
                {
                    self.show_search = !self.show_search;
                    self.show_settings = false;
                    self.show_annotations = false;
                }
                if icon_button(
                    ui,
                    Icon::Toc,
                    self.i18n.t("toolbar.show_toc"),
                    self.show_toc,
                )
                .clicked()
                {
                    self.show_toc = !self.show_toc;
                    if self.show_toc {
                        self.scroll_toc_to_current = true;
                    }
                }
            });
        });
    }
}
