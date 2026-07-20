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

// ── Thread-local deferred actions & per-frame block galley cache ──

/// A block identifier that remains unique when several chapters share one scroll area.
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

/// A text block laid out in the current frame for selection and hit testing.
#[derive(Clone)]
pub(crate) struct BlockGalleyEntry {
    pub(crate) key: BlockKey,
    pub(crate) galley: Arc<egui::Galley>,
    pub(crate) rect: egui::Rect,
    pub(crate) text: String,
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

pub(crate) struct ClickedLink {
    pub(crate) source_chapter: usize,
    pub(crate) url: String,
}

/// Chapter-level lazy loading and virtualization state for continuous scrolling.
#[derive(Debug, Default)]
pub(crate) struct ContinuousScrollState {
    pub(crate) start_chapter: usize,
    pub(crate) loaded_end: usize,
    pub(crate) visible_chapter: usize,
    pub(crate) chapter_heights: HashMap<usize, f32>,
    scroll_offset: f32,
    max_scroll_offset: f32,
    wheel_target_offset: Option<f32>,
    wheel_suppress_until: f64,
    layout_signature: Option<u64>,
    initialized: bool,
}

impl ContinuousScrollState {
    pub(crate) fn reset(&mut self, chapter: usize, total_chapters: usize) {
        let chapter = chapter.min(total_chapters.saturating_sub(1));
        self.start_chapter = chapter;
        self.loaded_end = (chapter + 1).min(total_chapters);
        self.visible_chapter = chapter;
        self.chapter_heights.clear();
        self.scroll_offset = 0.0;
        self.max_scroll_offset = 0.0;
        self.wheel_target_offset = None;
        self.wheel_suppress_until = 0.0;
        self.initialized = total_chapters > 0;
    }

    pub(crate) fn needs_reset(&self, current_chapter: usize, total_chapters: usize) -> bool {
        !self.initialized
            || self.loaded_end > total_chapters
            || self.start_chapter >= self.loaded_end
            || current_chapter < self.start_chapter
            || current_chapter >= self.loaded_end
            || self.visible_chapter != current_chapter
    }

    pub(crate) fn append_next(&mut self, total_chapters: usize) -> Option<usize> {
        if self.loaded_end >= total_chapters {
            return None;
        }
        let appended = self.loaded_end;
        self.loaded_end += 1;
        Some(appended)
    }

    pub(crate) fn set_visible_chapter(&mut self, chapter: usize) {
        self.visible_chapter = chapter;
    }

    pub(crate) fn update_layout_signature(&mut self, signature: u64) -> bool {
        let changed = self
            .layout_signature
            .is_some_and(|previous| previous != signature);
        if changed {
            self.chapter_heights.clear();
        }
        self.layout_signature = Some(signature);
        changed
    }

    pub(crate) fn record_height(&mut self, chapter: usize, height: f32) {
        if height.is_finite() && height > 0.0 {
            self.chapter_heights.insert(chapter, height);
        }
    }

    pub(crate) fn begin_wheel_scroll(&mut self, raw_delta_y: f32, now: f64) {
        let current_target = self.wheel_target_offset.unwrap_or(self.scroll_offset);
        self.wheel_target_offset =
            Some((current_target - raw_delta_y).clamp(0.0, self.max_scroll_offset));
        self.wheel_suppress_until = now + 0.35;
    }

    pub(crate) fn cancel_wheel_scroll(&mut self) {
        self.wheel_target_offset = None;
    }

    pub(crate) fn suppress_default_wheel_scroll(&self, now: f64) -> bool {
        now < self.wheel_suppress_until
    }

    pub(crate) fn advance_wheel_scroll(&mut self, dt: f32) -> Option<f32> {
        let target = self.wheel_target_offset?;
        let factor = 1.0 - (-24.0 * dt.clamp(0.0, 0.05)).exp();
        self.scroll_offset += (target - self.scroll_offset) * factor;
        if (target - self.scroll_offset).abs() < 0.5 {
            self.scroll_offset = target;
            self.wheel_target_offset = None;
        }
        Some(self.scroll_offset)
    }

