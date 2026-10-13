use super::design::{self, Icon, Palette};
use crate::app::{AppView, ReaderApp};
use eframe::egui;
use reader_core::i18n::Language;
impl ReaderApp {
    pub fn open_sync(&mut self, tab: usize) {
        if self.view == AppView::Reader {
            self.flush_reading_stats();
        }
        self.view = AppView::Sync;
        self.sync_tab = tab;
        self.show_settings = false;
        self.show_search = false;
        self.show_annotations = false;
        self.show_review_panel = false;
    }
    pub fn render_app_navigation(&mut self, ctx: &egui::Context) {
        let p = Palette::new(self.dark_mode);
        egui::SidePanel::left("app_navigation")
            .exact_width(196.)
            .resizable(false)
            .frame(
                egui::Frame::default()
                    .fill(p.surface)
                    .inner_margin(design::Geometry::CARD_INSET),
            )
            .show(ctx, |ui| {
                ui.add_space(12.);
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(32., 32.), egui::Sense::hover());
                    design::paint_icon(ui.painter(), r, Icon::Book, p.accent);
                    ui.label(egui::RichText::new("EpubReader").size(19.).strong());
                });
                ui.add_space(32.);
                for (key, icon, selected) in [
                    ("library.title", Icon::Book, self.view == AppView::Library),
                    ("sync.multiplatform", Icon::Sync, self.view == AppView::Sync),
                ] {
                    let response = design::navigation_item(ui, icon, self.i18n.t(key), selected);
                    if response.clicked() {
                        if key == "library.title" {
                            self.view = AppView::Library;
                        } else {
                            self.open_sync(0);
                        }
                    }
                    ui.add_space(4.);
                }
                if self.book.is_some() {
                    ui.add_space(16.);
                    if design::navigation_item(
                        ui,
                        Icon::Back,
                        self.i18n.t("ui.return_reader"),
                        false,
                    )
                    .clicked()
                    {
                        self.view = AppView::Reader;
                    }
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    let response = super::design::menu_button(
                        ui,
                        egui::Button::new("")
                            .min_size(egui::vec2(
                                ui.available_width(),
                                design::Geometry::NAV_HEIGHT,
                            ))
                            .stroke(egui::Stroke::NONE),
                        |ui| {
                            design::menu_style(ui);
                            if design::menu_item(ui, self.i18n.t("about.title"), false, false)
                                .clicked()
                            {
                                self.show_about = true;
                                ui.memory_mut(|m| m.close_popup());
                            }
                            if design::menu_item(
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
                            ui.separator();
                            for lang in Language::all() {
                                if design::menu_item(
                                    ui,
                                    lang.label(),
                                    self.i18n.language() == lang,
                                    false,
                                )
                                .clicked()
                                {
                                    self.i18n.set_language(lang.clone());
                                    ui.memory_mut(|m| m.close_popup());
                                }
                            }
                        },
                    );
                    design::paint_navigation_item(
                        ui,
                        &response.response,
                        Icon::Settings,
                        self.i18n.t("ui.more"),
                        false,
                    );
                });
            });
    }
    pub fn render_sync_page(&mut self, ui: &mut egui::Ui) {
        egui::Frame::default()
            .inner_margin(design::Geometry::PAGE_INSET)
            .show(ui, |ui| {
                ui.set_max_width(640.);
                ui.heading(self.i18n.t("sync.multiplatform"));
                design::block_gap(ui);
                ui.horizontal(|ui| {
                    for (index, key) in [(0, "ui.github_sync"), (1, "ui.local_sync")] {
                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(156., 40.), egui::Sense::click());
                        if response.clicked() {
                            self.sync_tab = index;
                        }
                        let selected = self.sync_tab == index;
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::SelectableLabel,
                                true,
                                self.i18n.t(key),
                            )
                        });
                        let color = if selected {
                            Palette::new(self.dark_mode).accent
                        } else {
                            ui.visuals().weak_text_color()
                        };
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            self.i18n.t(key),
                            egui::TextStyle::Button.resolve(ui.style()),
                            color,
                        );
                        if selected {
                            ui.painter().line_segment(
                                [rect.left_bottom(), rect.right_bottom()],
                                egui::Stroke::new(2_f32, color),
                            );
                        }
                    }
                });
                design::block_gap(ui);
                egui::ScrollArea::vertical()
                    .id_salt(("sync_tab_scroll", self.sync_tab))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        if self.sync_tab == 0 {
                            self.render_github_account_content(ui);
                        } else {
                            self.render_sharing_content(ui);
                        }
                    });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn switching_to_sync_keeps_resume_and_inflight_login() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let dir = tempfile::tempdir().unwrap();
        let mut app = ReaderApp::from_data_dir(dir.path().to_string_lossy().into_owned());
        app.view = AppView::Reader;
        app.current_chapter = 3;
        app.current_block = 7;
        app.show_search = true;
        app.show_annotations = true;
        app.show_settings = true;
        app.github_oauth_polling = true;
        app.open_sync(1);
        assert!(app.view == AppView::Sync);
        assert_eq!((app.current_chapter, app.current_block), (3, 7));
        assert_eq!(app.sync_tab, 1);
        assert!(!app.show_search && !app.show_annotations && !app.show_settings);
        assert!(app.github_oauth_polling);
        assert!(!app.github_sync_enabled);
    }
}
