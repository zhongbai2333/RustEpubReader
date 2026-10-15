use crate::app::ReaderApp;
use eframe::egui;
use reader_core::appearance::AppearanceLinkage;
use reader_core::appearance::{AutomaticSource, DisplayChoice};
use serde::{Deserialize, Serialize};

fn paint_appearance_link(
    painter: &egui::Painter,
    center: egui::Pos2,
    linked: bool,
    color: egui::Color32,
) {
    let stroke = egui::Stroke::new(1.5_f32, color);
    let angle = -std::f32::consts::FRAC_PI_4;
    for offset in [-1., 1.] {
        let offset = offset * if linked { 3.5 } else { 5. };
        let mut points = Vec::new();
        for (cap, start) in [
            (-2., std::f32::consts::FRAC_PI_2),
            (2., std::f32::consts::PI * 1.5),
        ] {
            for step in 0..=12 {
                let a = start + step as f32 * std::f32::consts::PI / 12.;
                let x = offset + cap + 3. * a.cos();
                let y = 3. * a.sin();
                points.push(
                    center
                        + egui::vec2(
                            x * angle.cos() - y * angle.sin(),
                            x * angle.sin() + y * angle.cos(),
                        ),
                );
            }
        }
        painter.add(egui::Shape::closed_line(points, stroke));
    }
    if !linked {
        painter.line_segment(
            [center + egui::vec2(-7., -7.), center + egui::vec2(7., 7.)],
            stroke,
        );
    }
}
fn paint_appearance_node(
    painter: &egui::Painter,
    rect: egui::Rect,
    index: usize,
    color: egui::Color32,
) {
    let stroke = egui::Stroke::new(1.4_f32, color);
    let center = rect.center();
    match index {
        0 => {
            painter.circle_stroke(center, 7., stroke);
            let mut points = vec![center];
            for step in 0..=16 {
                let angle = -std::f32::consts::FRAC_PI_2 + step as f32 * std::f32::consts::PI / 16.;
                points.push(center + egui::vec2(angle.cos() * 7., angle.sin() * 7.));
            }
            painter.add(egui::Shape::convex_polygon(
                points,
                color,
                egui::Stroke::NONE,
            ));
        }
        1 => {
            painter.rect_stroke(
                rect.translate(egui::vec2(2., -2.)).shrink(2.),
                2.,
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.rect_filled(
                rect.translate(egui::vec2(-2., 2.)).shrink(2.),
                2.,
                painter.ctx().style().visuals.panel_fill,
            );
            painter.rect_stroke(
                rect.translate(egui::vec2(-2., 2.)).shrink(2.),
                2.,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        _ => super::design::paint_icon(painter, rect, super::design::Icon::Book, color),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadingPaint {
    pub paper: [u8; 4],
    pub ink: Option<[u8; 4]>,
    pub image: Option<String>,
    pub strength: f32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadingPaintPair {
    pub light: ReadingPaint,
    pub dark: ReadingPaint,
}
impl Default for ReadingPaintPair {
    fn default() -> Self {
        Self {
            light: ReadingPaint {
                paper: [250, 246, 238, 255],
                ink: None,
                image: None,
                strength: 0.22,
            },
            dark: ReadingPaint {
                paper: [26, 26, 28, 255],
                ink: None,
                image: None,
                strength: 0.12,
            },
        }
    }
}
impl ReaderApp {
    #[cfg(debug_assertions)]
    pub(crate) fn capture_appearance_preview(&self, ctx: &egui::Context) {
        let introduction = std::env::args().any(|a| a == "--reader-introduction-preview");
        if !introduction && !std::env::args().any(|a| a == "--appearance-linkage-preview") {
            return;
        }
        if introduction && !self.show_reader_introduction {
            return;
        }
        let frames = ctx.data_mut(|data| {
            let id = egui::Id::new("appearance_preview_frames");
            let next = data.get_temp::<usize>(id).unwrap_or(0) + 1;
            data.insert_temp(id, next);
            next
        });
        if frames == 8 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let screenshot = ctx.input(|i| {
            i.events.iter().find_map(|event| {
                if let egui::Event::Screenshot { image, .. } = event {
                    Some(image.clone())
                } else {
                    None
                }
            })
        });
        if let Some(image) = screenshot {
            let rgba: Vec<u8> = image
                .pixels
                .iter()
                .flat_map(|color| color.to_array())
                .collect();
            if let Some(image) =
                image::RgbaImage::from_raw(image.size[0] as u32, image.size[1] as u32, rgba)
            {
                let _ = image.save(std::path::Path::new(&self.data_dir).join(if introduction {
                    "reader-introduction.png"
                } else {
                    "appearance-linkage.png"
                }));
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            ctx.request_repaint();
        }
    }
    fn appearance_text(&self, key: &str) -> String {
        self.i18n.t(&format!("appearance.link_{key}")).to_owned()
    }
    pub(crate) fn render_appearance_window(&mut self, ctx: &egui::Context) {
        if !self.show_appearance_linkage {
            return;
        }
        let mut open = true;
        egui::Window::new(self.appearance_text("title"))
            .open(&mut open)
            .default_width(480.)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(640.)
                    .show(ui, |ui| self.render_appearance_chain(ui));
            });
        self.show_appearance_linkage = open;
    }
    pub(crate) fn render_appearance_chain(&mut self, ui: &mut egui::Ui) {
        let values = [
            (
                "source",
                self.appearance_text(match self.appearance_linkage.source {
                    AutomaticSource::System => "system",
                    AutomaticSource::Ambient => "ambient",
                }),
            ),
            (
                "app",
                self.i18n
                    .t(
                        if self
                            .appearance_linkage
                            .app_dark(self.system_appearance_dark, None)
                        {
                            "appearance.mode_dark"
                        } else {
                            "appearance.mode_light"
                        },
                    )
                    .to_owned(),
            ),
            (
                "reading",
                self.i18n
                    .t(
                        if self
                            .appearance_linkage
                            .reading_dark(self.system_appearance_dark, None)
                        {
                            "appearance.mode_dark"
                        } else {
                            "appearance.mode_light"
                        },
                    )
                    .to_owned(),
            ),
        ];
        let gap = 4.;
        let width = ui.available_width();
        let edge_width = (width * 0.14).clamp(44., 56.);
        // Fit the actual sidebar width; rounding must not create horizontal overflow.
        let node_width = ((width - edge_width * 2. - gap * 4. - 1.) / 3.).max(0.);
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            ui.spacing_mut().button_padding.x = 4.;
            ui.horizontal(|ui| {
                for (index, (title, state)) in values.iter().enumerate() {
                    let selected = self.appearance_selected_node == index;
                    let label = format!("\n\n{}\n{}", self.appearance_text(title), state);
                    let button = egui::Button::new(egui::RichText::new(label).size(12.))
                        .wrap()
                        .corner_radius(14.);
                    let button = if selected {
                        button.selected(true)
                    } else {
                        button
                            .fill(ui.visuals().text_color().gamma_multiply(0.035))
                            .stroke(egui::Stroke::new(
                                1.0_f32,
                                ui.visuals().text_color().gamma_multiply(0.12),
                            ))
                    };
                    let response = ui.add_sized([node_width, 104.], button);
                    let icon_rect = egui::Rect::from_center_size(
                        egui::pos2(response.rect.center().x, response.rect.top() + 27.),
                        egui::vec2(18., 18.),
                    );
                    let ink = if selected {
                        ui.visuals().hyperlink_color
                    } else {
                        ui.visuals().text_color()
                    };
                    paint_appearance_node(ui.painter(), icon_rect, index, ink);
                    if response.clicked() {
                        self.appearance_selected_node = index;
                    }
                    if index < 2 {
                        let linked = if index == 0 {
                            self.appearance_linkage.app_linked
                        } else {
                            self.appearance_linkage.reading_linked
                        };
                        let action = self.appearance_text(if linked { "unlink" } else { "relink" });
                        let action = if action.chars().count() == 4
                            && action
                                .chars()
                                .all(|c| (0x4e00..=0x9fff).contains(&(c as u32)))
                        {
                            let (first, last) =
                                action.split_at(action.char_indices().nth(2).unwrap().0);
                            format!("{first}\n{last}")
                        } else {
                            action
                        };
                        let description = self.appearance_text(match (index, linked) {
                            (0, true) => "unlink_app",
                            (0, false) => "relink_app",
                            (_, true) => "unlink_reading",
                            (_, false) => "relink_reading",
                        });
                        let response = ui
                            .add_sized(
                                [edge_width, 104.],
                                egui::Button::new(
                                    egui::RichText::new(format!("\n\n{action}"))
                                        .size(12.)
                                        .color(ui.visuals().hyperlink_color),
                                )
                                .wrap()
                                .frame(false),
                            )
                            .on_hover_text(description);
                        paint_appearance_link(
                            ui.painter(),
                            egui::pos2(response.rect.center().x, response.rect.top() + 27.),
                            linked,
                            ui.visuals().hyperlink_color,
                        );
                        if response.clicked() {
                            if index == 0 {
                                self.appearance_linkage
                                    .toggle_app_link(self.system_appearance_dark, None);
                            } else {
                                self.appearance_linkage
                                    .toggle_reading_link(self.system_appearance_dark, None);
                            }
                            self.appearance_selected_node = index + 1;
                        }
                    }
                }
            });
        });
        ui.add_space(12.);
        ui.strong(self.appearance_text(match self.appearance_selected_node {
            0 => "source",
            1 => "app",
            _ => "reading",
        }));
        ui.add_space(6.);
        match self.appearance_selected_node {
            0 => {
                let mut source = self.appearance_linkage.source;
                self.appearance_source_picker(ui, &mut source, false);
                self.appearance_linkage.source = source;
            }
            1 if !self.appearance_linkage.app_linked => {
                ui.horizontal(|ui| {
                    for dark in [false, true] {
                        if ui
                            .selectable_label(
                                self.appearance_linkage.app_manual == dark,
                                self.i18n.t(if dark {
                                    "appearance.mode_dark"
                                } else {
                                    "appearance.mode_light"
                                }),
                            )
                            .clicked()
                        {
                            self.appearance_linkage.set_app(dark);
                        }
                    }
                });
            }
            2 if !self.appearance_linkage.reading_linked => {
                ui.horizontal(|ui| {
                    for (mode, key) in [
                        (DisplayChoice::Light, "appearance.mode_light"),
                        (DisplayChoice::Dark, "appearance.mode_dark"),
                        (DisplayChoice::Automatic, "appearance.mode_automatic"),
                    ] {
                        ui.selectable_value(
                            &mut self.appearance_linkage.reading_mode,
                            mode,
                            self.i18n.t(key),
                        );
                    }
                });
                if self.appearance_linkage.reading_mode == DisplayChoice::Automatic {
                    let mut source = self.appearance_linkage.reading_source;
                    self.appearance_source_picker(ui, &mut source, true);
                    self.appearance_linkage.reading_source = source;
                }
            }
            1 => {
                ui.label(self.appearance_text("follow_source"));
            }
            _ => {
                ui.label(self.appearance_text("follow_app"));
            }
        }
    }
    fn appearance_source_picker(
        &self,
        ui: &mut egui::Ui,
        source: &mut AutomaticSource,
        reading: bool,
    ) {
        ui.label(self.appearance_text(if reading { "reading_basis" } else { "basis" }));
        egui::ComboBox::from_id_salt(if reading {
            "reading_automatic_source"
        } else {
            "app_automatic_source"
        })
        .selected_text(self.appearance_text(if *source == AutomaticSource::System {
            "system"
        } else {
            "ambient"
        }))
        .show_ui(ui, |ui| {
            for (value, key) in [
                (AutomaticSource::System, "system"),
                (AutomaticSource::Ambient, "ambient"),
            ] {
                ui.selectable_value(source, value, self.appearance_text(key));
            }
        });
        if *source == AutomaticSource::Ambient && (reading || self.appearance_linkage.app_linked) {
            ui.small(self.appearance_text("unavailable"));
        }
    }
    pub(crate) fn capture_reading_paint(&self) -> ReadingPaint {
        ReadingPaint {
            paper: self.reader_bg_color.to_array(),
            ink: self.reader_font_color.map(|c| c.to_array()),
            image: self.reader_bg_image_path.clone(),
            strength: self.reader_bg_image_alpha,
        }
    }
    pub(crate) fn sync_appearance(&mut self, ctx: &egui::Context) {
        self.system_appearance_dark = ctx.system_theme() == Some(egui::Theme::Dark);
        let current = self.capture_reading_paint();
        if self.appearance_current_dark {
            self.appearance_paints.dark = current;
        } else {
            self.appearance_paints.light = current;
        }
        self.reading_themes.capture(self.appearance_paints.clone());
        self.dark_mode = self
            .appearance_linkage
            .app_dark(self.system_appearance_dark, None);
        let next = self
            .appearance_linkage
            .reading_dark(self.system_appearance_dark, None);
        if next != self.appearance_current_dark {
            self.appearance_current_dark = next;
            self.apply_selected_reading_theme();
            // Keep the exact reading anchor while invalidating paint caches.
            ctx.request_repaint();
        }
        if self.appearance_linkage.app_linked || !self.appearance_linkage.reading_linked {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
    }
    pub(crate) fn apply_selected_reading_theme(&mut self) {
        self.appearance_paints = self.reading_themes.selected_pair();
        let paint = self.appearance_paints.side(self.appearance_current_dark);
        self.reader_bg_color = egui::Color32::from_rgba_unmultiplied(
            paint.paper[0],
            paint.paper[1],
            paint.paper[2],
            paint.paper[3],
        );
        self.reader_font_color = paint
            .ink
            .map(|c| egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]));
        self.reader_bg_image_path = paint.image.clone();
        self.reader_bg_image_alpha = paint.strength;
        self.reader_bg_texture = None;
        self.page_curl.clear();
    }
    pub(crate) fn migrate_appearance(
        &mut self,
        linkage: Option<AppearanceLinkage>,
        paints: Option<ReadingPaintPair>,
        current: Option<bool>,
    ) {
        let [r, g, b, _] = self.reader_bg_color.to_array();
        let dark = f32::from(r) * 0.299 + f32::from(g) * 0.587 + f32::from(b) * 0.114 < 115.;
        self.appearance_linkage = linkage
            .filter(|v| v.version == 1)
            .unwrap_or_else(|| AppearanceLinkage::migrate(self.dark_mode, dark));
        self.appearance_paints = paints.unwrap_or_else(|| {
            let mut pair = ReadingPaintPair::default();
            pair.light.image = self.reader_bg_image_path.clone();
            pair.dark.image = self.reader_bg_image_path.clone();
            if dark {
                pair.dark = self.capture_reading_paint();
                if let Some((paper, ink)) = self.reader_day_palette {
                    pair.light.paper = paper.to_array();
                    pair.light.ink = ink.map(|c| c.to_array());
                }
            } else {
                pair.light = self.capture_reading_paint();
            }
            pair
        });
        self.appearance_current_dark = current.unwrap_or(dark);
    }
}
