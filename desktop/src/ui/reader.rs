//! The main reading interface, integrating text layout and UI overlays.
use std::sync::Arc;

use eframe::egui;
use egui::{Color32, FontId, UiBuilder};

use crate::app::{ReaderApp, TextSelection};
use reader_core::epub::ContentBlock;

use super::{reader_block::*, reader_state::*};

type ChapterHighlightRanges =
    std::collections::HashMap<usize, Vec<(usize, usize, reader_core::library::HighlightColor)>>;

fn chapter_highlight_ranges(
    config: Option<&reader_core::library::BookConfig>,
    chapter: usize,
) -> ChapterHighlightRanges {
    let mut ranges = ChapterHighlightRanges::new();
    if let Some(config) = config {
        for highlight in config
            .highlights
            .iter()
            .filter(|highlight| highlight.chapter == chapter)
        {
            ranges.entry(highlight.start_block).or_default().push((
                highlight.start_offset,
                highlight.end_offset,
                highlight.color.clone(),
            ));
        }
    }
    ranges
}

fn loaded_chapter_highlight_ranges(
    config: Option<&reader_core::library::BookConfig>,
    start_chapter: usize,
    loaded_end: usize,
) -> std::collections::HashMap<usize, ChapterHighlightRanges> {
    let mut chapters = std::collections::HashMap::new();
    if let Some(config) = config {
        for highlight in config.highlights.iter().filter(|highlight| {
            highlight.chapter >= start_chapter && highlight.chapter < loaded_end
        }) {
            chapters
                .entry(highlight.chapter)
                .or_insert_with(ChapterHighlightRanges::new)
                .entry(highlight.start_block)
                .or_default()
                .push((
                    highlight.start_offset,
                    highlight.end_offset,
                    highlight.color.clone(),
                ));
        }
    }
    chapters
}

fn set_csc_corrections(
    cache: &std::collections::HashMap<(usize, usize), Vec<reader_core::epub::CorrectionInfo>>,
    enabled: bool,
    include_chapter: impl Fn(usize) -> bool,
) {
    CSC_CORRECTIONS.with(|corrections| {
        let mut map = corrections.borrow_mut();
        map.clear();
        if enabled {
            for ((chapter, block_idx), values) in cache {
                if include_chapter(*chapter) {
                    map.insert(BlockKey::new(*chapter, *block_idx), values.clone());
                }
            }
        }
    });
}

