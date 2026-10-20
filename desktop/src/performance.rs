//! Opt-in local native-window benchmark. Never uses the user's credential store.
use crate::app::{AppView, ReaderApp};
use eframe::egui;
use reader_core::diagnostics::{peak_rss, rss};
use serde_json::{json, Value};
use std::{path::PathBuf, time::Instant};
pub(crate) struct ReaderPerformance {
    path: PathBuf,
    start: Instant,
    baseline: Option<u64>,
    first: Option<f64>,
    first_rss: Option<u64>,
    stage: usize,
    case: usize,
    jump: Instant,
    next_turn: Instant,
    turn: usize,
    intervals: Vec<f64>,
    cpu: Vec<f64>,
    last_frame: Option<Instant>,
    cases: Vec<Value>,
    walk: usize,
    walk_frame: usize,
    walk_end: usize,
    ready_frames: usize,
    turn_started: Option<Instant>,
    turn_source: (usize, usize),
    turns: Vec<f64>,
    viewport: Option<Value>,
    targets: Vec<usize>,
    walk_start: usize,
    walk_rss: Option<u64>,
    memory_samples: Vec<Value>,
    steady: Vec<Value>,
    next_memory: Instant,
    endurance: bool,
    case_origin: (usize, usize),
}
impl ReaderPerformance {
    pub(crate) fn new(path: PathBuf) -> Self {
        let baseline = rss();
        let now = Instant::now();
        Self {
            path,
            start: now,
            baseline,
            first: None,
            first_rss: None,
            stage: 0,
            case: 0,
            jump: now,
            next_turn: now,
            turn: 0,
            intervals: vec![],
            cpu: vec![],
            last_frame: None,
            cases: vec![],
            walk: 3,
            walk_frame: 0,
            walk_end: 203,
            ready_frames: 0,
            turn_started: None,
            turn_source: (0, 0),
            turns: vec![],
            viewport: None,
            targets: vec![],
            walk_start: 0,
            walk_rss: None,
            memory_samples: vec![],
            steady: vec![],
            next_memory: now,
            endurance: std::env::var("READER_PERFORMANCE_ENDURANCE").as_deref() == Ok("1"),
            case_origin: (0, 0),
        }
    }
    fn set_case(&mut self, app: &mut ReaderApp) {
        let count = app.book.as_ref().unwrap().chapters.len();
        let last = count.saturating_sub(1);
        self.targets = vec![3.min(last), count / 2, last];
        self.targets.sort_unstable();
        self.targets.dedup();
        if self.endurance {
            self.targets = vec![0];
        }
        app.reader_page_animation = if self.case < self.targets.len() {
            "Realistic"
        } else {
            "Slide"
        }
        .into();
        app.current_chapter = self.targets[self.case % self.targets.len()];
        app.current_page = 0;
        app.current_block = 0;
        app.current_char_offset = 0;
        app.reader_layout_anchor = None;
        app.pages_dirty = true;
        app.page_curl.clear();
        app.reader_render_blocks = None;
        self.jump = Instant::now();
        self.turn = 0;
        self.ready_frames = 0;
        self.intervals.clear();
        self.cpu.clear();
        self.last_frame = None;
        self.turns.clear();
        self.turn_started = None;
        self.stage = 1;
    }
    pub(crate) fn tick(&mut self, app: &mut ReaderApp, ctx: &egui::Context, cpu_ms: f64) -> bool {
        self.viewport = Some(
            json!({"width_points":ctx.screen_rect().width(),"height_points":ctx.screen_rect().height(),"pixels_per_point":ctx.pixels_per_point()}),
        );
        let ready = app.view == AppView::Reader
            && app.book.is_some()
            && !app.pages_dirty
            && !app.defer_custom_font_for_frame
            && !app.book_presentation.is_active();
        if self.start.elapsed().as_secs() > 180 {
            self.write(app, false);
            return true;
        }
        if self.stage == 0 && ready {
            self.ready_frames += 1;
            if self.ready_frames >= 2 {
                self.first = Some(self.start.elapsed().as_secs_f64() * 1000.);
                self.first_rss = rss();
                self.set_case(app);
            }
        } else if self.stage == 1 && ready {
            self.ready_frames += 1;
            if self.ready_frames >= 2 {
                self.cases.push(json!({"mode":app.reader_page_animation,"chapter":app.current_chapter,"chapter_first_visible_ms":self.jump.elapsed().as_secs_f64()*1000.}));
                self.case_origin = (app.current_chapter, app.current_page);
                self.cases.last_mut().unwrap()["turn_memory_samples"] = json!([]);
                self.next_turn = Instant::now();
                self.stage = 2;
            }
        } else if self.stage == 2 {
            let now = Instant::now();
            if let Some(last) = self.last_frame {
                self.intervals
                    .push(now.duration_since(last).as_secs_f64() * 1000.)
            }
            self.last_frame = Some(now);
            self.cpu.push(cpu_ms);
            if let Some(started) = self.turn_started {
                if ready && !app.page_curl.busy() && app.page_anim_progress >= 1.0 {
                    if (app.current_chapter, app.current_page) == self.turn_source {
                        self.write(app, false);
                        return true;
                    }
                    self.turns.push(started.elapsed().as_secs_f64() * 1000.);
                    if self.endurance && self.turns.len().is_multiple_of(16) {
                        self.cases.last_mut().unwrap()["turn_memory_samples"]
                            .as_array_mut()
                            .unwrap()
                            .push(json!({"turns":self.turns.len(),"rss_bytes":rss()}));
                    }
                    self.turn_started = None;
                    self.next_turn = now + std::time::Duration::from_millis(30);
                }
            } else if now >= self.next_turn {
                let turns = if self.endurance { 128 } else { 8 };
                if self.turn < turns {
                    self.turn_source = (app.current_chapter, app.current_page);
                    self.turn_started = Some(now);
                    app.turn_reader_page(self.turn < turns / 2);
                    self.turn += 1;
                } else {
                    self.intervals.sort_by(f64::total_cmp);
                    self.cpu.sort_by(f64::total_cmp);
                    let v = self.cases.last_mut().unwrap();
                    let frames = &self.intervals;
                    v["frame_samples"] = json!(frames.len());
                    v["frame_callback_p50_ms"] = json!(frames[frames.len() / 2]);
                    v["frame_callback_p95_ms"] = json!(frames[frames.len() * 95 / 100]);
                    v["frame_callback_max_ms"] = json!(frames.last());
                    v["update_cpu_p95_ms"] = json!(self.cpu[self.cpu.len() * 95 / 100]);
                    v["update_cpu_max_ms"] = json!(self.cpu.last());
                    v["rss_bytes"] = json!(rss());
                    v["turn_total_ms"] = json!(self.turns);
                    v["returned_to_origin"] =
                        json!((app.current_chapter, app.current_page) == self.case_origin);
                    if self.endurance && (app.current_chapter, app.current_page) != self.case_origin
                    {
                        self.write(app, false);
                        return true;
                    }
                    self.case += 1;
                    if self.case < self.targets.len() * 2 {
                        self.set_case(app)
                    } else {
                        self.stage = 3;
                        let count = app.book.as_ref().unwrap().chapters.len();
                        self.walk = 3.min(count.saturating_sub(1));
                        self.walk_start = self.walk;
                        self.walk_end = (self.walk + 200).min(count);
                        self.walk_frame = 0;
                        app.reader_page_animation = "None".into();
                    }
                }
            }
        } else if self.stage == 3 {
            if self.walk_frame == 0 {
                app.current_chapter = self.walk;
                app.current_page = 0;
                app.pages_dirty = true;
                app.reader_render_blocks = None;
                app.page_curl.clear();
                self.walk_frame = 1;
            } else if ready {
                self.walk_frame += 1;
                if self.walk_frame >= 3 {
                    self.walk += 1;
                    self.walk_frame = 0;
                    if (self.walk - self.walk_start).is_multiple_of(25)
                        || self.walk >= self.walk_end
                    {
                        self.memory_samples
                            .push(json!({"chapters":self.walk-self.walk_start,"rss_bytes":rss()}));
                    }
                    if self.walk >= self.walk_end {
                        self.walk_rss = rss();
                        self.stage = 4;
                        self.next_memory = Instant::now() + std::time::Duration::from_millis(750);
                    }
                }
            }
        }
        if self.stage == 4 && Instant::now() >= self.next_memory {
            self.steady.push(
                rss()
                    .map(|n| json!({"rss_bytes":n}))
                    .unwrap_or_else(|| json!({})),
            );
            self.next_memory = Instant::now() + std::time::Duration::from_millis(150);
            if self.steady.len() == 5 {
                self.write(app, true);
                return true;
            }
        }
        ctx.request_repaint();
        false
    }
    fn write(&self, app: &ReaderApp, passed: bool) {
        let stats = app.book.as_ref().map(|b| b.chapter_cache_stats());
        let report = json!({"harness_schema":3,"run_id":std::env::var("READER_PERFORMANCE_RUN").ok(),"profile":if cfg!(debug_assertions) {"debug"} else {"release"},"book_sha256":app.current_book_hash,"chapter_count":app.book.as_ref().map(|b|b.chapters.len()),"environment":"native eframe/glow window; callback pacing and CPU update timings, not direct GPU timestamps","viewport":self.viewport,"first_content_visible_ms":self.first,"baseline_rss_bytes":self.baseline,"first_content_rss_bytes":self.first_rss,"after_walk_rss_bytes":self.walk_rss,"peak_rss_bytes":peak_rss(),"walk_chapters":self.walk_end-self.walk_start,"walk_memory_samples":self.memory_samples,"steady_memory_samples":self.steady,"cache_stats":stats,"desktop_resident_chapters":app.book.as_ref().map(|b|b.chapters.iter().filter(|c|!c.blocks.is_empty()).count()),"cases":self.cases,"passed":passed});
        let mut report = report;
        report["scenario"] = json!(if self.endurance {
            "visible-endurance"
        } else {
            "reader"
        });
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&self.path, serde_json::to_vec_pretty(&report).unwrap());
    }
}
