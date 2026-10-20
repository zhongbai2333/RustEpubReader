//! Opt-in native-window correctness checks. Inputs and output live in an owned fixture directory.
use crate::app::{AppView, ReaderApp};
use crate::ui::reader_images;
use eframe::egui;
use reader_core::epub::{native::CssPolicy, ContentBlock};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};

#[derive(Default)]
struct GlyphProbe {
    seen: HashSet<(String, char)>,
    missing: BTreeSet<String>,
    selection: Option<(egui::Pos2, egui::Pos2)>,
    highlight: Option<(egui::Pos2, String)>,
}

pub(crate) fn observe_selection(
    ctx: &egui::Context,
    entry: &crate::ui::reader_state::BlockGalleyEntry,
) {
    let Some(state) = ctx.data(|data| {
        data.get_temp::<Arc<Mutex<GlyphProbe>>>(egui::Id::new("native_corpus_glyph_probe"))
    }) else {
        return;
    };
    if entry.key.chapter != 0 || entry.key.block != 1 || !entry.text.starts_with("文字 😀 中文")
    {
        return;
    }
    let mut points = [egui::Pos2::ZERO; 2];
    for (point, at) in points.iter_mut().zip([0, 7]) {
        let cursor = entry.galley.from_ccursor(egui::text::CCursor::new(at));
        let rect = entry.galley.pos_from_cursor(&cursor);
        *point = entry.rect.min + egui::vec2(rect.min.x + 1., rect.center().y);
        if !entry.rect.intersect(entry.clip).contains(*point) {
            return;
        }
    }
    state.lock().unwrap().selection = Some((points[0], points[1]));
}

pub(crate) fn observe_highlight(ctx: &egui::Context, rect: egui::Rect, text: &str) {
    if let Some(state) = ctx.data(|data| {
        data.get_temp::<Arc<Mutex<GlyphProbe>>>(egui::Id::new("native_corpus_glyph_probe"))
    }) {
        state.lock().unwrap().highlight = Some((rect.center(), text.into()));
    }
}

/// Called after native galley layout, using its actual per-section font identifiers.
pub(crate) fn observe_galley(ctx: &egui::Context, galley: &egui::Galley) {
    let Some(state) = ctx.data(|data| {
        data.get_temp::<Arc<Mutex<GlyphProbe>>>(egui::Id::new("native_corpus_glyph_probe"))
    }) else {
        return;
    };
    let mut state = state.lock().unwrap();
    for section in &galley.job.sections {
        let family = format!("{:?}", section.format.font_id.family);
        for ch in galley.job.text[section.byte_range.clone()].chars() {
            if ch.is_whitespace()
                || ch.is_control()
                || matches!(ch as u32,0x00ad|0x200b..=0x200f|0x202a..=0x202e|0x2060..=0x206f|0xfe00..=0xfe0f|0xfeff|0xe0100..=0xe01ef)
            {
                continue;
            }
            if state.seen.insert((family.clone(), ch))
                && !ctx.fonts(|fonts| fonts.has_glyph(&section.format.font_id, ch))
            {
                state
                    .missing
                    .insert(format!("U+{:04X} in {family}", ch as u32));
            }
        }
    }
}

#[derive(Clone, Deserialize)]
struct Case {
    name: String,
    chapter: usize,
    #[serde(default)]
    font_size: Option<f32>,
    #[serde(default)]
    min_images: usize,
    #[serde(default)]
    min_tables: usize,
    #[serde(default)]
    min_notes: usize,
}
#[derive(Deserialize)]
struct Input {
    path: PathBuf,
    sha256: String,
}
#[derive(Deserialize)]
struct Manifest {
    cases: Vec<Case>,
    files: HashMap<String, Input>,
}