    pub(crate) fn record_scroll_output(
        &mut self,
        offset: f32,
        content_height: f32,
        viewport_height: f32,
    ) {
        self.max_scroll_offset = (content_height - viewport_height).max(0.0);
        self.scroll_offset = offset.clamp(0.0, self.max_scroll_offset);
        if let Some(target) = &mut self.wheel_target_offset {
            *target = target.clamp(0.0, self.max_scroll_offset);
        }
    }
}

pub(crate) struct ContinuousLayoutSettings<'a> {
    pub(crate) text_width: f32,
    pub(crate) font_size: f32,
    pub(crate) title_font_scale: f32,
    pub(crate) line_spacing: f32,
    pub(crate) para_spacing: f32,
    pub(crate) text_indent: f32,
    pub(crate) latin_font: &'a str,
    pub(crate) cjk_font: &'a str,
}

pub(crate) fn continuous_layout_signature(settings: ContinuousLayoutSettings<'_>) -> u64 {
    let mut hasher = DefaultHasher::new();
    settings.text_width.to_bits().hash(&mut hasher);
    settings.font_size.to_bits().hash(&mut hasher);
    settings.title_font_scale.to_bits().hash(&mut hasher);
    settings.line_spacing.to_bits().hash(&mut hasher);
    settings.para_spacing.to_bits().hash(&mut hasher);
    settings.text_indent.to_bits().hash(&mut hasher);
    settings.latin_font.hash(&mut hasher);
    settings.cjk_font.hash(&mut hasher);
    hasher.finish()
}

pub(crate) fn remaining_scroll_height(
    content_height: f32,
    viewport_height: f32,
    offset: f32,
) -> f32 {
    let max_offset = (content_height - viewport_height).max(0.0);
    (max_offset - offset).max(0.0)
}

pub(crate) fn continuous_load_threshold(viewport_height: f32) -> f32 {
    viewport_height.clamp(600.0, 1600.0)
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

    #[test]
    fn continuous_scroll_handles_two_and_hundred_chapter_books() {
        for total_chapters in [2, 100] {
            let mut state = ContinuousScrollState::default();
            state.reset(0, total_chapters);
            let mut appended = Vec::new();

            while let Some(chapter) = state.append_next(total_chapters) {
                appended.push(chapter);
            }

            assert_eq!(appended, (1..total_chapters).collect::<Vec<_>>());
            assert_eq!(state.loaded_end, total_chapters);
            assert_eq!(state.append_next(total_chapters), None);
        }
    }

    #[test]
    fn continuous_scroll_detects_external_chapter_navigation() {
        let mut state = ContinuousScrollState::default();
        state.reset(4, 10);
        assert!(!state.needs_reset(4, 10));

        assert_eq!(state.append_next(10), Some(5));
        state.set_visible_chapter(5);
        assert!(!state.needs_reset(5, 10));
        assert!(state.needs_reset(7, 10));
    }

    #[test]
    fn remaining_height_is_clamped_for_short_and_completed_content() {
        assert_eq!(remaining_scroll_height(400.0, 800.0, 0.0), 0.0);
        assert_eq!(remaining_scroll_height(2400.0, 800.0, 1200.0), 400.0);
        assert_eq!(remaining_scroll_height(2400.0, 800.0, 2000.0), 0.0);
    }

    #[test]
    fn repeated_wheel_steps_accumulate_without_losing_distance() {
        let mut state = ContinuousScrollState::default();
        state.record_scroll_output(100.0, 1100.0, 100.0);

        state.begin_wheel_scroll(-40.0, 1.0);
        state.begin_wheel_scroll(-40.0, 1.01);

        assert_eq!(state.wheel_target_offset, Some(180.0));
    }

    #[test]
    fn layout_change_invalidates_measured_chapter_heights() {
        let mut state = ContinuousScrollState::default();
        state.reset(0, 2);
        state.update_layout_signature(10);
        state.record_height(0, 1200.0);

        assert!(!state.update_layout_signature(10));
        assert_eq!(state.chapter_heights.get(&0), Some(&1200.0));
        assert!(state.update_layout_signature(11));
        assert!(state.chapter_heights.is_empty());
    }
}
