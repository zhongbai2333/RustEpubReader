use super::design::{Icon, Palette};
use crate::app::{AppView, ReaderApp};
use eframe::egui::{self, Color32};

fn translucent(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

fn circle_button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    active: bool,
    p: Palette,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(44., 44.), egui::Sense::click());
    let color = if response.hovered() {
        p.raised
    } else {
        p.surface
    };
    ui.painter().circle_filled(
        rect.center() + egui::vec2(0., 2.),
        22.,
        Color32::from_black_alpha(18),
    );
    ui.painter()
        .circle_filled(rect.center(), 21., translucent(color, 228));
    ui.painter().circle_stroke(
        rect.center(),
        21.,
        egui::Stroke::new(0.5_f32, translucent(p.border, 170)),
    );
    if active && matches!(icon, Icon::Bookmark) {
        let center = rect.center();
        ui.painter().add(egui::Shape::convex_polygon(
            vec![
                center + egui::vec2(-6., -9.),
                center + egui::vec2(6., -9.),
                center + egui::vec2(6., 9.),
                center + egui::vec2(0., 5.),
                center + egui::vec2(-6., 9.),
            ],
            p.accent,
            egui::Stroke::NONE,
        ));
    } else {
        super::design::paint_icon(
            ui.painter(),
            rect,
            icon,
            if active { p.accent } else { p.text },
        );
    }
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response.on_hover_text(label)
}

fn menu_action(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    tile: bool,
    p: Palette,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(
            ui.available_width(),
            if tile {
                if ui.ctx().screen_rect().height() < 460. {
                    68.
                } else {
                    78.
                }
            } else {
                if ui.ctx().screen_rect().height() < 460. {
                    40.
                } else {
                    46.
                }
            },
        ),
        egui::Sense::click(),
    );
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, 12., translucent(p.raised, 200));
    }
    let icon_rect = if tile {
        egui::Rect::from_center_size(
            rect.center_top() + egui::vec2(0., 24.),
            egui::vec2(28., 28.),
        )
    } else {
        egui::Rect::from_center_size(
            rect.left_center() + egui::vec2(24., 0.),
            egui::vec2(24., 24.),
        )
    };
    super::design::paint_icon(ui.painter(), icon_rect, icon, p.text);
    let position = if tile {
        rect.center_bottom() - egui::vec2(0., 18.)
    } else {
        rect.left_center() + egui::vec2(50., 0.)
    };
    ui.painter().text(
        position,
        if tile {
            egui::Align2::CENTER_CENTER
        } else {
            egui::Align2::LEFT_CENTER
        },
        label,
        egui::FontId::proportional(15.),
        p.text,
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response
}

