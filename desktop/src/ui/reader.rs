use std::cell::Cell;
use std::cell::RefCell;
use std::sync::Arc;

use eframe::egui;
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontFamily, FontId, UiBuilder};

use crate::app::{ReaderApp, TextSelection};
use reader_core::epub::{ContentBlock, InlineStyle, TextSpan};

// ── Thread-local spacing configuration ──
thread_local! {
    static LINE_SPACING: Cell<f32> = const { Cell::new(1.8) };
    static PARA_SPACING: Cell<f32> = const { Cell::new(0.6) };
    static TEXT_INDENT:   Cell<f32> = const { Cell::new(2.0) };
}

fn set_spacing(line: f32, para: f32, indent: f32) {
    LINE_SPACING.set(line);
    PARA_SPACING.set(para);
    TEXT_INDENT.set(indent);
}
fn line_spacing() -> f32 {
    LINE_SPACING.get()
}
fn para_spacing() -> f32 {
    PARA_SPACING.get()
}
fn text_indent() -> f32 {
    TEXT_INDENT.get()
}

// ── Thread-local deferred actions & per-frame block galley cache ──

/// Per-frame cache entry: (block_idx, galley, screen_rect, plain_text)
type BlockGalleyEntry = (usize, Arc<egui::Galley>, egui::Rect, String);

thread_local! {
    /// Collected during render_block, consumed by render_reader for selection state machine.
    static BLOCK_GALLEYS: RefCell<Vec<BlockGalleyEntry>> = const { RefCell::new(Vec::new()) };
    /// TTS read-along highlight: Some(block_idx) when TTS is actively reading a block.
    static TTS_HIGHLIGHT_BLOCK: Cell<Option<usize>> = const { Cell::new(None) };
    /// CSC corrections for the current chapter: block_idx → Vec<CorrectionInfo>.
    /// Set before rendering, read inside render_block for Ruby annotation painting.
    static CSC_CORRECTIONS: RefCell<std::collections::HashMap<usize, Vec<reader_core::epub::CorrectionInfo>>>
        = RefCell::new(std::collections::HashMap::new());
    /// Whether ReadWrite mode is active (enables click-on-correction popups).
    static CSC_READWRITE: Cell<bool> = const { Cell::new(false) };
    /// Correction rects collected during render_block, consumed in render_reader for click detection.
    static CSC_RECTS: RefCell<Vec<CscRect>> = const { RefCell::new(Vec::new()) };
}

/// A clickable correction rect collected during rendering.
struct CscRect {
    block_idx: usize,
    char_offset: usize,
    original: String,
    corrected: String,
    confidence: f32,
    rect: egui::Rect,
}

/// Selection highlight colour (temporary blue overlay while dragging / toolbar open).
const SEL_BG: Color32 = Color32::from_rgba_premultiplied(66, 135, 245, 70);

/// TTS read-along highlight colour (soft blue tint, works on both dark and light themes).
const TTS_BG: Color32 = Color32::from_rgba_premultiplied(56, 132, 255, 30);

/// TTS accent bar color (left edge indicator).
const TTS_ACCENT: Color32 = Color32::from_rgb(56, 132, 255);

/// Paint TTS read-along highlight: soft background + left accent bar.
fn paint_tts_highlight(ui: &egui::Ui, rect: egui::Rect) {
    let r = rect.expand2(egui::vec2(4.0, 2.0));
    ui.painter()
        .rect_filled(r, egui::CornerRadius::same(3), TTS_BG);
    let bar = egui::Rect::from_min_size(
        egui::pos2(r.left() - 3.0, r.top()),
        egui::vec2(3.0, r.height()),
    );
    ui.painter()
        .rect_filled(bar, egui::CornerRadius::same(1), TTS_ACCENT);
}

/// Build mapping from block char_offset → galley char index.
/// Accounts for `wrap_cjk_text` inserting extra '\n' characters.
fn build_csc_char_mapping(spans: &[TextSpan], font_size: f32, max_width: f32) -> Vec<usize> {
    let mut mapping = Vec::new();
    let mut galley_pos = 0usize;
    for (si, span) in spans.iter().enumerate() {
        let leading = if si == 0 {
            font_size * text_indent()
        } else {
            0.0
        };
        let wrapped = wrap_cjk_text(&span.text, font_size, max_width, leading);
        let orig_chars: Vec<char> = span.text.chars().collect();
        let wrap_chars: Vec<char> = wrapped.chars().collect();
        let mut oi = 0;
        for (wi, &wc) in wrap_chars.iter().enumerate() {
            if oi < orig_chars.len() && wc == orig_chars[oi] {
                mapping.push(galley_pos + wi);
                oi += 1;
            }
        }
        galley_pos += wrap_chars.len();
    }
    mapping
}

