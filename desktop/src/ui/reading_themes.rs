use super::appearance_linkage::{ReadingPaint, ReadingPaintPair};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[cfg(test)]
use std::collections::BTreeSet;

pub const PRESET_IDS: [&str; 5] = ["paper", "gray", "green", "light", "warm"];

pub fn template(id: &str) -> Option<ReadingPaintPair> {
    let (light, dark) = match id {
        "paper" => ([250, 246, 238, 255], [26, 26, 28, 255]),
        "gray" => ([241, 243, 245, 255], [36, 38, 43, 255]),
        "green" => ([232, 240, 232, 255], [29, 40, 35, 255]),
        "light" => ([255, 255, 255, 255], [23, 23, 25, 255]),
        "warm" => ([238, 221, 196, 255], [41, 36, 30, 255]),
        "custom" => ([255, 255, 255, 255], [26, 26, 28, 255]),
        _ => return None,
    };
    Some(ReadingPaintPair {
        light: ReadingPaint {
            paper: light,
            ink: None,
            image: None,
            strength: 0.22,
        },
        dark: ReadingPaint {
            paper: dark,
            ink: None,
            image: None,
            strength: 0.12,
        },
    })
}

impl ReadingPaintPair {
    pub fn side(&self, dark: bool) -> &ReadingPaint {
        if dark {
            &self.dark
        } else {
            &self.light
        }
    }
    pub fn side_mut(&mut self, dark: bool) -> &mut ReadingPaint {
        if dark {
            &mut self.dark
        } else {
            &mut self.light
        }
    }
    fn normalized(mut self) -> Self {
        for (dark, fallback) in [(false, 0.22), (true, 0.12)] {
            let paint = self.side_mut(dark);
            paint.strength = if paint.strength.is_finite() {
                paint.strength.clamp(0., 1.)
            } else {
                fallback
            };
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReadingThemes {
    pub selected: String,
    pub custom: Option<ReadingPaintPair>,
    pub overrides: BTreeMap<String, ReadingPaintPair>,
}
impl Default for ReadingThemes {
    fn default() -> Self {
        Self {
            selected: "paper".into(),
            custom: None,
            overrides: BTreeMap::new(),
        }
    }
}
impl ReadingThemes {
    pub fn pair(&self, id: &str) -> ReadingPaintPair {
        if id == "custom" {
            return self
                .custom
                .clone()
                .unwrap_or_else(|| template("custom").unwrap());
        }
        self.overrides
            .get(id)
            .cloned()
            .or_else(|| template(id))
            .unwrap_or_else(|| template("paper").unwrap())
    }
    pub fn selected_pair(&self) -> ReadingPaintPair {
        self.pair(&self.selected)
    }
    pub fn save(&mut self, id: &str, pair: ReadingPaintPair) {
        if PRESET_IDS.contains(&id) {
            self.overrides.insert(id.into(), pair.normalized());
            self.selected = id.into();
        } else {
            self.custom = Some(pair.normalized());
            self.selected = "custom".into();
        }
    }
    // Legacy controls and display reset can still change the active paint directly.
    pub fn capture(&mut self, pair: ReadingPaintPair) {
        if pair != self.selected_pair() {
            let id = self.selected.clone();
            self.save(&id, pair);
        }
    }
    pub fn normalized(mut self) -> Self {
        self.custom = self.custom.map(ReadingPaintPair::normalized);
        self.overrides = self
            .overrides
            .into_iter()
            .filter(|(id, _)| PRESET_IDS.contains(&id.as_str()))
            .map(|(id, pair)| (id, pair.normalized()))
            .collect();
        if !PRESET_IDS.contains(&self.selected.as_str())
            && !(self.selected == "custom" && self.custom.is_some())
        {
            self.selected = "paper".into();
        }
        self
    }
    pub fn migrate(pair: ReadingPaintPair) -> Self {
        if let Some(id) = PRESET_IDS
            .into_iter()
            .find(|id| template(id).as_ref() == Some(&pair))
        {
            Self {
                selected: id.into(),
                ..Default::default()
            }
        } else {
            Self {
                selected: "custom".into(),
                custom: Some(pair.normalized()),
                ..Default::default()
            }
        }
    }
    #[cfg(test)]
    pub fn image_references(&self) -> BTreeSet<&str> {
        self.overrides
            .values()
            .chain(self.custom.iter())
            .flat_map(|pair| [&pair.light.image, &pair.dark.image])
            .filter_map(|image| image.as_deref())
            .collect()
    }
}

pub struct ReadingThemeDraft {
    pub id: String,
    pub paints: ReadingPaintPair,
    pub dark: bool,
    pub texture: Option<eframe::egui::TextureHandle>,
    pub texture_path: Option<String>,
    color_picker: Option<ReadingColorPicker>,
}
impl ReadingThemeDraft {
    pub fn new(id: &str, paints: ReadingPaintPair, dark: bool) -> Self {
        Self {
            id: id.into(),
            paints,
            dark,
            texture: None,
            texture_path: None,
            color_picker: None,
        }
    }
    pub fn restore(&mut self) {
        self.paints = template(&self.id).unwrap_or_else(|| template("custom").unwrap());
        self.texture = None;
        self.texture_path = None;
        self.color_picker = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn six_slots_have_independent_neutral_custom() {
        let state = ReadingThemes::default();
        assert_eq!(PRESET_IDS.len() + 1, 6);
        assert_eq!(state.selected, "paper");
        assert_eq!(state.pair("custom"), template("custom").unwrap());
        assert_ne!(
            state.pair("custom").light.paper,
            state.pair("paper").light.paper
        );
        assert!(
            state.pair("custom").light.image.is_none() && state.pair("custom").dark.image.is_none()
        );
    }
    #[test]
    fn saving_presets_and_custom_never_overwrites_other_slots() {
        let mut state = ReadingThemes::default();
        let mut paper = template("paper").unwrap();
        paper.light.paper = [1, 2, 3, 255];
        paper.dark.image = Some("shared.png".into());
        let mut custom = template("custom").unwrap();
        custom.light.image = Some("shared.png".into());
        let mut gray = template("gray").unwrap();
        gray.dark.image = Some("gray.png".into());
        state.save("custom", custom.clone());
        state.save("paper", paper.clone());
        state.save("gray", gray.clone());
        assert_eq!(state.selected, "gray");
        assert_eq!(state.custom.as_ref(), Some(&custom));
        assert_eq!(state.pair("paper"), paper);
        assert_eq!(state.pair("green"), template("green").unwrap());
        assert_eq!(
            state.image_references(),
            BTreeSet::from(["shared.png", "gray.png"])
        );
        let restored: ReadingThemes =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        assert_eq!(restored, state);
        state.save("paper", template("paper").unwrap());
        assert_eq!(state.pair("gray"), gray);
        assert_eq!(state.custom, Some(custom));
        assert!(state.image_references().contains("shared.png"));
    }
    #[test]
    fn discarded_and_restored_drafts_do_not_change_saved_state() {
        let state = ReadingThemes::migrate(ReadingPaintPair {
            light: ReadingPaint {
                image: Some("external.png".into()),
                ..template("paper").unwrap().light
            },
            ..template("paper").unwrap()
        });
        let before = state.clone();
        let mut draft = ReadingThemeDraft::new("custom", state.selected_pair(), false);
        draft.paints.light.paper = [7, 8, 9, 255];
        draft.restore();
        assert_eq!(draft.paints, template("custom").unwrap());
        drop(draft);
        assert_eq!(state, before);
    }
    #[test]
    fn legacy_pairs_preserve_both_variants_and_paths() {
        assert_eq!(
            ReadingThemes::migrate(template("gray").unwrap()).selected,
            "gray"
        );
        let mut pair = template("paper").unwrap();
        pair.light.ink = Some([4, 5, 6, 255]);
        pair.dark.image = Some("/external/background.png".into());
        let state = ReadingThemes::migrate(pair.clone());
        assert_eq!(state.selected, "custom");
        assert_eq!(state.selected_pair(), pair);
    }
    #[test]
    fn active_capture_keeps_theme_identity_and_other_variant() {
        let mut state = ReadingThemes {
            selected: "warm".into(),
            ..Default::default()
        };
        let mut pair = state.selected_pair();
        pair.dark.ink = Some([200, 201, 202, 255]);
        state.capture(pair.clone());
        assert_eq!(state.selected, "warm");
        assert!(state.custom.is_none());
        assert_eq!(state.selected_pair(), pair);
        assert_eq!(state.pair("warm").light, template("warm").unwrap().light);
    }
    #[test]
    fn missing_fields_invalid_ids_and_strengths_normalize_safely() {
        let old: ReadingThemes = serde_json::from_str("{}").unwrap();
        assert_eq!(old, ReadingThemes::default());
        let mut state = ReadingThemes {
            selected: "unknown".into(),
            ..Default::default()
        };
        let mut pair = template("green").unwrap();
        pair.light.strength = f32::NAN;
        pair.dark.strength = 2.;
        state.overrides.insert("green".into(), pair.clone());
        state.overrides.insert("unknown".into(), pair);
        let state = state.normalized();
        assert_eq!(state.selected, "paper");
        assert_eq!(state.overrides.len(), 1);
        assert_eq!(state.pair("green").light.strength, 0.22);
        assert_eq!(state.pair("green").dark.strength, 1.);
    }
}

pub fn color(rgba: [u8; 4]) -> eframe::egui::Color32 {
    eframe::egui::Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3])
}

impl crate::app::ReaderApp {
    pub(crate) fn render_reading_theme_editor(&mut self, ctx: &eframe::egui::Context) {
        use eframe::egui;
        if !self.show_settings || self.view != crate::app::AppView::Reader {
            self.reading_theme_editor = None;
            return;
        }
        let Some(mut draft) = self.reading_theme_editor.take() else {
            return;
        };
        let title = if draft.id == "custom" {
            self.i18n.t("appearance.custom_title").to_owned()
        } else {
            self.i18n.tf1(
                "appearance.adjust_named",
                self.i18n.t(&format!("appearance.style_{}", draft.id)),
            )
        };
        let mut save = false;
        let mut cancel = false;
        let modal_id = egui::Id::new("reading_theme_editor");
        let width = (ctx.screen_rect().width() - 80.).clamp(280., 520.);
        let height = (ctx.screen_rect().height() - 120.).max(180.);
        let modal = egui::Modal::new(modal_id)
            .area(egui::Modal::default_area(modal_id).default_size(egui::vec2(width + 16., height)))
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.set_max_height(height);
                let title_width = (ui.available_width() - 156.).max(80.);
                ui.horizontal(|ui| {
                    cancel = ui
                        .add_sized([70., 32.], egui::Button::new(self.i18n.t("common.cancel")))
                        .clicked();
                    ui.allocate_ui_with_layout(
                        egui::vec2(title_width, 32.),
                        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                        |ui| {
                            ui.label(egui::RichText::new(&title).strong());
                        },
                    );
                    let valid_color = draft
                        .color_picker
                        .as_ref()
                        .is_none_or(|p| parse_color_hex(&p.hex).is_some());
                    save = ui
                        .add_enabled_ui(valid_color, |ui| {
                            ui.add_sized([70., 32.], egui::Button::new(self.i18n.t("common.save")))
                        })
                        .inner
                        .clicked();
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("reading_theme_editor_body")
                    .max_height((ctx.screen_rect().height() - 180.).max(100.))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let width = (ui.available_width() - 8.) / 2.;
                            for dark in [false, true] {
                                if ui
                                    .add_sized(
                                        [width, 32.],
                                        egui::Button::new(self.i18n.t(if dark {
                                            "appearance.edit_dark"
                                        } else {
                                            "appearance.edit_light"
                                        }))
                                        .selected(draft.dark == dark)
                                        .corner_radius(8.),
                                    )
                                    .clicked()
                                {
                                    draft.dark = dark;
                                    draft.color_picker = None;
                                }
                            }
                        });
                        ui.add_space(8.);
                        let image_path = draft.paints.side(draft.dark).image.clone();
                        if image_path != draft.texture_path {
                            draft.texture = None;
                            draft.texture_path = image_path.clone();
                            if let Some(path) = &image_path {
                                // Keep draft textures bounded; dropping the draft releases its handle.
                                let image = image::ImageReader::open(path)
                                    .ok()
                                    .and_then(|reader| reader.into_dimensions().ok())
                                    .filter(|&(w, h)| {
                                        u64::from(w) * u64::from(h) <= 16 * 1024 * 1024
                                    })
                                    .and_then(|_| image::open(path).ok());
                                if let Some(image) = image {
                                    let rgba = image.thumbnail(512, 512).to_rgba8();
                                    draft.texture = Some(ctx.load_texture(
                                        "reading_theme_draft",
                                        egui::ColorImage::from_rgba_unmultiplied(
                                            [rgba.width() as usize, rgba.height() as usize],
                                            rgba.as_raw(),
                                        ),
                                        egui::TextureOptions::LINEAR,
                                    ));
                                }
                            }
                        }
                        let paint = draft.paints.side(draft.dark);
                        let paper = color(paint.paper);
                        let ink =
                            super::reader_state::effective_text_color(paper, paint.ink.map(color));
                        ui.small(format!(
                            "{} · {}",
                            self.i18n.t("appearance.preview"),
                            self.i18n.t(if draft.dark {
                                "appearance.mode_dark"
                            } else {
                                "appearance.mode_light"
                            })
                        ));
                        let font = egui::FontId::new(
                            self.font_size,
                            super::reader_block::family_from_name(&self.reader_font_family),
                        );
                        let galley = ui.painter().layout(
                            self.i18n.t("appearance.sample").to_owned(),
                            font,
                            ink,
                            (ui.available_width() - 32.).max(80.),
                        );
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), galley.size().y + 32.),
                            egui::Sense::hover(),
                        );
                        ui.painter().rect_filled(rect, 6., paper);
                        if let Some(texture) = &draft.texture {
                            ui.painter().image(
                                texture.id(),
                                rect,
                                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                                egui::Color32::from_white_alpha(
                                    (paint.strength * 255.).round() as u8
                                ),
                            );
                        }
                        ui.painter()
                            .galley(rect.min + egui::vec2(16., 16.), galley, ink);
                        ui.add_space(12.);
                        {
                            let paint = draft.paints.side_mut(draft.dark);
                            reading_color_control(
                                ui,
                                self.i18n.t("settings.background_color"),
                                self.i18n.t("appearance.hex"),
                                &mut paint.paper,
                                &mut draft.color_picker,
                                false,
                                &PAPER_COLORS,
                            );
                            ui.add_space(10.);
                            ui.horizontal(|ui| {
                                ui.label(self.i18n.t("settings.text_color"));
                                if ui
                                    .selectable_value(
                                        &mut paint.ink,
                                        None,
                                        self.i18n.t("settings.auto"),
                                    )
                                    .clicked()
                                    && draft.color_picker.as_ref().is_some_and(|p| p.ink)
                                {
                                    draft.color_picker = None;
                                }
                            });
                            let mut ink = paint.ink.unwrap_or_else(|| {
                                super::reader_state::effective_text_color(color(paint.paper), None)
                                    .to_array()
                            });
                            if reading_color_control(
                                ui,
                                self.i18n.t("settings.custom"),
                                self.i18n.t("appearance.hex"),
                                &mut ink,
                                &mut draft.color_picker,
                                true,
                                &INK_COLORS,
                            ) {
                                paint.ink = Some(ink);
                            }
                        }
                        ui.separator();
                        ui.strong(self.i18n.t("common.background_image"));
                        if let Some(path) = &image_path {
                            let filename = std::path::Path::new(path)
                                .file_name()
                                .map(|n| n.to_string_lossy())
                                .unwrap_or_default();
                            ui.label(filename).on_hover_text(path);
                            if draft.texture.is_none() {
                                ui.colored_label(
                                    ui.visuals().error_fg_color,
                                    self.i18n.t("appearance.image_error"),
                                );
                            }
                        } else {
                            ui.weak(self.i18n.t("settings.image_empty"));
                        }
                        ui.horizontal_wrapped(|ui| {
                            if ui.button(self.i18n.t("settings.pick_bg_image")).clicked() {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("Image", &["png", "jpg", "jpeg", "webp", "bmp"])
                                    .pick_file()
                                {
                                    draft.paints.side_mut(draft.dark).image =
                                        Some(path.to_string_lossy().into_owned());
                                    draft.texture = None;
                                    draft.texture_path = None;
                                }
                            }
                            if image_path.is_some()
                                && ui.button(self.i18n.t("settings.clear_bg_image")).clicked()
                            {
                                draft.paints.side_mut(draft.dark).image = None;
                                draft.texture = None;
                                draft.texture_path = None;
                            }
                        });
                        if let Some(path) = image_path {
                            let paint = draft.paints.side_mut(draft.dark);
                            ui.label(self.i18n.tf1(
                                "settings.opacity",
                                &(paint.strength * 100.).round().to_string(),
                            ));
                            ui.add(
                                egui::Slider::new(&mut paint.strength, 0.0..=1.0).show_value(false),
                            );
                            if ui
                                .button(self.i18n.t(if draft.dark {
                                    "appearance.copy_light"
                                } else {
                                    "appearance.copy_dark"
                                }))
                                .clicked()
                            {
                                draft.paints.side_mut(!draft.dark).image = Some(path);
                            }
                        }
                        ui.separator();
                        if ui
                            .button(self.i18n.t(if draft.id == "custom" {
                                "appearance.reset_custom"
                            } else {
                                "appearance.restore_preset"
                            }))
                            .clicked()
                        {
                            draft.restore();
                        }
                        ui.weak(self.i18n.t("appearance.reset_note"));
                        ui.add_space(8.);
                        ui.weak(self.i18n.t(if draft.id == "custom" {
                            "appearance.custom_save_note"
                        } else {
                            "appearance.preset_save_note"
                        }));
                    });
            });
        if save {
            self.reading_themes.save(&draft.id, draft.paints);
            self.apply_selected_reading_theme();
            ctx.request_repaint();
        } else if !cancel && !modal.should_close() {
            self.reading_theme_editor = Some(draft);
        }
    }
}