pub(crate) struct NativeCorpus {
    manifest: Manifest,
    output: PathBuf,
    started: Instant,
    case: usize,
    policy: usize,
    width: usize,
    phase: usize,
    stable: usize,
    pages: Vec<usize>,
    page: usize,
    layouts: Vec<Value>,
    layout: Value,
    capture: String,
    key: Option<egui::Key>,
    origin: usize,
    decoded: HashSet<usize>,
    glyphs: Arc<Mutex<GlyphProbe>>,
    glyph_checks: usize,
    cancellation: Value,
    cancel_started: Option<Instant>,
    cancel_settled: Option<Instant>,
    pointer_events: Vec<egui::Event>,
    native_selection: Value,
    selection_end: egui::Pos2,
}
impl NativeCorpus {
    pub(crate) fn new() -> Self {
        let root = std::env::current_dir().expect("Owned native fixture directory");
        let manifest: Manifest = serde_json::from_slice(
            &std::fs::read(root.join("native-corpus.fixture.json")).expect("Owned manifest"),
        )
        .expect("Native corpus manifest");
        assert!(!manifest.cases.is_empty());
        for case in &manifest.cases {
            let size = case.font_size.unwrap_or(18.);
            assert!(size.is_finite() && (8. ..=96.).contains(&size));
            assert!(
                !case.name.is_empty()
                    && case
                        .name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            );
            let file = manifest
                .files
                .get(&case.name)
                .expect("Declared fixture file");
            assert_eq!(
                reader_core::epub::EpubBook::file_hash(
                    file.path.to_str().expect("UTF-8 fixture path")
                )
                .unwrap(),
                file.sha256
            );
        }
        let output = root.join("native-corpus-results");
        std::fs::create_dir(&output).expect("New native corpus output directory");
        Self {
            manifest,
            output,
            started: Instant::now(),
            case: 0,
            policy: 0,
            width: 0,
            phase: 9,
            stable: 0,
            pages: vec![],
            page: 0,
            layouts: vec![],
            layout: Value::Null,
            capture: String::new(),
            key: None,
            origin: 0,
            decoded: HashSet::new(),
            glyphs: Default::default(),
            glyph_checks: 0,
            cancellation: Value::Null,
            cancel_started: None,
            cancel_settled: None,
            pointer_events: Vec::new(),
            native_selection: Value::Null,
            selection_end: egui::Pos2::ZERO,
        }
    }
    pub(crate) fn raw_input(&mut self, input: &mut egui::RawInput) {
        if let Some(key) = self.key.take() {
            for pressed in [true, false] {
                input.events.push(egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
        }
        if !self.pointer_events.is_empty() {
            input.focused = true;
            input.events.append(&mut self.pointer_events);
        }
    }
    fn pointer(&mut self, point: egui::Pos2, pressed: Option<bool>) {
        self.pointer_events.push(egui::Event::PointerMoved(point));
        if let Some(pressed) = pressed {
            self.pointer_events.push(egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
    }
    pub(crate) fn before_input(&mut self, ctx: &egui::Context) {
        *self.glyphs.lock().unwrap() = GlyphProbe::default();
        ctx.data_mut(|data| {
            data.insert_temp(
                egui::Id::new("native_corpus_glyph_probe"),
                self.glyphs.clone(),
            )
        });
    }
    fn policy(&self) -> CssPolicy {
        [CssPolicy::None, CssPolicy::Mixed, CssPolicy::Publisher][self.policy]
    }
    fn width(&self) -> f32 {
        [600., 1200.][self.width]
    }
    fn configure(&mut self, app: &mut ReaderApp, ctx: &egui::Context) {
        let case = &self.manifest.cases[self.case];
        app.current_chapter = case.chapter;
        app.current_page = 0;
        app.current_block = 0;
        app.current_char_offset = 0;
        app.reader_layout_anchor = None;
        app.pending_restore_block = None;
        app.reader_render_blocks = None;
        app.css_policy = self.policy();
        app.scripts_enabled = false;
        app.native_applied.clear();
        app.pages_dirty = true;
        app.scroll_mode = false;
        app.reader_page_animation = "None".into();
        app.force_single_column = true;
        app.show_reader_introduction = false;
        app.reader_introduction_seen = true;
        app.font_size = case.font_size.unwrap_or(18.);
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
            self.width(),
            800.,
        )));
        self.stable = 0;
        self.phase = 2;
        self.decoded.clear();
        self.glyph_checks = 0;
    }
    fn ready(&self, app: &ReaderApp, ctx: &egui::Context) -> bool {
        app.view == AppView::Reader
            && app.pending_book_open.is_none()
            && !app.pages_dirty
            && !app.defer_custom_font_for_frame
            && !app.book_presentation.is_active()
            && (ctx.screen_rect().width() - self.width()).abs() < 4.
    }
    fn inspect(&mut self, app: &ReaderApp, ctx: &egui::Context) -> Result<(), String> {
        let case = &self.manifest.cases[self.case];
        let book = app.book.as_ref().ok_or("No native book")?;
        if app.current_book_hash.as_deref() != Some(&self.manifest.files[&case.name].sha256) {
            return Err("Native book identity differs".into());
        }
        let chapter = book
            .chapters
            .get(case.chapter)
            .ok_or("Missing native chapter")?;
        check_ranges(&app.page_block_ranges, chapter.blocks.len())?;
        let mut semantic = BTreeSet::new();
        let mut notes = BTreeSet::new();
        let mut images = 0;
        let mut tables = 0;
        for (index, block) in chapter.blocks.iter().enumerate() {
            let style = block.native_style();
            if matches!(block, ContentBlock::Image { .. }) && !style.hidden {
                images += 1;
                semantic.insert(index);
            }
            if style.table.is_some() {
                tables += 1;
                semantic.insert(index);
            }
            for span in &style.inline {
                if let Some(note) = &span.note {
                    notes.insert(note.clone());
                    semantic.insert(index);
                }
            }
        }
        for image in book
            .supplemental_images
            .get(&case.chapter)
            .into_iter()
            .flatten()
        {
            images += 1;
            semantic.insert(image.block);
        }
        if images < case.min_images || tables < case.min_tables || notes.len() < case.min_notes {
            return Err(format!("Missing native corpus semantics: {}", case.name));
        }
        for href in &notes {
            if book
                .native_note(case.chapter, href)
                .is_none_or(|note| note.text.trim().is_empty())
            {
                return Err("Unresolved native note".into());
            }
        }
        let ranges = &app.page_block_ranges;
        let mut selected = BTreeSet::from([0, ranges.len() / 2, ranges.len() - 1]);
        for (page, &(start, end)) in ranges.iter().enumerate() {
            if semantic.range(start..end).next().is_some() {
                selected.insert(page);
            }
        }
        self.pages = selected.into_iter().collect();
        self.page = 0;
        self.layout = json!({"name":case.name,"chapter":case.chapter,"policy":self.policy().key(),"base_font_size":app.font_size,"requested_window_width":self.width(),"viewport":[ctx.screen_rect().width(),ctx.screen_rect().height()],"pages":ranges.len(),"drawn_pages":self.pages,"source_range_coverage":true,"images":images,"tables":tables,"notes_resolved":notes.len(),"screenshots":[]});
        Ok(())
    }
    fn next_layout(&mut self, app: &mut ReaderApp, ctx: &egui::Context) {
        self.layout["visible_decoded_images"] = json!(self.decoded.len());
        self.layout["checked_font_glyph_pairs"] = json!(self.glyph_checks);
        self.layouts.push(self.layout.take());
        self.width += 1;
        if self.width == 2 {
            self.width = 0;
            self.policy += 1;
        }
        if self.policy == 3 {
            self.policy = 0;
            self.case += 1;
        }
        if self.case == self.manifest.cases.len() {
            self.phase = 8;
        } else if self.width == 0 && self.policy == 0 {
            self.phase = 0;
        } else {
            self.configure(app, ctx);
        }
    }
    fn write(&self, passed: bool, error: Option<String>) {
        let report = json!({"schema":1,"passed":passed,"error":error,"native_window":true,"keyboard_input":"egui raw native input events; no physical keyboard claim","opening_cancellation":self.cancellation,"native_selection":self.native_selection,"layouts":self.layouts,"unfinished_layout":self.layout,"elapsed_seconds":self.started.elapsed().as_secs_f64()});
        std::fs::write(
            self.output.join("summary.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .expect("Write native corpus result");
    }
    pub(crate) fn tick(&mut self, app: &mut ReaderApp, ctx: &egui::Context) -> bool {
        match self.step(app, ctx) {
            Ok(done) => done,
            Err(error) => {
                self.write(false, Some(error));
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                true
            }
        }
    }
    fn step(&mut self, app: &mut ReaderApp, ctx: &egui::Context) -> Result<bool, String> {
        ctx.request_repaint();
        if self.started.elapsed().as_secs() > 900 {
            return Err("Native corpus timed out".into());
        }
        if let Some(error) = &app.error_msg {
            return Err(error.clone());
        }
        if self.phase == 8 {
            self.write(true, None);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return Ok(true);
        }
        // Use the normal native window, opening worker, Escape handler and
        // closing transition. Do not hold or slow the worker to manufacture an
        // in-flight timing claim; the independent core probe covers that case.
        if self.phase == 9 {
            let case = &self.manifest.cases[0];
            let path = &self.manifest.files[&case.name].path;
            app.open_book_from_path(
                path.to_str().ok_or("Non-UTF8 fixture path")?,
                Some(case.chapter),
            );
            self.cancel_started = Some(Instant::now());
            self.phase = 10;
            return Ok(false);
        }
        if self.phase == 10 {
            if self.cancel_started.unwrap().elapsed().as_millis() >= 60 {
                if !app.book_presentation.is_active() {
                    return Err("Opening finished before native cancellation input".into());
                }
                self.cancellation = json!({"opening_active_at_escape":true,"worker_pending_at_escape":app.pending_book_open.is_some(),"inflight_core_cancellation_proven":false});
                self.key = Some(egui::Key::Escape);
                self.cancel_started = Some(Instant::now());
                self.phase = 11;
            }
            return Ok(false);
        }
        if self.phase == 11 {
            if !app.book_presentation.is_active()
                && app.view == AppView::Library
                && app.pending_book_open.is_none()
            {
                self.cancellation["escape_to_shelf_ms"] =
                    json!(self.cancel_started.unwrap().elapsed().as_secs_f64() * 1000.);
                self.cancel_settled = Some(Instant::now());
                self.phase = 12;
            }
            return Ok(false);
        }
        if self.phase == 12 {
            if app.view != AppView::Library
                || app.pending_book_open.is_some()
                || app.book_presentation.is_active()
            {
                return Err("Cancelled opening republished after returning to shelf".into());
            }
            if self.cancel_settled.unwrap().elapsed().as_millis() >= 800 {
                self.cancellation["late_completion_observed_ms"] = json!(800);
                self.cancellation["passed"] = json!(true);
                self.phase = 0;
            }
            return Ok(false);
        }
        if (13..=17).contains(&self.phase) {
            if self.phase == 13 {
                self.pointer(self.selection_end, None);
                self.phase = 14;
            } else if self.phase == 14 {
                let selection = app
                    .text_selection
                    .as_ref()
                    .ok_or("Native mouse drag did not start selection")?;
                if selection.normalized_range() != (1, 0, 1, 7) || !selection.is_dragging {
                    return Err(format!(
                        "Native dragged scalar range differs: {selection:?}"
                    ));
                }
                self.pointer(self.selection_end, Some(false));
                self.stable = 0;
                self.phase = 15;
            } else if self.phase == 15 {
                let selection = app
                    .text_selection
                    .as_ref()
                    .ok_or("Native release lost selection")?;
                if selection.normalized_range() != (1, 0, 1, 7) || selection.is_dragging {
                    return Err("Native selection did not finalize".into());
                }
                self.stable += 1;
                if self.stable < 3 {
                    return Ok(false);
                }
                let (point, text) = self
                    .glyphs
                    .lock()
                    .unwrap()
                    .highlight
                    .clone()
                    .ok_or("Actual selection toolbar did not render")?;
                if text != "文字 😀 中文" {
                    return Err(format!("Native selected excerpt differs: {text:?}"));
                }
                self.selection_end = point;
                self.pointer(point, Some(true));
                self.phase = 16;
            } else if self.phase == 16 {
                self.pointer(self.selection_end, Some(false));
                self.phase = 17;
            } else {
                let config = app
                    .book_config
                    .as_ref()
                    .ok_or("Selection book config missing")?;
                let stored =
                    reader_core::library::Library::load_book_config(&app.data_dir, &config.id)?
                        .ok_or("Selected highlight config not persisted")?;
                if !stored.highlights.iter().any(|value| {
                    value.chapter == 0
                        && (
                            value.start_block,
                            value.start_offset,
                            value.end_block,
                            value.end_offset,
                        ) == (1, 0, 1, 7)
                }) {
                    return Err("Native toolbar highlight did not persist the scalar range".into());
                }
                self.native_selection = json!({"passed":true,"native_mouse_drag":true,"visible_toolbar_action":true,"persisted":true,"text":"文字 😀 中文","start_scalar":0,"end_scalar":7});
                self.phase = 2;
                self.stable = 0;
            }
            return Ok(false);
        }
        if self.ready(app, ctx) {
            let glyphs = self.glyphs.lock().unwrap();
            if !glyphs.missing.is_empty() {
                return Err(format!("Missing native glyphs: {:?}", glyphs.missing));
            }
            self.glyph_checks += glyphs.seen.len();
        }
        if self.phase == 0 {
            let case = &self.manifest.cases[self.case];
            let path = self.manifest.files[&case.name].path.clone();
            app.open_book_from_path(
                path.to_str().ok_or("Non-UTF8 fixture path")?,
                Some(case.chapter),
            );
            self.phase = 1;
        } else if self.phase == 1 {
            if app.book.is_some() && app.pending_book_open.is_none() {
                self.configure(app, ctx);
            }
        } else if self.phase == 2 && self.ready(app, ctx) {
            self.stable += 1;
            if self.stable < 4 {
                return Ok(false);
            }
            self.inspect(app, ctx)?;
            if self.manifest.cases[self.case].name == "native-typography"
                && self.policy == 0
                && self.width == 0
                && self.native_selection.is_null()
            {
                let (start, end) = self
                    .glyphs
                    .lock()
                    .unwrap()
                    .selection
                    .ok_or("Visible native selection target missing")?;
                self.selection_end = end;
                self.pointer(start, Some(true));
                self.phase = 13;
                return Ok(false);
            }
            if app.total_pages > 1 {
                self.origin = app.current_page;
                self.key = Some(egui::Key::ArrowRight);
                self.phase = 6;
            } else {
                app.current_page = self.pages[0];
                self.phase = 3;
                self.stable = 0;
            }
        } else if self.phase == 6 {
            if app.current_page != self.origin {
                self.key = Some(egui::Key::ArrowLeft);
                self.phase = 7;
            }
        } else if self.phase == 7 {
            if app.current_page == self.origin {
                self.layout["keyboard_exact_return"] = json!(true);
                app.current_page = self.pages[0];
                self.phase = 3;
                self.stable = 0;
            }
        } else if self.phase == 3
            && self.ready(app, ctx)
            && app.current_page == self.pages[self.page]
        {
            let (pending, failed, ready) = reader_images::diagnostic_visible(ctx);
            if failed > 0 {
                return Err("Native illustration decoder failed".into());
            }
            self.decoded.extend(ready);
            if pending > 0 {
                self.stable = 0;
                return Ok(false);
            }
            self.stable += 1;
            if self.stable < 3 {
                return Ok(false);
            }
            self.capture = format!(
                "{}-{}-{}-{}-{}.png",
                self.manifest.cases[self.case].name,
                self.manifest.cases[self.case].chapter,
                self.policy().key(),
                self.width() as usize,
                self.pages[self.page]
            );
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                self.capture.clone(),
            )));
            self.phase = 4;
        } else if self.phase == 4 {
            let captured = ctx.input(|input| {
                input.events.iter().find_map(|event| {
                    if let egui::Event::Screenshot {
                        image, user_data, ..
                    } = event
                    {
                        if user_data
                            .data
                            .as_ref()
                            .and_then(|data| data.downcast_ref::<String>())
                            == Some(&self.capture)
                        {
                            Some(image.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
            });
            if let Some(image) = captured {
                let rgba: Vec<u8> = image
                    .pixels
                    .iter()
                    .flat_map(|pixel| pixel.to_array())
                    .collect();
                image::save_buffer(
                    self.output.join(&self.capture),
                    &rgba,
                    image.size[0] as u32,
                    image.size[1] as u32,
                    image::ColorType::Rgba8,
                )
                .map_err(|error| error.to_string())?;
                self.layout["screenshots"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!(self.capture));
                self.page += 1;
                if self.page == self.pages.len() {
                    if self.decoded.len() < self.manifest.cases[self.case].min_images {
                        return Err("Native corpus images were not visibly decoded".into());
                    }
                    self.next_layout(app, ctx)
                } else {
                    app.current_page = self.pages[self.page];
                    self.phase = 3;
                    self.stable = 0;
                }
            }
        }
        Ok(false)
    }
}
fn check_ranges(
    ranges: &crate::ui::reader_pagination::PageRanges,
    blocks: usize,
) -> Result<(), String> {
    if ranges.is_empty()
        || ranges.fragments.len() != ranges.len()
        || ranges[0].0 != 0
        || ranges
            .fragments
            .first()
            .is_none_or(|first| first.offset.abs() > 0.01)
        || ranges.last().unwrap().1 != blocks
    {
        return Err("Native source range coverage incomplete".into());
    }
    for (index, &(start, end)) in ranges.iter().enumerate() {
        let fragment = ranges.fragments[index];
        if start >= end
            || end > blocks
            || !fragment.height.is_finite()
            || fragment.height <= 0.
            || !fragment.offset.is_finite()
            || fragment.offset < 0.
        {
            return Err("Invalid native page fragment".into());
        }
        if index > 0 {
            let previous = ranges[index - 1];
            if start > previous.1 || start < previous.1.saturating_sub(1) {
                return Err("Native source range gap/overlap".into());
            }
            let previous_fragment = ranges.fragments[index - 1];
            if start == previous.1 {
                if fragment.offset > 0.01 {
                    return Err("New source block begins with a gap".into());
                }
            } else if fragment.offset <= 0.
                || (start == previous.0 && fragment.offset <= previous_fragment.offset)
            {
                return Err("Repeated native source fragment".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::reader_pagination::{PageFragment, PageRanges};
    fn ranges(entries: Vec<(usize, usize)>, offsets: Vec<f32>) -> PageRanges {
        PageRanges {
            ranges: entries,
            fragments: offsets
                .into_iter()
                .map(|offset| PageFragment {
                    offset,
                    height: 100.,
                    char_offset: 0,
                })
                .collect(),
        }
    }
    #[test]
    fn accepts_contiguous_block_fragments_and_rejects_gaps_or_duplicates() {
        assert!(check_ranges(&ranges(vec![(0, 1), (0, 2), (2, 3)], vec![0., 100., 0.]), 3).is_ok());
        for (entries, offsets, blocks) in [
            (vec![(0, 1), (2, 3)], vec![0., 0.], 3),
            (vec![(0, 1), (0, 1)], vec![0., 0.], 1),
            (vec![(0, 1), (0, 1)], vec![100., 90.], 1),
            (vec![(0, 1), (1, 2)], vec![0., 20.], 2),
            (vec![(0, 1)], vec![0.], 2),
            (vec![(0, 1)], vec![10.], 1),
        ] {
            assert!(check_ranges(&ranges(entries, offsets), blocks).is_err());
        }
    }
    #[test]
    fn rejects_vacuous_and_invalid_fragment_geometry() {
        assert!(check_ranges(&PageRanges::default(), 1).is_err());
        let mut valid = ranges(vec![(0, 1)], vec![0.]);
        valid.fragments[0].height = f32::NAN;
        assert!(check_ranges(&valid, 1).is_err());
        valid.fragments[0].height = 100.;
        valid.fragments.clear();
        assert!(check_ranges(&valid, 1).is_err());
    }
}
