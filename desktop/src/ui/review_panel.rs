//! Review panel (段评覆盖层) — slides out from the right with a semi-transparent backdrop.
use std::cell::Cell;

use crate::app::ReaderApp;
use eframe::egui;

/// Parsed review card from a paragraph text like:
/// "1. 【内容】 作者：xxx | 时间：xxx | 赞：52"
struct ReviewCard {
    #[allow(dead_code)]
    index: usize,
    content: String,
    author: String,
    timestamp: String,
    likes: usize,
}

/// Try to parse a review paragraph into structured card data.
fn parse_review_card(text: &str) -> Option<ReviewCard> {
    let trimmed = text.trim();

    // Extract index: "1. ..."
    let dot_pos = trimmed.find('.')?;
    let index = trimmed[..dot_pos].trim().parse::<usize>().ok()?;
    let rest = trimmed[dot_pos + 1..].trim();

    // Split by " | " or " ｜ "
    let parts: Vec<&str> = rest.split(['|', '｜']).collect();
    if parts.len() < 3 {
        return None;
    }

    // Last part: "赞：52"
    let likes_part = parts.last()?.trim();
    let likes_str = likes_part
        .strip_prefix("赞：")
        .or_else(|| likes_part.strip_prefix("赞:"))?
        .trim();
    let likes = likes_str.parse::<usize>().ok()?;

    // Second-to-last part: "时间：1770036499"
    let time_part = parts[parts.len() - 2].trim();
    let timestamp = time_part
        .strip_prefix("时间：")
        .or_else(|| time_part.strip_prefix("时间:"))?
        .trim()
        .to_string();

    // First part: "【内容】 作者：吃草莓布丁吗"
    let first_part = parts[0].trim();
    const AUTHOR_FULL: &str = "作者："; // full-width colon (9 bytes)
    const AUTHOR_ASCII: &str = "作者:"; // ASCII colon (7 bytes)
    let (author_pos, author_marker_len) = if let Some(p) = first_part.rfind(AUTHOR_FULL) {
        (p, AUTHOR_FULL.len())
    } else if let Some(p) = first_part.rfind(AUTHOR_ASCII) {
        (p, AUTHOR_ASCII.len())
    } else {
        return None;
    };
    let content = first_part[..author_pos].trim().to_string();
    let author = first_part[author_pos + author_marker_len..]
        .trim()
        .to_string();

    Some(ReviewCard {
        index,
        content,
        author,
        timestamp,
        likes,
    })
}

/// Convert a Unix timestamp string (or any string) to human-readable format.
fn format_timestamp(s: &str) -> String {
    if let Ok(ts) = s.parse::<u64>() {
        format_unix_timestamp(ts)
    } else {
        s.to_string()
    }
}

