use super::design::{self, Icon, Palette};
use crate::app::{AppView, ReaderApp};
use eframe::egui;
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
        if !self.show_app_navigation {
            egui::TopBottomPanel::top("app_navigation_reopen")
                .frame(egui::Frame::default().fill(p.surface).inner_margin(8.))
                .show(ctx, |ui| {
                    if ui.button(self.i18n.t("ui.show_navigation")).clicked() {
                        self.show_app_navigation = true;
                    }
                });
            return;
        }
        let font = egui::TextStyle::Button.resolve(&ctx.style());
        let text_color = ctx.style().visuals.text_color();
        let label_width = ctx.fonts(|fonts| {
            [
                "library.title",
                "sync.multiplatform",
                "ui.return_reader",
                "stats.title",
                "settings.title",
            ]
            .iter()
            .map(|key| {
                fonts
                    .layout_no_wrap(self.i18n.t(key).to_owned(), font.clone(), text_color)
                    .size()
                    .x
            })
            .fold(0_f32, f32::max)
        });
        let navigation_width =
            (label_width + 58. + 2. * f32::from(design::Geometry::CARD_INSET)).clamp(196., 260.);
        egui::SidePanel::left("app_navigation")
            .exact_width(navigation_width)
            .resizable(false)
            .frame(
                egui::Frame::default()
                    .fill(p.surface)
                    .inner_margin(design::Geometry::CARD_INSET),
            )
            .show(ctx, |ui| {
                let panel_rect = ui
                    .max_rect()
                    .expand(f32::from(design::Geometry::CARD_INSET));
                ui.set_max_width(ui.available_width() - 16.);
                ui.add_space(12.);
                design::sidebar_title(ui, "EpubReader");
                ui.add_space(32.);
                for (key, icon, selected) in [
                    ("library.title", Icon::Book, self.view == AppView::Library),
                    ("sync.multiplatform", Icon::Sync, self.view == AppView::Sync),
                ] {
                    let response = design::navigation_item(ui, icon, self.i18n.t(key), selected);
                    if response.clicked() {
                        if key == "library.title" {
                            if self.view != AppView::Library {
                                self.book_presentation.shelf_order.reset();
                            }
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
                                self.appearance_linkage.set_app(!self.dark_mode);
                                ui.memory_mut(|m| m.close_popup());
                            }
                            if design::menu_item(
                                ui,
                                self.i18n.t("appearance.link_title"),
                                false,
                                false,
                            )
                            .clicked()
                            {
                                self.show_appearance_linkage = true;
                                ui.memory_mut(|m| m.close_popup());
                            }
                            ui.separator();
                            let language_label = format!(
                                "{} · {}",
                                self.i18n.t("language.title"),
                                self.i18n.choice_label(self.i18n.preference())
                            );
                            if design::menu_item(ui, &language_label, false, false).clicked() {
                                self.show_language = true;
                                ui.memory_mut(|m| m.close_popup());
                            }
                        },
                    );
                    design::paint_navigation_item(
                        ui,
                        &response.response,
                        Icon::Settings,
                        self.i18n.t("settings.title"),
                        false,
                    );
                    ui.add_space(4.);
                    if design::navigation_item(
                        ui,
                        Icon::Notes,
                        self.i18n.t("stats.title"),
                        self.show_stats,
                    )
                    .clicked()
                    {
                        self.open_reading_stats(false);
                    }
                });
                if design::sidebar_collapse_button(
                    ui,
                    panel_rect,
                    true,
                    self.i18n.t("common.collapse"),
                )
                .clicked()
                {
                    self.show_app_navigation = false;
                }
            });
    }
    pub fn render_sync_page(&mut self, ui: &mut egui::Ui) {
        let bounds = ui.available_rect_before_wrap();
        let margin = design::Geometry::PAGE_INSET as f32;
        let width = (bounds.width() - 2. * margin).clamp(0., 640.);
        let content = egui::Rect::from_min_max(
            egui::pos2(bounds.center().x - width / 2., bounds.top() + margin),
            egui::pos2(bounds.center().x + width / 2., bounds.bottom() - margin),
        );
        ui.allocate_new_ui(
            egui::UiBuilder::new()
                .max_rect(content)
                .layout(egui::Layout::top_down(egui::Align::Min)),
            |ui| {
                ui.set_width(width);
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
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn navigation_footer_fits_small_window_in_both_languages() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let directory = tempfile::tempdir().unwrap();
        let mut app = ReaderApp::from_data_dir(directory.path().to_string_lossy().into_owned());
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../core/tests/fixtures/illustrations.epub");
        app.book = Some(reader_core::epub::EpubBook::open(fixture.to_str().unwrap()).unwrap());
        fn collect(
            shape: &egui::Shape,
            result: &mut std::collections::HashMap<String, (egui::Rect, bool)>,
        ) {
            match shape {
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, result)
                    }
                }
                egui::Shape::Text(text) => {
                    result.insert(
                        text.galley.job.text.clone(),
                        (text.visual_bounding_rect(), text.galley.elided),
                    );
                }
                _ => {}
            }
        }
        for language in [
            reader_core::i18n::Language::ZhCN,
            reader_core::i18n::Language::En,
        ] {
            app.i18n = reader_core::i18n::I18n::with_system(language, "en");
            let ctx = egui::Context::default();
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600., 400.),
                    )),
                    ..Default::default()
                },
                |ctx| app.render_app_navigation(ctx),
            );
            let mut labels = std::collections::HashMap::new();
            for shape in output.shapes {
                collect(&shape.shape, &mut labels)
            }
            let resume = labels[app.i18n.t("ui.return_reader")].0;
            let stats = labels[app.i18n.t("stats.title")].0;
            let settings = labels[app.i18n.t("settings.title")].0;
            assert!(
                stats.min.y > resume.max.y + 16.,
                "Statistics must belong to the footer"
            );
            assert!(settings.min.y > stats.max.y + 4.);
            assert!(settings.max.y < 392.);
            for key in [
                "library.title",
                "sync.multiplatform",
                "ui.return_reader",
                "stats.title",
                "settings.title",
            ] {
                let (rect, elided) = labels[app.i18n.t(key)];
                assert!(!elided, "Navigation label truncated: {key}");
                assert!(rect.min.x >= 0. && rect.max.x < 260.);
            }
        }
    }
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
