//! Refactored or updated settings interface for the application.
use crate::app::ReaderApp;
use eframe::egui;
use egui::{Color32, Vec2};
use reader_core::i18n::Language;

impl ReaderApp {
    pub fn render_settings_panel(&mut self, ctx: &egui::Context) {
        let dark = self.dark_mode;
        let panel_bg = if dark {
            Color32::from_rgb(32, 32, 36)
        } else {
            Color32::from_rgb(248, 248, 252)
        };
        let heading_color = if dark {
            Color32::from_gray(220)
        } else {
            Color32::from_gray(30)
        };
        let subtitle_color = if dark {
            Color32::from_gray(140)
        } else {
            Color32::from_gray(100)
        };

        egui::SidePanel::right("settings_panel")
            .default_width(320.0)
            .max_width(400.0)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(panel_bg)
                    .inner_margin(16.0)
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
                                    if ui.small_button("✕").clicked() {
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

                            // ── CSC ──
                            ui.separator();
                            ui.add_space(8.0);
                            self.render_csc_settings(ui);
                            ui.add_space(16.0);
                        });
                    });
            });
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
                for lang in Language::all() {
                    if ui
                        .selectable_label(self.i18n.language() == lang, lang.label())
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
        ui.horizontal(|ui| {
            ui.label(
                self.i18n
                    .tf1("settings.font_size", &format!("{:.0}", self.font_size)),
            );
            if ui
                .add_sized(
                    [ui.available_width().min(200.0), 18.0],
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
        ui.horizontal(|ui| {
            ui.label(self.i18n.t("settings.line_spacing"));
            if ui
                .add_sized(
                    [ui.available_width().min(150.0), 18.0],
                    egui::Slider::new(&mut self.line_spacing, 1.0..=3.0).fixed_decimals(1),
                )
                .changed()
            {
                self.pages_dirty = true;
            }
        });

        // Paragraph spacing
        ui.horizontal(|ui| {
            ui.label(self.i18n.t("settings.para_spacing"));
            if ui
                .add_sized(
                    [ui.available_width().min(150.0), 18.0],
                    egui::Slider::new(&mut self.para_spacing, 0.0..=2.0).fixed_decimals(1),
                )
                .changed()
            {
                self.pages_dirty = true;
            }
        });

        // Text indent
        ui.horizontal(|ui| {
            ui.label(self.i18n.t("settings.text_indent"));
            let mut indent_f = self.text_indent as f32;
            if ui
                .add_sized(
                    [ui.available_width().min(150.0), 18.0],
                    egui::Slider::new(&mut indent_f, 0.0..=4.0).fixed_decimals(0),
                )
                .changed()
            {
                self.text_indent = indent_f as u8;
                self.pages_dirty = true;
            }
        });

        // Font family
        ui.horizontal_wrapped(|ui| {
            ui.label(self.i18n.t("settings.font"));
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
                    ui.set_min_width(220.0);
                    let te = ui.text_edit_singleline(&mut self.font_search);
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
                                    && ui
                                        .selectable_label(self.reader_font_family == fam, fam)
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
                                .filter(|n| query.is_empty() || n.to_lowercase().contains(&query))
                                .cloned()
                                .collect();
                            if !sys_filtered.is_empty() {
                                ui.separator();
                                for name in sys_filtered {
                                    if ui
                                        .selectable_label(self.reader_font_family == name, &name)
                                        .clicked()
                                    {
                                        self.reader_font_family = name;
                                        self.pages_dirty = true;
                                        self.embedded_fonts_registered = false;
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
                                    if ui
                                        .selectable_label(self.reader_font_family == name, &name)
                                        .clicked()
                                    {
                                        self.reader_font_family = name;
                                        self.pages_dirty = true;
                                        self.embedded_fonts_registered = false;
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
            for mode in ["Slide", "Cover", "None"] {
                let label = match mode {
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
            ui.add_sized(
                [ui.available_width().min(250.0), 18.0],
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
                    btn = btn.stroke(egui::Stroke::new(2.0, Color32::LIGHT_BLUE));
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
        ui.label(self.i18n.tf1(
            "settings.bg_opacity",
            &format!("{}", (self.reader_bg_opacity * 100.0) as i32),
        ));
        ui.add_sized(
            [ui.available_width().min(250.0), 18.0],
            egui::Slider::new(&mut self.reader_bg_opacity, 0.0..=1.0),
        );

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
        ui.add_sized(
            [ui.available_width().min(250.0), 18.0],
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
            egui::TextEdit::singleline(&mut self.translate_api_url)
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
            egui::TextEdit::singleline(&mut self.translate_api_key)
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
            egui::TextEdit::singleline(&mut self.dictionary_api_url)
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
            egui::TextEdit::singleline(&mut self.dictionary_api_key)
                .password(true)
                .hint_text("key-...")
                .desired_width(f32::INFINITY),
        );
    }
}
