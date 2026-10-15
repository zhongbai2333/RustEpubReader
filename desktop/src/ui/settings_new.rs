//! Refactored or updated settings interface for the application.
use crate::app::ReaderApp;
use eframe::egui;
use egui::{Color32, Vec2};
use reader_core::i18n::Language;

impl ReaderApp {
    pub fn render_settings_panel(&mut self, ctx: &egui::Context) {
        let palette = super::design::Palette::new(self.dark_mode);
        let width = (ctx.screen_rect().width() * 0.34).clamp(320., 410.);
        let collapse_label = self.i18n.t("common.collapse").to_owned();
        if super::design::animated_side_panel(
            ctx,
            "settings_panel",
            false,
            width,
            self.show_settings && self.view == crate::app::AppView::Reader,
            &collapse_label,
            |ui| {
                ui.spacing_mut().interact_size.y = 34.;
                ui.spacing_mut().button_padding = egui::vec2(10., 6.);
                super::design::sidebar_title(ui, self.i18n.t("settings.appearance_title"));
                ui.add_space(12.);
                egui::ScrollArea::vertical()
                    .id_salt("appearance_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        settings_group(
                            ui,
                            self.i18n.t("appearance.link_title").to_owned(),
                            palette,
                            |ui| self.render_appearance_chain(ui),
                        );
                        settings_group(
                            ui,
                            self.i18n.t("appearance.styles").to_owned(),
                            palette,
                            |ui| self.render_settings_visual(ui, palette.text),
                        );
                        settings_group(
                            ui,
                            self.i18n.t("settings.page_turn_method").to_owned(),
                            palette,
                            |ui| self.render_settings_modes(ui),
                        );
                        settings_group(
                            ui,
                            self.i18n.t("settings.font_group").to_owned(),
                            palette,
                            |ui| self.render_settings_fonts(ui),
                        );
                        settings_group(
                            ui,
                            self.i18n.t("settings.layout_style").to_owned(),
                            palette,
                            |ui| self.render_settings_presentation(ui),
                        );
                        settings_group(
                            ui,
                            self.i18n.t("settings.layout_group").to_owned(),
                            palette,
                            |ui| self.render_settings_layout(ui),
                        );
                        egui::CollapsingHeader::new(self.i18n.t("settings.more"))
                            .id_salt("appearance_more")
                            .show(ui, |ui| {
                                self.render_settings_language(ui, palette.text);
                                ui.add_space(12.);
                                self.render_settings_api(ui, palette.text, palette.muted);
                                ui.add_space(12.);
                                if ui
                                    .checkbox(
                                        &mut self.scripts_enabled,
                                        self.i18n.t("settings.book_scripts"),
                                    )
                                    .changed()
                                {
                                    self.native_events.clear();
                                    self.native_applied.clear();
                                    self.pages_dirty = true;
                                    self.pending_restore_block = Some(self.current_block);
                                    self.page_curl.clear();
                                }
                                ui.small(self.i18n.t("settings.book_scripts_desc"));
                                ui.add_space(12.);
                                if ui
                                    .add_sized(
                                        [ui.available_width(), 36.],
                                        egui::Button::new(self.i18n.t("settings.reset_display")),
                                    )
                                    .clicked()
                                {
                                    self.confirm_display_reset = true;
                                }
                                ui.small(self.i18n.t("settings.reset_display_scope"));
                            });
                    });
            },
        ) {
            self.show_settings = false;
        }
        self.render_reading_theme_editor(ctx);
    }

    pub(crate) fn render_display_reset_confirmation(&mut self, ctx: &egui::Context) {
        if !self.confirm_display_reset {
            return;
        }
        if self.view != crate::app::AppView::Reader || !self.show_settings {
            self.confirm_display_reset = false;
            return;
        }
        let modal = egui::Modal::new(egui::Id::new("confirm_display_reset")).show(ctx, |ui| {
            ui.set_width(340.);
            ui.label(
                egui::RichText::new(self.i18n.t("settings.reset_title"))
                    .size(18.)
                    .strong(),
            );
            ui.add_space(8.);
            ui.label(self.i18n.t("settings.reset_message"));
            ui.add_space(12.);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let palette = super::design::Palette::new(self.dark_mode);
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(self.i18n.t("settings.reset_confirm"))
                                .color(Color32::WHITE),
                        )
                        .fill(palette.accent),
                    )
                    .clicked()
                {
                    self.font_size = 18.;
                    self.reader_font_family = "Sans".into();
                    self.reader_cjk_font_family = "Sans".into();
                    self.css_policy = reader_core::epub::native::CssPolicy::Mixed;
                    self.scroll_mode = false;
                    self.reader_page_animation = "Realistic".into();
                    self.line_spacing = 1.8;
                    self.para_spacing = 0.6;
                    self.text_indent = 2;
                    self.title_font_scale = 1.5;
                    self.reader_bg_color = Color32::from_rgb(250, 246, 238);
                    self.reader_font_color = None;
                    self.reading_themes
                        .save("paper", super::reading_themes::template("paper").unwrap());
                    self.apply_selected_reading_theme();
                    self.pages_dirty = true;
                    self.pending_restore_block = Some(self.current_block);
                    self.embedded_fonts_registered = false;
                    self.page_curl.clear();
                    self.confirm_display_reset = false;
                }
                if ui.button(self.i18n.t("common.cancel")).clicked() {
                    self.confirm_display_reset = false;
                }
            });
        });
        if modal.should_close() {
            self.confirm_display_reset = false;
        }
    }

    fn render_settings_language(&mut self, ui: &mut egui::Ui, heading_color: Color32) {
        ui.label(
            egui::RichText::new(self.i18n.t("settings.language"))
                .size(15.0)
                .strong()
                .color(heading_color),
        );
        ui.add_space(4.0);
        let current_label = self.i18n.choice_label(self.i18n.preference()).to_string();
        if ui
            .add_sized(
                [ui.available_width(), 40.],
                egui::Button::new(current_label),
            )
            .clicked()
        {
            self.show_language = true;
        }
    }

    pub(crate) fn render_language_picker(&mut self, ctx: &egui::Context) {
        if !self.show_language {
            return;
        }
        let modal = egui::Modal::new(egui::Id::new("language_picker")).show(ctx, |ui| {
            ui.set_width(320_f32.min((ctx.screen_rect().width() - 64.).max(180.)));
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(self.i18n.t("language.title"))
                        .size(18.)
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if super::design::close_button(ui, self.i18n.t("common.close")).clicked() {
                        self.show_language = false;
                    }
                });
            });
            ui.add_space(8.);
            let mut choices = |ui: &mut egui::Ui| {
                for language in Language::all() {
                    let label = self.i18n.choice_label(language).to_owned();
                    let selected = self.i18n.preference() == language;
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 44.),
                        egui::Sense::click(),
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::RadioButton,
                            true,
                            selected,
                            label.clone(),
                        )
                    });
                    let accent = super::design::Palette::new(self.dark_mode).accent;
                    if response.hovered() || response.has_focus() {
                        ui.painter()
                            .rect_filled(rect, 6., ui.visuals().widgets.hovered.bg_fill);
                    }
                    let circle = egui::pos2(rect.left() + 14., rect.center().y);
                    ui.painter().circle_stroke(
                        circle,
                        7.,
                        egui::Stroke::new(
                            1.4_f32,
                            if selected {
                                accent
                            } else {
                                ui.visuals().weak_text_color()
                            },
                        ),
                    );
                    if selected {
                        ui.painter().circle_filled(circle, 3.5, accent);
                    }
                    ui.painter().text(
                        egui::pos2(rect.left() + 32., rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        label,
                        egui::TextStyle::Body.resolve(ui.style()),
                        ui.visuals().text_color(),
                    );
                    if response.clicked() {
                        self.i18n.set_language(language.clone());
                        self.show_language = false;
                    }
                }
            };
            if Language::all().len() <= 6 {
                choices(ui);
            } else {
                egui::ScrollArea::vertical()
                    .max_height((ctx.screen_rect().height() - 120.).max(44.))
                    .show(ui, choices);
            }
        });
        if modal.should_close() {
            self.show_language = false;
        }
    }

    fn render_settings_presentation(&mut self, ui: &mut egui::Ui) {
        // Reading mode
        let previous = self.css_policy;
        ui.label(self.i18n.t("settings.layout_style"));
        ui.horizontal(|ui| {
            use reader_core::epub::native::CssPolicy;
            for (mode, label) in [
                (CssPolicy::None, self.i18n.t("settings.layout_native")),
                (CssPolicy::Mixed, self.i18n.t("settings.layout_mixed")),
                (CssPolicy::Publisher, self.i18n.t("settings.layout_classic")),
            ] {
                ui.selectable_value(&mut self.css_policy, mode, label);
            }
        });
        ui.small(match self.css_policy {
            reader_core::epub::native::CssPolicy::None => {
                self.i18n.t("settings.layout_native_desc")
            }
            reader_core::epub::native::CssPolicy::Mixed => {
                self.i18n.t("settings.layout_mixed_desc")
            }
            reader_core::epub::native::CssPolicy::Publisher => {
                self.i18n.t("settings.layout_classic_desc")
            }
        });
        if previous != self.css_policy {
            self.pending_restore_block = Some(self.current_block);
            self.pages_dirty = true;
            self.page_curl.clear();
        }
    }

    fn render_settings_modes(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(self.i18n.t("settings.reading_mode"));
            if ui
                .selectable_label(self.scroll_mode, self.i18n.t("settings.scroll"))
                .clicked()
            {
                self.scroll_mode = true;
                self.pages_dirty = true;
            }
            if ui
                .selectable_label(!self.scroll_mode, self.i18n.t("settings.paging"))
                .clicked()
            {
                self.scroll_mode = false;
                self.pages_dirty = true;
            }
        });

        {
            // Page animation
            ui.horizontal_wrapped(|ui| {
                ui.label(self.i18n.t("settings.page_animation"));
                for mode in ["Realistic", "Slide", "Cover", "None"] {
                    let label = match mode {
                        "Realistic" => self.i18n.t("settings.realistic"),
                        "Slide" => self.i18n.t("settings.slide"),
                        "Cover" => self.i18n.t("settings.cover"),
                        _ => self.i18n.t("settings.none"),
                    };
                    if ui
                        .selectable_label(self.reader_page_animation == mode, label)
                        .clicked()
                    {
                        self.reader_page_animation = mode.to_string();
                    }
                }
            });
            if self.reader_page_animation != "None" {
                ui.add_space(4.0);
                ui.label(self.i18n.t("settings.animation_speed"));
                super::design::settings_slider(
                    ui,
                    egui::Slider::new(&mut self.reader_page_animation_speed, 0.04..=0.40)
                        .step_by(0.02)
                        .custom_formatter(|value, _| format!("{:.0}%", value / 0.4 * 100.))
                        .custom_parser(|text| {
                            text.trim_end_matches('%')
                                .trim()
                                .parse::<f64>()
                                .ok()
                                .map(|percent| percent.clamp(10., 100.) / 100. * 0.4)
                        }),
                );
            }
        }
    }
    fn render_settings_fonts(&mut self, ui: &mut egui::Ui) {
        self.render_font_library_window(ui);
        let enabled = self.css_policy != reader_core::epub::native::CssPolicy::Publisher;
        ui.add_enabled_ui(enabled, |ui| {
            ui.horizontal(|ui| {
                let width = ui.available_width();
                if ui
                    .add_sized(
                        [42., 40.],
                        egui::Button::new(egui::RichText::new("A").size(17.)),
                    )
                    .on_hover_text(self.i18n.t("settings.font_smaller"))
                    .clicked()
                {
                    self.font_size = (self.font_size - 1.).max(12.);
                    self.pages_dirty = true;
                }
                let center = width - 100.;
                ui.allocate_ui_with_layout(
                    egui::vec2(center.max(40.), 40.),
                    egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                    |ui| {
                        ui.label(
                            egui::RichText::new(format!("{:.0} pt", self.font_size))
                                .size(21.)
                                .monospace(),
                        );
                    },
                );
                if ui
                    .add_sized(
                        [42., 40.],
                        egui::Button::new(egui::RichText::new("A").size(25.)),
                    )
                    .on_hover_text(self.i18n.t("settings.font_larger"))
                    .clicked()
                {
                    self.font_size = (self.font_size + 1.).min(40.);
                    self.pages_dirty = true;
                }
            });
            if super::design::settings_slider(
                ui,
                egui::Slider::new(&mut self.font_size, 12.0..=40.0).show_value(false),
            )
            .changed()
            {
                self.pages_dirty = true;
            }
            // Latin font family
            ui.horizontal_wrapped(|ui| {
                ui.label(self.i18n.t("settings.font_latin"));
                let font_popup_id = ui.make_persistent_id("font_family_popup");
                let font_label = if self.reader_font_family.is_empty() {
                    self.i18n.t("fonts.follow_chinese").to_owned()
                } else {
                    self.reader_font_family.clone()
                };
                let btn = ui.add(egui::Button::new(font_label).truncate());
                if btn.clicked() {
                    ui.memory_mut(|m| m.toggle_popup(font_popup_id));
                }
                egui::popup_below_widget(
                    ui,
                    font_popup_id,
                    &btn,
                    egui::PopupCloseBehavior::CloseOnClickOutside,
                    |ui| {
                        super::design::menu_style(ui);
                        ui.set_width(248.0);
                        if super::design::menu_item(
                            ui,
                            self.i18n.t("fonts.follow_chinese"),
                            self.reader_font_family.is_empty(),
                            false,
                        )
                        .on_hover_text(self.i18n.t("fonts.follow_chinese_desc"))
                        .clicked()
                        {
                            self.reader_font_family.clear();
                            self.pages_dirty = true;
                            self.embedded_fonts_registered = false;
                            ui.memory_mut(|m| m.close_popup());
                        }
                        ui.separator();
                        if ui.button(self.i18n.t("fonts.import")).clicked() {
                            self.import_reader_font(ui);
                        }
                        if ui.button(self.i18n.t("fonts.library")).clicked() {
                            self.open_font_library(ui, true);
                            ui.memory_mut(|m| m.close_popup());
                        }
                        ui.separator();
                        let te = ui.add(super::design::text_edit(&mut self.font_search));
                        if btn.clicked() {
                            te.request_focus();
                        }
                        let query = self.font_search.to_lowercase();
                        let mut close_popup = false;
                        egui::ScrollArea::vertical()
                            .max_height(300.0)
                            .show(ui, |ui| {
                                for fam in ["Sans", "Serif", "Monospace"] {
                                    if (query.is_empty() || fam.to_lowercase().contains(&query))
                                        && super::design::menu_item(
                                            ui,
                                            fam,
                                            self.reader_font_family == fam,
                                            false,
                                        )
                                        .clicked()
                                    {
                                        self.reader_font_family = fam.to_string();
                                        self.pages_dirty = true;
                                        self.embedded_fonts_registered = false;
                                        close_popup = true;
                                    }
                                }
                                let sys_filtered: Vec<String> = self
                                    .system_font_names
                                    .iter()
                                    .filter(|n| {
                                        !crate::app::is_cjk_font_name(n)
                                            && (query.is_empty()
                                                || n.to_lowercase().contains(&query))
                                    })
                                    .cloned()
                                    .collect();
                                if !sys_filtered.is_empty() {
                                    ui.separator();
                                    for name in sys_filtered {
                                        if super::design::menu_item(
                                            ui,
                                            &name,
                                            self.reader_font_family == name,
                                            false,
                                        )
                                        .clicked()
                                        {
                                            self.reader_font_family = name;
                                            self.pages_dirty = true;
                                            self.embedded_fonts_registered = false;
                                            self.defer_custom_font_for_frame = true;
                                            close_popup = true;
                                        }
                                    }
                                }
                                let emb_filtered: Vec<String> = self
                                    .embedded_font_names
                                    .iter()
                                    .filter(|n| {
                                        query.is_empty() || n.to_lowercase().contains(&query)
                                    })
                                    .cloned()
                                    .collect();
                                if !emb_filtered.is_empty() {
                                    ui.separator();
                                    for name in emb_filtered {
                                        if super::design::menu_item(
                                            ui,
                                            &name,
                                            self.reader_font_family == name,
                                            false,
                                        )
                                        .clicked()
                                        {
                                            self.reader_font_family = name;
                                            self.pages_dirty = true;
                                            self.embedded_fonts_registered = false;
                                            self.defer_custom_font_for_frame = true;
                                            close_popup = true;
                                        }
                                    }
                                }
                            });
                        if close_popup {
                            ui.memory_mut(|m| m.close_popup());
                        }
                    },
                );
            });

            // CJK font family
            ui.horizontal_wrapped(|ui| {
                ui.label(self.i18n.t("settings.font_cjk"));
                let cjk_popup_id = ui.make_persistent_id("cjk_font_family_popup");
                let btn = ui.add(egui::Button::new(&self.reader_cjk_font_family).truncate());
                if btn.clicked() {
                    ui.memory_mut(|m| m.toggle_popup(cjk_popup_id));
                }
                egui::popup_below_widget(
                    ui,
                    cjk_popup_id,
                    &btn,
                    egui::PopupCloseBehavior::CloseOnClickOutside,
                    |ui| {
                        super::design::menu_style(ui);
                        ui.set_width(248.0);
                        if ui.button(self.i18n.t("fonts.import")).clicked() {
                            self.import_reader_font(ui);
                        }
                        if ui.button(self.i18n.t("fonts.library")).clicked() {
                            self.open_font_library(ui, false);
                            ui.memory_mut(|m| m.close_popup());
                        }
                        ui.separator();
                        let te = ui.add(super::design::text_edit(&mut self.cjk_font_search));
                        if btn.clicked() {
                            te.request_focus();
                        }
                        let query = self.cjk_font_search.to_lowercase();
                        let mut close_popup = false;
                        egui::ScrollArea::vertical()
                            .max_height(300.0)
                            .show(ui, |ui| {
                                if (query.is_empty() || "sans".contains(&query))
                                    && super::design::menu_item(
                                        ui,
                                        "Sans",
                                        self.reader_cjk_font_family == "Sans",
                                        false,
                                    )
                                    .clicked()
                                {
                                    self.reader_cjk_font_family = "Sans".to_string();
                                    self.pages_dirty = true;
                                    self.embedded_fonts_registered = false;
                                    close_popup = true;
                                }
                                let cjk_filtered: Vec<String> = self
                                    .system_font_names
                                    .iter()
                                    .filter(|n| {
                                        crate::app::is_cjk_font_name(n)
                                            && (query.is_empty()
                                                || n.to_lowercase().contains(&query))
                                    })
                                    .cloned()
                                    .collect();
                                if !cjk_filtered.is_empty() {
                                    ui.separator();
                                    for name in cjk_filtered {
                                        if super::design::menu_item(
                                            ui,
                                            &name,
                                            self.reader_cjk_font_family == name,
                                            false,
                                        )
                                        .clicked()
                                        {
                                            self.reader_cjk_font_family = name;
                                            self.pages_dirty = true;
                                            self.embedded_fonts_registered = false;
                                            self.defer_custom_font_for_frame = true;
                                            close_popup = true;
                                        }
                                    }
                                }
                            });
                        if close_popup {
                            ui.memory_mut(|m| m.close_popup());
                        }
                    },
                );
            });
        });
        if !enabled {
            ui.label(
                egui::RichText::new(self.i18n.t("settings.publisher_locked"))
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
        }
    }
    fn render_settings_layout(&mut self, ui: &mut egui::Ui) {
        ui.add_enabled_ui(
            self.css_policy != reader_core::epub::native::CssPolicy::Publisher,
            |ui| {
                // Line spacing
                ui.vertical(|ui| {
                    ui.label(self.i18n.t("settings.line_spacing"));
                    if super::design::settings_slider(
                        ui,
                        egui::Slider::new(&mut self.line_spacing, 1.0..=3.0).fixed_decimals(1),
                    )
                    .changed()
                    {
                        self.pages_dirty = true;
                    }
                });

                {
                    // Paragraph spacing
                    ui.vertical(|ui| {
                        ui.label(self.i18n.t("settings.para_spacing"));
                        if super::design::settings_slider(
                            ui,
                            egui::Slider::new(&mut self.para_spacing, 0.0..=2.0).fixed_decimals(1),
                        )
                        .changed()
                        {
                            self.pages_dirty = true;
                        }
                    });

                    ui.add_enabled_ui(
                        self.css_policy == reader_core::epub::native::CssPolicy::None,
                        |ui| {
                            // Text indent
                            ui.vertical(|ui| {
                                ui.label(self.i18n.t("settings.text_indent"));
                                let mut indent_f = self.text_indent as f32;
                                if super::design::settings_slider(
                                    ui,
                                    egui::Slider::new(&mut indent_f, 0.0..=4.0).fixed_decimals(0),
                                )
                                .changed()
                                {
                                    self.text_indent = indent_f as u8;
                                    self.pages_dirty = true;
                                }
                            });

                            // Title font scale
                            ui.vertical(|ui| {
                                ui.label(self.i18n.t("settings.heading_scale"));
                                if super::design::settings_slider(
                                    ui,
                                    egui::Slider::new(&mut self.title_font_scale, 1.0..=2.5)
                                        .fixed_decimals(1),
                                )
                                .changed()
                                {
                                    self.title_font_scale = self.title_font_scale.clamp(1.0, 2.5);
                                    self.pages_dirty = true;
                                }
                            });
                        },
                    );
                }
            },
        );
    }
    fn render_settings_visual(&mut self, ui: &mut egui::Ui, _heading_color: Color32) {
        let palette = super::design::Palette::new(self.dark_mode);
        let columns = ((ui.available_width() + 8.) / 104.).floor().clamp(2., 3.) as usize;
        let width = ((ui.available_width() - (columns - 1) as f32 * 8.) / columns as f32).max(40.);
        let ids = ["paper", "gray", "green", "light", "warm", "custom"];
        for row in ids.chunks(columns) {
            ui.horizontal(|ui| {
                for &id in row {
                    let pair = self.reading_themes.pair(id);
                    let paint = pair.side(self.appearance_current_dark);
                    let creating = id == "custom" && self.reading_themes.custom.is_none();
                    let paper = if creating {
                        palette.surface
                    } else {
                        super::reading_themes::color(paint.paper)
                    };
                    let ink = if creating {
                        palette.accent
                    } else {
                        super::reader_state::effective_text_color(
                            paper,
                            paint.ink.map(super::reading_themes::color),
                        )
                    };
                    let label = self.i18n.t(&format!("appearance.style_{id}")).to_owned();
                    let response = theme_card(
                        ui,
                        &label,
                        paper,
                        ink,
                        self.reading_themes.selected == id,
                        creating,
                        width,
                        palette.accent,
                    );
                    if response.clicked() {
                        if creating {
                            self.reading_theme_editor =
                                Some(super::reading_themes::ReadingThemeDraft::new(
                                    id,
                                    pair,
                                    self.appearance_current_dark,
                                ));
                        } else {
                            self.reading_themes.selected = id.into();
                            self.apply_selected_reading_theme();
                        }
                    }
                }
            });
            ui.add_space(6.);
        }
        if ui
            .add_sized(
                [ui.available_width(), 34.],
                egui::Button::new(self.i18n.t("appearance.adjust")).frame(false),
            )
            .clicked()
        {
            self.reading_theme_editor = Some(super::reading_themes::ReadingThemeDraft::new(
                &self.reading_themes.selected,
                self.reading_themes.selected_pair(),
                self.appearance_current_dark,
            ));
        }
    }

    fn render_settings_api(
        &mut self,
        ui: &mut egui::Ui,
        heading_color: Color32,
        subtitle_color: Color32,
    ) {
        ui.label(
            egui::RichText::new(self.i18n.t("settings.api_title"))
                .size(15.0)
                .strong()
                .color(heading_color),
        );
        ui.add_space(4.0);

        // Translation API
        ui.label(
            egui::RichText::new(self.i18n.t("settings.translate_section"))
                .size(13.0)
                .strong()
                .color(heading_color),
        );
        ui.add_space(2.0);

        ui.label(
            egui::RichText::new(self.i18n.t("settings.api_url"))
                .size(12.0)
                .color(subtitle_color),
        );
        ui.add(
            super::design::text_edit(&mut self.translate_api_url)
                .hint_text("https://api.example.com/translate")
                .desired_width(f32::INFINITY),
        );
        ui.add_space(2.0);

        ui.label(
            egui::RichText::new(self.i18n.t("settings.api_key"))
                .size(12.0)
                .color(subtitle_color),
        );
        ui.add(
            super::design::text_edit(&mut self.translate_api_key)
                .password(true)
                .hint_text("sk-...")
                .desired_width(f32::INFINITY),
        );
        ui.add_space(8.0);

        // Dictionary API
        ui.label(
            egui::RichText::new(self.i18n.t("settings.dictionary_section"))
                .size(13.0)
                .strong()
                .color(heading_color),
        );
        ui.add_space(2.0);

        ui.label(
            egui::RichText::new(self.i18n.t("settings.api_url"))
                .size(12.0)
                .color(subtitle_color),
        );
        ui.add(
            super::design::text_edit(&mut self.dictionary_api_url)
                .hint_text("https://api.example.com/dict")
                .desired_width(f32::INFINITY),
        );
        ui.add_space(2.0);

        ui.label(
            egui::RichText::new(self.i18n.t("settings.api_key"))
                .size(12.0)
                .color(subtitle_color),
        );
        ui.add(
            super::design::text_edit(&mut self.dictionary_api_key)
                .password(true)
                .hint_text("key-...")
                .desired_width(f32::INFINITY),
        );
    }
}

