//! The main reading interface, integrating text layout and UI overlays.
use std::sync::Arc;

use eframe::egui;
use egui::{Color32, FontId, UiBuilder};

use crate::app::{ReaderApp, TextSelection};
use reader_core::epub::ContentBlock;

use super::{reader_block::*, reader_state::*};

impl ReaderApp {
    pub fn recalculate_pages(&mut self, available_height: f32, max_width: f32) {
        set_spacing(
            self.line_spacing,
            self.para_spacing,
            self.text_indent as f32,
        );
        self.page_block_ranges.clear();
        if let Some(book) = &self.book {
            if let Some(chapter) = book.chapters.get(self.current_chapter) {
                let blocks = &chapter.blocks;
                let line_height = self.font_size * line_spacing();
                let mut page_start = 0;
                let mut current_h: f32 = 0.0;
                let first_is_heading = matches!(blocks.first(), Some(ContentBlock::Heading { .. }));
                let title_height = if first_is_heading {
                    TITLE_SPACING
                } else {
                    self.font_size * 2.0 + TITLE_SPACING
                };
                let usable = (available_height - FRAME_MARGIN).max(100.0);
                let mut first_page = true;
                for (i, block) in blocks.iter().enumerate() {
                    let bh = estimate_block_height(block, self.font_size, line_height, max_width);
                    let page_budget = if first_page {
                        usable - title_height
                    } else {
                        usable
                    };
                    if current_h + bh > page_budget && i > page_start {
                        self.page_block_ranges.push((page_start, i));
                        page_start = i;
                        current_h = 0.0;
                        first_page = false;
                    }
                    current_h += bh;
                }
                if page_start < blocks.len() {
                    self.page_block_ranges.push((page_start, blocks.len()));
                }
            }
        }
        self.total_pages = self.page_block_ranges.len().max(1);
        if self.current_page >= self.total_pages {
            self.current_page = self.total_pages.saturating_sub(1);
        }
        self.pages_dirty = false;
    }

