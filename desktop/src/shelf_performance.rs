//! Opt-in measurements of the actual library renderer and its cover worker.
use crate::app::{AppView, ReaderApp};
use eframe::egui;
use reader_core::diagnostics::{peak_rss, rss};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub(crate) struct ShelfPerformance {
    start: Instant,
    baseline: Option<u64>,
    expected: usize,
    manifest: String,
    output: PathBuf,
    stage: usize,
    ready_frames: usize,
    metadata: Option<f64>,
    covers: Option<f64>,
    first_memory: Option<u64>,
    rect: egui::Rect,
    content: f32,
    offset: f32,
    pending: Option<f32>,
    transition: Option<(Instant, f32, f32)>,
    forward: bool,
    steps: usize,
    last: Option<Instant>,
    intervals: Vec<f64>,
    cpu: Vec<f64>,
    curve: Vec<Value>,
    steady: Vec<Value>,
    next_memory: Instant,
    initial_requests: usize,
    pending_report: Option<Value>,
}
impl ShelfPerformance {
    pub(crate) fn new(start: Instant, baseline: Option<u64>) -> Self {
        if cfg!(debug_assertions) {
            panic!("Shelf measurements require Release");
        }
        let bytes = std::fs::read("manifest.json").expect("Provisioned shelf manifest");
        let manifest: Value = serde_json::from_slice(&bytes).unwrap();
        let expected = manifest["count"]
            .as_u64()
            .filter(|n| (1..=10000).contains(n))
            .expect("Valid shelf size") as usize;
        assert_eq!(manifest["scenario"], "dense-shelf");
        Self {
            start,
            baseline,
            expected,
            manifest: reader_core::bytes_hash(&bytes),
            output: std::env::var("READER_SHELF_REPORT")
                .expect("READER_SHELF_REPORT")
                .into(),
            stage: 0,
            ready_frames: 0,
            metadata: None,
            covers: None,
            first_memory: None,
            rect: egui::Rect::NOTHING,
            content: 0.,
            offset: 0.,
            pending: None,
            transition: None,
            forward: true,
            steps: 0,
            last: None,
            intervals: vec![],
            cpu: vec![],
            curve: vec![],
            steady: vec![],
            next_memory: Instant::now(),
            initial_requests: 0,
            pending_report: None,
        }
    }
    pub(crate) fn take_offset(&mut self) -> Option<f32> {
        self.pending.take()
    }
    pub(crate) fn observe(&mut self, rect: egui::Rect, content: f32, offset: f32) {
        self.rect = rect;
        self.content = content;
        self.offset = offset;
    }
    pub(crate) fn tick(&mut self, app: &ReaderApp, ctx: &egui::Context, cpu_ms: f64) -> bool {
        if self.start.elapsed().as_secs() > 90
            || app.error_msg.is_some()
            || app.view != AppView::Library
            || app.cover_textures.len() > 64
            || app.shelf_covers.pending_count() > 8
        {
            self.write(app, ctx, false);
            return true;
        }
        let now = Instant::now();
        if self.stage == 0
            && app.library.books.len() == self.expected
            && !app.book_presentation.covers.is_empty()
        {
            self.ready_frames += 1;
            if self.ready_frames >= 2 && self.metadata.is_none() {
                self.metadata = Some(self.start.elapsed().as_secs_f64() * 1000.);
                self.first_memory = rss();
                self.initial_requests = app.shelf_covers.seen_requests.len();
                if self.expected > 96 && self.initial_requests >= self.expected.min(1000) {
                    self.write(app, ctx, false);
                    return true;
                }
            }
            if self.metadata.is_some()
                && app
                    .cover_textures
                    .values()
                    .filter(|value| value.is_some())
                    .count()
                    >= self.expected.min(4)
            {
                self.covers = Some(self.start.elapsed().as_secs_f64() * 1000.);
                self.stage = 1;
                self.last = None;
            }
        }
        if self.stage == 1 {
            if let Some(last) = self.last {
                self.intervals
                    .push(now.duration_since(last).as_secs_f64() * 1000.);
            }
            self.last = Some(now);
            self.cpu.push(cpu_ms);
            if let Some((started, from, to)) = self.transition {
                let progress = (started.elapsed().as_secs_f32() / 0.3).clamp(0., 1.);
                self.pending = Some(
                    from + (to - from) * (0.5 - 0.5 * (progress * std::f32::consts::PI).cos()),
                );
                if started.elapsed() >= Duration::from_millis(450) {
                    if self.expected > 96 && (self.offset - to).abs() > 1. {
                        self.write(app, ctx, false);
                        return true;
                    }
                    self.steps += 1;
                    self.transition = None;
                    if self.steps.is_multiple_of(5) {
                        self.curve.push(json!({"scrolls":self.steps,"rss_bytes":rss(),"cover_cache_count":app.cover_textures.len(),"requested":app.shelf_covers.seen_requests.len(),"pending":app.shelf_covers.pending_count()}));
                    }
                    if self.steps == 30 {
                        self.stage = 2;
                        self.next_memory = now + Duration::from_millis(750);
                    }
                }
            } else {
                let limit = (self.content - self.rect.height()).max(0.);
                if self.offset >= limit - 1. {
                    self.forward = false;
                }
                if self.offset <= 0. {
                    self.forward = true;
                }
                let target = (self.offset
                    + (if self.forward { 1. } else { -1. }) * self.rect.height() * 0.7)
                    .clamp(0., limit);
                self.transition = Some((now, self.offset, target));
            }
        }
        if self.stage == 2 && now >= self.next_memory {
            self.steady.push(json!({"rss_bytes":rss()}));
            self.next_memory = now + Duration::from_millis(150);
            if self.steady.len() == 5 {
                let requested = app.shelf_covers.seen_requests.len();
                let passed = self.intervals.len() >= 100
                    && app.library.books.len() == self.expected
                    && requested >= self.expected.min(12)
                    && (self.expected <= 96 || requested < self.expected);
                self.write(app, ctx, passed);
                return !passed;
            }
        }
        if self.stage == 3 {
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
                            .is_some_and(|name| name == "reader-shelf-performance")
                        {
                            return Some(image.clone());
                        }
                    }
                    None
                })
            });
            if let Some(image) = captured {
                let mut report = self.pending_report.take().unwrap();
                let pixels: Vec<u8> = image
                    .pixels
                    .iter()
                    .flat_map(|pixel| pixel.to_array())
                    .collect();
                let result =
                    image::RgbaImage::from_raw(image.size[0] as u32, image.size[1] as u32, pixels)
                        .ok_or("Invalid shelf screenshot")
                        .and_then(|image| {
                            image
                                .save(self.output.with_extension("png"))
                                .map_err(|_| "Cannot save shelf screenshot")
                        });
                if let Err(error) = result {
                    report["passed"] = json!(false);
                    report["error"] = json!(error);
                }
                self.publish(&report);
                return true;
            }
        }
        ctx.request_repaint();
        false
    }
    fn write(&mut self, app: &ReaderApp, ctx: &egui::Context, passed: bool) {
        let mut frames = self.intervals.clone();
        frames.sort_by(f64::total_cmp);
        let mut cpu = self.cpu.clone();
        cpu.sort_by(f64::total_cmp);
        let percentile = |values: &[f64], n: usize| values.get(values.len() * n / 100).copied();
        let report = json!({"harness_schema":1,"scenario":"dense-shelf","profile":"release","run_id":std::env::var("READER_PERFORMANCE_RUN").ok(),"fixture_manifest_sha256":self.manifest,"book_count":self.expected,"metadata_first_layout_ms":self.metadata,"first_covers_ready_ms":self.covers,"baseline_rss_bytes":self.baseline,"first_metadata_rss_bytes":self.first_memory,"peak_rss_bytes":peak_rss(),"initial_cover_requests":self.initial_requests,"total_cover_requests":app.shelf_covers.seen_requests.len(),"cover_cache_count":app.cover_textures.len(),"pending":app.shelf_covers.pending_count(),"scrolls":self.steps,"scroll_memory_samples":self.curve,"steady_memory_samples":self.steady,"frame_samples":frames.len(),"frame_callback_p50_ms":percentile(&frames,50),"frame_callback_p95_ms":percentile(&frames,95),"frame_callback_max_ms":frames.last(),"update_cpu_p95_ms":percentile(&cpu,95),"update_cpu_max_ms":cpu.last(),"input_kind":"programmatic animated egui ScrollArea offset","viewport":{"width_points":ctx.screen_rect().width(),"height_points":ctx.screen_rect().height(),"pixels_per_point":ctx.pixels_per_point()},"passed":passed,"error":app.error_msg});
        if passed {
            self.pending_report = Some(report);
            self.stage = 3;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                "reader-shelf-performance".to_owned(),
            )));
            ctx.request_repaint();
        } else {
            self.publish(&report);
        }
    }
    fn publish(&self, report: &Value) {
        if let Some(parent) = self.output.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&self.output, serde_json::to_vec_pretty(report).unwrap());
    }
}
