use crate::app::ReaderApp;
use eframe::egui::{self, epaint::ClippedShape};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) struct ReaderLocation {
    pub chapter: usize,
    pub block: usize,
    pub char_offset: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ReturnPoint {
    pub location: ReaderLocation,
    pub title: String,
    pub page: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NavigationKind {
    Local,
    Remote,
    Return,
}

type LoadResult = std::sync::mpsc::Receiver<Result<(), String>>;

#[derive(Default)]
pub(crate) struct ReaderNavigation {
    pub history: Vec<ReturnPoint>,
    pending: Option<(ReaderLocation, NavigationKind, ReaderLocation, LoadResult)>,
    pub expanded_until: f64,
    pub restored_locator: Option<ReaderLocation>,
    previous: Vec<ClippedShape>,
    previous_textures: Vec<egui::TextureHandle>,
    fading: Vec<ClippedShape>,
    fading_textures: Vec<egui::TextureHandle>,
    begin_fade: bool,
    started: Option<f64>,
    retained_paper: Option<egui::TextureHandle>,
}

impl ReaderNavigation {
    pub(crate) fn pending_remote_from(&self, current: ReaderLocation) -> bool {
        self.pending.as_ref().is_some_and(|(_, kind, origin, _)| {
            *kind == NavigationKind::Remote && *origin == current
        })
    }
    fn remember(&mut self, point: ReturnPoint, target: ReaderLocation) {
        if point.location != target
            && self
                .history
                .last()
                .is_none_or(|p| p.location != point.location)
        {
            self.history.push(point);
            if self.history.len() > 16 {
                self.history.remove(0);
            }
        }
    }
    pub(crate) fn finish_frame(
        &mut self,
        ctx: &egui::Context,
        rect: egui::Rect,
        paper: Option<egui::TextureHandle>,
    ) {
        let now = ctx.input(|i| i.time);
        if self.begin_fade && super::reader_images::visible_images_ready(ctx) {
            self.begin_fade = false;
            self.started = Some(now);
        }
        self.previous = ctx
            .graphics(|g| g.clone().drain(&[], &Default::default()))
            .into_iter()
            .filter_map(|mut shape| {
                shape.clip_rect = shape.clip_rect.intersect(rect);
                shape.clip_rect.is_positive().then_some(shape)
            })
            .collect();
        self.previous_textures = super::reader_images::retain_textures(ctx, &self.previous);
        let t = if self.begin_fade {
            0.
        } else if let Some(started) = self.started {
            ((now - started) / 0.22).clamp(0., 1.) as f32
        } else {
            self.retained_paper = paper;
            return;
        };
        if t >= 1. {
            self.started = None;
            self.fading.clear();
            self.fading_textures.clear();
            self.retained_paper = paper;
            return;
        }
        let mut painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("reading_jump_transition"),
        ));
        painter.set_opacity(1. - t * t * (3. - 2. * t));
        for shape in &self.fading {
            painter
                .with_clip_rect(shape.clip_rect)
                .add(shape.shape.clone());
        }
        ctx.request_repaint();
    }
}

