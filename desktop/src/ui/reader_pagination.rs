//! Measured page fragments preserve canonical blocks and their selection offsets.
use eframe::egui::{self, Rect};
use std::{cell::RefCell, ops::Deref};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PageFragment {
    pub offset: f32,
    pub char_offset: usize,
    pub height: f32,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PageRanges {
    pub ranges: Vec<(usize, usize)>,
    pub fragments: Vec<PageFragment>,
}
impl Deref for PageRanges {
    type Target = [(usize, usize)];
    fn deref(&self) -> &Self::Target {
        &self.ranges
    }
}
impl From<Vec<(usize, usize)>> for PageRanges {
    fn from(ranges: Vec<(usize, usize)>) -> Self {
        Self {
            ranges,
            fragments: vec![],
        }
    }
}
impl PageRanges {
    pub fn fragment(&self, page: usize) -> Option<PageFragment> {
        self.fragments.get(page).copied()
    }
}

type TableSource = (std::sync::Arc<egui::Galley>, f32, usize, usize);

thread_local! {
    static TABLE_SOURCE: RefCell<Vec<TableSource>> = const { RefCell::new(Vec::new()) };
    static BANDS: RefCell<Option<Vec<Rect>>> = const { RefCell::new(None) };
}
pub(crate) fn begin_measurement() {
    TABLE_SOURCE.with(|cells| cells.borrow_mut().clear());
    BANDS.with(|b| *b.borrow_mut() = Some(vec![]));
}
pub(crate) fn finish_measurement() -> Vec<Rect> {
    BANDS.with(|b| b.borrow_mut().take().unwrap_or_default())
}
pub(crate) fn record_band(rect: Rect) {
    BANDS.with(|b| {
        if let Some(bands) = b.borrow_mut().as_mut() {
            bands.push(rect);
        }
    });
}
pub(crate) fn record_table_source(
    galley: &std::sync::Arc<egui::Galley>,
    top: f32,
    start: usize,
    end: usize,
) {
    if BANDS.with(|b| b.borrow().is_some()) {
        TABLE_SOURCE.with(|cells| {
            cells.borrow_mut().push((
                galley.clone(),
                top - super::reader_state::READER_CONTENT_TOP_PADDING,
                start,
                end,
            ))
        });
    }
}
pub(crate) fn record_galley(galley: &egui::Galley, origin: egui::Pos2, ruby: f32) {
    BANDS.with(|b| {
        if let Some(bands) = b.borrow_mut().as_mut() {
            for row in &galley.rows {
                let rect = row.rect.translate(origin.to_vec2());
                bands.push(Rect::from_min_max(
                    rect.min - egui::vec2(0., ruby),
                    rect.max,
                ));
            }
        }
    });
}

impl crate::app::ReaderApp {
    pub(crate) fn measured_chapter_pages(
        &self,
        ui: &mut egui::Ui,
        chapter: usize,
        height: f32,
        width: f32,
    ) -> PageRanges {
        use super::{reader_block::render_content_layout, reader_state::*};
        let Some(ch) = self.book.as_ref().and_then(|b| b.chapters.get(chapter)) else {
            return PageRanges::default();
        };
        let budget = (height - FRAME_MARGIN).max(32.);
        let mut measures = Vec::with_capacity(ch.blocks.len());
        let mut text_geometry = Vec::with_capacity(ch.blocks.len());
        let family = if self.defer_custom_font_for_frame {
            "Sans"
        } else {
            // Paint uses the composed Latin/CJK chain, not the preference's
            // display name. Measuring another family changes wraps and clips rows.
            "ReaderFont"
        };
        let galley_count = BLOCK_GALLEYS.with(|g| g.borrow().len());
        let correction_count = CSC_RECTS.with(|g| g.borrow().len());
        for index in 0..ch.blocks.len() {
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt(("measure_reader_block", chapter, index))
                    .max_rect(Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 0.)))
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            child.set_invisible();
            begin_measurement();
            render_content_layout(
                &mut child,
                0.,
                width,
                &ch.title,
                &ch.blocks,
                index,
                index + 1,
                false,
                None,
                self.font_size,
                self.title_font_scale,
                self.reader_bg_color,
                chapter,
                self.reader_font_color,
                family,
                &self.i18n,
                &mut None,
                &Default::default(),
            );
            let mut geometry =
                TABLE_SOURCE.with(|cells| cells.borrow_mut().drain(..).collect::<Vec<_>>());
            if let Some(entry) = BLOCK_GALLEYS.with(|g| {
                g.borrow()
                    .iter()
                    .rev()
                    .find(|entry| entry.key == BlockKey::new(chapter, index))
                    .cloned()
            }) {
                geometry.push((
                    entry.galley,
                    entry.rect.top() - READER_CONTENT_TOP_PADDING,
                    0,
                    entry.text.chars().count(),
                ));
            }
            text_geometry.push(geometry);
            let bands = finish_measurement()
                .into_iter()
                .map(|r| r.translate(egui::vec2(0., -READER_CONTENT_TOP_PADDING)))
                .collect();
            measures.push(((child.min_rect().height() - FRAME_MARGIN).max(0.), bands));
        }
        BLOCK_GALLEYS.with(|g| g.borrow_mut().truncate(galley_count));
        CSC_RECTS.with(|g| g.borrow_mut().truncate(correction_count));
        // Measure synthetic chapter titles with the same font/width as painting.
        let title = if matches!(
            ch.blocks.first(),
            Some(reader_core::epub::ContentBlock::Heading { .. })
        ) {
            0.
        } else {
            ui.painter()
                .layout(
                    ch.title.clone(),
                    egui::FontId::new(
                        self.font_size * self.title_font_scale,
                        super::reader_block::family_from_name(family),
                    ),
                    self.reader_font_color.unwrap_or(egui::Color32::BLACK),
                    width,
                )
                .size()
                .y
                + TITLE_SPACING
        };
        let mut pages = measured_pages(&measures, budget, title);
        for (&(block, _), fragment) in pages.ranges.iter().zip(&mut pages.fragments) {
            fragment.char_offset = text_geometry[block]
                .iter()
                .filter(|(galley, top, _, _)| fragment.offset < top + galley.size().y)
                .map(|(galley, top, start, end)| {
                    (start
                        + galley
                            .cursor_from_pos(egui::vec2(0., fragment.offset - top + 0.01))
                            .ccursor
                            .index)
                        .min(*end)
                })
                .min()
                .unwrap_or(0);
        }
        pages
    }
}