const PAPER_COLORS: [u32; 12] = [
    0xF7F2E8, 0xFFFFFF, 0xF1F3F5, 0xE2EADC, 0xEEDDC4, 0xE7EFF7, 0xF3E6EA, 0xE8E2F0, 0x1A1A1C,
    0x24262B, 0x1D2823, 0x29241E,
];
const INK_COLORS: [u32; 12] = [
    0x1A1A1A, 0x2D2D2D, 0x5B4636, 0x314C3B, 0x284B63, 0x604A70, 0x7C3E45, 0x7A5726, 0xDCDCDC,
    0xE6E6E6, 0xF1EAD8, 0xFFFFFF,
];
struct ReadingColorPicker {
    ink: bool,
    hsv: eframe::egui::ecolor::Hsva,
    hex: String,
}
impl ReadingColorPicker {
    fn new(ink: bool, rgba: [u8; 4]) -> Self {
        Self {
            ink,
            hsv: eframe::egui::ecolor::Hsva::from_srgba_unmultiplied(rgba),
            hex: color_hex(rgba),
        }
    }
}
fn color_hex([r, g, b, _]: [u8; 4]) -> String {
    format!("{r:02X}{g:02X}{b:02X}")
}
fn parse_color_hex(text: &str) -> Option<[u8; 4]> {
    let value = text.trim().trim_start_matches('#');
    if value.len() != 6 || !value.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let rgb = u32::from_str_radix(value, 16).ok()?;
    Some([(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255])
}
fn reading_color_control(
    ui: &mut eframe::egui::Ui,
    label: &str,
    hex_label: &str,
    rgba: &mut [u8; 4],
    picker: &mut Option<ReadingColorPicker>,
    ink: bool,
    presets: &[u32],
) -> bool {
    use eframe::egui::{self, Align2, Color32, FontId, Sense, Stroke};
    let before = *rgba;
    let mut edited = false;
    ui.push_id(("reading_color", ink), |ui| {
        let open = picker.as_ref().is_some_and(|p| p.ink == ink);
        let (row, response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.), Sense::click());
        let visuals = ui.style().interact(&response);
        ui.painter().rect(
            row,
            8.,
            visuals.bg_fill,
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );
        let swatch = egui::Rect::from_center_size(
            row.left_center() + egui::vec2(24., 0.),
            egui::vec2(28., 28.),
        );
        ui.painter().rect(
            swatch,
            6.,
            color(*rgba),
            Stroke::new(1_f32, ui.visuals().weak_text_color()),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            row.left_center() + egui::vec2(48., 0.),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(14.),
            ui.visuals().text_color(),
        );
        ui.painter().text(
            row.right_center() - egui::vec2(14., 0.),
            Align2::RIGHT_CENTER,
            format!("#{}  {}", color_hex(*rgba), if open { "▴" } else { "▾" }),
            FontId::monospace(12.),
            ui.visuals().weak_text_color(),
        );
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
        if response.clicked() {
            *picker = if open {
                None
            } else {
                Some(ReadingColorPicker::new(ink, *rgba))
            };
        }
        if let Some(state) = picker.as_mut().filter(|p| p.ink == ink) {
            egui::Frame::new()
                .fill(ui.visuals().faint_bg_color)
                .corner_radius(10.)
                .inner_margin(12.)
                .show(ui, |ui| {
                    let width = (ui.available_width() - 36.).max(100.);
                    let height = (width * 0.62).clamp(140., 220.);
                    let mut board_changed = false;
                    ui.horizontal(|ui| {
                        let (square, response) = ui.allocate_exact_size(
                            egui::vec2(width, height),
                            Sense::click_and_drag(),
                        );
                        if let Some(at) = response
                            .interact_pointer_pos()
                            .filter(|_| response.dragged() || response.clicked())
                        {
                            state.hsv.s = ((at.x - square.left()) / square.width()).clamp(0., 1.);
                            state.hsv.v =
                                (1. - (at.y - square.top()) / square.height()).clamp(0., 1.);
                            board_changed = true;
                        }
                        let mut mesh = egui::Mesh::default();
                        const N: u32 = 24;
                        for y in 0..=N {
                            for x in 0..=N {
                                let s = x as f32 / N as f32;
                                let v = 1. - y as f32 / N as f32;
                                mesh.colored_vertex(
                                    square.min
                                        + egui::vec2(
                                            s * square.width(),
                                            (1. - v) * square.height(),
                                        ),
                                    Color32::from(egui::ecolor::Hsva::new(state.hsv.h, s, v, 1.)),
                                );
                            }
                        }
                        for y in 0..N {
                            for x in 0..N {
                                let a = y * (N + 1) + x;
                                mesh.add_triangle(a, a + 1, a + N + 1);
                                mesh.add_triangle(a + 1, a + N + 2, a + N + 1);
                            }
                        }
                        ui.painter().add(mesh);
                        let at = square.min
                            + egui::vec2(
                                state.hsv.s * square.width(),
                                (1. - state.hsv.v) * square.height(),
                            );
                        ui.painter()
                            .circle_stroke(at, 6., Stroke::new(3_f32, Color32::BLACK));
                        ui.painter()
                            .circle_stroke(at, 6., Stroke::new(1.5_f32, Color32::WHITE));
                        let (hue, response) = ui
                            .allocate_exact_size(egui::vec2(24., height), Sense::click_and_drag());
                        if let Some(at) = response
                            .interact_pointer_pos()
                            .filter(|_| response.dragged() || response.clicked())
                        {
                            state.hsv.h = ((at.y - hue.top()) / hue.height()).clamp(0., 1.);
                            board_changed = true;
                        }
                        for i in 0..60 {
                            let a = i as f32 / 60.;
                            let b = (i + 1) as f32 / 60.;
                            let band = egui::Rect::from_min_max(
                                egui::pos2(hue.left(), hue.top() + a * hue.height()),
                                egui::pos2(hue.right(), hue.top() + b * hue.height() + 0.5),
                            );
                            ui.painter().rect_filled(
                                band,
                                0.,
                                Color32::from(egui::ecolor::Hsva::new(a, 1., 1., 1.)),
                            );
                        }
                        let marker = egui::Rect::from_center_size(
                            egui::pos2(hue.center().x, hue.top() + state.hsv.h * hue.height()),
                            egui::vec2(28., 6.),
                        );
                        ui.painter().rect_stroke(
                            marker,
                            2.,
                            Stroke::new(2_f32, Color32::WHITE),
                            egui::StrokeKind::Middle,
                        );
                    });
                    if board_changed {
                        edited = true;
                        state.hsv.a = 1.;
                        *rgba = state.hsv.to_srgba_unmultiplied();
                        state.hex = color_hex(*rgba);
                    }
                    ui.add_space(10.);
                    ui.horizontal(|ui| {
                        let (preview, _) =
                            ui.allocate_exact_size(egui::vec2(44., 32.), Sense::hover());
                        ui.painter().rect_filled(preview, 6., color(*rgba));
                        ui.label(hex_label);
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut state.hex)
                                    .desired_width(90.)
                                    .char_limit(7),
                            )
                            .changed()
                        {
                            if let Some(parsed) = parse_color_hex(&state.hex) {
                                edited = true;
                                *rgba = parsed;
                                state.hsv = egui::ecolor::Hsva::from_srgba_unmultiplied(parsed);
                            }
                        }
                        if parse_color_hex(&state.hex).is_none() {
                            ui.colored_label(ui.visuals().error_fg_color, "#RRGGBB");
                        }
                    });
                });
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4., 4.);
            for &rgb in presets {
                let candidate = [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255];
                let (hit, response) = ui.allocate_exact_size(egui::vec2(36., 36.), Sense::click());
                let rect = hit.shrink(4.);
                ui.painter().rect(
                    rect,
                    6.,
                    color(candidate),
                    Stroke::new(
                        if *rgba == candidate { 2_f32 } else { 1_f32 },
                        if *rgba == candidate {
                            ui.visuals().selection.stroke.color
                        } else {
                            ui.visuals().weak_text_color()
                        },
                    ),
                    egui::StrokeKind::Inside,
                );
                if *rgba == candidate {
                    ui.painter().circle_filled(
                        rect.right_bottom() - egui::vec2(4., 4.),
                        4.,
                        ui.visuals().selection.stroke.color,
                    );
                }
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("#{rgb:06X}"))
                });
                if response.on_hover_text(format!("#{rgb:06X}")).clicked() {
                    edited = true;
                    *rgba = candidate;
                    if let Some(state) = picker.as_mut().filter(|p| p.ink == ink) {
                        *state = ReadingColorPicker::new(ink, candidate);
                    }
                }
            }
        });
    });
    edited || *rgba != before
}