impl ReaderApp {
    fn reader_menu_action(&mut self, ctx: &egui::Context, action: &str) {
        match action {
            "toc" => {
                self.show_toc = !self.show_toc;
                if self.show_toc {
                    self.scroll_toc_to_current = true;
                }
            }
            "annotations" => {
                self.show_annotations = true;
                self.show_settings = false;
                self.show_search = false;
            }
            "search" => {
                self.show_search = true;
                self.show_settings = false;
                self.show_annotations = false;
            }
            "settings" => {
                self.show_settings = true;
                self.show_search = false;
                self.show_annotations = false;
            }
            "stats" => self.open_reading_stats(true),
            "correction" => self.show_csc_settings = true,
            _ => {}
        }
        if ctx.screen_rect().width() < 980. {
            if action == "toc" && self.show_toc {
                self.show_settings = false;
                self.show_search = false;
                self.show_annotations = false;
            } else if matches!(action, "annotations" | "search" | "settings") {
                self.show_toc = false;
            }
        }
    }
    pub fn reader_is_night(&self) -> bool {
        self.appearance_linkage
            .reading_dark(self.system_appearance_dark, None)
    }
    pub fn toggle_reader_day_night(&mut self) {
        self.appearance_linkage
            .toggle_reading(self.system_appearance_dark, None);
    }
    pub fn render_toolbar(&mut self, ui: &mut egui::Ui) {
        let p = Palette::new(self.dark_mode);
        ui.horizontal(|ui| {
            if circle_button(
                ui,
                Icon::Close,
                self.i18n.t("nav.back_to_library"),
                false,
                p,
            )
            .clicked()
            {
                self.close_book_presentation();
                self.show_review_panel = false;
                self.review_panel_chapter = None;
                self.review_panel_anchor = None;
                self.review_panel_just_opened = false;
                self.review_panel_scroll_offset = None;
                self.show_settings = false;
                self.show_csc_settings = false;
                self.show_search = false;
                self.show_annotations = false;
                ui.memory_mut(|m| m.close_popup());
            }
            let title_width = (ui.available_width() - 52.).max(0.);
            ui.allocate_ui_with_layout(
                egui::vec2(title_width, 44.),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    egui::Frame::NONE
                        .fill(translucent(p.surface, 228))
                        .corner_radius(16)
                        .inner_margin(egui::Margin::symmetric(12, 5))
                        .show(ui, |ui| {
                            ui.set_width((title_width - 24.).clamp(0., 320.));
                            if let Some(book) = &self.book {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&book.title).size(13.).color(p.text),
                                    )
                                    .truncate(),
                                );
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(self.i18n.tf2(
                                            "reader.chapter_indicator",
                                            &(self.current_chapter + 1).to_string(),
                                            &self.total_chapters().to_string(),
                                        ))
                                        .size(11.)
                                        .color(p.muted),
                                    )
                                    .truncate(),
                                );
                            }
                        });
                },
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let bookmarked = self.book_config.as_ref().is_some_and(|cfg| {
                    cfg.bookmarks
                        .iter()
                        .any(|b| b.chapter == self.current_chapter && b.block == self.current_block)
                });
                let label = self
                    .i18n
                    .t(if bookmarked {
                        "reader.remove_bookmark"
                    } else {
                        "annotations.add_bookmark"
                    })
                    .to_string();
                if circle_button(ui, Icon::Bookmark, &label, bookmarked, p).clicked() {
                    if let Some(cfg) = &mut self.book_config {
                        if bookmarked {
                            cfg.bookmarks.retain(|b| {
                                b.chapter != self.current_chapter || b.block != self.current_block
                            });
                        } else {
                            cfg.bookmarks.push(reader_core::library::Bookmark {
                                chapter: self.current_chapter,
                                block: self.current_block,
                                created_at: reader_core::now_secs(),
                            });
                        }
                        cfg.save(&self.data_dir);
                    }
                }
            });
        });
    }

    pub(crate) fn render_reader_correction_panel(&mut self, ctx: &egui::Context) {
        if self.view != AppView::Reader || !self.show_csc_settings {
            return;
        }
        let mut open = self.show_csc_settings;
        egui::Window::new(self.i18n.t("reader.correction"))
            .open(&mut open)
            .default_width(400.)
            .collapsible(false)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height((ctx.screen_rect().height() - 140.).max(120.))
                    .show(ui, |ui| {
                        self.render_csc_settings(ui);
                        ui.add_space(12.);
                        if ui.button(self.i18n.t("annotations.corrections")).clicked() {
                            self.show_annotations = true;
                            self.show_search = false;
                            self.show_settings = false;
                            self.show_csc_settings = false;
                        }
                    });
            });
        self.show_csc_settings = open && self.show_csc_settings;
    }

    pub(crate) fn render_reader_menu_button(&mut self, ui: &mut egui::Ui) {
        let p = Palette::new(self.dark_mode);
        let response = circle_button(ui, Icon::More, self.i18n.t("reader.menu"), false, p);
        let id = egui::Id::new("reader_quick_menu");
        let more_id = id.with("more");
        if response.clicked() {
            ui.memory_mut(|m| m.toggle_popup(id));
            ui.ctx().data_mut(|d| d.insert_temp(more_id, false));
        }
        let open = ui.memory(|m| m.is_popup_open(id));
        let opacity = ui
            .ctx()
            .animate_bool_with_time(id.with("visibility"), open, 0.16);
        // Closing popup memory releases its input immediately; retain only its
        // paint until the fade finishes. A reopen reverses the same animation.
        if opacity <= 0.001 {
            return;
        }
        let safe = ui.ctx().screen_rect().shrink(12.);
        if !safe.is_positive() {
            return;
        }
        let width = 300_f32.min(safe.width()).max(100.);
        let pos = response.rect.right_top() - egui::vec2(0., 8.);
        let popup = egui::Area::new(id)
            .kind(egui::UiKind::Popup)
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .pivot(egui::Align2::RIGHT_BOTTOM)
            .constrain_to(safe)
            .fade_in(false)
            .interactable(open)
            .show(ui.ctx(), |ui| {
                ui.set_opacity(opacity);
                if !open {
                    ui.disable();
                }
                let mut frame = egui::Frame::popup(ui.style())
                    .fill(translucent(p.surface, 242))
                    .corner_radius(24)
                    .inner_margin(12);
                frame.stroke = egui::Stroke::new(0.5_f32, translucent(p.border, 180));
                frame.show(ui, |ui| {
                    ui.set_width((width - 24.).max(76.));
                    ui.spacing_mut().item_spacing = egui::vec2(4., 2.);
                    ui.scope(|ui| {
                        let more = ui
                            .ctx()
                            .data(|d| d.get_temp::<bool>(more_id).unwrap_or(false));
                        if more {
                            if menu_action(ui, Icon::Back, self.i18n.t("settings.back"), false, p)
                                .clicked()
                            {
                                ui.ctx().data_mut(|d| d.insert_temp(more_id, false));
                            }
                            ui.separator();
                            for (icon, key, action) in [
                                (Icon::Search, "reader.search_book", "search"),
                                (Icon::Notes, "toolbar.stats", "stats"),
                                (Icon::Correction, "reader.correction", "correction"),
                            ] {
                                if menu_action(ui, icon, self.i18n.t(key), false, p).clicked() {
                                    self.reader_menu_action(ui.ctx(), action);
                                    ui.memory_mut(|m| m.close_popup());
                                }
                            }
                        } else {
                            ui.columns(2, |columns| {
                                if menu_action(
                                    &mut columns[0],
                                    Icon::Toc,
                                    self.i18n.t("nav.toc"),
                                    true,
                                    p,
                                )
                                .clicked()
                                {
                                    self.reader_menu_action(columns[0].ctx(), "toc");
                                    columns[0].memory_mut(|m| m.close_popup());
                                }
                                if menu_action(
                                    &mut columns[1],
                                    Icon::Audio,
                                    self.i18n.t("toolbar.tts"),
                                    true,
                                    p,
                                )
                                .clicked()
                                {
                                    self.show_tts_panel = !self.show_tts_panel;
                                    columns[1].memory_mut(|m| m.close_popup());
                                }
                            });
                            ui.add_space(6.);
                            ui.separator();
                            ui.add_space(6.);
                            if menu_action(
                                ui,
                                Icon::Bookmark,
                                self.i18n.t("reader.bookmarks_notes"),
                                false,
                                p,
                            )
                            .clicked()
                            {
                                self.reader_menu_action(ui.ctx(), "annotations");
                                ui.memory_mut(|m| m.close_popup());
                            }
                            let night = self.reader_is_night();
                            if menu_action(
                                ui,
                                if night { Icon::Sun } else { Icon::Moon },
                                self.i18n.t(if night {
                                    "toolbar.light_mode"
                                } else {
                                    "toolbar.dark_mode"
                                }),
                                false,
                                p,
                            )
                            .clicked()
                            {
                                self.toggle_reader_day_night();
                                ui.memory_mut(|m| m.close_popup());
                            }
                            if menu_action(
                                ui,
                                Icon::Settings,
                                self.i18n.t("settings.appearance_title"),
                                false,
                                p,
                            )
                            .clicked()
                            {
                                self.reader_menu_action(ui.ctx(), "settings");
                                ui.memory_mut(|m| m.close_popup());
                            }
                            if menu_action(
                                ui,
                                Icon::More,
                                self.i18n.t("reader.more_actions"),
                                false,
                                p,
                            )
                            .clicked()
                            {
                                ui.ctx().data_mut(|d| d.insert_temp(more_id, true));
                            }
                        }
                    });
                });
            });
        if open
            && (ui.input(|i| i.key_pressed(egui::Key::Escape))
                || (response.clicked_elsewhere() && popup.response.clicked_elsewhere()))
        {
            ui.memory_mut(|m| m.close_popup());
        }
    }
}