impl ReaderApp {
    pub(crate) fn reader_location(&self) -> ReaderLocation {
        ReaderLocation {
            chapter: self.current_chapter,
            block: self.current_block,
            char_offset: self.current_char_offset,
        }
    }
    fn navigation_path(&self) -> Option<std::path::PathBuf> {
        let id = &self.book_config.as_ref()?.id;
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
            return None;
        }
        Some(
            std::path::Path::new(&self.data_dir)
                .join("reading-navigation")
                .join(format!("{id}.json")),
        )
    }
    pub(crate) fn load_reader_navigation(&mut self) {
        self.reader_navigation = Default::default();
        if let Some(path) = self.navigation_path() {
            if let Ok(bytes) = std::fs::read(path) {
                if bytes.len() < 256 * 1024 {
                    if let Ok(history) = serde_json::from_slice::<Vec<ReturnPoint>>(&bytes) {
                        self.reader_navigation.history = history
                            .into_iter()
                            .rev()
                            .take(16)
                            .collect::<Vec<_>>()
                            .into_iter()
                            .rev()
                            .collect();
                    }
                }
            }
        }
    }
    fn persist_reader_navigation(&self) {
        if let Some(path) = self.navigation_path() {
            if let Ok(bytes) = serde_json::to_vec(&self.reader_navigation.history) {
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let temporary = path.with_extension("tmp");
                if std::fs::write(&temporary, bytes).is_ok() {
                    let _ = std::fs::rename(temporary, path);
                }
            }
        }
    }
    pub(crate) fn clear_reader_navigation(&mut self) {
        if let Some(path) = self.navigation_path() {
            let _ = std::fs::remove_file(path);
        }
        self.reader_navigation = Default::default();
    }
    pub(crate) fn navigate_reader(&mut self, target: ReaderLocation, kind: NavigationKind) {
        let Some(book) = self.book.as_ref() else {
            return;
        };
        if target.chapter >= book.chapters.len() || target == self.reader_location() {
            return;
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        self.reader_navigation.pending = Some((target, kind, self.reader_location(), receiver));
        let store = book.chapter_store.clone();
        let count = book.chapters.len();
        let ctx = self.last_egui_ctx.clone();
        std::thread::spawn(move || {
            let result = if let Some(store) = store {
                (target.chapter.saturating_sub(1)..(target.chapter + 2).min(count))
                    .try_for_each(|index| store.prefetch(index))
            } else {
                Ok(())
            };
            let _ = sender.send(result);
            if let Some(ctx) = ctx {
                ctx.request_repaint()
            }
        });
    }
    pub(crate) fn poll_reader_navigation(&mut self) {
        let Some((target, kind, origin, receiver)) = self.reader_navigation.pending.take() else {
            return;
        };
        if kind == NavigationKind::Remote && self.reader_location() != origin {
            return;
        }
        match receiver.try_recv() {
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.reader_navigation.pending = Some((target, kind, origin, receiver));
                return;
            }
            Err(_) => {
                self.error_msg = Some("Cannot load reading position".into());
                return;
            }
            Ok(Err(error)) => {
                self.error_msg = Some(error);
                return;
            }
            Ok(Ok(())) => {}
        }
        let Some(chapter) = self
            .book
            .as_ref()
            .and_then(|b| b.chapter(target.chapter).ok())
        else {
            return;
        };
        if !chapter.blocks.is_empty() && target.block >= chapter.blocks.len() {
            return;
        }
        let origin = ReturnPoint {
            location: self.reader_location(),
            title: self
                .book
                .as_ref()
                .and_then(|b| b.chapters.get(self.current_chapter))
                .map(|c| c.title.clone())
                .unwrap_or_default(),
            page: self.i18n.tf2(
                "reader.page_info",
                &(self.current_page + 1).to_string(),
                &self.total_pages.max(1).to_string(),
            ),
        };
        if kind == NavigationKind::Return {
            self.reader_navigation.history.pop();
        } else {
            self.reader_navigation.remember(origin, target);
        }
        if self.reader_page_animation != "None"
            && self
                .last_egui_ctx
                .as_ref()
                .is_none_or(|ctx| ctx.style().animation_time > 0.)
        {
            self.reader_navigation.fading = self.reader_navigation.previous.clone();
            self.reader_navigation.fading_textures =
                self.reader_navigation.previous_textures.clone();
            self.reader_navigation.started = None;
            self.reader_navigation.begin_fade = true;
        } else {
            self.reader_navigation.fading.clear();
            self.reader_navigation.fading_textures.clear();
            self.reader_navigation.started = None;
            self.reader_navigation.begin_fade = false;
        }
        self.page_curl.clear();
        self.tts_detach_view();
        self.text_selection = None;
        self.reader_navigation.restored_locator = Some(target);
        self.current_chapter = target.chapter;
        self.current_block = target.block;
        self.current_char_offset = target.char_offset;
        self.pending_restore_block = Some(target.block);
        self.scroll_to_top = target.block == 0 && target.char_offset == 0;
        self.pages_dirty = true;
        self.current_page = 0;
        if kind == NavigationKind::Remote {
            self.last_synced_chapter = Some(target.chapter);
            self.last_synced_block = Some(target.block);
            self.last_synced_char_offset = Some(target.char_offset);
        }
        if kind == NavigationKind::Remote {
            // The producer committed the revision before this animated UI notification.
            self.position_save_due = None;
        } else {
            self.position_save_modified_ms = reader_core::now_millis();
            self.position_save_due = Some(std::time::Instant::now());
            if let Err(error) = self.flush_reading_position_for_backup() {
                self.error_msg = Some(error);
            }
        }
        let now = self
            .last_egui_ctx
            .as_ref()
            .map_or(0., |ctx| ctx.input(|i| i.time));
        self.reader_navigation.expanded_until = now + 3.;
        self.reader_chrome.reveal(now);
        self.persist_reader_navigation();
    }
}

