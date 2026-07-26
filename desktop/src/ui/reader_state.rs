//! 阅读器状态与工具函数：线程局部缓存、排版常量、CSC 标注辅助、文本测量等。

use std::cell::Cell;
use std::cell::RefCell;
use std::collections::{hash_map::DefaultHasher, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use eframe::egui;
use egui::Color32;

use reader_core::epub::TextSpan;

// ── Thread-local spacing configuration ──
thread_local! {
    static LINE_SPACING: Cell<f32> = const { Cell::new(1.8) };
    static PARA_SPACING: Cell<f32> = const { Cell::new(0.6) };
    static TEXT_INDENT:   Cell<f32> = const { Cell::new(2.0) };
}

pub(crate) fn set_spacing(line: f32, para: f32, indent: f32) {
    LINE_SPACING.set(line);
    PARA_SPACING.set(para);
    TEXT_INDENT.set(indent);
}
pub(crate) fn line_spacing() -> f32 {
    LINE_SPACING.get()
}
pub(crate) fn para_spacing() -> f32 {
    PARA_SPACING.get()
}
pub(crate) fn text_indent() -> f32 {
    TEXT_INDENT.get()
}

/// State for the chapter window used by continuous reading mode.
/// This remains independent from paging state during the incremental rollout.
#[derive(Debug, Default)]
pub(crate) struct ContinuousScrollState {
    pub(crate) start_chapter: usize,
    pub(crate) loaded_end: usize,
    pub(crate) visible_chapter: usize,
    pub(crate) chapter_heights: HashMap<usize, f32>,
    pub(crate) scroll_offset: f32,
    pub(crate) max_scroll_offset: f32,
    pending_scroll_adjustment: f32,
    awaiting_prepend_measurement: bool,
    layout_signature: Option<u64>,
    initialized: bool,
}

impl ContinuousScrollState {
    pub(crate) fn reset(&mut self, chapter: usize, total_chapters: usize) {
        if total_chapters == 0 {
            *self = Self::default();
            return;
        }
        let chapter = chapter.min(total_chapters - 1);
        self.start_chapter = chapter;
        self.loaded_end = chapter + 1;
        self.visible_chapter = chapter;
        self.chapter_heights.clear();
        self.scroll_offset = 0.0;
        self.max_scroll_offset = 0.0;
        self.layout_signature = None;
        self.awaiting_prepend_measurement = false;
        self.initialized = true;
    }

    pub(crate) fn needs_reset(&self, current_chapter: usize, total_chapters: usize) -> bool {
        !self.initialized
            || total_chapters == 0
            || self.loaded_end > total_chapters
            || self.start_chapter >= self.loaded_end
            || current_chapter < self.start_chapter
            || current_chapter >= self.loaded_end
    }

    pub(crate) fn append_next(&mut self, total_chapters: usize) -> Option<usize> {
        if !self.initialized || self.loaded_end >= total_chapters {
            return None;
        }
        let chapter = self.loaded_end;
        self.loaded_end += 1;
        Some(chapter)
    }

    pub(crate) fn prepend_previous(&mut self) -> Option<usize> {
        if !self.initialized || self.start_chapter == 0 {
            return None;
        }
        self.start_chapter -= 1;
        if let Some(height) = self.chapter_heights.get(&self.start_chapter).copied() {
            self.pending_scroll_adjustment += height;
        } else {
            self.awaiting_prepend_measurement = true;
        }
        Some(self.start_chapter)
    }

    pub(crate) fn trim_window(&mut self) {
        const MAX_LOADED_CHAPTERS: usize = 4;
        while self.loaded_end.saturating_sub(self.start_chapter) > MAX_LOADED_CHAPTERS
            && self.start_chapter < self.visible_chapter
        {
            let Some(height) = self.chapter_heights.get(&self.start_chapter).copied() else {
                break;
            };
            self.pending_scroll_adjustment -= height;
            self.start_chapter += 1;
        }
    }

    pub(crate) fn trim_bottom(&mut self) {
        const MAX_LOADED_CHAPTERS: usize = 4;
        while self.loaded_end.saturating_sub(self.start_chapter) > MAX_LOADED_CHAPTERS
            && self.loaded_end > self.visible_chapter + 1
        {
            self.loaded_end -= 1;
        }
    }

    pub(crate) fn take_scroll_adjustment(&mut self) -> f32 {
        std::mem::take(&mut self.pending_scroll_adjustment)
    }

    pub(crate) fn near_end(&self) -> bool {
        self.max_scroll_offset <= 0.0
            || self.scroll_offset >= self.max_scroll_offset - self.prefetch_distance()
    }

    fn prefetch_distance(&self) -> f32 {
        600.0
    }

    pub(crate) fn set_visible_chapter(&mut self, chapter: usize) {
        self.visible_chapter = chapter;
    }

    pub(crate) fn update_layout_signature(&mut self, signature: u64) -> bool {
        let changed = self.layout_signature.is_some_and(|old| old != signature);
        if changed {
            self.chapter_heights.clear();
        }
        self.layout_signature = Some(signature);
        changed
    }

    pub(crate) fn record_height(&mut self, chapter: usize, height: f32) {
        if height.is_finite() && height > 0.0 {
            self.chapter_heights.insert(chapter, height);
            if self.awaiting_prepend_measurement && chapter == self.start_chapter {
                self.pending_scroll_adjustment += height;
                self.awaiting_prepend_measurement = false;
            }
        }
    }

    pub(crate) fn record_scroll_output(
        &mut self,
        offset: f32,
        content_height: f32,
        viewport_height: f32,
    ) {
        self.max_scroll_offset = (content_height - viewport_height).max(0.0);
        self.scroll_offset = offset.clamp(0.0, self.max_scroll_offset);
    }

    pub(crate) fn near_start(&self) -> bool {
        !self.awaiting_prepend_measurement
            && self.pending_scroll_adjustment == 0.0
            && self.scroll_offset <= self.prefetch_distance()
            && self.start_chapter > 0
    }
}

// ── Thread-local deferred actions & per-frame block galley cache ──

/// A block identifier that remains unique when multiple chapters share a scroll area.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct BlockKey {
    pub(crate) chapter: usize,
    pub(crate) block: usize,
}