    pub fn render_reader(&mut self, ui: &mut egui::Ui) {
        // Push current typography settings into thread-locals
        set_spacing(
            self.line_spacing,
            self.para_spacing,
            self.text_indent as f32,
        );

        // Clear per-frame block galley cache
        BLOCK_GALLEYS.with(|bg| bg.borrow_mut().clear());

        // Set TTS read-along highlight block
        if self.tts_playing && !self.tts_paused {
            TTS_HIGHLIGHT_BLOCK.set(Some(self.tts_current_block));
        } else {
            TTS_HIGHLIGHT_BLOCK.set(None);
        }

        // Set CSC corrections for the current chapter into thread-local
        CSC_CORRECTIONS.with(|csc| {
            let mut map = csc.borrow_mut();
            map.clear();
            if self.csc_mode != reader_core::csc::CorrectionMode::None {
                for ((ch, block_idx), corrs) in &self.csc_cache {
                    if *ch == self.current_chapter {
                        map.insert(*block_idx, corrs.clone());
                    }
                }
            }
        });
        CSC_READWRITE.set(self.csc_mode == reader_core::csc::CorrectionMode::ReadWrite);
        CSC_RECTS.with(|r| r.borrow_mut().clear());

        if self.page_anim_progress < 1.0 {
            self.page_anim_progress =
                (self.page_anim_progress + self.reader_page_animation_speed).min(1.0);
            // Request repaint after a short delay to cap animation frame rate (~60fps)
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }
        if self.page_anim_progress >= 1.0 && self.page_anim_cross_chapter {
            self.page_anim_cross_chapter = false;
            self.page_anim_cross_chapter_snapshot = None;
        }

        let effective_font_family = if self.defer_custom_font_for_frame
            && !matches!(
                self.reader_font_family.as_str(),
                "Sans" | "Serif" | "Monospace"
            ) {
            "Sans".to_string()
        } else {
            self.reader_font_family.clone()
        };

        let full_rect = ui.available_rect_before_wrap();
        if let Some(tex) = &self.reader_bg_texture {
            ui.painter().image(
                tex.id(),
                full_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::from_white_alpha((self.reader_bg_image_alpha * 255.0) as u8),
            );
        }

        let mut action_prev_chapter = false;
        let mut action_next_chapter = false;
        let mut action_go_back = false;
        let mut action_prev_page = false;
        let mut action_next_page = false;
        let mut clicked_link: Option<String> = None;
        let has_previous_chapter = self.previous_chapter.is_some();
        let mut is_dual_column = false;

        if let Some(book) = &self.book {
            if let Some(chapter) = book.chapters.get(self.current_chapter) {
                let available_width = ui.available_width();
                let available_height = ui.available_height();
                if (self.last_avail_width - available_width).abs() > 1.0
                    || (self.last_avail_height - available_height).abs() > 1.0
                {
                    self.pages_dirty = true;
                    self.last_avail_width = available_width;
                    self.last_avail_height = available_height;
                }
                let dual_column = !self.scroll_mode && available_width > DUAL_COLUMN_THRESHOLD;
                is_dual_column = dual_column;
                self.is_dual_column = dual_column;
                let (text_width, h_margin) = if dual_column {
                    let col_w = (available_width - DUAL_COLUMN_GAP) / 2.0;
                    let tw = (col_w - DUAL_COLUMN_PADDING).min(MAX_COLUMN_WIDTH);
                    let hm = ((col_w - tw) / 2.0).max(MIN_COLUMN_MARGIN);
                    (tw, hm)
                } else {
                    let hm = if available_width > MAX_TEXT_WIDTH_SINGLE {
                        (available_width - MAX_TEXT_WIDTH_SINGLE) / 2.0
                    } else {
                        SINGLE_MIN_MARGIN
                    };
                    let tw = MAX_TEXT_WIDTH_SINGLE.min(available_width - SINGLE_TEXT_PADDING);
                    (tw, hm)
                };
                let title = chapter.title.clone();
                let blocks = chapter.blocks.clone();
                let total_ch = book.chapters.len();
                if !self.scroll_mode && self.pages_dirty {
                    self.recalculate_pages(ui.available_height(), text_width);
                }
                if !self.scroll_mode
                    && self.total_pages > 0
                    && self.current_page >= self.total_pages
                {
                    self.current_page = self.total_pages - 1;
                }
                if dual_column && !self.current_page.is_multiple_of(2) {
                    self.current_page = self.current_page.saturating_sub(1);
                }
                let (block_start, block_end) = if self.scroll_mode {
                    (0, blocks.len())
                } else if let Some(&(s, e)) = self.page_block_ranges.get(self.current_page) {
                    (s.min(blocks.len()), e.min(blocks.len()))
                } else {
                    (0, blocks.len())
                };
                let show_title = self.scroll_mode || self.current_page == 0;

                // Build per-block highlight ranges for the current chapter
                // Each block can have multiple highlight ranges with different colors
                let highlight_ranges: std::collections::HashMap<
                    usize,
                    Vec<(usize, usize, reader_core::library::HighlightColor)>,
                > = self
                    .book_config
                    .as_ref()
                    .map(|cfg| {
                        let mut map: std::collections::HashMap<
                            usize,
                            Vec<(usize, usize, reader_core::library::HighlightColor)>,
                        > = std::collections::HashMap::new();
                        for h in cfg
                            .highlights
                            .iter()
                            .filter(|h| h.chapter == self.current_chapter)
                        {
                            // Only single-block highlights supported for now
                            map.entry(h.start_block).or_default().push((
                                h.start_offset,
                                h.end_offset,
                                h.color.clone(),
                            ));
                        }
                        map
                    })
                    .unwrap_or_default();

                if self.scroll_mode {
                    let mut scroll_area = egui::ScrollArea::vertical().auto_shrink([false; 2]);
                    if let Some(y) = self.anchor_scroll_offset.take() {
                        scroll_area = scroll_area.vertical_scroll_offset(y);
                    } else if self.scroll_to_top {
                        scroll_area = scroll_area.vertical_scroll_offset(0.0);
                        self.scroll_to_top = false;
                    }
                    scroll_area.show(ui, |ui| {
                        render_content_layout(
                            ui,
                            h_margin,
                            text_width,
                            &title,
                            &blocks,
                            block_start,
                            block_end,
                            show_title,
                            self.font_size,
                            self.reader_bg_color,
                            self.current_chapter,
                            total_ch,
                            &mut action_prev_chapter,
                            &mut action_next_chapter,
                            &mut action_go_back,
                            true,
                            has_previous_chapter,
                            self.reader_font_color,
                            &effective_font_family,
                            &self.i18n,
                            &mut clicked_link,
                            &highlight_ranges,
                        );
                    });
                } else {
                    let page_rect = ui.available_rect_before_wrap();
                    self.paging_page_rect = Some(page_rect);
                    if dual_column {
                        let col_w = (page_rect.width() - DUAL_COLUMN_GAP) / 2.0;
                        let left_rect = egui::Rect::from_min_size(
                            page_rect.min,
                            egui::vec2(col_w, page_rect.height()),
                        );
                        let right_rect = egui::Rect::from_min_size(
                            egui::pos2(page_rect.min.x + col_w + DUAL_COLUMN_GAP, page_rect.min.y),
                            egui::vec2(col_w, page_rect.height()),
                        );
                        let right_page = self.current_page + 1;
                        let is_anim_dual = self.reader_page_animation != "None"
                            && self.page_anim_progress < 1.0
                            && (self.page_anim_from != self.page_anim_to
                                || self.page_anim_cross_chapter);
                        if is_anim_dual {
                            let t = self.page_anim_progress;
                            let w = page_rect.width();
                            let dir = self.page_anim_direction;
                            let to_offset = egui::vec2(dir * (1.0 - t) * w, 0.0);
                            // "from" spread (sliding out, or static for Cover)
                            {
                                let from_offset = if self.reader_page_animation == "Cover" {
                                    egui::vec2(0.0, 0.0)
                                } else {
                                    egui::vec2(-dir * t * w, 0.0)
                                };
                                if let Some(snap) = &self.page_anim_cross_chapter_snapshot {
                                    let snap_blocks = Arc::clone(&snap.blocks);
                                    let snap_ranges = snap.block_ranges.clone();
                                    let snap_total = snap.total_pages;
                                    let snap_from = snap.from_page;
                                    let snap_title = snap.title.clone();
                                    let from_raw = snap_from.min(snap_total.saturating_sub(1));
                                    let from_left = (from_raw / 2) * 2;
                                    let (fls, fle) = snap_ranges
                                        .get(from_left)
                                        .copied()
                                        .map(|(s, e)| {
                                            (s.min(snap_blocks.len()), e.min(snap_blocks.len()))
                                        })
                                        .unwrap_or((0, snap_blocks.len()));
                                    let left_from_rect = left_rect.translate(from_offset);
                                    ui.allocate_new_ui(
                                        UiBuilder::new().max_rect(left_from_rect),
                                        |ui| {
                                            let clip = left_from_rect.intersect(page_rect);
                                            ui.set_clip_rect(clip);
                                            ui.painter().rect_filled(
                                                clip,
                                                0.0,
                                                self.reader_bg_color,
                                            );
                                            render_content_layout(
                                                ui,
                                                h_margin,
                                                text_width,
                                                &snap_title,
                                                &snap_blocks,
                                                fls,
                                                fle,
                                                from_left == 0,
                                                self.font_size,
                                                self.reader_bg_color,
                                                self.current_chapter,
                                                total_ch,
                                                &mut action_prev_chapter,
                                                &mut action_next_chapter,
                                                &mut action_go_back,
                                                false,
                                                has_previous_chapter,
                                                self.reader_font_color,
                                                &effective_font_family,
                                                &self.i18n,
                                                &mut clicked_link,
                                                &highlight_ranges,
                                            );
                                        },
                                    );
                                    let from_right = from_left + 1;
                                    if from_right < snap_total {
                                        let (frs, fre) = snap_ranges
                                            .get(from_right)
                                            .copied()
                                            .map(|(s, e)| {
                                                (s.min(snap_blocks.len()), e.min(snap_blocks.len()))
                                            })
                                            .unwrap_or((0, 0));
                                        let right_from_rect = right_rect.translate(from_offset);
                                        ui.allocate_new_ui(
                                            UiBuilder::new().max_rect(right_from_rect),
                                            |ui| {
                                                let clip = right_from_rect.intersect(page_rect);
                                                ui.set_clip_rect(clip);
                                                ui.painter().rect_filled(
                                                    clip,
                                                    0.0,
                                                    self.reader_bg_color,
                                                );
                                                render_content_layout(
                                                    ui,
                                                    h_margin,
                                                    text_width,
                                                    &snap_title,
                                                    &snap_blocks,
                                                    frs,
                                                    fre,
                                                    false,
                                                    self.font_size,
                                                    self.reader_bg_color,
                                                    self.current_chapter,
                                                    total_ch,
                                                    &mut action_prev_chapter,
                                                    &mut action_next_chapter,
                                                    &mut action_go_back,
                                                    false,
                                                    has_previous_chapter,
                                                    self.reader_font_color,
                                                    &effective_font_family,
                                                    &self.i18n,
                                                    &mut clicked_link,
                                                    &highlight_ranges,
                                                );
                                            },
                                        );
                                    }
                                } else {
                                    let from_raw =
                                        self.page_anim_from.min(self.total_pages.saturating_sub(1));
                                    let from_left = (from_raw / 2) * 2;
                                    let (fls, fle) = self
                                        .page_block_ranges
                                        .get(from_left)
                                        .copied()
                                        .map(|(s, e)| (s.min(blocks.len()), e.min(blocks.len())))
                                        .unwrap_or((0, blocks.len()));
                                    let left_from_rect = left_rect.translate(from_offset);
                                    ui.allocate_new_ui(
                                        UiBuilder::new().max_rect(left_from_rect),
                                        |ui| {
                                            let clip = left_from_rect.intersect(page_rect);
                                            ui.set_clip_rect(clip);
                                            ui.painter().rect_filled(
                                                clip,
                                                0.0,
                                                self.reader_bg_color,
                                            );
                                            render_content_layout(
                                                ui,
                                                h_margin,
                                                text_width,
                                                &title,
                                                &blocks,
                                                fls,
                                                fle,
                                                from_left == 0,
                                                self.font_size,
                                                self.reader_bg_color,
                                                self.current_chapter,
                                                total_ch,
                                                &mut action_prev_chapter,
                                                &mut action_next_chapter,
                                                &mut action_go_back,
                                                false,
                                                has_previous_chapter,
                                                self.reader_font_color,
                                                &effective_font_family,
                                                &self.i18n,
                                                &mut clicked_link,
                                                &highlight_ranges,
                                            );
                                        },
                                    );
                                    let from_right = from_left + 1;
                                    if from_right < self.total_pages {
                                        let (frs, fre) = self
                                            .page_block_ranges
                                            .get(from_right)
                                            .copied()
                                            .map(|(s, e)| {
                                                (s.min(blocks.len()), e.min(blocks.len()))
                                            })
                                            .unwrap_or((0, 0));
                                        let right_from_rect = right_rect.translate(from_offset);
                                        ui.allocate_new_ui(
                                            UiBuilder::new().max_rect(right_from_rect),
                                            |ui| {
                                                let clip = right_from_rect.intersect(page_rect);
                                                ui.set_clip_rect(clip);
                                                ui.painter().rect_filled(
                                                    clip,
                                                    0.0,
                                                    self.reader_bg_color,
                                                );
                                                render_content_layout(
                                                    ui,
                                                    h_margin,
                                                    text_width,
                                                    &title,
                                                    &blocks,
                                                    frs,
                                                    fre,
                                                    false,
                                                    self.font_size,
                                                    self.reader_bg_color,
                                                    self.current_chapter,
                                                    total_ch,
                                                    &mut action_prev_chapter,
                                                    &mut action_next_chapter,
                                                    &mut action_go_back,
                                                    false,
                                                    has_previous_chapter,
                                                    self.reader_font_color,
                                                    &effective_font_family,
                                                    &self.i18n,
                                                    &mut clicked_link,
                                                    &highlight_ranges,
                                                );
                                            },
                                        );
                                    }
                                }

                                // Cover animation: shadow on leading edge of incoming spread
                                if self.reader_page_animation == "Cover" {
                                    let to_rect_pos = left_rect.translate(to_offset);
                                    let shadow_w = 28.0f32;
                                    let steps = 8u32;
                                    for i in 0..steps {
                                        let sub_w = shadow_w / steps as f32;
                                        let (sub_x, alpha_val) = if dir > 0.0 {
                                            let x =
                                                to_rect_pos.left() - shadow_w + i as f32 * sub_w;
                                            let a = ((i + 1) as f32 * 70.0 / steps as f32) as u8;
                                            (x, a)
                                        } else {
                                            let x = to_rect_pos.right()
                                                + (page_rect.width() - left_rect.width())
                                                + i as f32 * sub_w;
                                            let a =
                                                ((steps - i) as f32 * 70.0 / steps as f32) as u8;
                                            (x, a)
                                        };
                                        let sub_rect = egui::Rect::from_min_size(
                                            egui::pos2(sub_x, page_rect.top()),
                                            egui::vec2(sub_w, page_rect.height()),
                                        );
                                        ui.painter().rect_filled(
                                            sub_rect,
                                            0.0,
                                            Color32::from_black_alpha(alpha_val),
                                        );
                                    }
                                }
                            }
                            // "to" spread (sliding in)
                            let to_raw = self.page_anim_to.min(self.total_pages.saturating_sub(1));
                            let to_left = (to_raw / 2) * 2;
                            let (tls, tle) = self
                                .page_block_ranges
                                .get(to_left)
                                .copied()
                                .map(|(s, e)| (s.min(blocks.len()), e.min(blocks.len())))
                                .unwrap_or((0, blocks.len()));
                            let left_to_rect = left_rect.translate(to_offset);
                            ui.allocate_new_ui(UiBuilder::new().max_rect(left_to_rect), |ui| {
                                let clip = left_to_rect.intersect(page_rect);
                                ui.set_clip_rect(clip);
                                ui.painter().rect_filled(clip, 0.0, self.reader_bg_color);
                                render_content_layout(
                                    ui,
                                    h_margin,
                                    text_width,
                                    &title,
                                    &blocks,
                                    tls,
                                    tle,
                                    to_left == 0,
                                    self.font_size,
                                    self.reader_bg_color,
                                    self.current_chapter,
                                    total_ch,
                                    &mut action_prev_chapter,
                                    &mut action_next_chapter,
                                    &mut action_go_back,
                                    false,
                                    has_previous_chapter,
                                    self.reader_font_color,
                                    &effective_font_family,
                                    &self.i18n,
                                    &mut clicked_link,
                                    &highlight_ranges,
                                );
                            });
                            let to_right = to_left + 1;
                            if to_right < self.total_pages {
                                let (trs, tre) = self
                                    .page_block_ranges
                                    .get(to_right)
                                    .copied()
                                    .map(|(s, e)| (s.min(blocks.len()), e.min(blocks.len())))
                                    .unwrap_or((0, 0));
                                let right_to_rect = right_rect.translate(to_offset);
                                ui.allocate_new_ui(
                                    UiBuilder::new().max_rect(right_to_rect),
                                    |ui| {
                                        let clip = right_to_rect.intersect(page_rect);
                                        ui.set_clip_rect(clip);
                                        ui.painter().rect_filled(clip, 0.0, self.reader_bg_color);
                                        render_content_layout(
                                            ui,
                                            h_margin,
                                            text_width,
                                            &title,
                                            &blocks,
                                            trs,
                                            tre,
                                            false,
                                            self.font_size,
                                            self.reader_bg_color,
                                            self.current_chapter,
                                            total_ch,
                                            &mut action_prev_chapter,
                                            &mut action_next_chapter,
                                            &mut action_go_back,
                                            false,
                                            has_previous_chapter,
                                            self.reader_font_color,
                                            &effective_font_family,
                                            &self.i18n,
                                            &mut clicked_link,
                                            &highlight_ranges,
                                        );
                                    },
                                );
                            }
                        } else {
                            ui.allocate_new_ui(UiBuilder::new().max_rect(left_rect), |ui| {
                                render_content_layout(
                                    ui,
                                    h_margin,
                                    text_width,
                                    &title,
                                    &blocks,
                                    block_start,
                                    block_end,
                                    show_title,
                                    self.font_size,
                                    self.reader_bg_color,
                                    self.current_chapter,
                                    total_ch,
                                    &mut action_prev_chapter,
                                    &mut action_next_chapter,
                                    &mut action_go_back,
                                    false,
                                    has_previous_chapter,
                                    self.reader_font_color,
                                    &effective_font_family,
                                    &self.i18n,
                                    &mut clicked_link,
                                    &highlight_ranges,
                                );
                            });
                            if right_page < self.total_pages {
                                let (rs, re) =
                                    if let Some(&(s, e)) = self.page_block_ranges.get(right_page) {
                                        (s.min(blocks.len()), e.min(blocks.len()))
                                    } else {
                                        (0, 0)
                                    };
                                ui.allocate_new_ui(UiBuilder::new().max_rect(right_rect), |ui| {
                                    render_content_layout(
                                        ui,
                                        h_margin,
                                        text_width,
                                        &title,
                                        &blocks,
                                        rs,
                                        re,
                                        right_page == 0,
                                        self.font_size,
                                        self.reader_bg_color,
                                        self.current_chapter,
                                        total_ch,
                                        &mut action_prev_chapter,
                                        &mut action_next_chapter,
                                        &mut action_go_back,
                                        false,
                                        has_previous_chapter,
                                        self.reader_font_color,
                                        &effective_font_family,
                                        &self.i18n,
                                        &mut clicked_link,
                                        &highlight_ranges,
                                    );
                                });
                            }
                        }
                        if !is_anim_dual {
                            let sep_x = page_rect.min.x + col_w + DUAL_COLUMN_GAP / 2.0;
                            ui.painter().line_segment(
                                [
                                    egui::pos2(sep_x, page_rect.top() + 20.0),
                                    egui::pos2(sep_x, page_rect.bottom() - 20.0),
                                ],
                                egui::Stroke::new(1.0, Color32::from_gray(80)),
                            );
                        }
                        let page_info = if right_page < self.total_pages {
                            format!(
                                "{}-{} / {}",
                                self.current_page + 1,
                                right_page + 1,
                                self.total_pages
                            )
                        } else {
                            format!("{} / {}", self.current_page + 1, self.total_pages)
                        };
                        ui.painter().text(
                            egui::pos2(page_rect.right() - 20.0, page_rect.top() + 8.0),
                            egui::Align2::RIGHT_TOP,
                            page_info,
                            FontId::proportional(13.0),
                            Color32::GRAY,
                        );
                        ui.painter().text(
                            egui::pos2(page_rect.right() - 20.0, page_rect.bottom() - 8.0),
                            egui::Align2::RIGHT_BOTTOM,
                            self.i18n.tf2(
                                "reader.chapter_indicator",
                                &(self.current_chapter + 1).to_string(),
                                &total_ch.to_string(),
                            ),
                            FontId::proportional(13.0),
                            Color32::GRAY,
                        );
                    } else {
                        let is_animating = self.reader_page_animation != "None"
                            && self.page_anim_progress < 1.0
                            && (self.page_anim_from != self.page_anim_to
                                || self.page_anim_cross_chapter);

                        if is_animating {
                            let t = self.page_anim_progress;
                            let w = page_rect.width();
                            let dir = self.page_anim_direction;
                            let to_offset = egui::vec2(dir * (1.0 - t) * w, 0.0);

                            let to_idx = self.page_anim_to.min(self.total_pages.saturating_sub(1));
                            let (ts, te) = self
                                .page_block_ranges
                                .get(to_idx)
                                .copied()
                                .unwrap_or((0, blocks.len()));

                            {
                                let from_offset = if self.reader_page_animation == "Cover" {
                                    egui::vec2(0.0, 0.0)
                                } else {
                                    egui::vec2(-dir * t * w, 0.0)
                                };
                                if let Some(snap) = &self.page_anim_cross_chapter_snapshot {
                                    let snap_blocks = Arc::clone(&snap.blocks);
                                    let snap_ranges = snap.block_ranges.clone();
                                    let snap_total = snap.total_pages;
                                    let snap_from = snap.from_page;
                                    let snap_title = snap.title.clone();
                                    let from_idx = snap_from.min(snap_total.saturating_sub(1));
                                    let (fs, fe) = snap_ranges
                                        .get(from_idx)
                                        .copied()
                                        .unwrap_or((0, snap_blocks.len()));
                                    let from_rect = page_rect.translate(from_offset);
                                    ui.allocate_new_ui(
                                        UiBuilder::new().max_rect(from_rect),
                                        |ui| {
                                            let clip = from_rect.intersect(page_rect);
                                            ui.set_clip_rect(clip);
                                            ui.painter().rect_filled(
                                                clip,
                                                0.0,
                                                self.reader_bg_color,
                                            );
                                            render_content_layout(
                                                ui,
                                                h_margin,
                                                text_width,
                                                &snap_title,
                                                &snap_blocks,
                                                fs.min(snap_blocks.len()),
                                                fe.min(snap_blocks.len()),
                                                from_idx == 0,
                                                self.font_size,
                                                self.reader_bg_color,
                                                self.current_chapter,
                                                total_ch,
                                                &mut action_prev_chapter,
                                                &mut action_next_chapter,
                                                &mut action_go_back,
                                                false,
                                                has_previous_chapter,
                                                self.reader_font_color,
                                                &effective_font_family,
                                                &self.i18n,
                                                &mut clicked_link,
                                                &highlight_ranges,
                                            );
                                        },
                                    );
                                } else {
                                    let from_idx =
                                        self.page_anim_from.min(self.total_pages.saturating_sub(1));
                                    let (fs, fe) = self
                                        .page_block_ranges
                                        .get(from_idx)
                                        .copied()
                                        .unwrap_or((0, blocks.len()));
                                    let from_rect = page_rect.translate(from_offset);
                                    ui.allocate_new_ui(
                                        UiBuilder::new().max_rect(from_rect),
                                        |ui| {
                                            let clip = from_rect.intersect(page_rect);
                                            ui.set_clip_rect(clip);
                                            ui.painter().rect_filled(
                                                clip,
                                                0.0,
                                                self.reader_bg_color,
                                            );
                                            render_content_layout(
                                                ui,
                                                h_margin,
                                                text_width,
                                                &title,
                                                &blocks,
                                                fs.min(blocks.len()),
                                                fe.min(blocks.len()),
                                                from_idx == 0,
                                                self.font_size,
                                                self.reader_bg_color,
                                                self.current_chapter,
                                                total_ch,
                                                &mut action_prev_chapter,
                                                &mut action_next_chapter,
                                                &mut action_go_back,
                                                false,
                                                has_previous_chapter,
                                                self.reader_font_color,
                                                &effective_font_family,
                                                &self.i18n,
                                                &mut clicked_link,
                                                &highlight_ranges,
                                            );
                                        },
                                    );
                                }

                                // Cover animation: draw shadow on leading edge of incoming page
                                if self.reader_page_animation == "Cover" {
                                    let to_rect_pos = page_rect.translate(to_offset);
                                    let shadow_w = 28.0f32;
                                    let steps = 8u32;
                                    for i in 0..steps {
                                        let sub_w = shadow_w / steps as f32;
                                        let (sub_x, alpha_val) = if dir > 0.0 {
                                            let x =
                                                to_rect_pos.left() - shadow_w + i as f32 * sub_w;
                                            let a = ((i + 1) as f32 * 70.0 / steps as f32) as u8;
                                            (x, a)
                                        } else {
                                            let x = to_rect_pos.right() + i as f32 * sub_w;
                                            let a =
                                                ((steps - i) as f32 * 70.0 / steps as f32) as u8;
                                            (x, a)
                                        };
                                        let sub_rect = egui::Rect::from_min_size(
                                            egui::pos2(sub_x, page_rect.top()),
                                            egui::vec2(sub_w, page_rect.height()),
                                        );
                                        ui.painter().rect_filled(
                                            sub_rect,
                                            0.0,
                                            Color32::from_black_alpha(alpha_val),
                                        );
                                    }
                                }
                            }

                            let to_rect = page_rect.translate(to_offset);

                            ui.allocate_new_ui(UiBuilder::new().max_rect(to_rect), |ui| {
                                let clip = to_rect.intersect(page_rect);
                                ui.set_clip_rect(clip);
                                ui.painter().rect_filled(clip, 0.0, self.reader_bg_color);
                                render_content_layout(
                                    ui,
                                    h_margin,
                                    text_width,
                                    &title,
                                    &blocks,
                                    ts.min(blocks.len()),
                                    te.min(blocks.len()),
                                    to_idx == 0,
                                    self.font_size,
                                    self.reader_bg_color,
                                    self.current_chapter,
                                    total_ch,
                                    &mut action_prev_chapter,
                                    &mut action_next_chapter,
                                    &mut action_go_back,
                                    false,
                                    has_previous_chapter,
                                    self.reader_font_color,
                                    &effective_font_family,
                                    &self.i18n,
                                    &mut clicked_link,
                                    &highlight_ranges,
                                );
                            });
                        } else {
                            render_content_layout(
                                ui,
                                h_margin,
                                text_width,
                                &title,
                                &blocks,
                                block_start,
                                block_end,
                                show_title,
                                self.font_size,
                                self.reader_bg_color,
                                self.current_chapter,
                                total_ch,
                                &mut action_prev_chapter,
                                &mut action_next_chapter,
                                &mut action_go_back,
                                false,
                                has_previous_chapter,
                                self.reader_font_color,
                                &effective_font_family,
                                &self.i18n,
                                &mut clicked_link,
                                &highlight_ranges,
                            );
                        }
                        ui.painter().text(
                            egui::pos2(page_rect.right() - 20.0, page_rect.top() + 8.0),
                            egui::Align2::RIGHT_TOP,
                            format!("{} / {}", self.current_page + 1, self.total_pages),
                            FontId::proportional(13.0),
                            Color32::GRAY,
                        );
                        ui.painter().text(
                            egui::pos2(page_rect.right() - 20.0, page_rect.bottom() - 8.0),
                            egui::Align2::RIGHT_BOTTOM,
                            self.i18n.tf2(
                                "reader.chapter_indicator",
                                &(self.current_chapter + 1).to_string(),
                                &total_ch.to_string(),
                            ),
                            FontId::proportional(13.0),
                            Color32::GRAY,
                        );
                    }
                    if !self.show_sharing_panel
                        && !self.show_stats
                        && !self.show_export_dialog
                        && self.text_selection.is_none()
                        && self.clicked_highlight_id.is_none()
                        && self.csc_popup.is_none()
                        && !self.csc_custom_replace_active
                        && !self.show_review_panel
                    {
                        let pointer_in_page = ui.input(|i| {
                            i.pointer
                                .hover_pos()
                                .map(|pos| page_rect.contains(pos))
                                .unwrap_or(false)
                        });
                        if pointer_in_page {
                            let scroll = ui.input(|i| i.raw_scroll_delta.y);
                            if scroll < -30.0 {
                                action_next_page = true;
                            } else if scroll > 30.0 {
                                action_prev_page = true;
                            }
                        }
                        // Click-to-turn is handled in the selection release handler
                        // to avoid conflict with sel_press_origin
                        if !self.show_review_panel
                            && clicked_link.is_none()
                            && self.sel_press_origin.is_none()
                            && ui.input(|i| i.pointer.primary_clicked())
                        {
                            // Check if click hits a CSC correction (skip page turn if so)
                            let hit_csc = CSC_RECTS.with(|rects| {
                                if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                                    rects.borrow().iter().any(|cr| cr.rect.contains(pos))
                                } else {
                                    false
                                }
                            });
                            if !hit_csc {
                                if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                                    if page_rect.contains(pos) {
                                        if pos.x < page_rect.center().x {
                                            action_prev_page = true;
                                        } else {
                                            action_next_page = true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            ui.centered_and_justified(|ui| {
                ui.label(
                    egui::RichText::new(self.i18n.t("reader.select_book_hint"))
                        .size(24.0)
                        .color(Color32::from_gray(128)),
                );
            });
        }

        if action_prev_chapter {
            self.prev_chapter();
        }
        if action_next_chapter {
            self.next_chapter();
        }
        if action_go_back {
            if let Some(prev) = self.previous_chapter.take() {
                let total = self.total_chapters();
                if total > 0 {
                    self.current_chapter = prev.min(total - 1);
                    self.scroll_to_top = true;
                    self.pages_dirty = true;
                    self.current_page = 0;
                    if let Some(p) = &self.book_path {
                        let chap_title = self
                            .book
                            .as_ref()
                            .and_then(|b| b.chapters.get(self.current_chapter))
                            .map(|c| c.title.clone());
                        self.library.update_chapter(
                            &self.data_dir,
                            p,
                            self.current_chapter,
                            chap_title,
                        );
                    }
                }
            }
        }
        if let Some(url) = clicked_link {
            let url = url.trim().to_string();
            let lowered = url.to_lowercase();
            if lowered.starts_with("http://")
                || lowered.starts_with("https://")
                || lowered.starts_with("mailto:")
                || lowered.starts_with("tel:")
            {
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            } else if url.starts_with('#') {
                let anchor = &url[1..];
                if let Some(book) = &self.book {
                    if let Some(chapter) = book.chapters.get(self.current_chapter) {
                        if let Some(block_idx) = chapter.blocks.iter().position(|block| {
                            match block {
                                ContentBlock::Heading { anchor_id: Some(id), .. } => id == anchor,
                                ContentBlock::Paragraph { anchor_id: Some(id), .. } => id == anchor,
                                _ => false,
                            }
                        }) {
                            if self.scroll_mode {
                                let available_width = ui.available_width();
                                let text_width = if available_width > DUAL_COLUMN_THRESHOLD {
                                    let col_w = (available_width - DUAL_COLUMN_GAP) / 2.0;
                                    (col_w - DUAL_COLUMN_PADDING).min(MAX_COLUMN_WIDTH)
                                } else {
                                    MAX_TEXT_WIDTH_SINGLE.min(available_width - SINGLE_TEXT_PADDING)
                                };
                                let line_height = self.font_size * line_spacing();
                                let mut offset = 0.0;
                                for (i, block) in chapter.blocks.iter().enumerate() {
                                    if i >= block_idx {
                                        break;
                                    }
                                    offset += estimate_block_height(
                                        block,
                                        self.font_size,
                                        line_height,
                                        text_width,
                                    );
                                }
                                self.anchor_scroll_offset = Some(offset);
                            } else {
                                for (page_idx, (start, end)) in
                                    self.page_block_ranges.iter().enumerate()
                                {
                                    if block_idx >= *start && block_idx < *end {
                                        self.current_page = page_idx;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                let normalized = normalize_epub_href(&url);
                let target_idx = if !normalized.is_empty() {
                    self.book.as_ref().and_then(|book| {
                        book.chapters.iter().position(|ch| {
                            let Some(ref src) = ch.source_href else {
                                return false;
                            };
                            let src_norm = normalize_epub_href(src);
                            src_norm == normalized
                                || src_norm.ends_with(&format!("/{normalized}"))
                                || normalized.ends_with(&format!("/{src_norm}"))
                        })
                    })
                } else {
                    None
                };
                if let Some(idx) = target_idx {
                    // Check if target is a review chapter (段评) — show overlay instead of navigating
                    if self.book.as_ref().map_or(false, |b| b.review_chapter_indices.contains(&idx)) {
                        self.show_review_panel = true;
                        self.review_panel_chapter = Some(idx);
                        self.review_panel_anchor = url.split('#').nth(1).map(|s| s.to_string());
                        self.review_panel_just_opened = true;
                        // Clear any open highlight popup / selection so they don't fight for focus
                        self.clicked_highlight_id = None;
                        self.hl_note_popup_rect = None;
                        self.text_selection = None;
                        self.sel_press_origin = None;
                    } else {
                        self.current_chapter = idx;
                        self.current_page = 0;
                        self.scroll_to_top = true;
                        self.pages_dirty = true;
                    }
                } else {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                }
            }
        }
        if action_prev_page {
            if is_dual_column {
                if self.current_page >= 2 {
                    self.trigger_page_animation_to(self.current_page - 2, -1.0);
                } else if self.current_chapter > 0 {
                    self.capture_cross_chapter_snapshot();
                    self.prev_chapter();
                    self.current_page = usize::MAX;
                    self.start_cross_chapter_animation(-1.0);
                }
            } else {
                self.prev_page();
            }
        }
        if action_next_page {
            if is_dual_column {
                if self.current_page + 2 < self.total_pages {
                    self.trigger_page_animation_to(self.current_page + 2, 1.0);
                } else {
                    self.capture_cross_chapter_snapshot();
                    self.next_chapter();
                    self.start_cross_chapter_animation(1.0);
                }
            } else {
                self.next_page();
            }
        }

        // ── Custom text selection state machine ──
        let block_galleys: Vec<BlockGalleyEntry> =
            BLOCK_GALLEYS.with(|bg| bg.borrow_mut().drain(..).collect());

        let primary_down = ui.ctx().input(|i| i.pointer.primary_down());
        let pointer_pos = ui.ctx().input(|i| i.pointer.interact_pos());

        // Detect primary pointer press / drag / release for selection
        if !self.show_review_panel {
            let primary_pressed = ui.ctx().input(|i| i.pointer.primary_pressed());
            let primary_released = ui.ctx().input(|i| i.pointer.primary_released());

        // Helper: find which block a screen position falls into and return (block_idx, char_offset)
        let hit_test = |pos: egui::Pos2| -> Option<(usize, usize)> {
            for (idx, galley, rect, _text) in &block_galleys {
                if rect.contains(pos) {
                    let local = egui::vec2(pos.x - rect.min.x, pos.y - rect.min.y);
                    let cursor = galley.cursor_from_pos(local);
                    return Some((*idx, cursor.ccursor.index));
                }
            }
            None
        };

        // Check if pointer is over a toolbar area (so we don't start selection there)
        let toolbar_id = egui::Id::new("sel_toolbar");
        let note_toolbar_id = egui::Id::new("hl_note_toolbar");
        // Use cached popup rect for reliable hit-testing (layer_id_at is unreliable in egui 0.29)
        let over_popup_rect = self
            .hl_note_popup_rect
            .is_some_and(|rect| pointer_pos.is_some_and(|pos| rect.contains(pos)));
        let over_toolbar = over_popup_rect
            || ui.ctx().memory(|mem| {
                mem.layer_id_at(pointer_pos.unwrap_or_default())
                    .is_some_and(|layer| layer.id == toolbar_id || layer.id == note_toolbar_id)
            });

        if let Some(pos) = pointer_pos {
            const DRAG_THRESHOLD: f32 = 5.0;

            if primary_pressed && !over_toolbar {
                if let Some((block_idx, char_idx)) = hit_test(pos) {
                    // Record press origin; don't create TextSelection yet
                    self.sel_press_origin = Some((pos, block_idx, char_idx));
                    // Clear any existing finalized selection
                    if self.text_selection.as_ref().is_some_and(|s| !s.is_dragging) {
                        self.text_selection = None;
                    }
                    // Don't clear clicked_highlight_id here — let the popup's
                    // own close detection handle it on primary_clicked (release).
                } else {
                    // Clicked outside any block → clear selection state
                    self.sel_press_origin = None;
                    self.text_selection = None;
                    // Don't clear clicked_highlight_id — popup close detection
                    // will handle it on primary_clicked.
                }
            } else if primary_down && !over_toolbar {
                // If we have a pending press origin but no selection yet, check threshold
                if let Some((origin, block_idx, char_idx)) = self.sel_press_origin {
                    if (pos - origin).length() >= DRAG_THRESHOLD {
                        // Threshold exceeded → promote to real selection
                        let cur_hit = hit_test(pos).unwrap_or((block_idx, char_idx));
                        self.text_selection = Some(TextSelection {
                            start_block: block_idx,
                            start_char: char_idx,
                            end_block: cur_hit.0,
                            end_char: cur_hit.1,
                            is_dragging: true,
                        });
                        self.sel_press_origin = None;
                        // Close any open highlight popup when starting a drag
                        if self.clicked_highlight_id.is_some() {
                            self.clicked_highlight_id = None;
                            self.hl_note_popup_rect = None;
                        }
                    }
                }
                // Update end of an active selection while dragging
                if let Some(sel) = &mut self.text_selection {
                    if sel.is_dragging {
                        if let Some((block_idx, char_idx)) = hit_test(pos) {
                            sel.end_block = block_idx;
                            sel.end_char = char_idx;
                        } else {
                            // Pointer is outside any block — find the closest block
                            // above or below to extend selection
                            let mut best: Option<(usize, usize)> = None;
                            for (idx, galley, rect, _) in &block_galleys {
                                if pos.y < rect.min.y {
                                    // Above this block → first char
                                    if best.is_none() || *idx < best.unwrap().0 {
                                        best = Some((*idx, 0));
                                    }
                                    break;
                                } else if pos.y > rect.max.y {
                                    // Below this block → last char
                                    let end = galley.text().chars().count();
                                    best = Some((*idx, end));
                                }
                            }
                            if let Some((bi, ci)) = best {
                                sel.end_block = bi;
                                sel.end_char = ci;
                            }
                        }
                    }
                }
            }

            if primary_released {
                // Check if this was a click (no drag) on a highlighted region
                let mut handled_as_highlight = false;
                if let Some((press_pos, press_block, press_char)) = self.sel_press_origin.take() {
                    // Look up if this block+char sits inside a highlight
                    if let Some(cfg) = &self.book_config {
                        if let Some(hl) = cfg.highlights.iter().find(|h| {
                            h.chapter == self.current_chapter
                                && h.start_block == press_block
                                && press_char >= h.start_offset
                                && press_char < h.end_offset
                        }) {
                            // Found a highlight under the click → show note popup
                            handled_as_highlight = true;
                            self.clicked_highlight_id = Some(hl.id.clone());
                            self.hl_note_just_opened = true;
                            // Load existing note content into edit buffer
                            if let Some(note) = cfg.notes.iter().find(|n| n.highlight_id == hl.id) {
                                self.editing_note_buf = note.content.clone();
                            } else {
                                self.editing_note_buf.clear();
                            }
                            // Position popup above the click point
                            if let Some((_, _, rect, _)) = block_galleys
                                .iter()
                                .find(|(idx, _, _, _)| *idx == press_block)
                            {
                                self.hl_note_toolbar_pos = egui::pos2(rect.center().x, rect.min.y);
                            }
                            // Clear any text selection
                            self.text_selection = None;
                        }
                    }
                    // Plain click on text (no highlight, no drag) → page turn
                    let hit_csc_rect = CSC_RECTS
                        .with(|rects| rects.borrow().iter().any(|cr| cr.rect.contains(press_pos)));
                    if !handled_as_highlight
                        && !hit_csc_rect
                        && !self.scroll_mode
                        && !self.show_sharing_panel
                        && !self.show_stats
                        && !self.show_export_dialog
                        && self.text_selection.is_none()
                        && !self.csc_custom_replace_active
                        && self.csc_popup.is_none()
                    {
                        if let Some(page_rect) = self.paging_page_rect {
                            if page_rect.contains(press_pos) {
                                if press_pos.x < page_rect.center().x {
                                    if is_dual_column {
                                        if self.current_page >= 2 {
                                            self.trigger_page_animation_to(
                                                self.current_page - 2,
                                                -1.0,
                                            );
                                        } else if self.current_chapter > 0 {
                                            self.capture_cross_chapter_snapshot();
                                            self.prev_chapter();
                                            self.current_page = usize::MAX;
                                            self.start_cross_chapter_animation(-1.0);
                                        }
                                    } else {
                                        self.prev_page();
                                    }
                                } else if is_dual_column {
                                    if self.current_page + 2 < self.total_pages {
                                        self.trigger_page_animation_to(self.current_page + 2, 1.0);
                                    } else {
                                        self.capture_cross_chapter_snapshot();
                                        self.next_chapter();
                                        self.start_cross_chapter_animation(1.0);
                                    }
                                } else {
                                    self.next_page();
                                }
                            }
                        }
                    }
                }

                if let Some(sel) = &mut self.text_selection {
                    if sel.is_dragging {
                        sel.is_dragging = false;
                        // If selection is empty (start == end), clear it
                        if sel.start_block == sel.end_block && sel.start_char == sel.end_char {
                            self.text_selection = None;
                        } else {
                            // Position the toolbar above the start of the selection
                            let (sel_start_block, _) = sel.normalized();
                            if let Some((_, _, rect, _)) = block_galleys
                                .iter()
                                .find(|(idx, _, _, _)| *idx == sel_start_block)
                            {
                                self.sel_toolbar_pos = egui::pos2(rect.center().x, rect.top());
                            }
                        }
                    }
                }
            }
        }
        }

        // ── Draw selection highlight overlay (blue rectangles) ──
        if let Some(sel) = &self.text_selection {
            let (sb, sc, eb, ec) = sel.normalized_range();
            for (idx, galley, rect, text) in &block_galleys {
                if *idx < sb || *idx > eb {
                    continue;
                }
                let char_len = text.chars().count();
                let sel_start = if *idx == sb { sc } else { 0 };
                let sel_end = if *idx == eb {
                    ec.min(char_len)
                } else {
                    char_len
                };
                if sel_start >= sel_end {
                    continue;
                }
                // Convert char offsets to galley cursors
                let c_start = galley.from_ccursor(egui::text::CCursor::new(sel_start));
                let c_end = galley.from_ccursor(egui::text::CCursor::new(sel_end));
                // Walk galley rows and draw highlight rect for each selected row range
                let start_row = c_start.rcursor.row;
                let end_row = c_end.rcursor.row;
                for row_idx in start_row..=end_row {
                    if row_idx >= galley.rows.len() {
                        break;
                    }
                    let row = &galley.rows[row_idx];
                    let row_min_x = if row_idx == start_row {
                        // Start of selection within first row

                        galley.pos_from_cursor(&c_start).min.x
                    } else {
                        row.rect.min.x
                    };
                    let row_max_x = if row_idx == end_row {
                        galley.pos_from_cursor(&c_end).max.x
                    } else {
                        row.rect.max.x
                    };
                    let hl_rect = egui::Rect::from_min_max(
                        egui::pos2(rect.min.x + row_min_x, rect.min.y + row.rect.min.y),
                        egui::pos2(rect.min.x + row_max_x, rect.min.y + row.rect.max.y),
                    );
                    ui.painter().rect_filled(hl_rect, 0.0, SEL_BG);
                }
            }
        }

        // ── Extract selected text from block galleys ──
        let selected_text: String = self
            .text_selection
            .as_ref()
            .filter(|s| !s.is_dragging || primary_down)
            .map(|sel| {
                let (sb, sc, eb, ec) = sel.normalized_range();
                let mut result = String::new();
                for (idx, _, _, text) in &block_galleys {
                    if *idx < sb || *idx > eb {
                        continue;
                    }
                    let chars: Vec<char> = text.chars().collect();
                    let start = if *idx == sb { sc } else { 0 };
                    let end = if *idx == eb {
                        ec.min(chars.len())
                    } else {
                        chars.len()
                    };
                    if start < end {
                        if !result.is_empty() {
                            result.push('\n');
                        }
                        result.extend(&chars[start..end]);
                    }
                }
                result
            })
            .unwrap_or_default();

        // ── Show floating selection toolbar (when selection finalized) ──
        if let Some(sel) = &self.text_selection {
            if !sel.is_dragging && !selected_text.is_empty() && !self.csc_custom_replace_active {
                let (sb, _, eb, _) = sel.normalized_range();
                let has_hl = self.book_config.as_ref().is_some_and(|cfg| {
                    cfg.highlights.iter().any(|h| {
                        h.chapter == self.current_chapter
                            && h.start_block >= sb
                            && h.start_block <= eb
                    })
                });
                let res = show_selection_toolbar(
                    ui.ctx(),
                    &self.i18n,
                    &selected_text,
                    self.sel_toolbar_pos,
                    has_hl,
                    self.csc_mode == reader_core::csc::CorrectionMode::ReadWrite,
                );
                match res {
                    SelToolbarResult::KeepOpen => {}
                    SelToolbarResult::Close => {
                        self.text_selection = None;
                    }
                    SelToolbarResult::CreateHighlight(color) => {
                        let sel_range = sel.normalized_range();
                        if let Some(cfg) = &mut self.book_config {
                            let (sb, sc, eb, ec) = sel_range;
                            for (idx, _, _, text) in &block_galleys {
                                if *idx < sb || *idx > eb {
                                    continue;
                                }
                                let char_len = text.chars().count();
                                let start = if *idx == sb { sc } else { 0 };
                                let end = if *idx == eb {
                                    ec.min(char_len)
                                } else {
                                    char_len
                                };
                                if start < end {
                                    cfg.highlights.push(reader_core::library::Highlight {
                                        id: format!("{}-{}", reader_core::now_secs(), idx),
                                        chapter: self.current_chapter,
                                        start_block: *idx,
                                        start_offset: start,
                                        end_block: *idx,
                                        end_offset: end,
                                        color: color.clone(),
                                        created_at: reader_core::now_secs(),
                                    });
                                }
                            }
                            cfg.save(&self.data_dir);
                        }
                        self.text_selection = None;
                    }
                    SelToolbarResult::DeleteHighlight => {
                        let (sb, _, eb, _) = sel.normalized_range();
                        if let Some(cfg) = &mut self.book_config {
                            cfg.highlights.retain(|h| {
                                !(h.chapter == self.current_chapter
                                    && h.start_block >= sb
                                    && h.start_block <= eb)
                            });
                            cfg.save(&self.data_dir);
                        }
                        self.text_selection = None;
                    }
                    SelToolbarResult::CustomReplace => {
                        // Activate custom replacement popup — keep selection for reference
                        self.csc_custom_replace_buf.clear();
                        self.csc_custom_replace_active = true;
                    }
                }
            }
        }

        // ── Custom CSC replacement popup (ReadWrite mode) ──
        if self.csc_custom_replace_active {
            if let Some(sel) = &self.text_selection {
                let popup_pos = self.sel_toolbar_pos;
                let sel_range = sel.normalized_range();
                let popup_id = egui::Id::new("csc_custom_replace_popup");
                let mut close = false;
                let mut submit = false;

                egui::Area::new(popup_id)
                    .fixed_pos(egui::pos2(popup_pos.x - 120.0, popup_pos.y - 80.0))
                    .order(egui::Order::Foreground)
                    .interactable(true)
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.label(self.i18n.t("csc.custom_replace_prompt"));
                            ui.add_space(4.0);
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut self.csc_custom_replace_buf)
                                    .desired_width(160.0)
                                    .hint_text(self.i18n.t("csc.custom_replace_hint")),
                            );
                            // Auto-focus on first frame
                            resp.request_focus();
                            if resp.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                && !self.csc_custom_replace_buf.is_empty()
                            {
                                submit = true;
                            }
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                if ui.button(self.i18n.t("csc.replace")).clicked()
                                    && !self.csc_custom_replace_buf.is_empty()
                                {
                                    submit = true;
                                }
                                if ui.button(self.i18n.t("csc.cancel")).clicked() {
                                    close = true;
                                }
                            });
                        });
                    });

                if submit {
                    // Create correction records for each selected character
                    let (sb, sc, eb, ec) = sel_range;
                    let replace_chars: Vec<char> = self.csc_custom_replace_buf.chars().collect();
                    for (idx, _, _, block_text) in &block_galleys {
                        if *idx < sb || *idx > eb {
                            continue;
                        }
                        let block_chars: Vec<char> = block_text.chars().collect();
                        let start = if *idx == sb { sc } else { 0 };
                        let end = if *idx == eb {
                            ec.min(block_chars.len())
                        } else {
                            block_chars.len()
                        };
                        // Map selected chars 1:1 to replacement chars
                        for (i, pos) in (start..end).enumerate() {
                            let original = block_chars[pos].to_string();
                            let corrected = if i < replace_chars.len() {
                                replace_chars[i].to_string()
                            } else {
                                continue;
                            };
                            if original == corrected {
                                continue;
                            }
                            // Insert into csc_cache as Accepted
                            let key = (self.current_chapter, *idx);
                            let corrs = self.csc_cache.entry(key).or_default();
                            if let Some(existing) = corrs.iter_mut().find(|c| c.char_offset == pos)
                            {
                                existing.corrected = corrected.clone();
                                existing.status = reader_core::epub::CorrectionStatus::Accepted;
                            } else {
                                corrs.push(reader_core::epub::CorrectionInfo {
                                    original: original.clone(),
                                    corrected: corrected.clone(),
                                    confidence: 1.0,
                                    char_offset: pos,
                                    status: reader_core::epub::CorrectionStatus::Accepted,
                                });
                            }
                            // Persist
                            if let Some(cfg) = &mut self.book_config {
                                if let Some(rec) = cfg.corrections.iter_mut().find(|r| {
                                    r.chapter == self.current_chapter
                                        && r.block_idx == *idx
                                        && r.char_offset == pos
                                }) {
                                    rec.corrected = corrected;
                                    rec.status = "accepted".to_string();
                                } else {
                                    cfg.corrections
                                        .push(reader_core::library::CorrectionRecord {
                                            chapter: self.current_chapter,
                                            block_idx: *idx,
                                            char_offset: pos,
                                            original,
                                            corrected,
                                            status: "accepted".to_string(),
                                        });
                                }
                            }
                        }
                    }
                    if let Some(cfg) = &mut self.book_config {
                        cfg.save(&self.data_dir);
                    }
                    close = true;
                }

                if close {
                    self.csc_custom_replace_active = false;
                    self.csc_custom_replace_buf.clear();
                    self.text_selection = None;
                }
            } else {
                // Selection was cleared externally
                self.csc_custom_replace_active = false;
                self.csc_custom_replace_buf.clear();
            }
        }

        // ── Floating note popup for clicked highlight ──
        if self.clicked_highlight_id.is_some() {
            let hl_id = self.clicked_highlight_id.clone().unwrap();
            let popup_pos = self.hl_note_toolbar_pos;

            let note_toolbar_id = egui::Id::new("hl_note_toolbar");
            let mut close_popup = false;

            let mut popup_rect = egui::Rect::NOTHING;
            egui::Area::new(note_toolbar_id)
                .fixed_pos(egui::pos2(popup_pos.x - 160.0, popup_pos.y - 170.0))
                .order(egui::Order::Foreground)
                .interactable(true)
                .show(ui.ctx(), |ui| {
                    let frame_resp = egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_max_width(320.0);

                        // Top row: highlight info + action buttons
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(self.i18n.t("context.note"))
                                    .strong()
                                    .size(13.0),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    // Delete highlight button
                                    if ui
                                        .small_button("🗑")
                                        .on_hover_text(self.i18n.t("context.delete_highlight"))
                                        .clicked()
                                    {
                                        if let Some(cfg) = &mut self.book_config {
                                            cfg.notes.retain(|n| n.highlight_id != hl_id);
                                            cfg.highlights.retain(|h| h.id != hl_id);
                                            cfg.save(&self.data_dir);
                                        }
                                        close_popup = true;
                                    }
                                    // Close button
                                    if ui.small_button("✕").clicked() {
                                        close_popup = true;
                                    }
                                },
                            );
                        });

                        ui.separator();

                        // Note text edit area
                        let response = ui.add(
                            egui::TextEdit::multiline(&mut self.editing_note_buf)
                                .desired_rows(3)
                                .desired_width(300.0)
                                .hint_text(self.i18n.t("context.note_hint")),
                        );

                        // Save button
                        ui.horizontal(|ui| {
                            if ui.button(self.i18n.t("context.save_note")).clicked()
                                || (response.lost_focus()
                                    && ui.input(|i| {
                                        i.key_pressed(egui::Key::Enter) && i.modifiers.ctrl
                                    }))
                            {
                                if let Some(cfg) = &mut self.book_config {
                                    if self.editing_note_buf.trim().is_empty() {
                                        // Remove note if empty
                                        cfg.notes.retain(|n| n.highlight_id != hl_id);
                                    } else if let Some(note) =
                                        cfg.notes.iter_mut().find(|n| n.highlight_id == hl_id)
                                    {
                                        note.content = self.editing_note_buf.clone();
                                        note.updated_at = reader_core::now_secs();
                                    } else {
                                        cfg.notes.push(reader_core::library::Note {
                                            highlight_id: hl_id.clone(),
                                            content: self.editing_note_buf.clone(),
                                            created_at: reader_core::now_secs(),
                                            updated_at: reader_core::now_secs(),
                                        });
                                    }
                                    cfg.save(&self.data_dir);
                                }
                                close_popup = true;
                            }
                        });
                    });
                    // Capture Frame's actual rect (includes padding + content)
                    popup_rect = frame_resp.response.rect;
                });