// Runs only in an explicitly requested, isolated debug preview; never in release builds.
#[cfg(debug_assertions)]
impl ReaderApp {
    pub(crate) fn check_reader_navigation(&mut self, ctx: &egui::Context) {
        if !std::env::args().any(|arg| arg == "--verify-navigation") {
            return;
        }
        let Ok(directory) = std::env::var("READER_NAVIGATION_REPORT") else {
            return;
        };
        let directory = std::path::Path::new(&directory);
        for event in ctx.input(|input| input.events.clone()) {
            if let egui::Event::Screenshot {
                image, user_data, ..
            } = event
            {
                if let Some(name) = user_data
                    .data
                    .as_ref()
                    .and_then(|data| data.downcast_ref::<String>())
                {
                    let rgba: Vec<u8> = image
                        .pixels
                        .iter()
                        .flat_map(|pixel| pixel.to_array())
                        .collect();
                    if let Some(pixels) =
                        image::RgbaImage::from_raw(image.size[0] as u32, image.size[1] as u32, rgba)
                    {
                        let _ = pixels.save(directory.join(name));
                    }
                }
            }
        }
        let id = egui::Id::new("reader_navigation_check");
        let (mut step, mut deadline) =
            ctx.data(|data| data.get_temp::<(u32, f64)>(id).unwrap_or((0, 0.)));
        let now = ctx.input(|input| input.time);
        let began = ctx.data_mut(|data| {
            *data.get_temp_mut_or_insert_with(egui::Id::new("navigation_check_start"), || now)
        });
        if step < 6 && now - began > 30. {
            let result = serde_json::json!({"passed": false, "step": step, "position": self.reader_location(), "error": "Navigation runtime timeout"});
            let _ = std::fs::write(directory.join("desktop-runtime.json"), result.to_string());
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let origin = ReaderLocation {
            chapter: 19,
            block: 3,
            char_offset: 37,
        };
        let target = ReaderLocation {
            chapter: 21,
            block: 3,
            char_offset: 3,
        };
        if now < deadline {
            ctx.request_repaint();
            return;
        }
        match step {
            0 if self.book.is_some()
                && self.view == crate::app::AppView::Reader
                && !self.book_presentation.is_active() =>
            {
                self.reader_introduction_seen = true;
                self.show_reader_introduction = false;
                self.reader_page_animation = "Slide".into();
                self.navigate_reader(origin, NavigationKind::Local);
                step = 1;
            }
            1 if self.reader_location() == origin => {
                self.reader_navigation.history.clear();
                self.navigate_reader(target, NavigationKind::Remote);
                step = 2;
                deadline = now + 0.6;
            }
            2 if self.reader_location() == target => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                    "desktop-expanded.png".to_owned(),
                )));
                step = 3;
                deadline = now + 3.2;
            }
            3 => {
                self.reader_chrome.reveal(now);
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                    "desktop-collapsed.png".to_owned(),
                )));
                step = 4;
                deadline = now + 0.4;
            }
            4 => {
                self.navigate_reader(origin, NavigationKind::Return);
                step = 5;
                deadline = now + 0.4;
            }
            5 if self.reader_location() == origin => {
                let valid = self.reader_navigation.history.is_empty();
                let result = serde_json::json!({"passed": valid, "native_window": true, "remote_jump": true, "precise_return": true, "chapter": self.current_chapter, "block": self.current_block, "scalar_offset": self.current_char_offset});
                let _ = std::fs::write(directory.join("desktop-runtime.json"), result.to_string());
                step = 6;
                deadline = now + 0.5;
            }
            6 => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                step = 7;
            }
            _ => {}
        }
        ctx.data_mut(|data| data.insert_temp(id, (step, deadline)));
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_is_bounded_deduplicated_and_keeps_same_chapter_offsets() {
        let mut navigation = ReaderNavigation::default();
        let point = ReturnPoint {
            location: ReaderLocation {
                chapter: 2,
                block: 3,
                char_offset: 37,
            },
            title: "Chapter".into(),
            page: "3 / 9".into(),
        };
        navigation.remember(point.clone(), point.location);
        assert!(navigation.history.is_empty());
        let target = ReaderLocation {
            char_offset: 400,
            ..point.location
        };
        navigation.remember(point.clone(), target);
        navigation.remember(point.clone(), target);
        assert_eq!(navigation.history, vec![point]);
        for offset in 40..80 {
            navigation.remember(
                ReturnPoint {
                    location: ReaderLocation {
                        char_offset: offset,
                        ..target
                    },
                    title: "Same chapter".into(),
                    page: "page".into(),
                },
                target,
            );
        }
        assert_eq!(navigation.history.len(), 16);
        assert_eq!(navigation.history.first().unwrap().location.char_offset, 64);
        let saved = serde_json::to_vec(&navigation.history).unwrap();
        assert_eq!(
            serde_json::from_slice::<Vec<ReturnPoint>>(&saved).unwrap(),
            navigation.history
        );
    }
}
