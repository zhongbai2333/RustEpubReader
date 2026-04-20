//! Review panel (段评覆盖层) — slides out from the right with a semi-transparent backdrop.
use std::cell::Cell;

use crate::app::ReaderApp;
use eframe::egui;

/// Estimate how many lines of text fit in the given width.
fn estimate_text_lines(text: &str, font_size: f32, content_width: f32) -> f32 {
    if text.is_empty() || content_width <= 0.0 {
        return 0.0;
    }
    // Conservative estimate for CJK / mixed text in egui's default font.
    let avg_char_width = font_size * 0.55;
    let chars_per_line = (content_width / avg_char_width).max(1.0);
    let text_len = text.chars().count() as f32;
    (text_len / chars_per_line).ceil().max(1.0)
}

/// Estimate the on-screen height of a content block inside the review panel.
fn estimate_block_height(
    block: &reader_core::epub::ContentBlock,
    font_size: f32,
    content_width: f32,
) -> f32 {
    match block {
        reader_core::epub::ContentBlock::Heading {
            level, spans, ..
        } => {
            let text: String = spans.iter().map(|s| s.text.as_str()).collect();
            let size = match level {
                1 => 18.0,
                2 => 16.0,
                _ => 14.0,
            };
            let lines = estimate_text_lines(&text, size, content_width);
            lines * size + 6.0 // trailing space
        }
        reader_core::epub::ContentBlock::Paragraph { spans, .. } => {
            let text: String = spans.iter().map(|s| s.text.as_str()).collect();
            if text.trim().is_empty() {
                return 0.0;
            }
            let lines = estimate_text_lines(&text, font_size, content_width);
            lines * font_size * 1.6 + 4.0
        }
        reader_core::epub::ContentBlock::Separator => 12.0,
        reader_core::epub::ContentBlock::BlankLine => font_size * 0.5,
        _ => 0.0,
    }
}

/// Compute scroll offset so the block matching `anchor_id` appears near the top.
fn compute_anchor_scroll_offset(
    blocks: &[reader_core::epub::ContentBlock],
    anchor_id: &str,
    font_size: f32,
    content_width: f32,
) -> Option<f32> {
    // The header (title bar, separator, chapter title) is rendered *outside* the
    // ScrollArea, so vertical_scroll_offset is relative to the ScrollArea content only.
    let mut offset = 0.0;
    for block in blocks {
        let is_match = match block {
            reader_core::epub::ContentBlock::Heading {
                anchor_id: Some(id),
                ..
            } => id == anchor_id,
            reader_core::epub::ContentBlock::Paragraph {
                anchor_id: Some(id),
                ..
            } => id == anchor_id,
            _ => false,
        };
        if is_match {
            return Some(offset);
        }
        offset += estimate_block_height(block, font_size, content_width);
    }
    None
}

