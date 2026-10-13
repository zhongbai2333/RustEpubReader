//! Refactored or updated settings interface for the application.
use crate::app::ReaderApp;
use eframe::egui;
use egui::{Color32, Vec2};
use reader_core::i18n::Language;

impl ReaderApp {
    pub fn render_settings_panel(&mut self, ctx: &egui::Context) {
        let palette = super::design::Palette::new(self.dark_mode);
        let panel_bg = palette.surface;
        let heading_color = palette.text;
        let subtitle_color = palette.muted;

        super::design::animated_side_panel(
            ctx,
            "settings_panel",
            false,
            320.,
            self.show_settings && self.view == crate::app::AppView::Reader,
            |ui| {
                ui.spacing_mut().interact_size.y = 30.;
                ui.spacing_mut().button_padding = egui::vec2(10., 5.);
                let widgets = &mut ui.visuals_mut().widgets;
                for w in [
                    &mut widgets.inactive,
                    &mut widgets.hovered,
                    &mut widgets.active,
                ] {
                    w.corner_radius = egui::CornerRadius::same(6);
                }
                egui::Frame::new()
                    .fill(panel_bg)
                    .inner_margin(0.)
                    .show(ui, |ui| {
                        // Header
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(self.i18n.t("settings.title"))
                                    .size(18.0)
                                    .strong()
                                    .color(heading_color),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if super::design::close_button(
                                        ui,
                                        self.i18n.t("settings.close"),
                                    )
                                    .clicked()
                                    {
                                        self.show_settings = false;
                                    }
                                },
                            );
                        });
                        ui.separator();
                        ui.add_space(4.0);

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            // ── Language ──
                            self.render_settings_language(ui, heading_color);
                            ui.add_space(12.0);

                            // ── Typography ──
                            self.render_settings_typography(ui, heading_color);
                            ui.add_space(12.0);

                            // ── Visual ──
                            self.render_settings_visual(ui, heading_color);
                            ui.add_space(12.0);

                            // ── Background Image ──
                            self.render_settings_bg_image(ui, heading_color);
                            ui.add_space(12.0);

                            // ── Translation API ──
                            ui.separator();
                            ui.add_space(8.0);
                            self.render_settings_api(ui, heading_color, subtitle_color);
                            ui.add_space(12.0);

                            self.render_cloud_sync_settings(ui);
                            ui.add_space(12.0);

                            // ── CSC ──
                            ui.separator();
                            ui.add_space(8.0);
                            self.render_csc_settings(ui);
                            ui.add_space(16.0);
                        });
                    });
            },
        );
    }

    fn render_settings_language(&mut self, ui: &mut egui::Ui, heading_color: Color32) {
        ui.label(
            egui::RichText::new(self.i18n.t("settings.language"))
                .size(15.0)
                .strong()
                .color(heading_color),
        );
        ui.add_space(4.0);
        let current_label = self.i18n.language().label().to_string();
        egui::ComboBox::from_id_salt("language_combo")
            .selected_text(&current_label)
            .show_ui(ui, |ui| {
                super::design::menu_style(ui);
                for lang in Language::all() {
                    if super::design::menu_item(
                        ui,
                        lang.label(),
                        self.i18n.language() == lang,
                        false,
                    )
                    .clicked()
                    {
                        self.i18n.set_language(lang.clone());
                    }
                }
            });
    }

    fn render_settings_typography(&mut self, ui: &mut egui::Ui, heading_color: Color32) {
        ui.label(
            egui::RichText::new(self.i18n.t("settings.typography"))
                .size(15.0)
                .strong()
                .color(heading_color),
        );
        ui.add_space(4.0);

        // Font size
        ui.vertical(|ui| {
            ui.label(
                self.i18n
                    .tf1("settings.font_size", &format!("{:.0}", self.font_size)),
            );
            if super::design::settings_slider(
                ui,
                egui::Slider::new(&mut self.font_size, 12.0..=40.0),
            )
            .changed()
            {
                self.pages_dirty = true;
            }
        });

        // Reading mode
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
            ui.label("标题字号倍率");
            if super::design::settings_slider(
                ui,
                egui::Slider::new(&mut self.title_font_scale, 1.0..=2.5).fixed_decimals(1),
            )
            .changed()
            {
                self.title_font_scale = self.title_font_scale.clamp(1.0, 2.5);
                self.pages_dirty = true;
            }
        });

        // Latin font family
        ui.horizontal_wrapped(|ui| {
            ui.label(self.i18n.t("settings.font_latin"));
            let font_popup_id = ui.make_persistent_id("font_family_popup");
            let btn = ui.button(&self.reader_font_family);
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
                                        && (query.is_empty() || n.to_lowercase().contains(&query))
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
                                .filter(|n| query.is_empty() || n.to_lowercase().contains(&query))
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
            let btn = ui.button(&self.reader_cjk_font_family);
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
                                        && (query.is_empty() || n.to_lowercase().contains(&query))
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
                egui::Slider::new(&mut self.reader_page_animation_speed, 0.04..=0.40).step_by(0.02),
            );
        }
    }

    fn render_settings_visual(&mut self, ui: &mut egui::Ui, heading_color: Color32) {
        ui.label(
            egui::RichText::new(self.i18n.t("settings.visual"))
                .size(15.0)
                .strong()
                .color(heading_color),
        );
        ui.add_space(4.0);

        // Background color
        ui.horizontal_wrapped(|ui| {
            ui.label(self.i18n.t("settings.bg_color"));
            let presets = [
                Color32::from_rgb(250, 246, 238),
                Color32::from_rgb(241, 243, 245),
                Color32::from_rgb(232, 240, 232),
                Color32::from_rgb(26, 26, 28),
                Color32::from_rgb(36, 38, 43),
            ];
            for p in presets {
                let mut btn = egui::Button::new(" ")
                    .fill(p)
                    .min_size(Vec2::new(22.0, 22.0));
                if self.reader_bg_color == p {
                    btn = btn.stroke(egui::Stroke::new(2.0_f32, Color32::LIGHT_BLUE));
                }
                if ui.add(btn).clicked() {
                    self.reader_bg_color = p;
                }
            }
            egui::color_picker::color_edit_button_srgba(
                ui,
                &mut self.reader_bg_color,
                egui::color_picker::Alpha::Opaque,
            );
        });

        // Font color
        ui.horizontal_wrapped(|ui| {
            ui.label(self.i18n.t("settings.font_color"));
            if ui
                .selectable_label(
                    self.reader_font_color.is_none(),
                    self.i18n.t("settings.auto"),
                )
                .clicked()
            {
                self.reader_font_color = None;
            }
            if ui
                .selectable_label(
                    self.reader_font_color.is_some(),
                    self.i18n.t("settings.custom"),
                )
                .clicked()
                && self.reader_font_color.is_none()
            {
                self.reader_font_color = Some(Color32::from_gray(30));
            }
            if let Some(ref mut color) = self.reader_font_color {
                egui::color_picker::color_edit_button_srgba(
                    ui,
                    color,
                    egui::color_picker::Alpha::Opaque,
                );
            }
        });
    }

    fn render_settings_bg_image(&mut self, ui: &mut egui::Ui, heading_color: Color32) {
        ui.label(
            egui::RichText::new(self.i18n.t("settings.bg_image"))
                .size(15.0)
                .strong()
                .color(heading_color),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button(self.i18n.t("settings.pick_bg_image")).clicked() {
                self.pick_reader_background_image();
            }
            if ui.button(self.i18n.t("settings.clear_bg_image")).clicked() {
                self.clear_reader_background_image();
            }
        });
        ui.label(self.i18n.tf1(
            "settings.opacity",
            &format!("{}", (self.reader_bg_image_alpha * 100.0) as i32),
        ));
        super::design::settings_slider(
            ui,
            egui::Slider::new(&mut self.reader_bg_image_alpha, 0.0..=1.0),
        );
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