// Layout constants
const DUAL_COLUMN_THRESHOLD: f32 = 1050.0;
const MAX_TEXT_WIDTH_SINGLE: f32 = 850.0;
const DUAL_COLUMN_GAP: f32 = 30.0;
const DUAL_COLUMN_PADDING: f32 = 64.0;
const MAX_COLUMN_WIDTH: f32 = 600.0;
const MIN_COLUMN_MARGIN: f32 = 28.0;
const SINGLE_MIN_MARGIN: f32 = 40.0;
const SINGLE_TEXT_PADDING: f32 = 80.0;
const TITLE_SPACING: f32 = 40.0;
const FRAME_MARGIN: f32 = 104.0;

/// Semi-transparent highlighter colours (fluorescent pen effect).
fn highlight_bg_color(color: &reader_core::library::HighlightColor) -> Color32 {
    use reader_core::library::HighlightColor;
    match color {
        HighlightColor::Yellow => Color32::from_rgba_unmultiplied(255, 245, 140, 70),
        HighlightColor::Green => Color32::from_rgba_unmultiplied(144, 238, 144, 60),
        HighlightColor::Blue => Color32::from_rgba_unmultiplied(135, 206, 250, 60),
        HighlightColor::Pink => Color32::from_rgba_unmultiplied(255, 182, 193, 60),
    }
}

