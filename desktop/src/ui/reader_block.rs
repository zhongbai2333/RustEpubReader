//! 阅读器内容块渲染：段落排版、标题渲染、选区工具栏、高亮着色等。

use eframe::egui;
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontFamily, FontId};

use reader_core::epub::{ContentBlock, InlineStyle, TextSpan};

use super::reader_state::*;

// ── Content layout (was `ReaderApp::render_content_layout`, no `&self`) ──

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_content_layout(
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
                                                egui::RichText::new(i18n.t("reader.next_chapter"))
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

// ── Block-level rendering ──

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_block(
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
        ContentBlock::Heading { level, spans, .. } => {
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
                ui.allocate_exact_size(galley_size, egui::Sense::click());

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
        ContentBlock::Paragraph { spans, .. } => {
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

                // Pre-build O(1) lookup by char_offset
                let corr_map: std::collections::HashMap<usize, &reader_core::epub::CorrectionInfo> =
                    corrections.iter().map(|c| (c.char_offset, c)).collect();

                let mut modified = Vec::new();
                let mut annotations: Vec<(usize, String, String, f32, u8)> = Vec::new();
                let mut offset = 0usize;

                for span in spans {
                    let chars: Vec<char> = span.text.chars().collect();
                    let mut new_text = String::new();
                    for (i, &ch) in chars.iter().enumerate() {
                        let abs = offset + i;
                        if let Some(corr) = corr_map.get(&abs) {
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
                ui.allocate_exact_size(galley_size, egui::Sense::click());
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
                // Pre-fetch hover position once, skip per-char hover checks if pointer outside block
                let hover_in_block = ui.ctx().pointer_hover_pos().filter(|p| rect.contains(*p));

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
                            if let Some(hover_pos) = hover_in_block {
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

// ── Selection toolbar ──

/// Result from the selection toolbar.
pub(crate) enum SelToolbarResult {
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
pub(crate) fn show_selection_toolbar(
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
                    if is_csc_readwrite && ui.small_button(i18n.t("csc.custom_replace")).clicked() {
                        result = SelToolbarResult::CustomReplace;
                    }
                });
            });
        });
    result
}

// ── Layout job builder ──

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_layout_job(
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

// ── Block height estimation ──

pub(crate) fn estimate_block_height(
    block: &ContentBlock,
    font_size: f32,
    line_height: f32,
    max_width: f32,
) -> f32 {
    match block {
        ContentBlock::Heading { level, spans, .. } => {
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
        ContentBlock::Paragraph { spans, .. } => {
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