impl BlockKey {
    pub(crate) const fn new(chapter: usize, block: usize) -> Self {
        Self { chapter, block }
    }
}

/// Per-frame cache entry used by selection, context menus, and position tracking.
#[derive(Clone)]
pub(crate) struct BlockGalleyEntry {
    pub(crate) key: BlockKey,
    pub(crate) galley: Arc<egui::Galley>,
    pub(crate) rect: egui::Rect,
    pub(crate) text: String,
    pub(crate) response: egui::Response,
}

pub(crate) type CscCharMapCacheData = (u64, u32, u32, Vec<usize>);

thread_local! {
    /// Collected during render_block, consumed by render_reader for selection state machine.
    pub(crate) static BLOCK_GALLEYS: RefCell<Vec<BlockGalleyEntry>> = const { RefCell::new(Vec::new()) };
    /// TTS read-along highlight for the chapter and block currently being read.
    pub(crate) static TTS_HIGHLIGHT_BLOCK: Cell<Option<BlockKey>> = const { Cell::new(None) };
    /// CSC corrections keyed by chapter and block.
    /// Set before rendering, read inside render_block for Ruby annotation painting.
    pub(crate) static CSC_CORRECTIONS: RefCell<std::collections::HashMap<BlockKey, Vec<reader_core::epub::CorrectionInfo>>>
        = RefCell::new(std::collections::HashMap::new());
    /// Whether ReadWrite mode is active (enables click-on-correction popups).
    pub(crate) static CSC_READWRITE: Cell<bool> = const { Cell::new(false) };
    /// Correction rects collected during render_block, consumed in render_reader for click detection.
    pub(crate) static CSC_RECTS: RefCell<Vec<CscRect>> = const { RefCell::new(Vec::new()) };
    /// Cache for `build_csc_char_mapping`: (content_hash, font_size_bits, max_width_bits) → mapping.
    static CSC_CHAR_MAP_CACHE: RefCell<Option<CscCharMapCacheData>> = const { RefCell::new(None) };
}

/// A clickable correction rect collected during rendering.
pub(crate) struct CscRect {
    pub(crate) key: BlockKey,
    pub(crate) char_offset: usize,
    pub(crate) original: String,
    pub(crate) corrected: String,
    pub(crate) confidence: f32,
    pub(crate) rect: egui::Rect,
}

/// Selection highlight colour (temporary blue overlay while dragging / toolbar open).
pub(crate) const SEL_BG: Color32 = Color32::from_rgba_premultiplied(66, 135, 245, 70);

/// TTS read-along highlight colour (soft blue tint, works on both dark and light themes).
pub(crate) const TTS_BG: Color32 = Color32::from_rgba_premultiplied(56, 132, 255, 30);

/// TTS accent bar color (left edge indicator).
pub(crate) const TTS_ACCENT: Color32 = Color32::from_rgb(56, 132, 255);