            // Cache the popup rect for next frame's over_toolbar check
            self.hl_note_popup_rect = Some(popup_rect);

            // Close popup on click outside (skip the frame it was just opened)
            if !close_popup {
                if self.hl_note_just_opened {
                    self.hl_note_just_opened = false;
                } else if !self.show_review_panel {
                    let any_click = ui.ctx().input(|i| i.pointer.primary_clicked());
                    if any_click {
                        let over_note_popup = ui.ctx()
                            .pointer_interact_pos()
                            .is_some_and(|pos| popup_rect.contains(pos));
                        if !over_note_popup {
                            close_popup = true;
                        }
                    }
                }
            }

            if close_popup {
                self.clicked_highlight_id = None;
                self.editing_note_buf.clear();
                self.hl_note_popup_rect = None;
            }
        } else {
            // Popup not shown — clear cached rect
            self.hl_note_popup_rect = None;
        }

        // ── CSC correction click detection + popup ──
        {
            // Check if user clicked on a correction rect (ReadWrite mode)
            let any_click = ui.ctx().input(|i| i.pointer.primary_clicked());
            if any_click && !self.show_review_panel && self.csc_popup.is_none() && self.text_selection.is_none() {
                if let Some(click_pos) = ui.ctx().pointer_interact_pos() {
                    CSC_RECTS.with(|rects| {
                        let r = rects.borrow();
                        for cr in r.iter() {
                            if cr.rect.contains(click_pos) {
                                self.csc_popup = Some(crate::app::CscPopupInfo {
                                    chapter: self.current_chapter,
                                    block_idx: cr.block_idx,
                                    char_offset: cr.char_offset,
                                    original: cr.original.clone(),
                                    corrected: cr.corrected.clone(),
                                    confidence: cr.confidence,
                                    pos: egui::pos2(cr.rect.center().x, cr.rect.min.y),
                                    just_opened: true,
                                });
                                break;
                            }
                        }
                    });
                }
            }

            // Render the CSC popup if open
            if let Some(popup) = self.csc_popup.clone() {
                let popup_id = egui::Id::new("csc_action_popup");
                let mut close = false;
                let mut action: Option<reader_core::epub::CorrectionStatus> = None;

                let area_resp = egui::Area::new(popup_id)
                    .fixed_pos(egui::pos2(popup.pos.x - 100.0, popup.pos.y - 70.0))
                    .order(egui::Order::Foreground)
                    .interactable(true)
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            // Info line: original → corrected (confidence%)
                            ui.horizontal(|ui| {
                                ui.colored_label(Color32::from_rgb(220, 60, 50), &popup.original);
                                ui.label("→");
                                ui.colored_label(Color32::from_rgb(60, 180, 80), &popup.corrected);
                                ui.label(format!(
                                    "  {}：{:.1}%",
                                    self.i18n.t("csc.confidence"),
                                    popup.confidence * 100.0
                                ));
                            });
                            ui.add_space(4.0);
                            // Action buttons: Replace / Don't Replace
                            ui.horizontal(|ui| {
                                if ui
                                    .button(self.i18n.t("csc.replace"))
                                    .on_hover_text(self.i18n.t("csc.replace_tip"))
                                    .clicked()
                                {
                                    action = Some(reader_core::epub::CorrectionStatus::Accepted);
                                    close = true;
                                }
                                if ui
                                    .button(self.i18n.t("csc.keep_original"))
                                    .on_hover_text(self.i18n.t("csc.keep_original_tip"))
                                    .clicked()
                                {
                                    action = Some(reader_core::epub::CorrectionStatus::Rejected);
                                    close = true;
                                }
                            });
                        });
                    });

                // Handle action
                if let Some(new_status) = action {
                    // Update status in csc_cache
                    if let Some(corrs) = self.csc_cache.get_mut(&(popup.chapter, popup.block_idx)) {
                        if let Some(c) = corrs
                            .iter_mut()
                            .find(|c| c.char_offset == popup.char_offset)
                        {
                            c.status = new_status.clone();
                        }
                    }
                    // Persist to BookConfig
                    let status_str = match &new_status {
                        reader_core::epub::CorrectionStatus::Accepted => "accepted",
                        reader_core::epub::CorrectionStatus::Rejected => "rejected",
                        reader_core::epub::CorrectionStatus::Ignored => "ignored",
                        _ => "pending",
                    };
                    if let Some(cfg) = &mut self.book_config {
                        // Upsert correction record
                        if let Some(rec) = cfg.corrections.iter_mut().find(|r| {
                            r.chapter == popup.chapter
                                && r.block_idx == popup.block_idx
                                && r.char_offset == popup.char_offset
                        }) {
                            rec.status = status_str.to_string();
                        } else {
                            cfg.corrections
                                .push(reader_core::library::CorrectionRecord {
                                    chapter: popup.chapter,
                                    block_idx: popup.block_idx,
                                    char_offset: popup.char_offset,
                                    original: popup.original.clone(),
                                    corrected: popup.corrected.clone(),
                                    status: status_str.to_string(),
                                });
                        }
                        cfg.save(&self.data_dir);
                    }
                    self.push_feedback_log(format!(
                        "[CSC] correction action: ch={} blk={} off={} '{}' → '{}' status={}",
                        popup.chapter,
                        popup.block_idx,
                        popup.char_offset,
                        popup.original,
                        popup.corrected,
                        status_str,
                    ));
                }

                // Close on click outside popup
                if !close {
                    if let Some(ref mut p) = self.csc_popup {
                        if p.just_opened {
                            p.just_opened = false;
                        } else if any_click {
                            let popup_rect = area_resp.response.rect;
                            let over_popup = ui
                                .ctx()
                                .pointer_interact_pos()
                                .is_some_and(|pos| popup_rect.contains(pos));
                            if !over_popup {
                                close = true;
                            }
                        }
                    }
                }

                if close {
                    self.csc_popup = None;
                }
            }
        }
    }
}
