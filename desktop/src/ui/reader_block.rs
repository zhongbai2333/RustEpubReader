//! 阅读器内容块渲染：段落排版、标题渲染、选区工具栏、高亮着色等。

use eframe::egui;
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontFamily, FontId};

use reader_core::epub::{ContentBlock, InlineStyle, TextSpan};

use super::reader_state::*;

#[inline]
fn record_reader_galley(_ui: &egui::Ui, galley: &egui::Galley, origin: egui::Pos2, ruby: f32) {
    #[cfg(feature = "performance-checks")]
    crate::native_corpus::observe_galley(_ui.ctx(), galley);
    super::reader_pagination::record_galley(galley, origin, ruby);
}

#[inline]
fn record_selectable_galley(_ui: &egui::Ui, entry: BlockGalleyEntry) {
    #[cfg(feature = "performance-checks")]
    crate::native_corpus::observe_selection(_ui.ctx(), &entry);
    BLOCK_GALLEYS.with(|galleys| galleys.borrow_mut().push(entry));
}

pub(crate) fn family_from_name(name: &str) -> FontFamily {
    match name {
        "" => FontFamily::Name("ReaderFont".into()),
        "Monospace" => FontFamily::Monospace,
        "Serif" => FontFamily::Name("Serif".into()),
        "Sans" => FontFamily::Proportional,
        other => FontFamily::Name(other.into()),
    }
}

fn uses_cjk_font(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0x3040..=0x30FF
            | 0x2018
            | 0x2019
            | 0x201C
            | 0x201D
            | 0x3000..=0x303F
            | 0xFE10..=0xFE1F
            | 0xFE30..=0xFE4F
            | 0xFF01..=0xFF65
    )
}

fn append_font_runs(
    job: &mut LayoutJob,
    text: &str,
    leading: f32,
    base_format: TextFormat,
    cjk_family: &FontFamily,
) {
    if base_format.font_id.family == *cjk_family {
        job.append(text, leading, base_format);
        return;
    }
    let mut chars = text.char_indices();
    let Some((_, first)) = chars.next() else {
        return;
    };
    let mut run_start = 0;
    let mut run_is_cjk = uses_cjk_font(first);
    let mut first_run = true;

    for (index, ch) in chars.chain(std::iter::once((text.len(), '\0'))) {
        let is_cjk = index < text.len() && uses_cjk_font(ch);
        if index < text.len() && is_cjk == run_is_cjk {
            continue;
        }
        let mut format = base_format.clone();
        if run_is_cjk {
            format.font_id = FontId::new(format.font_id.size, cjk_family.clone());
        }
        job.append(
            &text[run_start..index],
            if first_run { leading } else { 0.0 },
            format,
        );
        first_run = false;
        run_start = index;
        run_is_cjk = is_cjk;
    }
}

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
    fragment: Option<super::reader_pagination::PageFragment>,
    font_size: f32,
    title_font_scale: f32,
    bg_color: Color32,
    current_chapter: usize,
    font_color: Option<Color32>,
    font_family_name: &str,
    i18n: &reader_core::i18n::I18n,
    clicked_link: &mut Option<String>,
    highlight_ranges: &std::collections::HashMap<
        usize,
        Vec<(usize, usize, reader_core::library::HighlightColor)>,
    >,
) {
    super::reader_images::reading_column(blocks, text_width, font_size, title_font_scale);
    let show_title = show_title
        && !blocks
            .first()
            .is_some_and(|b| matches!(b, ContentBlock::Heading { .. }));
    let original_clip = ui.clip_rect();
    let mut fragment_ui;
    let ui = if let Some(fragment) = fragment {
        let top = ui.cursor().top();
        let visible_top = top + READER_CONTENT_TOP_PADDING;
        let clip = egui::Rect::from_min_max(
            egui::pos2(ui.max_rect().left(), visible_top),
            egui::pos2(ui.max_rect().right(), visible_top + fragment.height.max(0.)),
        )
        .intersect(original_clip);
        fragment_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt(("page_fragment", block_start, fragment.offset.to_bits()))
                .max_rect(ui.max_rect().translate(egui::vec2(0., -fragment.offset)))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        fragment_ui.set_clip_rect(clip);
        &mut fragment_ui
    } else {
        ui
    };
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: 0,
            right: 0,
            top: READER_CONTENT_TOP_PADDING as i8,
            bottom: READER_CONTENT_BOTTOM_PADDING as i8,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(h_margin);
                ui.vertical(|ui| {
                    ui.set_max_width(text_width);
                    ui.spacing_mut().item_spacing.y = 0.;
                    if show_title {
                        let title_color = effective_text_color(bg_color, font_color);
                        let title_family = family_from_name(font_family_name);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new(title)
                                    .size(font_size * title_font_scale)
                                    .strong()
                                    .color(title_color)
                                    .family(title_family),
                            );
                        });
                        ui.add_space(TITLE_SPACING);
                    }
                    let effective_start = block_start;
                    for (i, block) in blocks[effective_start..block_end].iter().enumerate() {
                        let abs_idx = effective_start + i;
                        let hl_ranges = highlight_ranges
                            .get(&abs_idx)
                            .map(|v| v.as_slice())
                            .unwrap_or(&[]);
                        let media = super::reader_images::media_for(BlockKey::new(
                            current_chapter,
                            abs_idx,
                        ));
                        let replaced_heading = show_title
                            && abs_idx == 0
                            && matches!(block, ContentBlock::Heading { .. });
                        let text_height = if replaced_heading {
                            0.
                        } else {
                            estimate_block_height(
                                block,
                                font_size,
                                title_font_scale,
                                font_size * line_spacing(),
                                text_width,
                            )
                        };
                        let limit = super::reader_images::media_limit(
                            text_height,
                            media.len(),
                            font_size,
                            super::reader_images::current_limit(),
                        );
                        for image in media.iter().filter(|image| !image.after) {
                            super::reader_images::styled_paint(
                                ui,
                                &image.data,
                                image.alt.as_deref(),
                                text_width,
                                limit,
                                font_size,
                                i18n,
                                &block.native_style(),
                            );
                        }
                        if !replaced_heading {
                            render_block(
                                ui,
                                block,
                                font_size,
                                title_font_scale,
                                bg_color,
                                text_width,
                                font_color,
                                font_family_name,
                                i18n,
                                clicked_link,
                                BlockKey::new(current_chapter, abs_idx),
                                hl_ranges,
                            );
                        }
                        for image in media.iter().filter(|image| image.after) {
                            super::reader_images::styled_paint(
                                ui,
                                &image.data,
                                image.alt.as_deref(),
                                text_width,
                                limit,
                                font_size,
                                i18n,
                                &block.native_style(),
                            );
                        }
                    }
                });
                ui.add_space(h_margin);
            });
        });
}