impl ReaderApp {
    fn update_position_from_continuous_scroll(&mut self, position: BlockKey) {
        if position.chapter >= self.total_chapters() {
            return;
        }
        let chapter_changed = position.chapter != self.current_chapter;
        self.continuous_scroll.set_visible_chapter(position.chapter);
        if chapter_changed {
            self.current_page = 0;
            self.pages_dirty = true;
        }
        self.schedule_position_save(position.chapter, position.block);

        if chapter_changed && position.chapter + 1 < self.total_chapters() {
            self.csc_trigger_chapter(position.chapter + 1);
        }
    }

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
                    self.font_size * self.title_font_scale * 1.2 + TITLE_SPACING
                };
                let usable = (available_height - FRAME_MARGIN).max(100.0);
                let mut first_page = true;
                for (i, block) in blocks.iter().enumerate() {
                    let bh = estimate_block_height(
                        block,
                        self.font_size,
                        self.title_font_scale,
                        line_height,
                        max_width,
                    );
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
        if let Some(block) = self.pending_restore_block.take() {
            self.current_page = self
                .page_block_ranges
                .iter()
                .position(|(start, end)| *start <= block && block < *end)
                .unwrap_or_else(|| self.total_pages.saturating_sub(1));
        }
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
            TTS_HIGHLIGHT_BLOCK.set(Some(BlockKey::new(
                self.tts_chapter,
                self.tts_current_block,
            )));
        } else {
            TTS_HIGHLIGHT_BLOCK.set(None);
        }

        // Paging renders the current chapter and may briefly render a previous chapter snapshot.
        // Continuous mode installs corrections one rendered chapter at a time below.
        let snapshot_chapter = self
            .page_anim_cross_chapter_snapshot
            .as_ref()
            .map(|snapshot| snapshot.chapter);
        set_csc_corrections(
            &self.csc_cache,
            self.csc_mode != reader_core::csc::CorrectionMode::None && !self.scroll_mode,
            |chapter| chapter == self.current_chapter || Some(chapter) == snapshot_chapter,
        );
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

        let effective_font_family = if self.defer_custom_font_for_frame {
            "Sans".to_string()
        } else {
            "ReaderFont".to_string()
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
        let mut clicked_link: Option<ClickedLink> = None;
        let has_previous_chapter = self.previous_chapter.is_some();
        let mut continuous_visible_position = None;

        let available_width = ui.available_width();
        let available_height = ui.available_height();
        if (self.last_avail_width - available_width).abs() > 1.0
            || (self.last_avail_height - available_height).abs() > 1.0
        {
            self.pages_dirty = true;
            self.last_avail_width = available_width;
            self.last_avail_height = available_height;
        }
        let layout = reader_text_layout(available_width, available_height, self.scroll_mode);
        let dual_column = layout.is_dual_column;
        let is_dual_column = dual_column;
        self.is_dual_column = dual_column;
        let text_width = layout.text_width;
        let h_margin = layout.h_margin;
        if !self.scroll_mode && self.pages_dirty {
            self.recalculate_pages(ui.available_height(), text_width);
        }

        if let Some(book) = &self.book {
            if let Some(chapter) = book.chapters.get(self.current_chapter) {
                let (title, blocks) = if self.scroll_mode {
                    (String::new(), Vec::new())
                } else {
                    (chapter.title.clone(), chapter.blocks.clone())
                };
                let total_ch = book.chapters.len();
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

                let highlight_ranges = if self.scroll_mode {
                    ChapterHighlightRanges::new()
                } else {
                    chapter_highlight_ranges(self.book_config.as_ref(), self.current_chapter)
                };

                if self.scroll_mode {
                    let layout_signature = continuous_layout_signature(ContinuousLayoutSettings {
                        text_width,
                        font_size: self.font_size,
                        title_font_scale: self.title_font_scale,
                        line_spacing: self.line_spacing,
                        para_spacing: self.para_spacing,
                        text_indent: self.text_indent as f32,
                        latin_font: &self.reader_font_family,
                        cjk_font: &self.reader_cjk_font_family,
                    });
                    let layout_changed = self
                        .continuous_scroll
                        .update_layout_signature(layout_signature);
                    let reset_scroll = self.scroll_to_top
                        || layout_changed
                        || self
                            .continuous_scroll
                            .needs_reset(self.current_chapter, total_ch);
                    if reset_scroll {
                        self.continuous_scroll.reset(self.current_chapter, total_ch);
                        self.scroll_to_top = false;
                        self.text_selection = None;
                        self.sel_press_origin = None;
                        self.clicked_highlight_id = None;
                        self.csc_popup = None;
                    }

                    let start_chapter = self.continuous_scroll.start_chapter;
                    let loaded_end = self.continuous_scroll.loaded_end;
                    let visible_chapter = self.continuous_scroll.visible_chapter;
                    let cached_heights = self.continuous_scroll.chapter_heights.clone();
                    let restore_target = self
                        .pending_restore_block
                        .map(|block| BlockKey::new(self.current_chapter, block));
                    let chapter_highlights = loaded_chapter_highlight_ranges(
                        self.book_config.as_ref(),
                        start_chapter,
                        loaded_end,
                    );
                    let mut scroll_area = egui::ScrollArea::vertical()
                        .id_salt("reader_continuous_scroll")
                        .auto_shrink([false; 2]);
                    if reset_scroll {
                        scroll_area = scroll_area.vertical_scroll_offset(0.0);
                    }
                    let (now, stable_dt, raw_wheel_delta, modifiers, primary_down) =
                        ui.ctx().input(|input| {
                            (
                                input.time,
                                input.stable_dt,
                                input.raw_scroll_delta.y,
                                input.modifiers,
                                input.pointer.primary_down(),
                            )
                        });
                    if primary_down {
                        self.continuous_scroll.cancel_wheel_scroll();
                    }
                    let discrete_wheel = ui.rect_contains_pointer(full_rect)
                        && !modifiers.ctrl
                        && !modifiers.command
                        && raw_wheel_delta.abs() >= 8.0;
                    if discrete_wheel {
                        self.continuous_scroll
                            .begin_wheel_scroll(raw_wheel_delta, now);
                    }
                    if self.continuous_scroll.suppress_default_wheel_scroll(now) {
                        ui.ctx()
                            .input_mut(|input| input.smooth_scroll_delta.y = 0.0);
                    }
                    let wheel_scroll_offset =
                        self.continuous_scroll.advance_wheel_scroll(stable_dt);
                    if let Some(offset) = wheel_scroll_offset {
                        scroll_area = scroll_area.vertical_scroll_offset(offset);
                    }
                    let output = scroll_area.show_viewport(ui, |ui, viewport| {
                        ui.set_min_width(available_width);
                        let content_origin_y = ui.cursor().min.y;
                        let render_bounds = viewport.expand2(egui::vec2(0.0, viewport.height()));
                        let anchor_y = viewport.min.y + (viewport.height() * 0.12).min(64.0);
                        let mut anchored_chapter = visible_chapter;
                        let mut measured_heights = Vec::new();
                        let empty_ranges = ChapterHighlightRanges::new();

                        for chapter_idx in start_chapter..loaded_end {
                            let Some(chapter) = book.chapters.get(chapter_idx) else {
                                continue;
                            };
                            let estimated_height = cached_heights
                                .get(&chapter_idx)
                                .copied()
                                .unwrap_or_else(|| {
                                    estimate_chapter_height(
                                        &chapter.blocks,
                                        self.font_size,
                                        self.title_font_scale,
                                        text_width,
                                    )
                                })
                                .max(1.0);
                            let chapter_start = ui.cursor().min.y - content_origin_y;
                            let estimated_end = chapter_start + estimated_height;
                            let should_render = estimated_end >= render_bounds.min.y
                                && chapter_start <= render_bounds.max.y;

                            if should_render {
                                set_csc_corrections(
                                    &self.csc_cache,
                                    self.csc_mode != reader_core::csc::CorrectionMode::None,
                                    |chapter| chapter == chapter_idx,
                                );
                                ui.push_id(("reader_chapter", chapter_idx), |ui| {
                                    let ranges = chapter_highlights
                                        .get(&chapter_idx)
                                        .unwrap_or(&empty_ranges);
                                    render_content_layout(
                                        ui,
                                        h_margin,
                                        text_width,
                                        &chapter.title,
                                        &chapter.blocks,
                                        0,
                                        chapter.blocks.len(),
                                        true,
                                        self.font_size,
                                        self.title_font_scale,
                                        self.reader_bg_color,
                                        chapter_idx,
                                        total_ch,
                                        &mut action_prev_chapter,
                                        &mut action_next_chapter,
                                        &mut action_go_back,
                                        chapter_idx + 1 == total_ch,
                                        has_previous_chapter,
                                        self.reader_font_color,
                                        &effective_font_family,
                                        &self.i18n,
                                        &mut clicked_link,
                                        ranges,
                                    );
                                });
                            } else {
                                ui.add_space(estimated_height);
                            }

                            let chapter_end = ui.cursor().min.y - content_origin_y;
                            let actual_height = (chapter_end - chapter_start).max(1.0);
                            if should_render {
                                measured_heights.push((chapter_idx, actual_height));
                            }
                            if anchor_y >= chapter_start && anchor_y < chapter_end {
                                anchored_chapter = chapter_idx;
                            }
                        }

                        let restore_rect = restore_target.and_then(|target| {
                            BLOCK_GALLEYS.with(|galleys| {
                                let galleys = galleys.borrow();
                                galleys
                                    .iter()
                                    .find(|entry| entry.key == target)
                                    .or_else(|| {
                                        galleys
                                            .iter()
                                            .filter(|entry| {
                                                entry.key.chapter == target.chapter
                                                    && entry.key.block >= target.block
                                            })
                                            .min_by_key(|entry| entry.key.block)
                                    })
                                    .or_else(|| {
                                        galleys
                                            .iter()
                                            .filter(|entry| entry.key.chapter == target.chapter)
                                            .max_by_key(|entry| entry.key.block)
                                    })
                                    .map(|entry| entry.rect)
                            })
                        });
                        if let Some(rect) = restore_rect {
                            ui.scroll_to_rect(rect, Some(egui::Align::Min));
                        }

                        (anchored_chapter, measured_heights, restore_rect.is_some())
                    });
                    let (anchored_chapter, measured_heights, restored) = output.inner;
                    for (chapter_idx, height) in measured_heights {
                        self.continuous_scroll.record_height(chapter_idx, height);
                    }
                    self.continuous_scroll.record_scroll_output(
                        output.state.offset.y,
                        output.content_size.y,
                        output.inner_rect.height(),
                    );
                    if wheel_scroll_offset.is_some() {
                        ui.ctx().request_repaint();
                    }
                    if restored {
                        self.pending_restore_block = None;
                        ui.ctx().request_repaint();
                    } else if restore_target.is_none() {
                        continuous_visible_position = BLOCK_GALLEYS.with(|galleys| {
                            galleys
                                .borrow()
                                .iter()
                                .filter(|entry| entry.rect.intersects(output.inner_rect))
                                .min_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
                                .map(|entry| entry.key)
                        });
                        if continuous_visible_position.is_none() {
                            continuous_visible_position = Some(BlockKey::new(anchored_chapter, 0));
                        }
                    }

                    let remaining = remaining_scroll_height(
                        output.content_size.y,
                        output.inner_rect.height(),
                        output.state.offset.y,
                    );
                    if remaining < continuous_load_threshold(output.inner_rect.height())
                        && self.continuous_scroll.append_next(total_ch).is_some()
                    {
                        ui.ctx().request_repaint();
                    }
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
                                    let snap_chapter = snap.chapter;
                                    let snap_highlight_ranges = chapter_highlight_ranges(
                                        self.book_config.as_ref(),
                                        snap_chapter,
                                    );
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
                                                self.title_font_scale,
                                                self.reader_bg_color,
                                                snap_chapter,
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
                                                &snap_highlight_ranges,
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
                                                    self.title_font_scale,
                                                    self.reader_bg_color,
                                                    snap_chapter,
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
                                                    &snap_highlight_ranges,
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
                                                self.title_font_scale,
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
                                                    self.title_font_scale,
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
                                    self.title_font_scale,
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
                                            self.title_font_scale,
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
                                    self.title_font_scale,
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
                                        self.title_font_scale,
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
                                egui::Stroke::new(1.0_f32, Color32::from_gray(80)),
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
                                    let snap_chapter = snap.chapter;
                                    let snap_highlight_ranges = chapter_highlight_ranges(
                                        self.book_config.as_ref(),
                                        snap_chapter,
                                    );
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
                                                self.title_font_scale,
                                                self.reader_bg_color,
                                                snap_chapter,
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
                                                &snap_highlight_ranges,
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
                                                self.title_font_scale,
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
                                    self.title_font_scale,
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
                                self.title_font_scale,
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
                        if clicked_link.is_none()
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

        if let Some(position) = continuous_visible_position {
            self.update_position_from_continuous_scroll(position);
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
                    self.current_block = 0;
                    self.pending_restore_block = None;
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
        if let Some(clicked) = clicked_link {
            let source_chapter = clicked.source_chapter;
            let url = clicked.url.trim().to_string();
            let lowered = url.to_lowercase();
            if lowered.starts_with("http://")
                || lowered.starts_with("https://")
                || lowered.starts_with("mailto:")
                || lowered.starts_with("tel:")
            {
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            } else if !url.starts_with('#') {
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
                    // Check if target is a review chapter (娈佃瘎) 鈥?show overlay instead of navigating
                    if self
                        .book
                        .as_ref()
                        .is_some_and(|b| b.review_chapter_indices.contains(&idx))
                    {
                        self.show_review_panel = true;
                        self.review_panel_chapter = Some(idx);
                        self.review_panel_anchor = url.split('#').nth(1).map(|s| s.to_string());
                        self.review_panel_just_opened = true;
                    } else {
                        if idx != source_chapter {
                            self.previous_chapter = Some(source_chapter);
                        }
                        self.current_chapter = idx;
                        self.current_block = 0;
                        self.pending_restore_block = None;
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

        if !self.scroll_mode {
            if let Some((block, _)) = self.page_block_ranges.get(self.current_page).copied() {
                self.schedule_position_save(self.current_chapter, block);
            }
        }

        // 鈹€鈹€ Custom text selection state machine 鈹€鈹€
        let block_galleys: Vec<BlockGalleyEntry> =
            BLOCK_GALLEYS.with(|bg| bg.borrow_mut().drain(..).collect());

        // Detect primary pointer press / drag / release for selection
        let pointer_pos = ui.ctx().input(|i| i.pointer.interact_pos());
        let primary_down = ui.ctx().input(|i| i.pointer.primary_down());
        let primary_pressed = ui.ctx().input(|i| i.pointer.primary_pressed());
        let primary_released = ui.ctx().input(|i| i.pointer.primary_released());

        // Find the chapter/block and character under a screen position.
        let hit_test = |pos: egui::Pos2| -> Option<(BlockKey, usize)> {
            for entry in &block_galleys {
                if entry.rect.contains(pos) {
                    let local = egui::vec2(pos.x - entry.rect.min.x, pos.y - entry.rect.min.y);
                    let cursor = entry.galley.cursor_from_pos(local);
                    return Some((entry.key, cursor.ccursor.index));
                }
            }
            None
        };

        // Check if pointer is over a toolbar area (so we don't start selection there)
        let toolbar_id = egui::Id::new("sel_toolbar");
        let note_toolbar_id = egui::Id::new("hl_note_toolbar");
        let over_toolbar = ui.ctx().memory(|mem| {
            mem.layer_id_at(pointer_pos.unwrap_or_default())
                .is_some_and(|layer| layer.id == toolbar_id || layer.id == note_toolbar_id)
        });

        if let Some(pos) = pointer_pos {
            const DRAG_THRESHOLD: f32 = 5.0;

            if primary_pressed && !over_toolbar {
                if let Some((block_key, char_idx)) = hit_test(pos) {
                    // Record press origin; don't create TextSelection yet
                    self.sel_press_origin = Some((pos, block_key, char_idx));
                    // Clear any existing finalized selection or highlight popup
                    if self.text_selection.as_ref().is_some_and(|s| !s.is_dragging) {
                        self.text_selection = None;
                    }
                    if self.clicked_highlight_id.is_some() {
                        self.clicked_highlight_id = None;
                    }
                } else {
                    // Clicked outside any block 鈫?clear everything
                    self.sel_press_origin = None;
                    self.text_selection = None;
                    self.clicked_highlight_id = None;
                }
            } else if primary_down && !over_toolbar {
                // If we have a pending press origin but no selection yet, check threshold
                if let Some((origin, block_key, char_idx)) = self.sel_press_origin {
                    if (pos - origin).length() >= DRAG_THRESHOLD {
                        // Threshold exceeded 鈫?promote to real selection
                        let cur_hit = hit_test(pos)
                            .filter(|(key, _)| key.chapter == block_key.chapter)
                            .unwrap_or((block_key, char_idx));
                        self.text_selection = Some(TextSelection {
                            chapter: block_key.chapter,
                            start_block: block_key.block,
                            start_char: char_idx,
                            end_block: cur_hit.0.block,
                            end_char: cur_hit.1,
                            is_dragging: true,
                        });
                        self.sel_press_origin = None;
                    }
                }
                // Update end of an active selection while dragging
                if let Some(sel) = &mut self.text_selection {
                    if sel.is_dragging {
                        if let Some((block_key, char_idx)) =
                            hit_test(pos).filter(|(key, _)| key.chapter == sel.chapter)
                        {
                            sel.end_block = block_key.block;
                            sel.end_char = char_idx;
                        } else {
                            // Pointer is outside any block 鈥?find the closest block
                            // above or below to extend selection
                            let mut best: Option<(usize, usize)> = None;
                            for entry in block_galleys
                                .iter()
                                .filter(|entry| entry.key.chapter == sel.chapter)
                            {
                                if pos.y < entry.rect.min.y {
                                    // Above this block 鈫?first char
                                    best = Some((entry.key.block, 0));
                                    break;
                                } else if pos.y > entry.rect.max.y {
                                    // Below this block 鈫?last char
                                    let end = entry.galley.text().chars().count();
                                    best = Some((entry.key.block, end));
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
                if let Some((press_pos, press_key, press_char)) = self.sel_press_origin.take() {
                    // Look up if this block+char sits inside a highlight
                    if let Some(cfg) = &self.book_config {
                        if let Some(hl) = cfg.highlights.iter().find(|h| {
                            h.chapter == press_key.chapter
                                && h.start_block == press_key.block
                                && press_char >= h.start_offset
                                && press_char < h.end_offset
                        }) {
                            // Found a highlight under the click 鈫?show note popup
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
                            if let Some(entry) =
                                block_galleys.iter().find(|entry| entry.key == press_key)
                            {
                                self.hl_note_toolbar_pos =
                                    egui::pos2(entry.rect.center().x, entry.rect.min.y);
                            }
                            // Clear any text selection
                            self.text_selection = None;
                        }
                    }
                    // Plain click on text (no highlight, no drag) 鈫?page turn
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
                        && !self.show_review_panel
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
                                    } else if self.current_chapter + 1 < self.total_chapters() {
                                        self.capture_cross_chapter_snapshot();
                                        self.next_chapter();
                                        self.start_cross_chapter_animation(1.0);
                                    }
                                } else if self.current_page + 1 < self.total_pages {
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
                            if let Some(entry) = block_galleys.iter().find(|entry| {
                                entry.key == BlockKey::new(sel.chapter, sel_start_block)
                            }) {
                                self.sel_toolbar_pos =
                                    egui::pos2(entry.rect.center().x, entry.rect.top());
                            }
                        }
                    }
                }
            }
        }

        // 鈹€鈹€ Draw selection highlight overlay (blue rectangles) 鈹€鈹€
        if let Some(sel) = &self.text_selection {
            let (sb, sc, eb, ec) = sel.normalized_range();
            for entry in &block_galleys {
                let idx = entry.key.block;
                if entry.key.chapter != sel.chapter || idx < sb || idx > eb {
                    continue;
                }
                let char_len = entry.text.chars().count();
                let sel_start = if idx == sb { sc } else { 0 };
                let sel_end = if idx == eb {
                    ec.min(char_len)
                } else {
                    char_len
                };
                if sel_start >= sel_end {
                    continue;
                }
                // Convert char offsets to galley cursors
                let c_start = entry
                    .galley
                    .from_ccursor(egui::text::CCursor::new(sel_start));
                let c_end = entry.galley.from_ccursor(egui::text::CCursor::new(sel_end));
                // Walk galley rows and draw highlight rect for each selected row range
                let start_row = c_start.rcursor.row;
                let end_row = c_end.rcursor.row;
                for row_idx in start_row..=end_row {
                    if row_idx >= entry.galley.rows.len() {
                        break;
                    }
                    let row = &entry.galley.rows[row_idx];
                    let row_min_x = if row_idx == start_row {
                        // Start of selection within first row

                        entry.galley.pos_from_cursor(&c_start).min.x
                    } else {
                        row.rect.min.x
                    };
                    let row_max_x = if row_idx == end_row {
                        entry.galley.pos_from_cursor(&c_end).max.x
                    } else {
                        row.rect.max.x
                    };
                    let hl_rect = egui::Rect::from_min_max(
                        egui::pos2(
                            entry.rect.min.x + row_min_x,
                            entry.rect.min.y + row.rect.min.y,
                        ),
                        egui::pos2(
                            entry.rect.min.x + row_max_x,
                            entry.rect.min.y + row.rect.max.y,
                        ),
                    );
                    ui.painter().rect_filled(hl_rect, 0.0, SEL_BG);
                }
            }
        }

        // 鈹€鈹€ Extract selected text from block galleys 鈹€鈹€
        let selected_text: String = self
            .text_selection
            .as_ref()
            .filter(|s| !s.is_dragging || primary_down)
            .map(|sel| {
                let (sb, sc, eb, ec) = sel.normalized_range();
                let mut result = String::new();
                for entry in &block_galleys {
                    let idx = entry.key.block;
                    if entry.key.chapter != sel.chapter || idx < sb || idx > eb {
                        continue;
                    }
                    let chars: Vec<char> = entry.text.chars().collect();
                    let start = if idx == sb { sc } else { 0 };
                    let end = if idx == eb {
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

        // 鈹€鈹€ Show floating selection toolbar (when selection finalized) 鈹€鈹€
        if let Some(sel) = &self.text_selection {
            if !sel.is_dragging && !selected_text.is_empty() && !self.csc_custom_replace_active {
                let (sb, _, eb, _) = sel.normalized_range();
                let has_hl = self.book_config.as_ref().is_some_and(|cfg| {
                    cfg.highlights.iter().any(|h| {
                        h.chapter == sel.chapter && h.start_block >= sb && h.start_block <= eb
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
                            for entry in &block_galleys {
                                let idx = entry.key.block;
                                if entry.key.chapter != sel.chapter || idx < sb || idx > eb {
                                    continue;
                                }
                                let char_len = entry.text.chars().count();
                                let start = if idx == sb { sc } else { 0 };
                                let end = if idx == eb {
                                    ec.min(char_len)
                                } else {
                                    char_len
                                };
                                if start < end {
                                    cfg.highlights.push(reader_core::library::Highlight {
                                        id: format!(
                                            "{}-{}-{}",
                                            reader_core::now_secs(),
                                            sel.chapter,
                                            idx
                                        ),
                                        chapter: sel.chapter,
                                        start_block: idx,
                                        start_offset: start,
                                        end_block: idx,
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
                                !(h.chapter == sel.chapter
                                    && h.start_block >= sb
                                    && h.start_block <= eb)
                            });
                            cfg.save(&self.data_dir);
                        }
                        self.text_selection = None;
                    }
                    SelToolbarResult::CustomReplace => {
                        // Activate custom replacement popup 鈥?keep selection for reference
                        self.csc_custom_replace_buf.clear();
                        self.csc_custom_replace_active = true;
                    }
                }
            }
        }

        // 鈹€鈹€ Custom CSC replacement popup (ReadWrite mode) 鈹€鈹€
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
                    for entry in &block_galleys {
                        let idx = entry.key.block;
                        if entry.key.chapter != sel.chapter || idx < sb || idx > eb {
                            continue;
                        }
                        let block_chars: Vec<char> = entry.text.chars().collect();
                        let start = if idx == sb { sc } else { 0 };
                        let end = if idx == eb {
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
                            let key = (sel.chapter, idx);
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
                                    r.chapter == sel.chapter
                                        && r.block_idx == idx
                                        && r.char_offset == pos
                                }) {
                                    rec.corrected = corrected;
                                    rec.status = "accepted".to_string();
                                } else {
                                    cfg.corrections
                                        .push(reader_core::library::CorrectionRecord {
                                            chapter: sel.chapter,
                                            block_idx: idx,
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

        // 鈹€鈹€ Floating note popup for clicked highlight 鈹€鈹€
        if let Some(hl_id) = self.clicked_highlight_id.clone() {
            let popup_pos = self.hl_note_toolbar_pos;

            let note_toolbar_id = egui::Id::new("hl_note_toolbar");
            let mut close_popup = false;

            egui::Area::new(note_toolbar_id)
                .fixed_pos(egui::pos2(popup_pos.x - 160.0, popup_pos.y - 170.0))
                .order(egui::Order::Foreground)
                .interactable(true)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
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
                                        .small_button("馃棏")
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
                });

            // Close popup on click outside (skip the frame it was just opened)
            if !close_popup && !self.show_review_panel {
                if self.hl_note_just_opened {
                    self.hl_note_just_opened = false;
                } else {
                    let any_click = ui.ctx().input(|i| i.pointer.primary_clicked());
                    if any_click {
                        let over_note_popup = ui.ctx().memory(|mem| {
                            mem.layer_id_at(pointer_pos.unwrap_or_default())
                                .is_some_and(|layer| layer.id == note_toolbar_id)
                        });
                        if !over_note_popup {
                            close_popup = true;
                        }
                    }
                }
            }

            if close_popup {
                self.clicked_highlight_id = None;
                self.editing_note_buf.clear();
            }
        }

        // 鈹€鈹€ CSC correction click detection + popup 鈹€鈹€
        {
            // Check if user clicked on a correction rect (ReadWrite mode)
            let any_click = ui.ctx().input(|i| i.pointer.primary_clicked());
            if any_click && self.csc_popup.is_none() && self.text_selection.is_none() {
                if let Some(click_pos) = ui.ctx().pointer_interact_pos() {
                    CSC_RECTS.with(|rects| {
                        let r = rects.borrow();
                        for cr in r.iter() {
                            if cr.rect.contains(click_pos) {
                                self.csc_popup = Some(crate::app::CscPopupInfo {
                                    chapter: cr.key.chapter,
                                    block_idx: cr.key.block,
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
                            // Info line: original 鈫?corrected (confidence%)
                            ui.horizontal(|ui| {
                                ui.colored_label(Color32::from_rgb(220, 60, 50), &popup.original);
                                ui.label("→");
                                ui.colored_label(Color32::from_rgb(60, 180, 80), &popup.corrected);
                                ui.label(format!(
                                    "  {}: {:.1}%",
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