fn format_unix_timestamp(ts: u64) -> String {
    const SECONDS_PER_DAY: u64 = 86400;
    let mut days = ts / SECONDS_PER_DAY;
    let rem = ts % SECONDS_PER_DAY;
    let mut year: u64 = 1970;
    loop {
        let dim = if is_leap_year(year) { 366 } else { 365 };
        if days < dim {
            break;
        }
        days -= dim;
        year += 1;
    }
    let month_days = [
        31,
        if is_leap_year(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1;
    let mut day = days as u32;
    for dim in month_days.iter() {
        if day < *dim {
            break;
        }
        day -= *dim;
        month += 1;
    }
    let hour = rem / 3600;
    let minute = (rem % 3600) / 60;
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        year,
        month,
        day + 1,
        hour,
        minute
    )
}

fn is_leap_year(y: u64) -> bool {
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
}

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
        reader_core::epub::ContentBlock::Heading { level, spans, .. } => {
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
        // ESC / Android back key closes the panel
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.show_review_panel = false;
            self.review_panel_chapter = None;
            self.review_panel_anchor = None;
            self.review_panel_scroll_offset = None;
            return;
        }
        // Consume just_opened immediately so it never survives an early return.
        let was_just_opened = self.review_panel_just_opened;
        self.review_panel_just_opened = false;

        // When opening from an anchor link, default to filtered view (only that paragraph)
        if was_just_opened && self.review_panel_anchor.is_some() {
            self.review_panel_show_all = false;
        }

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
        let panel_width = (screen_rect.width() * 0.42).clamp(360.0, 500.0);
        let close = Cell::new(false);

        // ── Compute anchor scroll offset (only on the frame the panel opens) ──
        let content_width = panel_width - 32.0; // margin * 2
        let scroll_offset = if was_just_opened && self.review_panel_show_all {
            self.review_panel_scroll_offset.take().or_else(|| {
                self.review_panel_anchor.as_ref().and_then(|anchor| {
                    compute_anchor_scroll_offset(
                        &chapter.blocks,
                        anchor,
                        self.font_size,
                        content_width,
                    )
                })
            })
        } else {
            None
        };

        // ── Backdrop (semi-transparent black overlay, visual only) ──
        // Backdrop does NOT register click interaction — close only via ✕ button or ESC.
        // This avoids egui focus/state issues that caused permanent input lock.
        egui::Area::new(egui::Id::new("review_backdrop"))
            .fixed_pos(screen_rect.min)
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                ui.set_min_size(screen_rect.size());
                let rect = ui.max_rect();
                ui.painter()
                    .rect_filled(rect, 0.0, egui::Color32::from_black_alpha(140));
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

                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(content_rect), |ui| {
                    // Header
                    ui.horizontal(|ui| {
                        ui.heading(self.i18n.t("review.panel_title"));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("✕").clicked() {
                                close.set(true);
                            }
                        });
                    });

                    // Filter toggle (only when opened from an anchor)
                    if self.review_panel_anchor.is_some() {
                        ui.horizontal(|ui| {
                            let label = if self.review_panel_show_all {
                                self.i18n.t("review.show_current_only")
                            } else {
                                self.i18n.t("review.show_all")
                            };
                            if ui.button(label).clicked() {
                                self.review_panel_show_all = !self.review_panel_show_all;
                            }
                        });
                    }
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
                    let anchor_filter = self.review_panel_anchor.clone();
                    let show_all = self.review_panel_show_all;

                    // Build filtered block list: when filtering, find the Heading that
                    // matches the anchor and include it plus all following non-Heading
                    // blocks until the next Heading (paragraph group).

                    let filtered_blocks: Vec<&reader_core::epub::ContentBlock> =
                        if let Some(ref filter) = anchor_filter {
                            if !show_all {
                                let mut result = Vec::new();
                                let mut in_group = false;
                                for block in &chapter.blocks {
                                    match block {
                                        reader_core::epub::ContentBlock::Heading {
                                            anchor_id,
                                            ..
                                        } => {
                                            if anchor_id.as_ref() == Some(filter) {
                                                in_group = true;
                                                result.push(block);
                                            } else if in_group {
                                                // Reached next heading — stop
                                                break;
                                            }
                                            // Before match: skip headings
                                        }
                                        reader_core::epub::ContentBlock::Paragraph {
                                            anchor_id,
                                            ..
                                        } => {
                                            if !in_group && anchor_id.as_ref() == Some(filter) {
                                                in_group = true;
                                            }
                                            if in_group {
                                                result.push(block);
                                            }
                                        }
                                        _ => {
                                            if in_group {
                                                result.push(block);
                                            }
                                        }
                                    }
                                }
                                // Fallback: show all blocks if no anchor match was found
                                if result.is_empty() {
                                    chapter.blocks.iter().collect()
                                } else {
                                    result
                                }
                            } else {
                                chapter.blocks.iter().collect()
                            }
                        } else {
                            chapter.blocks.iter().collect()
                        };

                    scroll_area.show(ui, |ui| {
                        for block in &filtered_blocks {
                            match block {
                                reader_core::epub::ContentBlock::Heading {
                                    level, spans, ..
                                } => {
                                    let text: String =
                                        spans.iter().map(|s| s.text.as_str()).collect();
                                    let size = match level {
                                        1 => 18.0,
                                        2 => 16.0,
                                        _ => 14.0,
                                    };
                                    ui.label(egui::RichText::new(&text).size(size).strong());
                                    ui.add_space(6.0);
                                }
                                reader_core::epub::ContentBlock::Paragraph { spans, .. } => {
                                    let text: String =
                                        spans.iter().map(|s| s.text.as_str()).collect();
                                    let trimmed = text.trim();
                                    if trimmed.is_empty() {
                                        continue;
                                    }

                                    // Try to render as review card
                                    if let Some(card) = parse_review_card(&text) {
                                        let card_bg = ui.visuals().extreme_bg_color;
                                        let frame = egui::Frame::new()
                                            .fill(card_bg)
                                            .corner_radius(6.0)
                                            .inner_margin(10.0)
                                            .stroke(ui.visuals().widgets.noninteractive.bg_stroke);
                                        frame.show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            // Author
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new(&card.author)
                                                        .size(13.0)
                                                        .color(egui::Color32::from_rgb(
                                                            64, 128, 200,
                                                        ))
                                                        .strong(),
                                                );
                                            });
                                            ui.add_space(4.0);
                                            // Content
                                            ui.label(egui::RichText::new(&card.content).size(14.0));
                                            ui.add_space(6.0);
                                            // Meta row: time + likes
                                            ui.horizontal(|ui| {
                                                let time_str = format_timestamp(&card.timestamp);
                                                ui.label(
                                                    egui::RichText::new(&time_str)
                                                        .size(11.0)
                                                        .color(egui::Color32::GRAY),
                                                );
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        ui.label(
                                                            egui::RichText::new(format!(
                                                                "❤ {}",
                                                                card.likes
                                                            ))
                                                            .size(11.0)
                                                            .color(egui::Color32::GRAY),
                                                        );
                                                    },
                                                );
                                            });
                                        });
                                        ui.add_space(8.0);
                                    } else {
                                        // Fallback: render as normal paragraph with link support
                                        ui.horizontal_wrapped(|ui| {
                                            for span in spans {
                                                let mut label = egui::RichText::new(&span.text)
                                                    .size(self.font_size);
                                                if span.link_url.is_some() {
                                                    label = label
                                                        .color(egui::Color32::from_rgb(30, 80, 200))
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
                                                            close.set(true);
                                                        } else {
                                                            ctx.open_url(egui::OpenUrl::new_tab(
                                                                url,
                                                            ));
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
                });
            });

        if close.get() {
            self.show_review_panel = false;
            self.review_panel_chapter = None;
            self.review_panel_anchor = None;
            self.review_panel_scroll_offset = None;
        }
    }
}