type NativeGalleyCacheEntry = (
    std::sync::Weak<egui::Galley>,
    std::sync::Arc<egui::Galley>,
    u64,
);

#[derive(Clone, Default)]
struct NativeGalleyCache {
    entries: std::collections::HashMap<(usize, u64), NativeGalleyCacheEntry>,
    clock: u64,
}

// egui center/right rows have negative coordinates around their alignment
// origin. Move glyphs and meshes into the native reading column together so
// painting, hit testing and highlights use the same coordinates.
fn native_galley(
    ui: &egui::Ui,
    mut job: LayoutJob,
    _style: &reader_core::epub::native::NativeStyle,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    // egui applies explicit line_height to the glyph's decoration rectangle.
    // Lay out glyphs at their real font height first, then space the rows. This
    // keeps underlines, superscripts and ruby tied to their actual baselines.
    let heights: Vec<_> = job.sections.iter().map(|s| s.format.line_height).collect();
    for section in &mut job.sections {
        section.format.line_height = None;
    }
    let horizontal_align = job.halign;
    let mut galley = ui.painter().layout_job(job);
    let source = std::sync::Arc::downgrade(&galley);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    for height in &heights {
        height.map(f32::to_bits).hash(&mut hasher);
    }
    horizontal_align.hash(&mut hasher);
    width.to_bits().hash(&mut hasher);
    let key = (std::sync::Arc::as_ptr(&galley) as usize, hasher.finish());
    let id = egui::Id::new("reader_native_galleys");
    if let Some(cached) = ui.ctx().data_mut(|store| {
        let cache = store.get_temp_mut_or_default::<NativeGalleyCache>(id);
        cache.clock += 1;
        cache
            .entries
            .get_mut(&key)
            .filter(|entry| entry.0.strong_count() > 0 && entry.0.ptr_eq(&source))
            .map(|entry| {
                entry.2 = cache.clock;
                entry.1.clone()
            })
    }) {
        return cached;
    }
    let value = std::sync::Arc::make_mut(&mut galley);
    let x = match horizontal_align {
        egui::Align::Max => width,
        egui::Align::Center => width * 0.5,
        _ => 0.,
    };
    let mut next_y = 0.;
    let mut mesh_bounds = egui::Rect::NOTHING;
    for row in &mut value.rows {
        let target = row
            .glyphs
            .iter()
            .filter_map(|g| heights[g.section_index as usize])
            .fold(row.rect.height(), f32::max);
        let delta = egui::vec2(
            x,
            next_y - row.rect.top() + (target - row.rect.height()) * 0.5,
        );
        for glyph in &mut row.glyphs {
            glyph.pos += delta;
        }
        for vertex in &mut row.visuals.mesh.vertices {
            vertex.pos += delta;
        }
        row.visuals.mesh_bounds = row.visuals.mesh_bounds.translate(delta);
        mesh_bounds = mesh_bounds.union(row.visuals.mesh_bounds);
        row.rect = egui::Rect::from_min_size(
            egui::pos2(row.rect.left() + x, next_y),
            egui::vec2(row.rect.width(), target),
        );
        next_y += target;
    }
    value.rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, next_y));
    value.mesh_bounds = mesh_bounds;
    ui.ctx().data_mut(|store| {
        let cache = store.get_temp_mut_or_default::<NativeGalleyCache>(id);
        cache.entries.retain(|_, entry| entry.0.strong_count() > 0);
        cache
            .entries
            .insert(key, (source, galley.clone(), cache.clock));
        while cache.entries.len() > 64
            || cache
                .entries
                .values()
                .map(|entry| {
                    entry
                        .1
                        .rows
                        .iter()
                        .map(|row| row.glyphs.len())
                        .sum::<usize>()
                })
                .sum::<usize>()
                > 80_000
        {
            let oldest = cache
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.2)
                .map(|(key, _)| *key)
                .unwrap();
            cache.entries.remove(&oldest);
        }
    });
    galley
}