impl ReaderApp {
    pub fn render_review_panel(&mut self, ctx: &egui::Context) {
        if !self.show_review_panel {
            return;
        }
        // Consume just_opened immediately so it never survives an early return.
        let was_just_opened = self.review_panel_just_opened;
        self.review_panel_just_opened = false;

        let Some(review_ch_idx) = self.review_panel_chapter else {
            return;
        };
        let chapter = self
            .book
            .as_ref()
            .and_then(|b| b.chapters.get(review_ch_idx));
        let Some(chapter) = chapter else {
            return;
        };

        let screen_rect = ctx.screen_rect();
        let panel_width = (screen_rect.width() * 0.42).max(360.0).min(500.0);
        let close = Cell::new(false);

        // ── Compute anchor scroll offset (only on the frame the panel opens) ──
        let content_width = panel_width - 32.0; // margin * 2
        let scroll_offset = if was_just_opened {
            self.review_panel_anchor.as_ref().and_then(|anchor| {
                compute_anchor_scroll_offset(&chapter.blocks, anchor, self.font_size, content_width)
            })
        } else {
            None
        };

        // ── Backdrop (semi-transparent black overlay) ──
        let backdrop_id = egui::Id::new("review_backdrop");
        let backdrop_resp = egui::Area::new(backdrop_id)
            .fixed_pos(screen_rect.min)
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_min_size(screen_rect.size());
                let rect = ui.max_rect();
                ui.painter()
                    .rect_filled(rect, 0.0, egui::Color32::from_black_alpha(140));
                ui.interact(rect, ui.id(), egui::Sense::click())
            });

        // ── Right-side sliding panel ──
        let panel_pos = egui::pos2(screen_rect.right() - panel_width, screen_rect.top());
        let panel_rect =
            egui::Rect::from_min_size(panel_pos, egui::vec2(panel_width, screen_rect.height()));

        egui::Area::new(egui::Id::new("review_panel"))
            .fixed_pos(panel_pos)
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                let bg = ctx.style().visuals.panel_fill;
                ui.painter().rect_filled(panel_rect, 0.0, bg);
                // Interact over the whole panel so clicks on margins/title don't fall through to the backdrop
                ui.interact(panel_rect, ui.id(), egui::Sense::click());

                // Shadow on left edge of panel
                let shadow_steps = 12u32;
                let shadow_w = 20.0f32;
                for i in 0..shadow_steps {
                    let alpha = ((shadow_steps - i) as f32 * 35.0 / shadow_steps as f32) as u8;
                    let x = panel_rect.left() + i as f32 * (shadow_w / shadow_steps as f32);
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(
                            egui::pos2(x, panel_rect.top()),
                            egui::vec2(shadow_w / shadow_steps as f32, panel_rect.height()),
                        ),
                        0.0,
                        egui::Color32::from_black_alpha(alpha),
                    );
                }

                let margin = 16.0;
                let content_rect = panel_rect.shrink2(egui::vec2(margin, margin));
                ui.set_clip_rect(panel_rect);

                ui.allocate_new_ui(
                    egui::UiBuilder::new().max_rect(content_rect),
                    |ui| {
                        // Header
                        ui.horizontal(|ui| {
                            ui.heading(self.i18n.t("review.panel_title"));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.button("✕").clicked() {
                                        close.set(true);
                                    }
                                },
                            );
                        });
                        ui.separator();

                        // Chapter title
                        ui.label(
                            egui::RichText::new(&chapter.title)
                                .size(14.0)
                                .color(egui::Color32::GRAY),
                        );
                        ui.add_space(8.0);

                        // Scrollable content
                        let mut scroll_area = egui::ScrollArea::vertical();
                        if let Some(y) = scroll_offset {
                            scroll_area = scroll_area.vertical_scroll_offset(y);
                        }
                        scroll_area.show(ui, |ui| {
                            for block in &chapter.blocks {
                                match block {
                                    reader_core::epub::ContentBlock::Heading {
                                        level,
                                        spans,
                                        ..
                                    } => {
                                        let text: String =
                                            spans.iter().map(|s| s.text.as_str()).collect();
                                        let size = match level {
                                            1 => 18.0,
                                            2 => 16.0,
                                            _ => 14.0,
                                        };
                                        ui.label(
                                            egui::RichText::new(&text).size(size).strong(),
                                        );
                                        ui.add_space(6.0);
                                    }
                                    reader_core::epub::ContentBlock::Paragraph { spans, .. } => {
                                        let text: String =
                                            spans.iter().map(|s| s.text.as_str()).collect();
                                        if !text.trim().is_empty() {
                                            // Render spans with link support
                                            ui.horizontal_wrapped(|ui| {
                                                for span in spans {
                                                    let mut label = egui::RichText::new(&span.text)
                                                        .size(self.font_size);
                                                    if span.link_url.is_some() {
                                                        label = label
                                                            .color(egui::Color32::from_rgb(
                                                                30, 80, 200,
                                                            ))
                                                            .underline();
                                                    }
                                                    let resp = ui.add(egui::Label::new(label).sense(
                                                        if span.link_url.is_some() {
                                                            egui::Sense::click()
                                                        } else {
                                                            egui::Sense::hover()
                                                        },
                                                    ));
                                                    if resp.clicked() {
                                                        if let Some(url) = &span.link_url {
                                                            let url = url.trim();
                                                            if url.starts_with('#') || url.is_empty() {
                                                                // "Back to main" link — close panel
                                                                close.set(true);
                                                            } else {
                                                                // External or other internal link
                                                                ctx.open_url(
                                                                    egui::OpenUrl::new_tab(url),
                                                                );
                                                            }
                                                        }
                                                    }
                                                }
                                            });
                                            ui.add_space(4.0);
                                        }
                                    }
                                    reader_core::epub::ContentBlock::Separator => {
                                        ui.separator();
                                        ui.add_space(4.0);
                                    }
                                    reader_core::epub::ContentBlock::BlankLine => {
                                        ui.add_space(self.font_size * 0.5);
                                    }
                                    _ => {}
                                }
                            }
                        });
                    },
                );
            });

        // Close on backdrop click (skip the frame it was just opened)
        if !was_just_opened && backdrop_resp.inner.clicked() {
            close.set(true);
        }

        if close.get() {
            self.show_review_panel = false;
            self.review_panel_chapter = None;
            self.review_panel_anchor = None;
        }
    }
}
