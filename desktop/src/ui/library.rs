//! The main library view, displaying the collection of books.
use crate::app::ReaderApp;
use eframe::egui;
use egui::{Color32, CornerRadius, Stroke, StrokeKind, Vec2};
use reader_core::epub::EpubBook;
use reader_core::i18n::Language;

const COVER_PALETTE: [Color32; 6] = [
    Color32::from_rgb(56, 132, 255),
    Color32::from_rgb(120, 87, 255),
    Color32::from_rgb(255, 100, 100),
    Color32::from_rgb(50, 180, 130),
    Color32::from_rgb(255, 160, 50),
    Color32::from_rgb(200, 80, 180),
];

impl ReaderApp {
    pub fn render_library(&mut self, ui: &mut egui::Ui) {
        let mut action_open_path: Option<(String, usize)> = None;
        let mut action_remove_path: Option<String> = None;
        let mut action_export_path: Option<String> = None;
        let mut action_open_dialog = false;

        let dark = self.dark_mode;
        let accent = Color32::from_rgb(56, 132, 255);
        let card_bg = if dark {
            Color32::from_rgb(38, 38, 42)
        } else {
            Color32::from_rgb(255, 255, 255)
        };
        let card_hover_bg = if dark {
            Color32::from_rgb(48, 48, 54)
        } else {
            Color32::from_rgb(245, 245, 250)
        };
        let subtitle_color = if dark {
            Color32::from_gray(140)
        } else {
            Color32::from_gray(110)
        };
        let border_color = if dark {
            Color32::from_gray(55)
        } else {
            Color32::from_gray(220)
        };
        let heading_color = if dark {
            Color32::from_gray(230)
        } else {
            Color32::from_gray(30)
        };

        ui.add_space(28.0);
        ui.horizontal(|ui| {
            ui.add_space(32.0);
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(self.i18n.t("library.title"))
                        .size(26.0)
                        .strong()
                        .color(heading_color),
                );
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(self.i18n.t("library.subtitle"))
                        .size(12.5)
                        .color(subtitle_color),
                );
                ui.add_space(2.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(self.i18n.t("library.author"))
                            .size(12.0)
                            .color(subtitle_color),
                    );
                    ui.add_space(10.0);
                    ui.hyperlink_to(
                        self.i18n.t("library.project_link"),
                        "https://github.com/zhongbai233/RustEpubReader",
                    );
                });
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(32.0);
                let btn = egui::Button::new(
                    egui::RichText::new(format!("＋ {}", self.i18n.t("library.open_new")))
                        .size(14.0)
                        .color(Color32::WHITE),
                )
                .fill(accent)
                .stroke(Stroke::NONE)
                .corner_radius(CornerRadius::same(6))
                .min_size(Vec2::new(120.0, 36.0));
                if ui.add(btn).clicked() {
                    action_open_dialog = true;
                }
                ui.add_space(8.0);
                if ui
                    .selectable_label(
                        self.show_sharing_panel,
                        egui::RichText::new(self.i18n.t("share.toolbar")).size(14.0),
                    )
                    .clicked()
                {
                    self.show_sharing_panel = !self.show_sharing_panel;
                }
                ui.add_space(8.0);
                if ui
                    .button(egui::RichText::new(self.i18n.t("about.title")).size(14.0))
                    .clicked()
                {
                    self.show_about = true;
                }
                ui.add_space(8.0);
                let current_label = self.i18n.language().label().to_string();
                egui::ComboBox::from_id_salt("library_language_combo")
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
            });
        });
        ui.add_space(8.0);
        let separator_rect = ui.available_rect_before_wrap();
        ui.painter().line_segment(
            [
                egui::pos2(separator_rect.left() + 32.0, separator_rect.top()),
                egui::pos2(separator_rect.right() - 32.0, separator_rect.top()),
            ],
            Stroke::new(1.0, border_color),
        );
        ui.add_space(12.0);

        if self.library.books.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(80.0);
                ui.label(egui::RichText::new("📚").size(48.0));
                ui.add_space(16.0);
                ui.label(
                    egui::RichText::new(self.i18n.t("library.empty"))
                        .size(18.0)
                        .color(subtitle_color),
                );
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(self.i18n.t("library.empty_hint"))
                        .size(14.0)
                        .color(subtitle_color),
                );
            });
        } else {
            let sorted = self.library.sorted_indices_by_recent();
            let ctx = ui.ctx().clone();
            // Load covers incrementally: max 2 per frame to avoid blocking UI
            let mut covers_loaded = 0usize;
            for &idx in &sorted {
                let path = self.library.books[idx].path.clone();
                if let std::collections::hash_map::Entry::Vacant(e) =
                    self.cover_textures.entry(path.clone())
                {
                    if covers_loaded >= 2 {
                        ctx.request_repaint();
                        break;
                    }
                    covers_loaded += 1;
                    let tex = (|| {
                        let book = EpubBook::open(&path).ok()?;
                        let cover_bytes = book.cover_data.as_ref()?;
                        let img = image::load_from_memory(cover_bytes).ok()?;
                        let mut rgba = img.to_rgba8();
                        apply_rounded_corners_rgba(&mut rgba, 18);
                        let size = [rgba.width() as usize, rgba.height() as usize];
                        let ci = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                        Some(ctx.load_texture(
                            format!("cover_{}", path),
                            ci,
                            egui::TextureOptions::LINEAR,
                        ))
                    })();
                    e.insert(tex);
                }
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(32.0);
                    ui.label(
                        egui::RichText::new(
                            self.i18n
                                .tf1("library.book_count", &sorted.len().to_string()),
                        )
                        .size(13.0)
                        .color(subtitle_color),
                    );
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new(self.i18n.t("library.tip"))
                            .size(12.5)
                            .color(subtitle_color),
                    );
                });
                ui.add_space(12.0);
                // Match the 32px horizontal padding used by the header so the
                // last card always has a visible right margin.
                let padding = 32.0_f32;
                let gap = 16.0_f32;
                let available_width = (ui.available_width() - padding * 2.0).max(0.0);
                // Responsive: determine columns first, then compute card width to fill space.
                // min_card_w 210 ensures the action button row (read + export + delete)
                // fits within the content area at the smallest size.
                let min_card_w = 210.0_f32;
                let max_card_w = 420.0_f32;
                let cols = ((available_width + gap) / (min_card_w + gap))
                    .floor()
                    .max(1.0) as usize;
                let card_width = if cols == 1 {
                    available_width.min(max_card_w)
                } else {
                    ((available_width - gap * (cols as f32 - 1.0)) / cols as f32)
                        .clamp(min_card_w, max_card_w)
                };
                // Fixed card height keeps rows uniform regardless of text wrapping.
                let card_height = 160.0_f32;
                // Cover takes a slightly larger share so artwork is more visible.
                let cover_w = (card_width * 0.36).clamp(72.0, 110.0);
                let chunks: Vec<Vec<usize>> = sorted.chunks(cols).map(|c| c.to_vec()).collect();
                // Total width occupied by a full row; if cards were clamped to max_card_w
                // there will be leftover space — split it evenly so left/right margins match.
                let row_width = card_width * cols as f32 + gap * (cols as f32 - 1.0);
                let leading_pad = padding + ((available_width - row_width).max(0.0) * 0.5);
                for chunk in &chunks {
                    ui.horizontal(|ui| {
                        ui.add_space(leading_pad);
                        for (ci, &idx) in chunk.iter().enumerate() {
                            let entry = &self.library.books[idx];
                            let title = entry.title.clone();
                            let path = entry.path.clone();
                            let chapter = entry.last_chapter;
                            let palette = &COVER_PALETTE;
                            let hash = title
                                .bytes()
                                .fold(0u32, |acc, b| acc.wrapping_add(b as u32));
                            let cover_color = palette[(hash as usize) % palette.len()];
                            let card_id = ui.id().with(("card", idx));
                            let (card_rect, card_response) = ui.allocate_exact_size(
                                Vec2::new(card_width, card_height),
                                egui::Sense::click(),
                            );
                            let hovered = card_response.hovered();
                            let bg = if hovered { card_hover_bg } else { card_bg };
                            ui.painter()
                                .rect_filled(card_rect, CornerRadius::same(10), bg);
                            ui.painter().rect_stroke(
                                card_rect,
                                CornerRadius::same(10),
                                Stroke::new(
                                    1.0,
                                    if hovered {
                                        accent.linear_multiply(0.5)
                                    } else {
                                        border_color
                                    },
                                ),
                                StrokeKind::Outside,
                            );
                            let cover_rect = egui::Rect::from_min_max(
                                egui::pos2(card_rect.left() + 10.0, card_rect.top() + 8.0),
                                egui::pos2(
                                    card_rect.left() + 10.0 + cover_w,
                                    card_rect.bottom() - 8.0,
                                ),
                            );
                            let cover_rounding = CornerRadius::same(8);
                            let cover_texture =
                                self.cover_textures.get(&path).and_then(|t| t.as_ref());
                            if let Some(tex) = cover_texture {
                                ui.painter().rect_filled(
                                    cover_rect,
                                    cover_rounding,
                                    Color32::from_gray(30),
                                );
                                let tex_size = tex.size_vec2();
                                let scale = (cover_rect.width() / tex_size.x)
                                    .max(cover_rect.height() / tex_size.y);
                                let img_size = tex_size * scale;
                                let img_rect =
                                    egui::Rect::from_center_size(cover_rect.center(), img_size);
                                let clipped = ui.painter().with_clip_rect(cover_rect);
                                clipped.image(
                                    tex.id(),
                                    img_rect,
                                    egui::Rect::from_min_max(
                                        egui::pos2(0.0, 0.0),
                                        egui::pos2(1.0, 1.0),
                                    ),
                                    Color32::WHITE,
                                );
                            } else {
                                ui.painter()
                                    .rect_filled(cover_rect, cover_rounding, cover_color);
                                let first_char = title.chars().next().unwrap_or('📖').to_string();
                                ui.painter().text(
                                    cover_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    &first_char,
                                    egui::FontId::proportional(36.0),
                                    Color32::WHITE,
                                );
                            }
                            let content_rect = egui::Rect::from_min_max(
                                egui::pos2(cover_rect.right() + 12.0, card_rect.top() + 14.0),
                                egui::pos2(card_rect.right() - 12.0, card_rect.bottom() - 14.0),
                            );
                            let mut child = ui.new_child(
                                egui::UiBuilder::new()
                                    .id_salt(card_id)
                                    .max_rect(content_rect)
                                    .layout(egui::Layout::top_down(egui::Align::LEFT)),
                            );
                            // Clip the child painter to content_rect so widgets never
                            // paint outside the card border.
                            let parent_clip = ui.clip_rect();
                            child.set_clip_rect(parent_clip.intersect(content_rect));
                            child.add_space(4.0);
                            // Use egui's built-in single-line truncation so CJK titles
                            // never wrap regardless of card width.
                            child.add(
                                egui::Label::new(
                                    egui::RichText::new(&title)
                                        .size(15.0)
                                        .strong()
                                        .color(heading_color),
                                )
                                .truncate(),
                            );
                            child.add_space(6.0);
                            child.add(
                                egui::Label::new(
                                    egui::RichText::new(
                                        self.i18n.tf1(
                                            "library.last_read_chapter",
                                            entry
                                                .last_chapter_title
                                                .as_deref()
                                                .unwrap_or(&format!("{}", chapter + 1)),
                                        ),
                                    )
                                    .size(12.0)
                                    .color(subtitle_color),
                                )
                                .truncate(),
                            );
                            child.add_space(8.0);
                            child.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                let read_btn = egui::Button::new(
                                    egui::RichText::new(self.i18n.t("library.continue_reading"))
                                        .size(12.5)
                                        .color(Color32::WHITE),
                                )
                                .fill(accent)
                                .stroke(Stroke::NONE)
                                .corner_radius(CornerRadius::same(5))
                                .min_size(Vec2::new(0.0, 26.0));
                                if ui.add(read_btn).clicked() {
                                    action_open_path = Some((path.clone(), chapter));
                                }
                                let export_btn = egui::Button::new(
                                    egui::RichText::new("↗").size(12.0).color(subtitle_color),
                                )
                                .fill(Color32::TRANSPARENT)
                                .stroke(Stroke::new(1.0, border_color))
                                .corner_radius(CornerRadius::same(5))
                                .min_size(Vec2::new(24.0, 26.0));
                                if ui
                                    .add(export_btn)
                                    .on_hover_text(self.i18n.t("toolbar.export"))
                                    .clicked()
                                {
                                    action_export_path = Some(path.clone());
                                }
                                let del_btn = egui::Button::new(
                                    egui::RichText::new("🗑").size(12.0).color(subtitle_color),
                                )
                                .fill(Color32::TRANSPARENT)
                                .stroke(Stroke::new(1.0, border_color))
                                .corner_radius(CornerRadius::same(5))
                                .min_size(Vec2::new(24.0, 26.0));
                                if ui.add(del_btn).clicked() {
                                    action_remove_path = Some(path.clone());
                                }
                            });
                            if card_response.clicked() {
                                action_open_path = Some((path.clone(), chapter));
                            }
                            // Add gap between cards, not after the last one
                            if ci < chunk.len() - 1 {
                                ui.add_space(gap);
                            }
                        }
                    });
                    ui.add_space(gap);
                }
                ui.add_space(32.0);
            });
        }

        if action_open_dialog {
            self.open_file_dialog();
        }
        if let Some(path) = action_remove_path {
            self.push_feedback_log(format!("[Library] remove book: {}", path));
            self.cover_textures.remove(&path);
            self.library.remove_by_path(&self.data_dir, &path);
            if self.book_path.as_deref() == Some(path.as_str()) {
                self.book_path = None;
                self.current_book_hash = None;
                self.last_synced_chapter = None;
            }
        } else if let Some((path, chapter)) = action_open_path {
            self.open_book_from_path(&path, Some(chapter));
        }
        if let Some(path) = action_export_path {
            self.export_library_path = Some(path);
            self.show_export_dialog = true;
        }
    }
}

fn apply_rounded_corners_rgba(image: &mut image::RgbaImage, radius: u32) {
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 {
        return;
    }

    let r = radius.min(w / 2).min(h / 2) as i32;
    if r <= 0 {
        return;
    }

    let wi = w as i32;
    let hi = h as i32;
    let rr = r * r;

    for y in 0..hi {
        for x in 0..wi {
            let dx = if x < r {
                r - 1 - x
            } else if x >= wi - r {
                x - (wi - r)
            } else {
                0
            };

            let dy = if y < r {
                r - 1 - y
            } else if y >= hi - r {
                y - (hi - r)
            } else {
                0
            };

            if dx > 0 && dy > 0 && (dx * dx + dy * dy) > rr {
                let p = image.get_pixel_mut(x as u32, y as u32);
                p.0[3] = 0;
            }
        }
    }
}