// ── Block-level rendering ──

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_block(
    ui: &mut egui::Ui,
    block: &ContentBlock,
    font_size: f32,
    title_font_scale: f32,
    bg_color: Color32,
    max_width: f32,
    font_color: Option<Color32>,
    font_family_name: &str,
    i18n: &reader_core::i18n::I18n,
    clicked_link: &mut Option<String>,
    block_key: BlockKey,
    highlight_ranges: &[(usize, usize, reader_core::library::HighlightColor)],
) {
    let style = block.native_style();
    if style.hidden {
        return;
    }
    let font = style.size_at(font_size);
    let (left, right, top, bottom) = native_insets(&style, font, max_width);
    ui.add_space(top);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 0.;
        ui.horizontal(|ui| {
            ui.add_space(left);
            ui.vertical(|ui| {
                ui.set_max_width((max_width - left - right).max(32.));
                render_block_content(
                    ui,
                    block,
                    font_size,
                    title_font_scale,
                    bg_color,
                    (max_width - left - right).max(32.),
                    font_color,
                    font_family_name,
                    i18n,
                    clicked_link,
                    block_key,
                    highlight_ranges,
                );
            });
            ui.add_space(right);
        });
    });
    ui.add_space(bottom);
}
fn native_insets(
    style: &reader_core::epub::native::NativeStyle,
    font: f32,
    width: f32,
) -> (f32, f32, f32, f32) {
    let resolve = |v: &Option<reader_core::epub::native::Dimension>| {
        v.as_ref().map_or(0., |v| v.resolve(font, width))
    };
    let left = (resolve(&style.inset_left) + resolve(&style.padding_left)).min(width * 0.4);
    let right = (resolve(&style.inset_right) + resolve(&style.padding_right)).min(width * 0.4);
    (
        left,
        right,
        resolve(&style.margin_top) + resolve(&style.padding_top),
        resolve(&style.margin_bottom) + resolve(&style.padding_bottom),
    )
}
#[allow(clippy::too_many_arguments)]
fn render_block_content(
    ui: &mut egui::Ui,
    block: &ContentBlock,
    font_size: f32,
    title_font_scale: f32,
    bg_color: Color32,
    max_width: f32,
    font_color: Option<Color32>,
    font_family_name: &str,
    i18n: &reader_core::i18n::I18n,
    clicked_link: &mut Option<String>,
    block_key: BlockKey,
    highlight_ranges: &[(usize, usize, reader_core::library::HighlightColor)],
) {
    let style = block.native_style();
    if style.hidden {
        return;
    }
    let font_size = style.size_at(font_size);
    ui.add_space(font_size * style.before.unwrap_or(0.0));
    if !style.ruby.is_empty() {
        ui.add_space(font_size * 0.65);
    }
    if let Some(action) = style.action {
        let label = style.text.clone().unwrap_or_else(|| match block {
            ContentBlock::Paragraph { spans, .. } | ContentBlock::Heading { spans, .. } => {
                spans.iter().map(|s| s.text.as_str()).collect()
            }
            _ => String::new(),
        });
        if ui.button(label).clicked() {
            *clicked_link = Some(format!("reader-event:{}:{action}", block_key.chapter));
        }
        return;
    }
    if let Some(text) = &style.text {
        let galley = ui.painter().layout(
            text.clone(),
            egui::FontId::new(font_size, family_from_name(font_family_name)),
            effective_text_color(bg_color, font_color),
            max_width,
        );
        let (rect, _) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
        record_reader_galley(ui, &galley, rect.min, 0.);
        ui.painter().galley(rect.min, galley, Color32::PLACEHOLDER);
        ui.add_space(font_size * style.after.unwrap_or(para_spacing()));
        return;
    }
    if !style.list.is_empty() {
        for line in &style.list {
            ui.horizontal_top(|ui| {
                ui.add_space(
                    font_size * 1.2 * line.depth.saturating_sub(style.list_depth as usize) as f32,
                );
                ui.label(
                    egui::RichText::new(&line.marker)
                        .size(font_size)
                        .color(effective_text_color(bg_color, font_color)),
                );
                let available = ui.available_width();
                let spans = [TextSpan {
                    text: line.text.clone(),
                    style: InlineStyle::Normal,
                    link_url: None,
                    correction: None,
                }];
                let mut line_style = style.clone();
                line_style.list.clear();
                line_style.indent = Some(0.);
                line_style.inline.clear();
                let galley = native_galley(
                    ui,
                    build_layout_job(
                        &spans,
                        font_size,
                        bg_color,
                        false,
                        available,
                        font_color,
                        font_family_name,
                        &[],
                        &line_style,
                    ),
                    &line_style,
                    available,
                );
                let (rect, _) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
                record_reader_galley(ui, &galley, rect.min, 0.);
                ui.painter().galley(rect.min, galley, Color32::PLACEHOLDER);
            });
            ui.add_space(font_size * 0.3);
        }
        return;
    }
    if let Some(table) = &style.table {
        let color = effective_text_color(bg_color, font_color);
        let column = max_width / table.columns.max(1) as f32;
        let jobs: Vec<_> = table
            .cells
            .iter()
            .map(|cell| {
                let mut cell_style = style.clone();
                cell_style.table = None;
                cell_style.inline.clear();
                cell_style.indent = Some(0.);
                cell_style.align = cell.align.clone();
                let spans = [TextSpan {
                    text: cell.text.clone(),
                    style: if cell.header {
                        InlineStyle::Bold
                    } else {
                        InlineStyle::Normal
                    },
                    link_url: None,
                    correction: None,
                }];
                let width = (column * cell.span as f32 - 12.).max(16.);
                let galley = native_galley(
                    ui,
                    build_layout_job(
                        &spans,
                        font_size,
                        bg_color,
                        cell.header,
                        width,
                        font_color,
                        font_family_name,
                        &[],
                        &cell_style,
                    ),
                    &cell_style,
                    width,
                );
                (cell, galley)
            })
            .collect();
        let height = jobs
            .iter()
            .map(|(_, g)| g.size().y)
            .fold(font_size, f32::max)
            + 12.;
        let (area, _) = ui.allocate_exact_size(egui::vec2(max_width, height), egui::Sense::hover());
        let mut x = area.left();
        for (cell, galley) in jobs {
            let width = column * cell.span as f32;
            let rect =
                egui::Rect::from_min_size(egui::pos2(x, area.top()), egui::vec2(width, height));
            if cell.header {
                ui.painter()
                    .rect_filled(rect, 0., color.linear_multiply(0.07));
            }
            ui.painter().rect_stroke(
                rect,
                0.,
                egui::Stroke::new(0.5_f32, color.linear_multiply(0.3)),
                egui::StrokeKind::Inside,
            );
            record_reader_galley(ui, &galley, rect.min + egui::vec2(6., 6.), 0.);
            super::reader_pagination::record_table_source(
                &galley,
                rect.top() + 6.,
                cell.start,
                cell.end,
            );
            ui.painter()
                .galley(rect.min + egui::vec2(6., 6.), galley, Color32::PLACEHOLDER);
            x += width;
        }
        ui.add_space(font_size * style.after.unwrap_or(0.));
        return;
    }
    match block {
        ContentBlock::Heading { level, spans, .. } => {
            let scale = if style.size.is_some() || style.size_px.is_some() {
                1.0
            } else {
                match level {
                    1 => title_font_scale * 1.3,
                    2 => title_font_scale * 1.1,
                    3 => title_font_scale * 0.9,
                    _ => title_font_scale * 0.8,
                }
                .max(1.0)
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
                &style,
            );
            if style.before.is_none() {
                ui.add_space(font_size * 0.8);
            }
            let is_tts_block = TTS_HIGHLIGHT_BLOCK.get() == Some(block_key);
            let galley = native_galley(ui, job, &style, max_width);
            record_reader_galley(
                ui,
                &galley,
                ui.cursor().min,
                if style.ruby.is_empty() {
                    0.
                } else {
                    font_size * 0.65
                },
            );
            let galley_size = galley.size();
            let (rect, response) =
                ui.allocate_exact_size(galley_size, egui::Sense::click_and_drag());

            if is_tts_block {
                paint_tts_highlight(ui, rect);
            }
            ui.painter()
                .galley(rect.min, galley.clone(), Color32::PLACEHOLDER);
            paint_native_ruby(
                ui,
                &galley,
                rect,
                &style,
                font_size,
                bg_color,
                font_color,
                font_family_name,
            );

            let text: String = spans.iter().map(|span| span.text.as_str()).collect();
            record_selectable_galley(
                ui,
                BlockGalleyEntry {
                    key: block_key,
                    galley: galley.clone(),
                    rect,
                    clip: ui.clip_rect(),
                    text,
                    response: response.clone(),
                },
            );

            // Handle individual link clicks via pointer position matching
            if let Some(hover_pos) = ui.ctx().pointer_hover_pos() {
                if rect.intersect(ui.clip_rect()).contains(hover_pos) {
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

                    let hovered_url = style
                        .inline
                        .iter()
                        .rev()
                        .find(|r| {
                            cursor.ccursor.index >= r.start
                                && cursor.ccursor.index < r.end
                                && r.note.is_some()
                        })
                        .and_then(|r| r.note.as_ref())
                        .map(|href| format!("reader-note:{href}"))
                        .or(hovered_url);
                    if let Some(url) = hovered_url {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        if response.clicked() {
                            *clicked_link = Some(url);
                        }
                    }
                }
            }
            ui.add_space(font_size * style.after.unwrap_or(0.4));
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
                let corrections = match map.get(&block_key) {
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
                &style,
            );
            let text: String = display_spans.iter().map(|s| s.text.as_str()).collect();

            // Layout into Galley, allocate space, and paint manually
            let galley = native_galley(ui, job, &style, max_width);
            record_reader_galley(
                ui,
                &galley,
                ui.cursor().min,
                if style.ruby.is_empty() {
                    0.
                } else {
                    font_size * 0.65
                },
            );
            let galley_size = galley.size();
            let (rect, response) =
                ui.allocate_exact_size(galley_size, egui::Sense::click_and_drag());
            // TTS read-along highlight (paint behind text)
            if TTS_HIGHLIGHT_BLOCK.get() == Some(block_key) {
                paint_tts_highlight(ui, rect);
            }
            ui.painter()
                .galley(rect.min, galley.clone(), Color32::PLACEHOLDER);
            paint_native_ruby(
                ui,
                &galley,
                rect,
                &style,
                font_size,
                bg_color,
                font_color,
                font_family_name,
            );

            // ── Paint CSC annotations ──
            if !csc_annotations.is_empty() {
                let mapping: Vec<_> = (0..text.chars().count()).collect();
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
                                egui::Stroke::new(2.0_f32, Color32::from_rgb(220, 60, 50)),
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
                                    key: block_key,
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
            record_selectable_galley(
                ui,
                BlockGalleyEntry {
                    key: block_key,
                    galley: galley.clone(),
                    rect,
                    clip: ui.clip_rect(),
                    text: text.clone(),
                    response: response.clone(),
                },
            );

            // Handle individual link clicks via pointer position matching
            if let Some(hover_pos) = ui.ctx().pointer_hover_pos() {
                if rect.intersect(ui.clip_rect()).contains(hover_pos) {
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

                    let hovered_url = style
                        .inline
                        .iter()
                        .rev()
                        .find(|r| {
                            cursor.ccursor.index >= r.start
                                && cursor.ccursor.index < r.end
                                && r.note.is_some()
                        })
                        .and_then(|r| r.note.as_ref())
                        .map(|href| format!("reader-note:{href}"))
                        .or(hovered_url);
                    if let Some(url) = hovered_url {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        if response.clicked() {
                            *clicked_link = Some(url);
                        }
                    }
                }
            }
            ui.add_space(font_size * style.after.unwrap_or(para_spacing()));
        }
        ContentBlock::Separator => {
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);
        }
        ContentBlock::BlankLine => {
            ui.add_space(font_size * 0.5);
        }
        ContentBlock::Image { data, alt, .. } => {
            super::reader_images::styled_paint(
                ui,
                data,
                alt.as_deref(),
                max_width,
                super::reader_images::current_limit(),
                font_size,
                i18n,
                &style,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_native_ruby(
    ui: &egui::Ui,
    galley: &egui::Galley,
    rect: egui::Rect,
    style: &reader_core::epub::native::NativeStyle,
    font: f32,
    bg: Color32,
    fg: Option<Color32>,
    family: &str,
) {
    for ruby in &style.ruby {
        let start =
            galley.pos_from_cursor(&galley.from_ccursor(egui::text::CCursor::new(ruby.start)));
        let end = galley.pos_from_cursor(&galley.from_ccursor(egui::text::CCursor::new(ruby.end)));
        let start_cursor = galley.from_ccursor(egui::text::CCursor::new(ruby.start));
        let end_cursor = galley.from_ccursor(egui::text::CCursor::new(ruby.end));
        let width = if start_cursor.rcursor.row == end_cursor.rcursor.row {
            end.left() - start.left()
        } else {
            font * (ruby.end - ruby.start) as f32
        };
        let ruby_family = if family == "ReaderFont" && ruby.text.chars().any(uses_cjk_font) {
            "ReaderCjk"
        } else {
            family
        };
        let color = effective_text_color(bg, fg);
        let mut annotation = ui.painter().layout_no_wrap(
            ruby.text.clone(),
            FontId::new(font * 0.5, family_from_name(ruby_family)),
            color,
        );
        let extent = width + font;
        if annotation.size().x > extent {
            let size = (font * 0.5 * extent / annotation.size().x).max(4.);
            annotation = ui.painter().layout_no_wrap(
                ruby.text.clone(),
                FontId::new(size, family_from_name(ruby_family)),
                color,
            );
        }
        let cursor = galley.from_ccursor(egui::text::CCursor::new(ruby.start));
        let base_top = galley
            .rows
            .get(cursor.rcursor.row)
            .and_then(|row| row.glyphs.get(cursor.rcursor.column))
            .map_or(start.top(), |glyph| glyph.logical_rect().top());
        let x = rect.left() + start.left() + (width - annotation.size().x) * 0.5;
        ui.painter().galley(
            egui::pos2(x, rect.top() + base_top - annotation.size().y - 1.),
            annotation,
            Color32::PLACEHOLDER,
        );
    }
}

// ── Selection toolbar ──

/// Result from the selection toolbar.
pub(crate) enum SelToolbarResult {
    KeepOpen,
    Service(&'static str),
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
                        let response = ui.add(btn);
                        #[cfg(feature = "performance-checks")]
                        if matches!(color, reader_core::library::HighlightColor::Yellow) {
                            crate::native_corpus::observe_highlight(ctx, response.rect, text);
                        }
                        if response.clicked() {
                            result = SelToolbarResult::CreateHighlight(color);
                        }
                    }
                    // Delete highlight (only shown when block has existing highlight)
                    if has_highlight
                        && ui
                            .small_button("x")
                            .on_hover_text(i18n.t("context.delete_highlight"))
                            .clicked()
                    {
                        result = SelToolbarResult::DeleteHighlight;
                    }
                    // Dictionary
                    if ui.small_button(i18n.t("context.dictionary")).clicked() {
                        result = SelToolbarResult::Service("dictionary");
                    }
                    if ui.small_button(i18n.t("context.translate")).clicked() {
                        result = SelToolbarResult::Service("translate");
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
    style: &reader_core::epub::native::NativeStyle,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = max_width;
    job.halign = match style.align.as_deref() {
        Some("center") => egui::Align::Center,
        Some("right") => egui::Align::Max,
        None if is_heading => egui::Align::Center,
        _ => egui::Align::Min,
    };
    job.justify = style.align.as_deref() == Some("justify");
    let line = style.line.unwrap_or(line_spacing());
    let font_family_name = style
        .family
        .as_deref()
        .filter(|n| {
            matches!(
                *n,
                "Sans" | "Serif" | "Monospace" | "serif" | "sans-serif" | "monospace"
            )
        })
        .map(|n| match n {
            "serif" => "Serif",
            "sans-serif" => "Sans",
            "monospace" => "Monospace",
            n => n,
        })
        .unwrap_or(font_family_name);
    let bg_lum = {
        let [r, g, b, _] = bg_color.to_array();
        (r as f32 * 0.299 + g as f32 * 0.587 + b as f32 * 0.114) / 255.0
    };
    let base_color = if bg_lum > 0.45 {
        style
            .color
            .as_deref()
            .and_then(|v| u32::from_str_radix(v.trim_start_matches('#'), 16).ok())
            .map(|n| Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
            .unwrap_or_else(|| effective_text_color(bg_color, font_color))
    } else {
        effective_text_color(bg_color, font_color)
    };
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

    {
        let max_scale = style
            .inline
            .iter()
            .filter(|r| !r.hidden)
            .filter_map(|r| r.size)
            .fold(1., f32::max);
        let line_height =
            font_size * (line * max_scale).max(if style.ruby.is_empty() { 1. } else { 1.7 });
        let mut offset = 0;
        let mut previous: Option<TextFormat> = None;
        let mut run = String::new();
        let mut first = true;
        for span in spans {
            for ch in span.text.chars() {
                let mut format = TextFormat {
                    font_id: FontId::new(font_size, family_from_name(font_family_name)),
                    underline: if span.link_url.is_some() {
                        egui::Stroke::new(1.0_f32, link_color)
                    } else {
                        egui::Stroke::NONE
                    },
                    color: if span.link_url.is_some() {
                        link_color
                    } else {
                        base_color
                    },
                    italics: matches!(span.style, InlineStyle::Italic | InlineStyle::BoldItalic)
                        || style.italic,
                    line_height: Some(line_height),
                    ..Default::default()
                };
                let mut bold = is_heading
                    || style.bold
                    || matches!(span.style, InlineStyle::Bold | InlineStyle::BoldItalic);
                let mut hidden = false;
                let mut caps = false;
                for item in style
                    .inline
                    .iter()
                    .filter(|r| offset >= r.start && offset < r.end)
                {
                    if let Some(scale) = item.size {
                        format.font_id.size = font_size * scale;
                    }
                    if let Some(value) = item.bold {
                        bold = value;
                    }
                    if let Some(value) = item.italic {
                        format.italics = value;
                    }
                    if let Some(family) = &item.family {
                        format.font_id.family = match family.as_str() {
                            "serif" => family_from_name("Serif"),
                            "monospace" => FontFamily::Monospace,
                            _ => format.font_id.family,
                        };
                    }
                    if bg_lum > 0.45 {
                        if let Some(color) = &item.color {
                            if let Ok(n) = u32::from_str_radix(color.trim_start_matches('#'), 16) {
                                format.color =
                                    Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8);
                            }
                        }
                    }
                    if item.underline || item.note.is_some() {
                        format.underline = egui::Stroke::new(1.0_f32, format.color);
                    }
                    if item.strike {
                        format.strikethrough = egui::Stroke::new(1.0_f32, format.color);
                    }
                    if item.baseline != 0. {
                        format.valign = if item.baseline > 0. {
                            egui::Align::Min
                        } else {
                            egui::Align::Max
                        };
                    }
                    hidden |= item.hidden;
                    caps = item.small_caps;
                }
                if bold {
                    format.font_id.family = family_from_name("Bold");
                } else if uses_cjk_font(ch) && font_family_name == "ReaderFont" {
                    format.font_id.family = family_from_name("ReaderCjk");
                }
                if let Some(color) = hl_at(offset) {
                    format.color = highlight_text_color(color);
                    format.background = highlight_bg_color(color);
                }
                if hidden {
                    format.font_id.size = 0.01;
                    format.color = Color32::TRANSPARENT;
                    format.underline = egui::Stroke::NONE;
                    format.strikethrough = egui::Stroke::NONE;
                }
                if caps && ch.is_ascii_lowercase() {
                    format.font_id.size *= 0.8;
                }
                if previous.as_ref() != Some(&format) && !run.is_empty() {
                    append_font_runs(
                        &mut job,
                        &run,
                        if first && !is_heading {
                            style
                                .indent_length
                                .as_ref()
                                .map_or(font_size * style.indent.unwrap_or(text_indent()), |v| {
                                    v.resolve(font_size, max_width)
                                })
                        } else {
                            0.
                        },
                        previous.as_ref().unwrap().clone(),
                        &previous.as_ref().unwrap().font_id.family,
                    );

                    run.clear();
                    first = false;
                }
                previous = Some(format);
                run.push(if caps { ch.to_ascii_uppercase() } else { ch });
                offset += 1;
            }
        }
        if let Some(format) = previous {
            append_font_runs(
                &mut job,
                &run,
                if first && !is_heading {
                    style
                        .indent_length
                        .as_ref()
                        .map_or(font_size * style.indent.unwrap_or(text_indent()), |v| {
                            v.resolve(font_size, max_width)
                        })
                } else {
                    0.
                },
                format.clone(),
                &format.font_id.family,
            );
        }
        job
    }
}

// ── Block height estimation ──

pub(crate) fn estimate_block_height(
    block: &ContentBlock,
    font_size: f32,
    title_font_scale: f32,
    _line_height: f32,
    max_width: f32,
) -> f32 {
    let style = block.native_style();
    if style.hidden {
        return 0.0;
    }
    let font_size = style.size_at(font_size);
    let (left, right, top, bottom) = native_insets(&style, font_size, max_width);
    let max_width = (max_width - left - right).max(32.);
    let max_scale = style
        .inline
        .iter()
        .filter(|r| !r.hidden)
        .filter_map(|r| r.size)
        .fold(1., f32::max);
    let line_height = font_size
        * (style.line.unwrap_or(line_spacing()) * max_scale).max(if style.ruby.is_empty() {
            1.
        } else {
            1.7
        });
    let height = if !style.list.is_empty() {
        style
            .list
            .iter()
            .map(|r| {
                estimate_lines(
                    &r.text,
                    font_size,
                    (max_width
                        - font_size
                            * 1.2
                            * r.depth.saturating_sub(style.list_depth as usize) as f32
                        - 32.)
                        .max(32.),
                    0.,
                ) * line_height
                    + font_size * 0.3
            })
            .sum()
    } else if let Some(table) = &style.table {
        let column = max_width / table.columns.max(1) as f32;
        table
            .cells
            .iter()
            .map(|cell| {
                estimate_lines(
                    &cell.text,
                    font_size,
                    (column * cell.span as f32 - 12.).max(16.),
                    0.,
                ) * line_height
            })
            .fold(font_size, f32::max)
            + 12.
            + font_size * style.after.unwrap_or(0.)
    } else {
        match block {
            ContentBlock::Heading { level, spans, .. } => {
                let scale = if style.size.is_some() || style.size_px.is_some() {
                    1.
                } else {
                    (title_font_scale
                        * match level {
                            1 => 1.3,
                            2 => 1.1,
                            3 => 0.9,
                            _ => 0.8,
                        })
                    .max(1.)
                };
                let text: String = spans.iter().map(|s| s.text.as_str()).collect();
                estimate_lines(&text, font_size * scale * max_scale, max_width, 0.)
                    * line_height
                    * scale
                    + font_size * (style.before.unwrap_or(0.8) + style.after.unwrap_or(0.4))
            }
            ContentBlock::Paragraph { spans, .. } => {
                let text: String = spans.iter().map(|s| s.text.as_str()).collect();
                let indent = style
                    .indent_length
                    .as_ref()
                    .map_or(font_size * style.indent.unwrap_or(text_indent()), |v| {
                        v.resolve(font_size, max_width)
                    });
                estimate_lines(&text, font_size * max_scale, max_width, indent) * line_height
                    + font_size
                        * (style.before.unwrap_or(0.) + style.after.unwrap_or(para_spacing()))
            }
            ContentBlock::Image { data, .. } => super::reader_images::styled_height(
                data,
                max_width,
                super::reader_images::current_limit(),
                font_size,
                &style,
            ),
            ContentBlock::Separator => 24.,
            ContentBlock::BlankLine => font_size * 0.5,
        }
    };
    height
        + top
        + bottom
        + if style.ruby.is_empty() {
            0.
        } else {
            font_size * 0.65
        }
}
fn estimate_lines(text: &str, font: f32, width: f32, indent: f32) -> f32 {
    text.split('\n')
        .enumerate()
        .map(|(i, line)| {
            ((estimate_text_width(line, font) + if i == 0 { indent } else { 0. }) / width)
                .ceil()
                .max(1.)
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_punctuation_wraps_across_font_runs_without_changing_source_offsets() {
        let ctx = egui::Context::default();
        crate::setup_fonts(&ctx);
        assert_unicode_punctuation_wraps(&ctx);

        // Exercise the CI case even on hosts with installed Chinese fonts.
        let ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        crate::register_reader_font_families(&mut fonts);
        ctx.set_fonts(fonts);
        assert_unicode_punctuation_wraps(&ctx);
    }

    fn assert_unicode_punctuation_wraps(ctx: &egui::Context) {
        let prefix = "不管怎么说，就算真有什么圣女，也还是很离谱呀，上哪和圣女说话去";
        for font in [14., 18., 26.] {
            for ending in [
                "？", "！", "，", "；", "：", "）", "》", "？！", "？）", "?", "!",
            ] {
                let source = format!("{prefix}{ending}");
                let spans = [
                    TextSpan {
                        text: prefix.into(),
                        style: InlineStyle::Normal,
                        link_url: None,
                        correction: None,
                    },
                    TextSpan {
                        text: ending.into(),
                        style: InlineStyle::Bold,
                        link_url: Some("#note".into()),
                        correction: None,
                    },
                ];
                let _ = ctx.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let style = reader_core::epub::native::NativeStyle::default();
                        let prefix_job = build_layout_job(
                            &spans[..1],
                            font,
                            Color32::WHITE,
                            false,
                            f32::INFINITY,
                            None,
                            "Sans",
                            &[],
                            &style,
                        );
                        let width = ui.painter().layout_job(prefix_job).rect.width() + 0.01;
                        let job = build_layout_job(
                            &spans,
                            font,
                            Color32::WHITE,
                            false,
                            width,
                            None,
                            "Sans",
                            &[],
                            &style,
                        );
                        assert_eq!(job.text, source);
                        let sections = job.sections.len();
                        let galley = native_galley(ui, job, &style, width);
                        let rows: Vec<_> = galley.rows.iter().map(|r| r.text()).collect();
                        assert_eq!(rows.concat(), source);
                        assert!(
                            rows.iter()
                                .skip(1)
                                .all(|row| !row.chars().next().is_some_and(|c| ending.contains(c))),
                            "orphan at {font}pt: {rows:?}"
                        );
                        assert!(galley
                            .rows
                            .iter()
                            .flat_map(|r| &r.glyphs)
                            .all(|g| (g.section_index as usize) < sections));
                    });
                });
            }
        }
    }

    #[test]
    fn enriched_native_layout_preserves_unicode_and_fits_columns() {
        let book = reader_core::epub::EpubBook::open(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../core/tests/fixtures/native-typography.epub"
        ))
        .unwrap();
        let ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        for family in ["Bold", "Serif", "ReaderFont", "ReaderCjk"] {
            fonts.families.insert(
                egui::FontFamily::Name(family.into()),
                fonts.families[&egui::FontFamily::Proportional].clone(),
            );
        }
        ctx.set_fonts(fonts);
        set_spacing(1.5, 0.5, 2.);
        for policy in [
            reader_core::epub::native::CssPolicy::None,
            reader_core::epub::native::CssPolicy::Mixed,
            reader_core::epub::native::CssPolicy::Publisher,
        ] {
            let styles = book.chapters[0]
                .native_document
                .present(policy, false, &[])
                .styles;
            for width in [320., 640.] {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width + 40., 3000.),
                    )),
                    ..Default::default()
                };
                let _ = ctx.run(input, |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        for (block, style) in book.chapters[0].blocks.iter().zip(&styles) {
                            if let ContentBlock::Paragraph { spans, .. }
                            | ContentBlock::Heading { spans, .. } = block
                            {
                                if style.inline.is_empty() {
                                    continue;
                                }
                                let font = style.size_at(18.);
                                let (left, right, _, _) = native_insets(style, font, width);
                                let extent = (width - left - right).max(32.);
                                let source: String =
                                    spans.iter().map(|s| s.text.as_str()).collect();
                                let job = build_layout_job(
                                    spans,
                                    font,
                                    Color32::WHITE,
                                    false,
                                    extent,
                                    None,
                                    "ReaderFont",
                                    &[],
                                    style,
                                );
                                assert_eq!(
                                    job.text, source,
                                    "Native layout changed shared scalar offsets"
                                );
                                let galley = native_galley(ui, job, style, extent);
                                assert!(
                                    galley.mesh_bounds.width() <= extent + font * 2.,
                                    "Native text escaped column"
                                );
                                for ruby in &style.ruby {
                                    assert!(ruby.end <= source.chars().count());
                                }
                            }
                        }
                    });
                });
            }
        }
    }

    #[test]
    fn routes_only_cjk_characters_and_punctuation_to_cjk_font() {
        for ch in "中文日本語？！：；，。、“”‘’「」".chars() {
            assert!(uses_cjk_font(ch), "{ch} should use ReaderCjk");
        }
        for ch in "English?!:;\"'😀".chars() {
            assert!(!uses_cjk_font(ch), "{ch} should use ReaderFont");
        }
    }

    #[test]
    fn layout_job_groups_script_runs_into_reader_families() {
        let mut job = LayoutJob::default();
        append_font_runs(
            &mut job,
            "中文？ English?",
            0.0,
            TextFormat {
                font_id: FontId::new(18.0, FontFamily::Name("ReaderFont".into())),
                ..Default::default()
            },
            &FontFamily::Name("ReaderCjk".into()),
        );

        let families: Vec<String> = job
            .sections
            .iter()
            .map(|section| match &section.format.font_id.family {
                FontFamily::Name(name) => name.to_string(),
                other => format!("{other:?}"),
            })
            .collect();
        assert_eq!(families, ["ReaderCjk", "ReaderFont"]);
    }

    #[test]
    fn highlighted_punctuation_keeps_cjk_font() {
        let spans = [TextSpan {
            text: "你好！”".to_owned(),
            style: InlineStyle::Normal,
            link_url: None,
            correction: None,
        }];
        let job = build_layout_job(
            &spans,
            18.0,
            Color32::WHITE,
            false,
            600.0,
            None,
            "ReaderFont",
            &[(0, 3, reader_core::library::HighlightColor::Yellow)],
            &Default::default(),
        );
        assert!(job.sections.iter().all(|section| {
            section.format.font_id.family == FontFamily::Name("ReaderCjk".into())
        }));
    }
    #[test]
    fn publisher_alignment_keeps_glyphs_inside_the_native_column() {
        for (align, is_heading) in [
            (Some("right"), false),
            (Some("center"), false),
            (None, true),
            (Some("left"), true),
            (Some("right"), true),
        ] {
            let ctx = egui::Context::default();
            let mut fonts = egui::FontDefinitions::default();
            let proportional = fonts.families[&FontFamily::Proportional].clone();
            fonts
                .families
                .insert(FontFamily::Name("Bold".into()), proportional);
            ctx.set_fonts(fonts);
            let style = reader_core::epub::native::NativeStyle {
                align: align.map(str::to_owned),
                ..Default::default()
            };
            let mut checked = false;
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let spans = [TextSpan {
                        text: "A native publication paragraph with a heading that wraps across multiple lines".into(),
                        style: InlineStyle::Normal,
                        link_url: None,
                        correction: None,
                    }];
                    let job = build_layout_job(
                        &spans,
                        18.,
                        Color32::WHITE,
                        is_heading,
                        340.,
                        None,
                        "Sans",
                        &[],
                        &style,
                    );
                    let galley = native_galley(ui, job, &style, 340.);
                    assert_eq!(galley.rect.width(), 340.);
                    for row in &galley.rows {
                        assert!(row.rect.left() >= -0.1 && row.rect.right() <= 340.1);
                        if align == Some("center") || (align.is_none() && is_heading) {
                            assert!((row.rect.center().x - 170.).abs() < 0.1);
                        }
                    }
                    checked = true;
                });
            });
            assert!(checked);
        }
    }
}