/// Text colour when highlighted — darkened tone related to the background.
fn highlight_text_color(color: &reader_core::library::HighlightColor) -> Color32 {
    use reader_core::library::HighlightColor;
    match color {
        HighlightColor::Yellow => Color32::from_rgb(120, 90, 0),
        HighlightColor::Green => Color32::from_rgb(20, 100, 30),
        HighlightColor::Blue => Color32::from_rgb(15, 60, 130),
        HighlightColor::Pink => Color32::from_rgb(140, 20, 60),
    }
}

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
                    if self.scroll_to_top {
                        scroll_area = scroll_area.vertical_scroll_offset(0.0);
                        self.scroll_to_top = false;
                    }
                    scroll_area.show(ui, |ui| {
                        Self::render_content_layout(
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
                                            Self::render_content_layout(
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
                                                Self::render_content_layout(
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
                                            Self::render_content_layout(
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
                                                Self::render_content_layout(
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
                                Self::render_content_layout(
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
                                        Self::render_content_layout(
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
                                Self::render_content_layout(
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
                                    Self::render_content_layout(
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
                                            Self::render_content_layout(
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
                                            Self::render_content_layout(
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
                                Self::render_content_layout(
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
                            Self::render_content_layout(
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
                    self.current_chapter = idx;
                    self.current_page = 0;
                    self.scroll_to_top = true;
                    self.pages_dirty = true;
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

        // Detect primary pointer press / drag / release for selection
        let pointer_pos = ui.ctx().input(|i| i.pointer.interact_pos());
        let primary_down = ui.ctx().input(|i| i.pointer.primary_down());
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
        let over_toolbar = ui.ctx().memory(|mem| {
            mem.layer_id_at(pointer_pos.unwrap_or_default())
                .is_some_and(|layer| layer.id == toolbar_id || layer.id == note_toolbar_id)
        });

        if let Some(pos) = pointer_pos {
            const DRAG_THRESHOLD: f32 = 5.0;

            if primary_pressed && !over_toolbar {
                if let Some((block_idx, char_idx)) = hit_test(pos) {
                    // Record press origin; don't create TextSelection yet
                    self.sel_press_origin = Some((pos, block_idx, char_idx));
                    // Clear any existing finalized selection or highlight popup
                    if self.text_selection.as_ref().is_some_and(|s| !s.is_dragging) {
                        self.text_selection = None;
                    }
                    if self.clicked_highlight_id.is_some() {
                        self.clicked_highlight_id = None;
                    }
                } else {
                    // Clicked outside any block → clear everything
                    self.sel_press_origin = None;
                    self.text_selection = None;
                    self.clicked_highlight_id = None;
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
                                        self.trigger_page_animation_to(
                                            self.current_page + 2,
                                            1.0,
                                        );
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
                });

            // Close popup on click outside (skip the frame it was just opened)
            if !close_popup {
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

        // ── CSC correction click detection + popup ──
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

    #[allow(clippy::too_many_arguments)]
    fn render_content_layout(
        ui: &mut egui::Ui,
        h_margin: f32,
        text_width: f32,
        title: &str,
        blocks: &[ContentBlock],
        block_start: usize,
        block_end: usize,
        show_title: bool,
        font_size: f32,
        bg_color: Color32,
        current_chapter: usize,
        total_ch: usize,
        action_prev: &mut bool,
        action_next: &mut bool,
        action_go_back: &mut bool,
        show_chapter_nav: bool,
        has_previous_chapter: bool,
        font_color: Option<Color32>,
        font_family_name: &str,
        i18n: &reader_core::i18n::I18n,
        clicked_link: &mut Option<String>,
        highlight_ranges: &std::collections::HashMap<
            usize,
            Vec<(usize, usize, reader_core::library::HighlightColor)>,
        >,
    ) {
        egui::Frame::new()
            .inner_margin(egui::Margin {
                left: 0,
                right: 0,
                top: 48,
                bottom: 56,
            })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(h_margin);
                    ui.vertical(|ui| {
                        ui.set_max_width(text_width);
                        if show_title {
                            let title_color = effective_text_color(bg_color, font_color);
                            let title_family = match font_family_name {
                                "Monospace" => FontFamily::Monospace,
                                "Serif" => FontFamily::Name("Serif".into()),
                                "Sans" => FontFamily::Proportional,
                                other => FontFamily::Name(other.into()),
                            };
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    egui::RichText::new(title)
                                        .size(font_size * 1.8)
                                        .strong()
                                        .color(title_color)
                                        .family(title_family),
                                );
                            });
                            ui.add_space(TITLE_SPACING);
                        }
                        let effective_start = if show_title && block_start == 0 {
                            if matches!(blocks.first(), Some(ContentBlock::Heading { .. })) {
                                1
                            } else {
                                0
                            }
                        } else {
                            block_start
                        };
                        for (i, block) in blocks[effective_start..block_end].iter().enumerate() {
                            let abs_idx = effective_start + i;
                            let hl_ranges = highlight_ranges
                                .get(&abs_idx)
                                .map(|v| v.as_slice())
                                .unwrap_or(&[]);
                            render_block(
                                ui,
                                block,
                                font_size,
                                bg_color,
                                text_width,
                                font_color,
                                font_family_name,
                                i18n,
                                clicked_link,
                                abs_idx,
                                hl_ranges,
                            );
                        }
                        if show_chapter_nav {
                            ui.add_space(60.0);
                            if has_previous_chapter {
                                ui.vertical_centered(|ui| {
                                    if ui
                                        .button(
                                            egui::RichText::new(i18n.t("reader.go_back_chapter"))
                                                .size(15.0),
                                        )
                                        .clicked()
                                    {
                                        *action_go_back = true;
                                    }
                                });
                                ui.add_space(8.0);
                            }
                            ui.separator();
                            ui.add_space(30.0);
                            ui.horizontal(|ui| {
                                if current_chapter > 0
                                    && ui
                                        .button(
                                            egui::RichText::new(i18n.t("reader.prev_chapter"))
                                                .size(16.0),
                                        )
                                        .clicked()
                                {
                                    *action_prev = true;
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if current_chapter + 1 < total_ch
                                            && ui
                                                .button(
                                                    egui::RichText::new(
                                                        i18n.t("reader.next_chapter"),
                                                    )
                                                    .size(16.0),
                                                )
                                                .clicked()
                                        {
                                            *action_next = true;
                                        }
                                    },
                                );
                            });
                        }
                    });
                    ui.add_space(h_margin);
                });
            });
    }
}

fn effective_text_color(bg_color: Color32, font_color: Option<Color32>) -> Color32 {
    let bg_lum = {
        let [r, g, b, _] = bg_color.to_array();
        (r as f32 * 0.299 + g as f32 * 0.587 + b as f32 * 0.114) / 255.0
    };
    font_color.unwrap_or_else(|| {
        if bg_lum < 0.45 {
            Color32::from_gray(220)
        } else {
            Color32::from_gray(30)
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn render_block(
    ui: &mut egui::Ui,
    block: &ContentBlock,
    font_size: f32,
    bg_color: Color32,
    max_width: f32,
    font_color: Option<Color32>,
    font_family_name: &str,
    i18n: &reader_core::i18n::I18n,
    clicked_link: &mut Option<String>,
    chapter_block_idx: usize,
    highlight_ranges: &[(usize, usize, reader_core::library::HighlightColor)],
) {
    match block {
        ContentBlock::Heading { level, spans } => {
            let scale = match level {
                1 => 2.0,
                2 => 1.6,
                3 => 1.3,
                _ => 1.2,
            };
            let job = build_layout_job(
                spans,
                font_size * scale,
                bg_color,
                true,
                max_width,
                font_color,
                font_family_name,
                &[],
            );
            ui.add_space(font_size * 0.8);
            let is_tts_block = TTS_HIGHLIGHT_BLOCK.get() == Some(chapter_block_idx);
            let galley = ui.painter().layout_job(job);
            let galley_size = galley.size();
            let (rect, response) =
                ui.allocate_exact_size(galley_size, egui::Sense::click_and_drag());

            if is_tts_block {
                paint_tts_highlight(ui, rect);
            }
            ui.painter()
                .galley(rect.min, galley.clone(), Color32::PLACEHOLDER);

            // Handle individual link clicks via pointer position matching
            if let Some(hover_pos) = ui.ctx().pointer_hover_pos() {
                if rect.contains(hover_pos) {
                    let cursor = galley.cursor_from_pos(hover_pos - rect.min);
                    let mut cumulative = 0;
                    let mut hovered_url = None;
                    for span in spans {
                        let span_len = span.text.chars().count();
                        if cursor.ccursor.index >= cumulative
                            && cursor.ccursor.index < cumulative + span_len
                        {
                            hovered_url = span.link_url.clone();
                            break;
                        }
                        cumulative += span_len;
                    }

                    if let Some(url) = hovered_url {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        if response.clicked() {
                            *clicked_link = Some(url);
                        }
                    }
                }
            }
            ui.add_space(font_size * 0.4);
        }
        ContentBlock::Paragraph { spans } => {
            let is_readwrite = CSC_READWRITE.get();

            // ── Build display spans and annotation info based on CSC state ──
            // annotation tuple: (char_offset, top_text, confidence, status_tag)
            //   status_tag: 0=Pending(ReadOnly ruby), 1=Pending(ReadWrite underline),
            //               2=Accepted, 3=Rejected
            let (display_spans, csc_annotations) = CSC_CORRECTIONS.with(|csc| {
                let map = csc.borrow();
                let empty_result = (
                    spans.to_vec(),
                    Vec::<(usize, String, String, f32, u8)>::new(),
                );
                let corrections = match map.get(&chapter_block_idx) {
                    Some(c) if !c.is_empty() => c,
                    _ => return empty_result,
                };

                let mut modified = Vec::new();
                let mut annotations: Vec<(usize, String, String, f32, u8)> = Vec::new();
                let mut offset = 0usize;

                for span in spans {
                    let chars: Vec<char> = span.text.chars().collect();
                    let mut new_text = String::new();
                    for (i, &ch) in chars.iter().enumerate() {
                        let abs = offset + i;
                        if let Some(corr) = corrections.iter().find(|c| c.char_offset == abs) {
                            match (&corr.status, is_readwrite) {
                                // Pending + ReadWrite: show original text (underline later)
                                (reader_core::epub::CorrectionStatus::Pending, true) => {
                                    new_text.push(ch);
                                    annotations.push((
                                        abs,
                                        corr.corrected.clone(),
                                        corr.original.clone(),
                                        corr.confidence,
                                        1,
                                    ));
                                }
                                // Pending + ReadOnly: show corrected text, original gray above
                                (reader_core::epub::CorrectionStatus::Pending, false) => {
                                    if corr.corrected.chars().count() == 1 {
                                        new_text.push_str(&corr.corrected);
                                        annotations.push((
                                            abs,
                                            corr.original.clone(),
                                            corr.corrected.clone(),
                                            corr.confidence,
                                            0,
                                        ));
                                    } else {
                                        new_text.push(ch);
                                    }
                                }
                                // Accepted: show corrected text, original gray above
                                (reader_core::epub::CorrectionStatus::Accepted, _) => {
                                    if corr.corrected.chars().count() == 1 {
                                        new_text.push_str(&corr.corrected);
                                        annotations.push((
                                            abs,
                                            corr.original.clone(),
                                            corr.corrected.clone(),
                                            corr.confidence,
                                            2,
                                        ));
                                    } else {
                                        new_text.push(ch);
                                    }
                                }
                                // Rejected: show original text, corrected gray above
                                (reader_core::epub::CorrectionStatus::Rejected, _) => {
                                    new_text.push(ch);
                                    annotations.push((
                                        abs,
                                        corr.corrected.clone(),
                                        corr.original.clone(),
                                        corr.confidence,
                                        3,
                                    ));
                                }
                                // Ignored: show original, no annotation
                                _ => {
                                    new_text.push(ch);
                                }
                            }
                        } else {
                            new_text.push(ch);
                        }
                    }
                    modified.push(TextSpan {
                        text: new_text,
                        ..span.clone()
                    });
                    offset += chars.len();
                }
                (modified, annotations)
            });

            let job = build_layout_job(
                &display_spans,
                font_size,
                bg_color,
                false,
                max_width,
                font_color,
                font_family_name,
                highlight_ranges,
            );
            let text: String = display_spans.iter().map(|s| s.text.as_str()).collect();

            // Layout into Galley, allocate space, and paint manually
            let galley = ui.painter().layout_job(job);
            let galley_size = galley.size();
            let (rect, response) =
                ui.allocate_exact_size(galley_size, egui::Sense::click_and_drag());
            // TTS read-along highlight (paint behind text)
            if TTS_HIGHLIGHT_BLOCK.get() == Some(chapter_block_idx) {
                paint_tts_highlight(ui, rect);
            }
            ui.painter()
                .galley(rect.min, galley.clone(), Color32::PLACEHOLDER);

            // ── Paint CSC annotations ──
            if !csc_annotations.is_empty() {
                let mapping = build_csc_char_mapping(&display_spans, font_size, max_width);
                let ruby_font = FontId::new(font_size * 0.45, FontFamily::Proportional);
                let ruby_color = Color32::from_gray(140);

                for &(char_offset, ref top_text, ref _main_text, confidence, tag) in
                    &csc_annotations
                {
                    if char_offset >= mapping.len() {
                        continue;
                    }
                    let gi = mapping[char_offset];
                    let cursor_start = galley.from_ccursor(egui::text::CCursor::new(gi));
                    let cursor_end = galley.from_ccursor(egui::text::CCursor::new(gi + 1));
                    let pos_start = galley.pos_from_cursor(&cursor_start);
                    let pos_end = galley.pos_from_cursor(&cursor_end);

                    let x = rect.min.x + pos_start.min.x;
                    // Use next cursor's min.x for character width; fallback to font_size
                    let x2 = if pos_end.min.y == pos_start.min.y && pos_end.min.x > pos_start.min.x
                    {
                        rect.min.x + pos_end.min.x
                    } else {
                        // Next char wrapped to new line — estimate width from font size
                        x + font_size
                    };
                    let y_top = rect.min.y + pos_start.min.y;
                    let y_bottom = rect.min.y + pos_start.max.y;

                    match tag {
                        // ReadWrite + Pending: red underline + hover tooltip + click rect
                        1 => {
                            // Draw red underline
                            let underline_y = y_bottom - 1.0;
                            ui.painter().line_segment(
                                [egui::pos2(x, underline_y), egui::pos2(x2, underline_y)],
                                egui::Stroke::new(2.0, Color32::from_rgb(220, 60, 50)),
                            );

                            // Hover tooltip: show corrected char + confidence
                            let char_rect = egui::Rect::from_min_max(
                                egui::pos2(x, y_top),
                                egui::pos2(x2, y_bottom),
                            );
                            if let Some(hover_pos) = ui.ctx().pointer_hover_pos() {
                                if char_rect.contains(hover_pos) {
                                    egui::show_tooltip_at_pointer(
                                        ui.ctx(),
                                        ui.layer_id(),
                                        egui::Id::new("csc_rw_tooltip"),
                                        |ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(_main_text);
                                                ui.colored_label(
                                                    Color32::from_rgb(60, 180, 80),
                                                    format!("→ {}", top_text),
                                                );
                                                ui.label(format!("({:.0}%)", confidence * 100.0));
                                            });
                                        },
                                    );
                                }
                            }

                            // Store rect for click detection (popup)
                            let click_rect = egui::Rect::from_min_max(
                                egui::pos2(x, y_top - 2.0),
                                egui::pos2(x2, y_bottom + 2.0),
                            );
                            CSC_RECTS.with(|r| {
                                r.borrow_mut().push(CscRect {
                                    block_idx: chapter_block_idx,
                                    char_offset,
                                    original: _main_text.clone(), // original text
                                    corrected: top_text.clone(),  // corrected text
                                    confidence,
                                    rect: click_rect,
                                });
                            });
                        }
                        // ReadOnly Pending / Accepted / Rejected: Ruby annotation (top_text above)
                        0 | 2 | 3 => {
                            ui.painter().text(
                                egui::pos2(x, y_top - 1.0),
                                egui::Align2::LEFT_BOTTOM,
                                top_text,
                                ruby_font.clone(),
                                ruby_color,
                            );
                        }
                        _ => {}
                    }
                }
            }

            // Push into per-frame cache for the selection state machine
            BLOCK_GALLEYS.with(|bg| {
                bg.borrow_mut()
                    .push((chapter_block_idx, galley.clone(), rect, text.clone()));
            });

            // Handle individual link clicks via pointer position matching
            if let Some(hover_pos) = ui.ctx().pointer_hover_pos() {
                if rect.contains(hover_pos) {
                    let cursor = galley.cursor_from_pos(hover_pos - rect.min);
                    let mut cumulative = 0;
                    let mut hovered_url = None;
                    for span in &display_spans {
                        let span_len = span.text.chars().count();
                        if cursor.ccursor.index >= cumulative
                            && cursor.ccursor.index < cumulative + span_len
                        {
                            hovered_url = span.link_url.clone();
                            break;
                        }
                        cumulative += span_len;
                    }

                    if let Some(url) = hovered_url {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        if response.clicked() {
                            *clicked_link = Some(url);
                        }
                    }
                }
            }
            ui.add_space(font_size * para_spacing());
        }
        ContentBlock::Separator => {
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);
        }
        ContentBlock::BlankLine => {
            ui.add_space(font_size * 0.5);
        }
        ContentBlock::Image { alt, .. } => {
            ui.add_space(font_size * 0.6);
            let text = alt
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .map(|s| i18n.tf1("reader.image_with_alt", s))
                .unwrap_or_else(|| i18n.t("reader.image").to_string());
            ui.label(egui::RichText::new(text).italics().color(Color32::GRAY));
            ui.add_space(font_size * 0.6);
        }
    }
}

/// Result from the selection toolbar.
enum SelToolbarResult {
    KeepOpen,
    Close,
    /// User clicked a highlight colour – create the highlight with saved selection.
    CreateHighlight(reader_core::library::HighlightColor),
    /// Delete all highlights overlapping this block.
    DeleteHighlight,
    /// User wants to custom-replace the selected text (ReadWrite CSC mode).
    CustomReplace,
}

/// Render the floating selection toolbar as an `egui::Area` above
/// the block the user just finished selecting.
fn show_selection_toolbar(
    ctx: &egui::Context,
    i18n: &reader_core::i18n::I18n,
    text: &str,
    pos: egui::Pos2,
    has_highlight: bool,
    is_csc_readwrite: bool,
) -> SelToolbarResult {
    let mut result = SelToolbarResult::KeepOpen;
    egui::Area::new(egui::Id::new("sel_toolbar"))
        .fixed_pos(egui::pos2(pos.x - 140.0, pos.y - 36.0))
        .order(egui::Order::Foreground)
        .interactable(true)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Copy
                    if ui.small_button(i18n.t("context.copy")).clicked() {
                        ui.ctx().copy_text(text.to_string());
                        result = SelToolbarResult::Close;
                    }
                    // Highlight colour buttons (softer preview colours)
                    for (color, c32) in [
                        (
                            reader_core::library::HighlightColor::Yellow,
                            Color32::from_rgb(255, 245, 140),
                        ),
                        (
                            reader_core::library::HighlightColor::Green,
                            Color32::from_rgb(144, 238, 144),
                        ),
                        (
                            reader_core::library::HighlightColor::Blue,
                            Color32::from_rgb(135, 206, 250),
                        ),
                        (
                            reader_core::library::HighlightColor::Pink,
                            Color32::from_rgb(255, 182, 193),
                        ),
                    ] {
                        let btn = egui::Button::new(" ")
                            .fill(c32)
                            .min_size(egui::vec2(18.0, 18.0));
                        if ui.add(btn).clicked() {
                            result = SelToolbarResult::CreateHighlight(color);
                        }
                    }
                    // Delete highlight (only shown when block has existing highlight)
                    if has_highlight && ui.small_button("x").on_hover_text("删除标注").clicked()
                    {
                        result = SelToolbarResult::DeleteHighlight;
                    }
                    // Dictionary
                    if ui.small_button(i18n.t("context.dictionary")).clicked() {
                        let trimmed = text.chars().take(20).collect::<String>();
                        let url = format!("https://www.zdic.net/hans/{}", trimmed);
                        let _ = open::that(&url);
                        result = SelToolbarResult::Close;
                    }
                    // Translate
                    if ui.small_button(i18n.t("context.translate")).clicked() {
                        let trimmed: String = text.chars().take(200).collect();
                        let encoded = urlencoding::encode(&trimmed);
                        let url = format!(
                            "https://translate.google.com/?sl=auto&tl=en&text={}",
                            encoded,
                        );
                        let _ = open::that(&url);
                        result = SelToolbarResult::Close;
                    }
                    // Custom Replace (only in ReadWrite CSC mode)
                    if is_csc_readwrite
                        && ui.small_button(i18n.t("csc.custom_replace")).clicked() {
                            result = SelToolbarResult::CustomReplace;
                        }
                });
            });
        });
    result
}

#[allow(clippy::too_many_arguments)]
fn build_layout_job(
    spans: &[TextSpan],
    font_size: f32,
    bg_color: Color32,
    is_heading: bool,
    max_width: f32,
    font_color: Option<Color32>,
    font_family_name: &str,
    highlight_ranges: &[(usize, usize, reader_core::library::HighlightColor)],
) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = max_width;
    let bg_lum = {
        let [r, g, b, _] = bg_color.to_array();
        (r as f32 * 0.299 + g as f32 * 0.587 + b as f32 * 0.114) / 255.0
    };
    let base_color = effective_text_color(bg_color, font_color);
    let link_color = if bg_lum < 0.45 {
        Color32::from_rgb(100, 160, 255)
    } else {
        Color32::from_rgb(30, 80, 200)
    };

    // Helper: lookup highlight that covers a given char position
    let hl_at = |char_pos: usize| -> Option<&reader_core::library::HighlightColor> {
        highlight_ranges.iter().find_map(|(s, e, c)| {
            if char_pos >= *s && char_pos < *e {
                Some(c)
            } else {
                None
            }
        })
    };

    let mut char_offset: usize = 0; // cumulative char offset across spans
    for (i, span) in spans.iter().enumerate() {
        let is_bold =
            matches!(span.style, InlineStyle::Bold | InlineStyle::BoldItalic) || is_heading;
        let is_italic = matches!(span.style, InlineStyle::Italic | InlineStyle::BoldItalic);
        let is_link = span.link_url.is_some();
        let family = if is_bold {
            FontFamily::Name("Bold".into())
        } else {
            match font_family_name {
                "Monospace" => FontFamily::Monospace,
                "Serif" => FontFamily::Name("Serif".into()),
                "Sans" => FontFamily::Proportional,
                other => FontFamily::Name(other.into()),
            }
        };
        let normal_color = if is_link { link_color } else { base_color };
        let leading = if i == 0 && !is_heading {
            font_size * text_indent()
        } else {
            0.0
        };
        let wrapped = wrap_cjk_text(
            &span.text,
            font_size,
            max_width,
            if i == 0 && !is_heading { leading } else { 0.0 },
        );

        // If no highlights touch this span, fast-path: append entire span at once
        let span_char_len = span.text.chars().count();
        let span_start = char_offset;
        let span_end = char_offset + span_char_len;
        let any_hl = highlight_ranges
            .iter()
            .any(|(s, e, _)| *s < span_end && *e > span_start);

        if !any_hl || highlight_ranges.is_empty() {
            let format = TextFormat {
                font_id: FontId::new(font_size, family),
                color: normal_color,
                italics: is_italic,
                underline: if is_link {
                    egui::Stroke::new(1.0, link_color)
                } else {
                    egui::Stroke::NONE
                },
                line_height: Some(font_size * line_spacing()),
                ..Default::default()
            };
            job.append(&wrapped, leading, format);
        } else {
            // Split span text at highlight boundaries
            let mut first_section = true;
            let chars: Vec<char> = wrapped.chars().collect();
            let mut ci = 0usize; // index into chars
            let mut cur_hl = hl_at(span_start);
            let mut seg_start = 0usize;

            for (j, _ch) in chars.iter().enumerate() {
                let abs_pos = span_start + j;
                let this_hl = hl_at(abs_pos);
                let same = match (&cur_hl, &this_hl) {
                    (None, None) => true,
                    (Some(a), Some(b)) => std::mem::discriminant(*a) == std::mem::discriminant(*b),
                    _ => false,
                };
                if !same {
                    // Flush segment
                    let seg_text: String = chars[seg_start..j].iter().collect();
                    let (fg, bg_c) = match cur_hl {
                        Some(hc) => (highlight_text_color(hc), highlight_bg_color(hc)),
                        None => (normal_color, Color32::TRANSPARENT),
                    };
                    let format = TextFormat {
                        font_id: FontId::new(font_size, family.clone()),
                        color: fg,
                        italics: is_italic,
                        underline: if is_link {
                            egui::Stroke::new(1.0, link_color)
                        } else {
                            egui::Stroke::NONE
                        },
                        background: bg_c,
                        line_height: Some(font_size * line_spacing()),
                        ..Default::default()
                    };
                    let lead = if first_section { leading } else { 0.0 };
                    job.append(&seg_text, lead, format);
                    first_section = false;
                    seg_start = j;
                    cur_hl = this_hl;
                }
                ci = j + 1;
            }
            // Flush final segment
            let seg_text: String = chars[seg_start..ci].iter().collect();
            let (fg, bg_c) = match cur_hl {
                Some(hc) => (highlight_text_color(hc), highlight_bg_color(hc)),
                None => (normal_color, Color32::TRANSPARENT),
            };
            let format = TextFormat {
                font_id: FontId::new(font_size, family),
                color: fg,
                italics: is_italic,
                underline: if is_link {
                    egui::Stroke::new(1.0, link_color)
                } else {
                    egui::Stroke::NONE
                },
                background: bg_c,
                line_height: Some(font_size * line_spacing()),
                ..Default::default()
            };
            let lead = if first_section { leading } else { 0.0 };
            job.append(&seg_text, lead, format);
        }

        char_offset = span_end;
    }
    if job.sections.is_empty() {
        job.append(
            " ",
            0.0,
            TextFormat {
                font_id: FontId::new(font_size, FontFamily::Proportional),
                color: Color32::TRANSPARENT,
                line_height: Some(font_size * 1.0),
                ..Default::default()
            },
        );
    }
    job
}

fn estimate_block_height(
    block: &ContentBlock,
    font_size: f32,
    line_height: f32,
    max_width: f32,
) -> f32 {
    match block {
        ContentBlock::Heading { level, spans } => {
            let scale = match level {
                1 => 2.0,
                2 => 1.6,
                3 => 1.3,
                _ => 1.2,
            };
            let sz = font_size * scale;
            let text_len: f32 = spans.iter().map(|s| estimate_text_width(&s.text, sz)).sum();
            (text_len / max_width).ceil().max(1.0) * sz * line_spacing() + font_size * 1.2
        }
        ContentBlock::Paragraph { spans } => {
            let text_len: f32 = spans
                .iter()
                .map(|s| estimate_text_width(&s.text, font_size))
                .sum();
            ((text_len + font_size * text_indent()) / max_width)
                .ceil()
                .max(1.0)
                * line_height
                + font_size * para_spacing()
        }
        ContentBlock::Separator => 24.0,
        ContentBlock::BlankLine => font_size * 0.5,
        ContentBlock::Image { .. } => font_size * 3.0,
    }
}

fn estimate_text_width(text: &str, font_size: f32) -> f32 {
    text.chars()
        .map(|c| {
            if c.is_ascii() {
                font_size * 0.55
            } else {
                font_size
            }
        })
        .sum()
}

fn wrap_cjk_text(text: &str, font_size: f32, max_width: f32, first_line_indent: f32) -> String {
    const NO_BREAK_BEFORE: &[char] = &[
        '\u{3002}', '\u{FF0C}', '\u{FF01}', '\u{FF1F}', '\u{FF1B}', '\u{FF1A}', '\u{3001}',
        '\u{FF09}', '\u{300B}', '\u{300D}', '\u{300F}', '\u{3011}', '\u{3015}', '\u{3009}',
        '\u{3017}', '\u{FF5E}', '\u{2026}', ',', '.', '!', '?', ';', ':', ')', ']', '}',
        '\u{2014}', '\u{2013}', '\u{201C}', '\u{201D}', '\u{2018}', '\u{2019}',
    ];
    const NO_BREAK_AFTER: &[char] = &[
        '\u{FF08}', '\u{300A}', '\u{300C}', '\u{300E}', '\u{3010}', '\u{3014}', '\u{3008}',
        '\u{3016}', '(', '[', '{', '\u{201C}', '\u{201D}', '\u{2018}', '\u{2019}',
    ];
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    let effective_max = max_width - font_size * 0.5;
    // Use a Vec<char> buffer to track the result, avoiding repeated String/Vec<char> conversions
    let mut buf: Vec<char> = Vec::with_capacity(chars.len() + chars.len() / 8);
    let mut line_width: f32 = first_line_indent;
    let char_width = |c: char| -> f32 {
        if c.is_ascii() {
            font_size * 0.55
        } else {
            font_size
        }
    };
    for (i, &ch) in chars.iter().enumerate() {
        let cw = char_width(ch);
        if line_width + cw > effective_max && i > 0 && ch != '\n' {
            if NO_BREAK_BEFORE.contains(&ch) {
                // Backtrack: find a good break point before the no-break-before char
                let mut backtrack = 0;
                let mut pos = buf.len();
                while pos > 0 && NO_BREAK_BEFORE.contains(&buf[pos - 1]) {
                    pos -= 1;
                    backtrack += 1;
                    if backtrack > 5 {
                        break;
                    }
                }
                if pos == buf.len() && pos > 0 {
                    pos -= 1;
                }
                if pos > 0 && NO_BREAK_AFTER.contains(&buf[pos - 1]) && pos > 1 {
                    pos -= 1;
                }
                if pos > 0 && pos < buf.len() {
                    buf.insert(pos, '\n');
                    line_width = buf[pos + 1..].iter().map(|&c| char_width(c)).sum();
                } else {
                    buf.push('\n');
                    line_width = 0.0;
                }
            } else if i > 0 && NO_BREAK_AFTER.contains(&chars[i - 1]) {
                let pos = buf.len().saturating_sub(1);
                if pos > 0 {
                    buf.insert(pos, '\n');
                    line_width = buf[pos + 1..].iter().map(|&c| char_width(c)).sum();
                } else {
                    buf.push('\n');
                    line_width = 0.0;
                }
            } else {
                buf.push('\n');
                line_width = 0.0;
            }
        }
        if ch == '\n' {
            buf.push(ch);
            line_width = 0.0;
        } else {
            buf.push(ch);
            line_width += cw;
        }
    }
    buf.into_iter().collect()
}

fn normalize_epub_href(href: &str) -> String {
    let s = href.trim().split('#').next().unwrap_or("").trim();
    if s.is_empty() {
        return String::new();
    }
    s.trim_start_matches("./").trim_matches('/').to_string()
}
