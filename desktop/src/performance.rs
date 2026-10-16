//! Opt-in local native-window benchmark. Never uses the user's credential store.
use crate::app::{AppView, ReaderApp};
use eframe::egui;
use serde_json::{json, Value};
use std::{path::PathBuf, time::Instant};
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
pub(crate) struct ReaderPerformance {
    path: PathBuf,
    start: Instant,
    baseline: u64,
    first: Option<f64>,
    first_rss: u64,
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
            first_rss: 0,
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
        }
    }
    fn set_case(&mut self, app: &mut ReaderApp) {
        app.reader_page_animation = if self.case < 3 { "Realistic" } else { "Slide" }.into();
        let count = app.book.as_ref().unwrap().chapters.len();
        let last = count.saturating_sub(1);
        app.current_chapter = [3.min(last), count / 2, last][self.case % 3];
        app.current_page = 0;
        app.pages_dirty = true;
        app.page_curl.clear();
        app.reader_render_blocks = None;
        self.jump = Instant::now();
        self.turn = 0;
        self.ready_frames = 0;
        self.intervals.clear();
        self.cpu.clear();
        self.last_frame = None;
        self.stage = 1;
    }
    pub(crate) fn tick(&mut self, app: &mut ReaderApp, ctx: &egui::Context, cpu_ms: f64) -> bool {
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
            if now >= self.next_turn {
                if self.turn < 8 {
                    if self.turn < 4 {
                        app.next_page()
                    } else {
                        app.prev_page()
                    };
                    self.turn += 1;
                    self.next_turn = now + std::time::Duration::from_millis(450);
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
                    self.case += 1;
                    if self.case < 6 {
                        self.set_case(app)
                    } else {
                        self.stage = 3;
                        let count = app.book.as_ref().unwrap().chapters.len();
                        self.walk = 3.min(count.saturating_sub(1));
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
                    if self.walk >= self.walk_end {
                        self.write(app, true);
                        return true;
                    }
                }
            }
        }
        ctx.request_repaint();
        false
    }
    fn write(&self, app: &ReaderApp, passed: bool) {
        let stats = app.book.as_ref().map(|b| b.chapter_cache_stats());
        let report = json!({"profile":"release","environment":"macOS native eframe/glow window; callback pacing and CPU update timings, not direct GPU timestamps","first_content_visible_ms":self.first,"baseline_rss_bytes":self.baseline,"first_content_rss_bytes":self.first_rss,"after_200_chapters_rss_bytes":rss(),"cache_stats":stats,"desktop_resident_chapters":app.book.as_ref().map(|b|b.chapters.iter().filter(|c|!c.blocks.is_empty()).count()),"cases":self.cases,"passed":passed});
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&self.path, serde_json::to_vec_pretty(&report).unwrap());
    }
}