/// Cuts only between painted lines/images. Overlapping cells/ruby form one band.
pub(crate) fn measured_pages(blocks: &[(f32, Vec<Rect>)], budget: f32, title: f32) -> PageRanges {
    let budget = budget.max(32.);
    let mut result = PageRanges::default();
    let mut start = 0;
    let mut offset = 0.;
    let mut used = title;
    for (index, (height, bands)) in blocks.iter().enumerate() {
        if *height <= 0. {
            continue;
        }
        // Fill the remaining page with complete painted lines, even when this
        // paragraph would fit on a fresh page. Bands still keep images and
        // overlapping table cells intact whenever they fit in the viewport.
        let mut consumed = 0.;
        while used + height - consumed > budget + 0.01 {
            let target = consumed + (budget - used).max(0.);
            let mut cut = target;
            loop {
                let prior = cut;
                for band in bands {
                    if band.top() < cut - 0.01 && band.bottom() > cut + 0.01 {
                        cut = cut.min(band.top().max(0.));
                    }
                }
                if (prior - cut).abs() < 0.01 {
                    break;
                }
            }
            if cut <= consumed + 0.01 {
                if used > 0.01 {
                    // The next complete line does not fit: move it to a fresh page.
                    result
                        .ranges
                        .push((start, index + usize::from(consumed > 0.)));
                    result.fragments.push(PageFragment {
                        offset,
                        char_offset: 0,
                        height: used,
                    });
                    start = index;
                    offset = consumed;
                    used = 0.;
                    continue;
                }
                // Only possible for a single line larger than the entire viewport.
                // Retain every pixel over adjacent slices instead of dropping it.
                cut = (consumed + budget).min(*height);
            }
            used += cut - consumed;
            result.ranges.push((start, index + 1));
            result.fragments.push(PageFragment {
                offset,
                char_offset: 0,
                height: used,
            });
            start = index;
            offset = cut;
            consumed = cut;
            used = 0.;
        }
        used += height - consumed;
    }
    if start < blocks.len() {
        result.ranges.push((start, blocks.len()));
        result.fragments.push(PageFragment {
            offset,
            char_offset: 0,
            height: used,
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::super::{reader_block::render_content_layout, reader_state::*};
    use super::*;
    use reader_core::epub::{ContentBlock, EpubBook, InlineStyle, TextSpan};

    #[test]
    fn ordinary_paragraphs_fill_remaining_space_without_splitting_lines() {
        // Each paragraph fits on a page, but not in the space after two others.
        // Keeping these whole used to waste nearly a third of every page.
        let bands = (0..6)
            .map(|row| Rect::from_min_size(egui::pos2(0., row as f32 * 18.), egui::vec2(200., 16.)))
            .collect::<Vec<_>>();
        let blocks = vec![(116., bands); 12];
        let pages = measured_pages(&blocks, 320., 24.);
        assert_eq!(pages.len(), 5);
        let mut lines = std::collections::BTreeMap::new();
        for (page, &(start, end)) in pages.iter().enumerate() {
            let fragment = pages.fragment(page).unwrap();
            if page + 1 < pages.len() {
                assert!(fragment.height >= 302., "unused line space: {fragment:?}");
            }
            let mut y = if page == 0 { 24. } else { 0. };
            for (block, (height, bands)) in blocks.iter().enumerate().take(end).skip(start) {
                let offset = if block == start { fragment.offset } else { 0. };
                for (line, band) in bands.iter().enumerate() {
                    let top = y + band.top() - offset;
                    let bottom = y + band.bottom() - offset;
                    if bottom > 0.01 && top < fragment.height - 0.01 {
                        assert!(top >= -0.01 && bottom <= fragment.height + 0.01);
                        *lines.entry((block, line)).or_insert(0) += 1;
                    }
                }
                y += height - offset;
            }
        }
        assert_eq!(lines.len(), 12 * 6, "missing lines");
        assert!(lines.values().all(|count| *count == 1), "duplicated lines");
    }

    #[test]
    #[ignore = "explicit Release benchmark with READER_LARGE_BOOK and READER_PERFORMANCE_REPORT"]
    fn huge_book_release_native_renderer_performance() {
        use reader_core::epub::EpubBook;
        use std::time::Instant;
        fn rss() -> u64 {
            std::process::Command::new("ps")
                .args(["-o", "rss=", "-p", &std::process::id().to_string()])
                .output()
                .ok()
                .and_then(|v| String::from_utf8(v.stdout).ok())
                .and_then(|v| v.trim().parse::<u64>().ok())
                .unwrap_or(0)
                * 1024
        }
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let root = tempfile::tempdir().unwrap();
        let path = std::env::var("READER_LARGE_BOOK").unwrap();
        let mut app =
            crate::app::ReaderApp::from_data_dir(root.path().to_string_lossy().into_owned());
        app.reader_introduction_seen = true;
        app.scroll_mode = false;
        app.reader_page_animation = "Realistic".into();
        app.font_size = 18.;
        let ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        #[cfg(target_os = "macos")]
        crate::system_fonts::register_defaults(&mut fonts);
        for name in ["Bold", "ReaderFont", "ReaderCjk"] {
            fonts.families.insert(
                egui::FontFamily::Name(name.into()),
                fonts.families[&egui::FontFamily::Proportional].clone(),
            );
        }
        ctx.set_fonts(fonts);
        let baseline = rss();
        let opened = Instant::now();
        let book = EpubBook::open_cached(&path, &root.path().join("chapter-index")).unwrap();
        let catalogue_ms = opened.elapsed().as_secs_f64() * 1000.;
        app.book = Some(book);
        app.book_path = Some(path.clone());
        app.view = crate::app::AppView::Reader;
        app.pages_dirty = true;
        let mut time = 0.;
        fn frame(app: &mut crate::app::ReaderApp, ctx: &egui::Context, time: &mut f64) -> f64 {
            *time += 1. / 60.;
            let t = Instant::now();
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200., 800.),
                    )),
                    time: Some(*time),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| app.render_reader(ui));
                },
            );
            let _ = ctx.tessellate(output.shapes, output.pixels_per_point);
            t.elapsed().as_secs_f64() * 1000.
        }
        let first_frame = frame(&mut app, &ctx, &mut time);
        let first_content = opened.elapsed().as_secs_f64() * 1000.;
        assert!(!app.pages_dirty && app.total_pages > 0);
        let first_rss = rss();
        let mut cases = Vec::new();
        for mode in ["Realistic", "Slide"] {
            app.reader_page_animation = mode.into();
            for chapter in [3, 1772, 3544] {
                app.current_chapter = chapter;
                app.current_page = 0;
                app.pages_dirty = true;
                app.page_curl.clear();
                app.reader_render_blocks = None;
                let jump_frame = frame(&mut app, &ctx, &mut time);
                let mut timings = Vec::new();
                for turn in 0..8 {
                    if turn < 4 {
                        app.next_page()
                    } else {
                        app.prev_page()
                    };
                    for _ in 0..36 {
                        timings.push(frame(&mut app, &ctx, &mut time));
                    }
                }
                timings.sort_by(f64::total_cmp);
                cases.push(serde_json::json!({"mode":mode,"chapter":chapter,"chapter_first_frame_cpu_ms":jump_frame,"frame_count":timings.len(),"frame_cpu_p50_ms":timings[timings.len()/2],"frame_cpu_p95_ms":timings[timings.len()*95/100],"frame_cpu_max_ms":timings.last(),"rss_bytes":rss()}));
            }
        }
        for chapter in 3..203 {
            app.current_chapter = chapter;
            app.current_page = 0;
            app.pages_dirty = true;
            app.page_curl.clear();
            app.reader_render_blocks = None;
            frame(&mut app, &ctx, &mut time);
        }
        let stats = app.book.as_ref().unwrap().chapter_cache_stats();
        assert!(stats.0 <= 8);
        assert!(
            app.book
                .as_ref()
                .unwrap()
                .chapters
                .iter()
                .filter(|ch| !ch.blocks.is_empty())
                .count()
                <= 5
        );
        let report = serde_json::json!({"profile":"release","environment":"macOS native egui rendering and tessellation, headless CPU; excludes GPU presentation","catalogue_ms":catalogue_ms,"first_content_layout_draw_ms":first_content,"first_frame_cpu_ms":first_frame,"baseline_rss_bytes":baseline,"first_content_rss_bytes":first_rss,"after_200_chapters_rss_bytes":rss(),"cache_stats":stats,"cases":cases,"passed":true});
        std::fs::write(
            std::env::var("READER_PERFORMANCE_REPORT").unwrap(),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }

    #[test]
    #[ignore = "requires downloaded, fingerprinted Chinese corpus"]
    fn real_chinese_books_reflow_in_three_styles_with_stable_source_anchors() {
        let root = std::path::PathBuf::from(
            std::env::var("EPUB_CHINESE_CORPUS").expect("EPUB_CHINESE_CORPUS"),
        );
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let data = tempfile::tempdir().unwrap();
        let mut app =
            crate::app::ReaderApp::from_data_dir(data.path().to_string_lossy().into_owned());
        let ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        #[cfg(target_os = "macos")]
        crate::system_fonts::register_defaults(&mut fonts);
        for name in ["Bold", "ReaderFont", "ReaderCjk"] {
            if !fonts
                .families
                .contains_key(&egui::FontFamily::Name(name.into()))
            {
                fonts.families.insert(
                    egui::FontFamily::Name(name.into()),
                    fonts.families[&egui::FontFamily::Proportional].clone(),
                );
            }
        }
        ctx.set_fonts(fonts);
        app.font_size = 18.;
        set_spacing(1.5, 0.5, 2.);
        let mut reports = vec![];
        for name in ["hongloumeng", "aq", "zhaohuaxishi", "tiangongkaiwu"] {
            let original = EpubBook::open(root.join(format!("{name}.epub"))).unwrap();
            let chapter = original
                .chapters
                .iter()
                .enumerate()
                .max_by_key(|(_, ch)| {
                    ch.blocks
                        .iter()
                        .map(|block| match block {
                            ContentBlock::Paragraph { spans, .. }
                            | ContentBlock::Heading { spans, .. } => {
                                spans.iter().map(|s| s.text.len()).sum::<usize>()
                            }
                            _ => 0,
                        })
                        .sum::<usize>()
                })
                .unwrap()
                .0;
            for policy in [
                reader_core::epub::native::CssPolicy::None,
                reader_core::epub::native::CssPolicy::Mixed,
                reader_core::epub::native::CssPolicy::Publisher,
            ] {
                let mut book = original.clone();
                book.chapters[chapter].apply_presentation(policy, false, &[]);
                app.book = Some(book);
                for width in [320., 640.] {
                    super::super::reader_images::configure(
                        app.book.as_ref(),
                        650.,
                        false,
                        18.,
                        1.5,
                    );
                    let started = std::time::Instant::now();
                    let input = egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width + 20., 650.),
                        )),
                        ..Default::default()
                    };
                    let mut ranges = PageRanges::default();
                    let _ = ctx.run(input.clone(), |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            ranges = app.measured_chapter_pages(ui, chapter, 650., width);
                        });
                    });
                    let measure_ms = started.elapsed().as_secs_f64() * 1000.;
                    let ch = &app.book.as_ref().unwrap().chapters[chapter];
                    assert!(!ranges.is_empty());
                    assert_eq!(ranges.first().unwrap().0, 0);
                    assert_eq!(ranges.last().unwrap().1, ch.blocks.len());
                    for page in 0..ranges.len() {
                        let (start, end) = ranges[page];
                        let fragment = ranges.fragment(page).unwrap();
                        assert!(
                            start < end
                                && fragment.height >= 0.
                                && fragment.height <= 650. - FRAME_MARGIN + 0.1
                        );
                        if page > 0 {
                            let prior = ranges[page - 1];
                            assert!(start == prior.1 || start + 1 == prior.1);
                            if start + 1 == prior.1 && start == prior.0 {
                                assert!(
                                    fragment.offset > ranges.fragment(page - 1).unwrap().offset
                                );
                            }
                        }
                        let source_len = match &ch.blocks[start] {
                            ContentBlock::Paragraph { spans, .. }
                            | ContentBlock::Heading { spans, .. } => {
                                spans.iter().map(|s| s.text.chars().count()).sum()
                            }
                            _ => 0,
                        };
                        assert!(fragment.char_offset <= source_len);
                    }
                    for page in [0, ranges.len() / 2, ranges.len() - 1] {
                        let (start, end) = ranges[page];
                        let _ = ctx.run(input.clone(), |ctx| {
                            egui::CentralPanel::default().show(ctx, |ui| {
                                render_content_layout(
                                    ui,
                                    0.,
                                    width,
                                    &ch.title,
                                    &ch.blocks,
                                    start,
                                    end,
                                    page == 0,
                                    ranges.fragment(page),
                                    18.,
                                    1.5,
                                    egui::Color32::WHITE,
                                    chapter,
                                    None,
                                    "Sans",
                                    &app.i18n,
                                    &mut None,
                                    &Default::default(),
                                );
                            });
                        });
                        BLOCK_GALLEYS.with(|g| g.borrow_mut().clear());
                    }
                    reports.push(serde_json::json!({"book":name,"chapter":chapter,"style":policy.key(),"width":width,"pages":ranges.len(),"measure_ms":measure_ms,"source_anchors_valid":true,"drawn_pages":3}));
                }
            }
        }
        if let Ok(path) = std::env::var("EPUB_CHINESE_REPORT") {
            std::fs::write(path, serde_json::to_string_pretty(&reports).unwrap()).unwrap();
        }
    }

    #[test]
    fn long_paragraphs_and_table_rows_cover_every_native_line_at_both_widths() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let dir = tempfile::tempdir().unwrap();
        let mut app =
            crate::app::ReaderApp::from_data_dir(dir.path().to_string_lossy().into_owned());
        let mut book = EpubBook::open(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../core/tests/fixtures/native-typography.epub"
        ))
        .unwrap();
        book.chapters[0].apply_presentation(
            reader_core::epub::native::CssPolicy::Mixed,
            false,
            &[],
        );
        let first_added = book.chapters[0].blocks.len();
        for _ in 0..12 {
            book.chapters[0].blocks.push(ContentBlock::Paragraph {
                spans: vec![TextSpan {
                    text: "普通段落 Mixed WWW iii 英文也要跨页，不能漏行。".repeat(8),
                    style: InlineStyle::Normal,
                    link_url: None,
                    correction: None,
                }],
                anchor_id: None,
                native_style: Default::default(),
            });
        }
        let source = "正文不能漏字😀，跨页之后保持原有位置。".repeat(200);
        book.chapters[0].blocks.push(ContentBlock::Paragraph {
            spans: vec![TextSpan {
                text: source.clone(),
                style: InlineStyle::Normal,
                link_url: None,
                correction: None,
            }],
            anchor_id: None,
            native_style: Default::default(),
        });
        let table = book.chapters[0]
            .blocks
            .iter_mut()
            .find(|block| block.native_style().table.is_some())
            .unwrap();
        let mut style = table.native_style();
        style.table.as_mut().unwrap().cells[0].text = "表格内容必须完整展示。".repeat(180);
        table.set_native_style(style);
        app.book = Some(book);
        app.font_size = 18.;
        set_spacing(1.5, 0.5, 2.);
        let ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        for name in ["Bold", "ReaderFont", "ReaderCjk"] {
            fonts.families.insert(
                egui::FontFamily::Name(name.into()),
                fonts.families[&egui::FontFamily::Proportional].clone(),
            );
        }
        fonts.families.insert(
            egui::FontFamily::Name("ReaderFont".into()),
            fonts.families[&egui::FontFamily::Monospace].clone(),
        );
        ctx.set_fonts(fonts);
        for (font, line) in [(18., 1.5), (17.3, 1.63)] {
            app.font_size = font;
            set_spacing(line, 0.5, 2.);
            for width in [280., 620.] {
                let book = app.book.as_ref().unwrap();
                super::super::reader_images::configure(Some(book), 600., false, font, 1.5);
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width + 20., 600.),
                    )),
                    ..Default::default()
                };
                let mut ranges = PageRanges::default();
                let _ = ctx.run(input.clone(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ranges = app.measured_chapter_pages(ui, 0, 600., width);
                    });
                });
                assert!(ranges.len() > 10);
                let last = book.chapters[0].blocks.len() - 1;
                let mut visible_rows = std::collections::BTreeMap::new();
                let mut total_rows = std::collections::BTreeMap::new();
                for (page, &(start, end)) in ranges.iter().enumerate() {
                    let fragment = ranges.fragment(page).unwrap();
                    assert!(
                        fragment.height <= 600. - FRAME_MARGIN + 0.1,
                        "page overflow: {fragment:?}"
                    );
                    BLOCK_GALLEYS.with(|g| g.borrow_mut().clear());
                    let _ = ctx.run(input.clone(), |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            render_content_layout(
                                ui,
                                0.,
                                width,
                                &book.chapters[0].title,
                                &book.chapters[0].blocks,
                                start,
                                end,
                                page == 0,
                                Some(fragment),
                                font,
                                1.5,
                                egui::Color32::WHITE,
                                0,
                                None,
                                "ReaderFont",
                                &app.i18n,
                                &mut None,
                                &Default::default(),
                            );
                        });
                    });
                    BLOCK_GALLEYS.with(|g| {
                        for entry in g
                            .borrow()
                            .iter()
                            .filter(|entry| entry.key.block >= first_added)
                        {
                            if entry.key.block == last {
                                assert_eq!(entry.text, source);
                            }
                            total_rows.insert(entry.key.block, entry.galley.rows.len());
                            for (row, value) in entry.galley.rows.iter().enumerate() {
                                let rect = value
                                    .rect
                                    .union(value.visuals.mesh_bounds)
                                    .translate(entry.rect.min.to_vec2());
                                if rect.intersects(entry.clip)
                                    && rect.height() > 0.
                                    && rect.bottom() > entry.clip.top() + 0.1
                                    && rect.top() < entry.clip.bottom() - 0.1
                                {
                                    assert!(
                                        rect.top() >= entry.clip.top() - 0.1
                                            && rect.bottom() <= entry.clip.bottom() + 0.1,
                                        "partially clipped line {row}, {rect:?}, {:?}",
                                        entry.clip
                                    );
                                    *visible_rows.entry((entry.key.block, row)).or_insert(0) += 1;
                                }
                            }
                        }
                    });
                }
                assert_eq!(total_rows.len(), 13, "missing paragraphs");
                assert_eq!(
                    visible_rows.len(),
                    total_rows.values().sum::<usize>(),
                    "missing paragraph lines"
                );
                assert!(
                    visible_rows.values().all(|count| *count == 1),
                    "duplicated lines"
                );
            }
        }
    }
}