fn settings_group(
    ui: &mut egui::Ui,
    title: String,
    palette: super::design::Palette,
    body: impl FnOnce(&mut egui::Ui),
) {
    ui.label(
        egui::RichText::new(title)
            .size(12.)
            .strong()
            .color(palette.muted),
    );
    egui::Frame::new()
        .fill(palette.surface)
        .stroke(egui::Stroke::new(0.5_f32, palette.border))
        .corner_radius(12)
        .inner_margin(12.)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            body(ui);
        });
    ui.add_space(16.);
}
#[allow(clippy::too_many_arguments)] // Swatch appearance and interaction state stay explicit at each call.
fn theme_card(
    ui: &mut egui::Ui,
    label: &str,
    paper: Color32,
    ink: Color32,
    selected: bool,
    creating: bool,
    width: f32,
    accent: Color32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 90.), egui::Sense::click());
    let swatch = egui::Rect::from_min_size(rect.min, Vec2::new(width, 64.));
    ui.painter().rect_filled(swatch, 10., paper);
    ui.painter().rect_stroke(
        swatch,
        10.,
        egui::Stroke::new(
            if selected { 2_f32 } else { 1_f32 },
            if selected {
                accent
            } else {
                ui.visuals().widgets.noninteractive.bg_stroke.color
            },
        ),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        swatch.center(),
        egui::Align2::CENTER_CENTER,
        if creating { "+" } else { "Aa" },
        egui::FontId::proportional(24.),
        ink,
    );
    ui.painter().text(
        egui::pos2(rect.center().x, rect.bottom() - 10.),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(12.),
        ui.visuals().text_color(),
    );
    if selected {
        let center = swatch.right_bottom() - egui::vec2(10., 10.);
        ui.painter().circle_filled(center, 8., accent);
        ui.painter().add(egui::Shape::line(
            vec![
                center + egui::vec2(-3.5, 0.),
                center + egui::vec2(-1., 2.5),
                center + egui::vec2(4., -3.),
            ],
            egui::Stroke::new(1.5_f32, Color32::WHITE),
        ));
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, label)
    });
    response
}