/// Paint TTS read-along highlight: soft background + left accent bar.
pub(crate) fn paint_tts_highlight(ui: &egui::Ui, rect: egui::Rect) {
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
fn build_csc_char_mapping_inner(spans: &[TextSpan], font_size: f32, max_width: f32) -> Vec<usize> {
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

/// Cached version of char mapping builder. Reuses previous result when
/// spans content, font_size and max_width haven't changed.
pub(crate) fn build_csc_char_mapping(
    spans: &[TextSpan],
    font_size: f32,
    max_width: f32,
) -> Vec<usize> {
    let mut hasher = DefaultHasher::new();
    for s in spans {
        s.text.hash(&mut hasher);
    }
    let content_hash = hasher.finish();
    let fs_bits = font_size.to_bits();
    let mw_bits = max_width.to_bits();

    CSC_CHAR_MAP_CACHE.with(|cache| {
        let cached = cache.borrow();
        if let Some((h, f, m, ref mapping)) = *cached {
            if h == content_hash && f == fs_bits && m == mw_bits {
                return mapping.clone();
            }
        }
        drop(cached);
        let mapping = build_csc_char_mapping_inner(spans, font_size, max_width);
        *cache.borrow_mut() = Some((content_hash, fs_bits, mw_bits, mapping.clone()));
        mapping
    })
}

// Layout constants
pub(crate) const DUAL_COLUMN_THRESHOLD: f32 = 1050.0;
pub(crate) const DUAL_COLUMN_GAP: f32 = 30.0;
pub(crate) const DUAL_COLUMN_PADDING: f32 = 48.0;
pub(crate) const MIN_COLUMN_MARGIN: f32 = 20.0;
pub(crate) const SINGLE_MIN_MARGIN: f32 = 24.0;
pub(crate) const SINGLE_TEXT_PADDING: f32 = 48.0;
pub(crate) const TITLE_SPACING: f32 = 40.0;
pub(crate) const READER_CONTENT_TOP_PADDING: f32 = 48.0;
pub(crate) const READER_CONTENT_BOTTOM_PADDING: f32 = 80.0;
pub(crate) const FRAME_MARGIN: f32 = READER_CONTENT_TOP_PADDING + READER_CONTENT_BOTTOM_PADDING;

/// Computed reader content layout for the current viewport.
pub(crate) struct ReaderTextLayout {
    pub(crate) is_dual_column: bool,
    pub(crate) text_width: f32,
    pub(crate) h_margin: f32,
}

/// Choose dual-column layout only when the viewport is both wide enough and landscape-like.
///
/// The original fixed width caps (`850px` for single column and `600px` for each dual column)
/// made text occupy less than half of the page on high-DPI / 4K displays. This responsive
/// calculation keeps a modest visual margin while allowing the text area to scale with the
/// actual egui point width.
pub(crate) fn reader_text_layout(
    available_width: f32,
    available_height: f32,
    scroll_mode: bool,
) -> ReaderTextLayout {
    let landscape_enough = available_width >= available_height * 1.15;
    let is_dual_column =
        !scroll_mode && available_width > DUAL_COLUMN_THRESHOLD && landscape_enough;

    if is_dual_column {
        let col_w = ((available_width - DUAL_COLUMN_GAP) / 2.0).max(1.0);
        let margin = (col_w * 0.04).clamp(MIN_COLUMN_MARGIN, DUAL_COLUMN_PADDING / 2.0);
        ReaderTextLayout {
            is_dual_column,
            text_width: (col_w - margin * 2.0).max(1.0),
            h_margin: margin,
        }
    } else {
        let margin = (available_width * 0.06).clamp(SINGLE_MIN_MARGIN, SINGLE_TEXT_PADDING);
        ReaderTextLayout {
            is_dual_column,
            text_width: (available_width - margin * 2.0).max(1.0),
            h_margin: margin,
        }
    }
}

/// Semi-transparent highlighter colours (fluorescent pen effect).
pub(crate) fn highlight_bg_color(color: &reader_core::library::HighlightColor) -> Color32 {
    use reader_core::library::HighlightColor;
    match color {
        HighlightColor::Yellow => Color32::from_rgba_unmultiplied(255, 245, 140, 70),
        HighlightColor::Green => Color32::from_rgba_unmultiplied(144, 238, 144, 60),
        HighlightColor::Blue => Color32::from_rgba_unmultiplied(135, 206, 250, 60),
        HighlightColor::Pink => Color32::from_rgba_unmultiplied(255, 182, 193, 60),
    }
}

/// Text colour when highlighted — darkened tone related to the background.
pub(crate) fn highlight_text_color(color: &reader_core::library::HighlightColor) -> Color32 {
    use reader_core::library::HighlightColor;
    match color {
        HighlightColor::Yellow => Color32::from_rgb(120, 90, 0),
        HighlightColor::Green => Color32::from_rgb(20, 100, 30),
        HighlightColor::Blue => Color32::from_rgb(15, 60, 130),
        HighlightColor::Pink => Color32::from_rgb(140, 20, 60),
    }
}

pub(crate) fn effective_text_color(bg_color: Color32, font_color: Option<Color32>) -> Color32 {
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

pub(crate) fn estimate_text_width(text: &str, font_size: f32) -> f32 {
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

pub(crate) fn wrap_cjk_text(
    text: &str,
    font_size: f32,
    max_width: f32,
    first_line_indent: f32,
) -> String {
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

pub(crate) fn normalize_epub_href(href: &str) -> String {
    let s = href.trim().split('#').next().unwrap_or("").trim();
    if s.is_empty() {
        return String::new();
    }
    s.trim_start_matches("./").trim_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuous_scroll_loads_chapters_incrementally() {
        let mut state = ContinuousScrollState::default();
        state.reset(2, 5);

        assert_eq!(state.start_chapter, 2);
        assert_eq!(state.loaded_end, 3);
        assert_eq!(state.append_next(5), Some(3));
        assert_eq!(state.append_next(5), Some(4));
        assert_eq!(state.append_next(5), None);
    }

    #[test]
    fn continuous_scroll_keeps_a_bounded_window() {
        let mut state = ContinuousScrollState::default();
        state.reset(0, 8);
        for chapter in 0..4 {
            state.record_height(chapter, 100.0);
            assert_eq!(state.append_next(8), Some(chapter + 1));
        }
        state.set_visible_chapter(1);
        state.trim_window();

        assert_eq!(state.start_chapter, 1);
        assert_eq!(state.loaded_end, 5);
        assert_eq!(state.take_scroll_adjustment(), -100.0);
    }

    #[test]
    fn continuous_scroll_prepending_restores_scroll_anchor() {
        let mut state = ContinuousScrollState::default();
        state.reset(2, 5);
        state.record_height(1, 240.0);
        assert_eq!(state.prepend_previous(), Some(1));
        assert_eq!(state.start_chapter, 1);
        assert_eq!(state.take_scroll_adjustment(), 240.0);
    }

    #[test]
    fn continuous_scroll_waits_for_prepend_measurement() {
        let mut state = ContinuousScrollState::default();
        state.reset(2, 5);
        state.set_visible_chapter(2);
        assert_eq!(state.prepend_previous(), Some(1));
        assert!(!state.near_start());
        state.record_height(1, 180.0);
        assert_eq!(state.take_scroll_adjustment(), 180.0);
        assert!(state.near_start());
    }

    #[test]
    fn continuous_scroll_trims_bottom_when_loading_previous() {
        let mut state = ContinuousScrollState::default();
        state.reset(2, 8);
        state.loaded_end = 6;
        state.set_visible_chapter(2);
        assert_eq!(state.prepend_previous(), Some(1));
        state.trim_bottom();
        assert_eq!(state.start_chapter, 1);
        assert_eq!(state.loaded_end, 5);
    }

    #[test]
    fn continuous_scroll_invalidates_measured_heights_on_layout_change() {
        let mut state = ContinuousScrollState::default();
        state.reset(0, 2);
        assert!(!state.update_layout_signature(10));
        state.record_height(0, 1200.0);
        assert!(!state.update_layout_signature(10));
        assert!(state.update_layout_signature(11));
        assert!(state.chapter_heights.is_empty());
    }

    #[test]
    fn continuous_scroll_clamps_scroll_output() {
        let mut state = ContinuousScrollState::default();
        state.reset(0, 2);
        state.record_scroll_output(900.0, 1000.0, 400.0);
        assert_eq!(state.max_scroll_offset, 600.0);
        assert_eq!(state.scroll_offset, 600.0);
        assert!(state.near_end());

        state.record_scroll_output(0.0, 2400.0, 400.0);
        assert!(!state.near_end());
    }

    #[test]
    fn dual_column_scales_on_4k_width() {
        let layout = reader_text_layout(2560.0, 1440.0, false);

        assert!(layout.is_dual_column);
        assert!(layout.text_width > 900.0);
        assert!(layout.h_margin <= DUAL_COLUMN_PADDING / 2.0);
    }

    #[test]
    fn portrait_view_stays_single_column_and_uses_width() {
        let layout = reader_text_layout(1200.0, 2000.0, false);

        assert!(!layout.is_dual_column);
        assert!(layout.text_width > 1000.0);
        assert_eq!(layout.h_margin, SINGLE_TEXT_PADDING);
    }

    #[test]
    fn scroll_mode_stays_single_column() {
        let layout = reader_text_layout(2560.0, 1440.0, true);

        assert!(!layout.is_dual_column);
        assert!(layout.text_width > 2400.0);
    }
}
