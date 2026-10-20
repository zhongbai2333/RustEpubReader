//! Native reading activity dashboard and foreground activity timer.
use crate::app::{AppView, ReaderApp};
use eframe::egui;
use reader_core::reading_activity::{self, ActivityReport};
use std::time::{Duration, Instant};

pub struct DesktopActivity {
    id: Option<String>,
    session: String,
    last: Instant,
    touched: Instant,
    mode: Option<bool>,
    reading: Duration,
    listening: Duration,
    saved: [u64; 2],
    pub current_book: bool,
    pub range: usize,
    selected_day: Option<usize>,
    report: Option<ActivityReport>,
    loaded: Option<Instant>,
    error: Option<String>,
}
impl Default for DesktopActivity {
    fn default() -> Self {
        Self {
            id: None,
            session: format!("{:016x}", rand::random::<u64>()),
            last: Instant::now(),
            touched: Instant::now(),
            mode: None,
            reading: Duration::ZERO,
            listening: Duration::ZERO,
            saved: [0, 0],
            current_book: true,
            range: 7,
            selected_day: None,
            report: None,
            loaded: None,
            error: None,
        }
    }
}
impl ReaderApp {
    fn activity_device(&self) -> String {
        self.peer_store
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .device_id
            .clone()
    }
    pub fn poll_reading_activity(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let id = self.book_config.as_ref().map(|c| c.id.clone());
        if id != self.activity.id {
            self.flush_reading_stats();
            self.activity = DesktopActivity {
                id,
                ..Default::default()
            };
        }
        if ctx.input(|i| {
            i.events
                .iter()
                .any(|e| !matches!(e, egui::Event::PointerMoved(_) | egui::Event::PointerGone))
        }) {
            self.activity.touched = now;
        }
        let delta = now.saturating_duration_since(self.activity.last);
        if delta <= Duration::from_secs(2) {
            match self.activity.mode {
                Some(false) => self.activity.reading += delta,
                Some(true) => self.activity.listening += delta,
                None => {}
            }
        }
        self.activity.last = now;
        let reading = self.view == AppView::Reader
            && !self.show_reader_introduction
            && self.pending_book_open.is_none()
            && ctx.input(|i| i.viewport().focused.unwrap_or(true))
            && now.duration_since(self.activity.touched) < Duration::from_secs(300)
            && !self.show_stats
            && !self.show_settings
            && !self.show_search
            && !self.show_annotations
            && !self.show_review_panel
            && !self.show_csc_settings
            && !self.show_about
            && !ctx.memory(|m| m.is_popup_open(egui::Id::new("reader_quick_menu")));
        let audible = self.tts_playing
            && !self.tts_paused
            && self
                .tts_audio_sink
                .as_ref()
                .is_some_and(|sink| !sink.empty() && !sink.is_paused());
        let mode = if audible {
            Some(true)
        } else if reading {
            Some(false)
        } else {
            None
        };
        if mode != self.activity.mode
            || self
                .activity
                .reading
                .as_secs()
                .saturating_sub(self.activity.saved[0])
                >= 15
            || self
                .activity
                .listening
                .as_secs()
                .saturating_sub(self.activity.saved[1])
                >= 15
        {
            self.flush_reading_stats();
        }
        self.activity.mode = mode;
        if mode.is_some() {
            ctx.request_repaint_after(Duration::from_secs(1));
        }
    }
    pub fn flush_reading_stats(&mut self) {
        let Some(id) = self.activity.id.clone() else {
            return;
        };
        let device = self.activity_device();
        let now = chrono::Local::now();
        for (index, elapsed) in [
            self.activity.reading.as_secs(),
            self.activity.listening.as_secs(),
        ]
        .into_iter()
        .enumerate()
        {
            if elapsed <= self.activity.saved[index] {
                continue;
            }
            match reading_activity::checkpoint(
                &self.data_dir,
                &device,
                &id,
                &format!("{}{}", self.activity.session, index),
                elapsed,
                now.timestamp().max(0) as u64,
                now.offset().local_minus_utc(),
                index == 1,
            ) {
                Ok(()) => {
                    self.activity.saved[index] = elapsed;
                    self.activity.loaded = None;
                }
                Err(error) => self.activity.error = Some(error),
            }
        }
    }
    pub(crate) fn quiesce_activity_for_backup(&mut self) -> Result<(), String> {
        self.tts_stop_playback();
        self.activity.error = None;
        self.flush_reading_stats();
        self.activity.mode = None;
        self.activity.last = Instant::now();
        self.activity.error.clone().map_or(Ok(()), Err)
    }
    pub fn open_reading_stats(&mut self, current_book: bool) {
        self.flush_reading_stats();
        self.activity.current_book = current_book;
        self.activity.loaded = None;
        self.show_stats = true;
    }
    pub fn render_stats_window(&mut self, ctx: &egui::Context) {
        if !self.show_stats {
            return;
        }
        let mut open = true;
        egui::Window::new(self.i18n.t("stats.title"))
            .open(&mut open)
            .collapsible(false)
            .default_width(580.)
            .default_height((ctx.screen_rect().height() - 150.).clamp(260., 650.))
            .min_width(300.)
            .max_height((ctx.screen_rect().height() - 80.).max(240.))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let mut changed = ui
                        .selectable_value(
                            &mut self.activity.current_book,
                            false,
                            self.i18n.t("stats.all_books"),
                        )
                        .changed();
                    if self.book_config.is_some() {
                        changed |= ui
                            .selectable_value(
                                &mut self.activity.current_book,
                                true,
                                self.i18n.t("stats.current_book"),
                            )
                            .changed();
                    }
                    if changed {
                        self.activity.loaded = None;
                        self.activity.selected_day = None;
                    }
                });
                if self
                    .activity
                    .loaded
                    .is_none_or(|v| v.elapsed() > Duration::from_secs(15))
                {
                    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
                    let filter = if self.activity.current_book {
                        self.current_book_hash.as_deref()
                    } else {
                        None
                    };
                    match reading_activity::report(
                        &self.data_dir,
                        &self.activity_device(),
                        &today,
                        filter,
                    ) {
                        Ok(report) => {
                            self.activity.report = Some(report);
                            self.activity.error = None;
                        }
                        Err(error) => self.activity.error = Some(error),
                    }
                    self.activity.loaded = Some(Instant::now());
                }
                if let Some(error) = &self.activity.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                let Some(report) = self.activity.report.clone() else {
                    return;
                };
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_space(16.);
                        ui.label(self.i18n.t("stats.today"));
                        ui.label(
                            egui::RichText::new(duration(report.today, &self.i18n))
                                .size(32.)
                                .strong(),
                        );
                        ui.horizontal_wrapped(|ui| {
                            ui.label(format!(
                                "{}  {}",
                                self.i18n.t("stats.week"),
                                duration(report.week, &self.i18n)
                            ));
                            ui.separator();
                            ui.label(format!(
                                "{}  {}",
                                self.i18n.t("stats.total"),
                                duration(report.total, &self.i18n)
                            ));
                        });
                        ui.add_space(16.);
                        if report.total == 0 {
                            ui.label(self.i18n.t("stats.empty"));
                            ui.label(self.i18n.t("stats.empty_hint"));
                        } else {
                            ui.horizontal(|ui| {
                                ui.strong(self.i18n.t("stats.trend"));
                                ui.selectable_value(
                                    &mut self.activity.range,
                                    7,
                                    self.i18n.t("stats.seven_days"),
                                );
                                ui.selectable_value(
                                    &mut self.activity.range,
                                    30,
                                    self.i18n.t("stats.thirty_days"),
                                );
                            });
                            let range = self.activity.range;
                            let slice = &report.days[30 - range..];
                            let peak = slice
                                .iter()
                                .map(|d| d.reading + d.listening)
                                .max()
                                .unwrap_or(0)
                                .max(60);
                            let peak = peak.div_ceil(60) * 60;
                            let max = peak as f32;
                            let (chart, _) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 140.),
                                egui::Sense::hover(),
                            );
                            let rect = egui::Rect::from_min_max(
                                chart.min + egui::vec2(76., 0.),
                                chart.max,
                            );
                            for (y, label) in [
                                (rect.top() + 20., duration(peak, &self.i18n)),
                                (
                                    rect.bottom() - 60.,
                                    if peak % 120 == 60 {
                                        format!(
                                            "{} {}",
                                            duration(peak / 2 / 60 * 60, &self.i18n),
                                            duration(30, &self.i18n)
                                        )
                                    } else {
                                        duration(peak / 2, &self.i18n)
                                    },
                                ),
                                (rect.bottom(), "0".to_owned()),
                            ] {
                                ui.painter().text(
                                    egui::pos2(chart.left(), y),
                                    egui::Align2::LEFT_CENTER,
                                    label,
                                    egui::FontId::proportional(12.),
                                    ui.visuals().weak_text_color(),
                                );
                                ui.painter().line_segment(
                                    [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                                    egui::Stroke::new(
                                        0.5_f32,
                                        ui.visuals().weak_text_color().gamma_multiply(0.35),
                                    ),
                                );
                            }
                            let reading = super::design::Palette::new(self.dark_mode).accent;
                            let listening = egui::Color32::from_rgb(204, 142, 66);
                            for (idx, day) in slice.iter().enumerate() {
                                let w = rect.width() / range as f32;
                                let x = rect.left() + idx as f32 * w;
                                let r = egui::Rect::from_min_max(
                                    egui::pos2(x + 2., rect.top()),
                                    egui::pos2(x + w - 2., rect.bottom()),
                                );
                                let response = ui.interact(
                                    r,
                                    ui.id().with(("activity_day", idx)),
                                    egui::Sense::click(),
                                );
                                response.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::WidgetType::Button,
                                        true,
                                        format!(
                                            "{} · {} {} · {} {}",
                                            day.date,
                                            self.i18n.t("stats.reading"),
                                            duration(day.reading, &self.i18n),
                                            self.i18n.t("stats.listening"),
                                            duration(day.listening, &self.i18n)
                                        ),
                                    )
                                });
                                if response.clicked() {
                                    self.activity.selected_day = Some(30 - range + idx);
                                }
                                response.on_hover_text(format!(
                                    "{}\n{} {}\n{} {}",
                                    day.date,
                                    self.i18n.t("stats.reading"),
                                    duration(day.reading, &self.i18n),
                                    self.i18n.t("stats.listening"),
                                    duration(day.listening, &self.i18n)
                                ));
                                let h = 120. * day.reading as f32 / max;
                                let l = 120. * day.listening as f32 / max;
                                ui.painter().rect_filled(
                                    egui::Rect::from_min_max(
                                        egui::pos2(r.left(), r.bottom() - h),
                                        r.max,
                                    ),
                                    3,
                                    reading,
                                );
                                ui.painter().rect_filled(
                                    egui::Rect::from_min_max(
                                        egui::pos2(r.left(), r.bottom() - h - l),
                                        egui::pos2(r.right(), r.bottom() - h),
                                    ),
                                    3,
                                    listening,
                                );
                                if day.reading + day.listening == 0 {
                                    ui.painter().line_segment(
                                        [egui::pos2(r.left(), r.bottom()), r.max],
                                        egui::Stroke::new(1_f32, ui.visuals().weak_text_color()),
                                    );
                                }
                            }
                            let (dates, _) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 20.),
                                egui::Sense::hover(),
                            );
                            for (idx, day) in slice.iter().enumerate() {
                                if range == 7 || idx == 0 || idx == range / 2 || idx == range - 1 {
                                    let x = rect.left()
                                        + (idx as f32 + 0.5) * rect.width() / range as f32;
                                    ui.painter().text(
                                        egui::pos2(x, dates.center().y),
                                        egui::Align2::CENTER_CENTER,
                                        &day.date[5..],
                                        egui::FontId::proportional(11.),
                                        ui.visuals().text_color(),
                                    );
                                }
                            }
                            if let Some(day) =
                                self.activity.selected_day.and_then(|i| report.days.get(i))
                            {
                                ui.label(format!(
                                    "{} · {} {} · {} {}",
                                    day.date,
                                    self.i18n.t("stats.reading"),
                                    duration(day.reading, &self.i18n),
                                    self.i18n.t("stats.listening"),
                                    duration(day.listening, &self.i18n)
                                ));
                            }
                            ui.horizontal_wrapped(|ui| {
                                ui.colored_label(
                                    reading,
                                    format!(
                                        "{} {}",
                                        self.i18n.t("stats.reading"),
                                        duration(report.reading, &self.i18n)
                                    ),
                                );
                                ui.colored_label(
                                    listening,
                                    format!(
                                        "{} {}",
                                        self.i18n.t("stats.listening"),
                                        duration(report.listening, &self.i18n)
                                    ),
                                );
                                ui.label(format!(
                                    "{} {} · {} {}",
                                    self.i18n.t("stats.active_days"),
                                    report.active_days,
                                    self.i18n.t("stats.streak"),
                                    report.streak
                                ));
                            });
                            ui.add_space(16.);
                            ui.strong(self.i18n.t("stats.books"));
                            for book in &report.books {
                                ui.horizontal(|ui| {
                                    let title_width = (ui.available_width() - 100.).max(0.);
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(title_width, 28.),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.add(egui::Label::new(&book.title).truncate());
                                        },
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.label(duration(
                                                book.reading + book.listening + book.undated,
                                                &self.i18n,
                                            ));
                                        },
                                    );
                                });
                                ui.separator();
                            }
                        }

                        ui.add_space(12.);
                        ui.horizontal(|ui| {
                            ui.collapsing(self.i18n.t("stats.rules"), |ui| {
                                ui.label(self.i18n.t("stats.timer_hint"));
                                ui.label(self.i18n.t("stats.sync_hint"));
                                ui.label(self.i18n.t("stats.date_hint"));
                            });
                            if ui.button(self.i18n.t("stats.sync_settings")).clicked() {
                                self.show_stats = false;
                                self.open_sync(0);
                            }
                        });
                    });
            });
        if !open {
            self.show_stats = false;
        }
    }
}
fn duration(seconds: u64, i18n: &reader_core::i18n::I18n) -> String {
    let chinese = i18n.language() == &reader_core::i18n::Language::ZhCN;
    let (h, m, s) = if chinese {
        (
            i18n.t("stats.unit_hours"),
            i18n.t("stats.unit_minutes"),
            i18n.t("stats.unit_seconds"),
        )
    } else {
        ("h", "m", "s")
    };
    if seconds >= 3600 {
        format!("{}{h} {}{m}", seconds / 3600, seconds % 3600 / 60)
    } else if seconds >= 60 {
        format!("{}{m}", seconds / 60)
    } else {
        format!("{seconds}{s}")
    }
}