#[cfg(test)]
mod color_picker_tests {
    use super::*;
    #[test]
    fn precise_input_is_opaque_and_rejects_incomplete_or_invalid_colors() {
        assert_eq!(parse_color_hex(" #a1B2c3 "), Some([0xA1, 0xB2, 0xC3, 255]));
        for input in ["", "ABC", "#12345678", "蓝色", "12GG34"] {
            assert!(parse_color_hex(input).is_none());
        }
    }
    #[test]
    fn selecting_the_current_auto_color_still_requests_a_manual_override() {
        use eframe::egui::{self, Event, Modifiers, PointerButton};
        let ctx = egui::Context::default();
        let mut rgba = [26, 26, 26, 255];
        let mut picker = None;
        let render =
            |ui: &mut egui::Ui, rgba: &mut [u8; 4], picker: &mut Option<ReadingColorPicker>| {
                reading_color_control(ui, "Ink", "Hex", rgba, picker, true, &[0x1A1A1A])
            };
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400., 300.),
            )),
            ..Default::default()
        };
        let _ = ctx.run(input(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                render(ui, &mut rgba, &mut picker);
            });
        });
        let mut click = input();
        let point = egui::pos2(26., 78.);
        click.events = vec![
            Event::PointerMoved(point),
            Event::PointerButton {
                pos: point,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            Event::PointerButton {
                pos: point,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ];
        let mut manual = false;
        let _ = ctx.run(click, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                manual = render(ui, &mut rgba, &mut picker);
            });
        });
        assert!(
            manual,
            "Explicitly choosing the current auto color must disable automatic ink"
        );
        assert_eq!(rgba, [26, 26, 26, 255]);
    }
    #[test]
    fn adjusting_either_color_leaves_the_other_appearance_side_untouched() {
        let mut draft = ReadingThemeDraft::new("custom", template("custom").unwrap(), false);
        let dark = draft.paints.dark.clone();
        draft.paints.light.paper = parse_color_hex("E7EFF7").unwrap();
        draft.paints.light.ink = Some(parse_color_hex("284B63").unwrap());
        assert_eq!(draft.paints.dark, dark);
        draft.restore();
        assert!(draft.color_picker.is_none());
    }
}
