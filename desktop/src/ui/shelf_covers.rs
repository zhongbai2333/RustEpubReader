//! One bounded background worker; egui only uploads small ready thumbnails.
use eframe::egui;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::mpsc,
};

struct Request {
    path: String,
    cache: PathBuf,
    context: egui::Context,
}
struct Loaded {
    path: String,
    image: Option<egui::ColorImage>,
}
pub(crate) struct ShelfCovers {
    requests: mpsc::SyncSender<Request>,
    results: mpsc::Receiver<Loaded>,
    pending: HashSet<String>,
    touched: HashMap<String, u64>,
    clock: u64,
    #[cfg(feature = "performance-checks")]
    pub(crate) seen_requests: HashSet<String>,
}
impl Default for ShelfCovers {
    fn default() -> Self {
        Self::with_loader(|request| {
            let bytes = reader_core::library::covers::thumbnail(
                std::path::Path::new(&request.path),
                &request.cache,
            )?;
            let image = image::load_from_memory(&bytes).ok()?.to_rgba8();
            Some(egui::ColorImage::from_rgba_unmultiplied(
                [image.width() as usize, image.height() as usize],
                image.as_raw(),
            ))
        })
    }
}
impl ShelfCovers {
    fn with_loader(load: impl Fn(&Request) -> Option<egui::ColorImage> + Send + 'static) -> Self {
        let (requests, receive) = mpsc::sync_channel::<Request>(8);
        let (send, results) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(request) = receive.recv() {
                let image = load(&request);
                if send
                    .send(Loaded {
                        path: request.path,
                        image,
                    })
                    .is_err()
                {
                    break;
                }
                request.context.request_repaint();
            }
        });
        Self {
            requests,
            results,
            pending: HashSet::new(),
            touched: HashMap::new(),
            clock: 0,
            #[cfg(feature = "performance-checks")]
            seen_requests: HashSet::new(),
        }
    }
    pub fn poll(
        &mut self,
        context: &egui::Context,
        textures: &mut HashMap<String, Option<egui::TextureHandle>>,
        books: &[reader_core::library::BookEntry],
    ) {
        for _ in 0..2 {
            let Ok(result) = self.results.try_recv() else {
                break;
            };
            self.pending.remove(&result.path);
            if !books.iter().any(|book| book.path == result.path) {
                continue;
            }
            let texture = result.image.map(|image| {
                context.load_texture(
                    format!("shelf:{}", result.path),
                    image,
                    egui::TextureOptions::LINEAR,
                )
            });
            textures.insert(result.path, texture);
            context.request_repaint();
        }
    }
    pub fn request(
        &mut self,
        path: &str,
        cache: &std::path::Path,
        context: &egui::Context,
        textures: &HashMap<String, Option<egui::TextureHandle>>,
    ) {
        self.clock += 1;
        self.touched.insert(path.into(), self.clock);
        if textures.contains_key(path) || self.pending.contains(path) || self.pending.len() >= 8 {
            return;
        }
        if self
            .requests
            .try_send(Request {
                path: path.into(),
                cache: cache.into(),
                context: context.clone(),
            })
            .is_ok()
        {
            self.pending.insert(path.into());
            #[cfg(feature = "performance-checks")]
            self.seen_requests.insert(path.into());
        }
    }
    #[cfg(feature = "performance-checks")]
    pub(crate) fn pending_count(&self) -> usize {
        self.pending.len()
    }
    pub fn trim(&mut self, textures: &mut HashMap<String, Option<egui::TextureHandle>>) {
        if textures.len() <= 64 {
            return;
        }
        let mut oldest: Vec<_> = textures
            .keys()
            .map(|path| (self.touched.get(path).copied().unwrap_or(0), path.clone()))
            .collect();
        oldest.sort_unstable();
        for (_, path) in oldest.into_iter().take(textures.len() - 64) {
            textures.remove(&path);
            self.touched.remove(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slow_cover_work_does_not_block_requests_and_queue_is_bounded() {
        let (release, wait) = mpsc::channel();
        let wait = std::sync::Mutex::new(wait);
        let (started, seen) = mpsc::channel();
        let ui_thread = std::thread::current().id();
        let mut covers = ShelfCovers::with_loader(move |_| {
            started.send(std::thread::current().id()).unwrap();
            wait.lock().unwrap().recv().unwrap();
            None
        });
        let ctx = egui::Context::default();
        let mut textures = HashMap::new();
        for i in 0..1000 {
            covers.request(
                &format!("book-{i}"),
                std::path::Path::new("unused"),
                &ctx,
                &textures,
            );
        }
        assert_eq!(covers.pending.len(), 8);
        assert_ne!(
            seen.recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            ui_thread
        );
        // Requests and polling already returned while the worker is still blocked.
        covers.poll(&ctx, &mut textures, &[]);
        assert!(textures.is_empty());
        for _ in 0..8 {
            release.send(()).unwrap();
        }
        drop(covers);
    }

    #[test]
    fn large_library_renders_and_searches_while_cover_worker_is_blocked() {
        let (release, wait) = mpsc::channel();
        let wait = std::sync::Mutex::new(wait);
        let covers = ShelfCovers::with_loader(move |_| {
            let _ = wait.lock().unwrap().recv();
            None
        });
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let dir = tempfile::tempdir().unwrap();
        let mut app = crate::app::ReaderApp::from_data_dir(dir.path().to_string_lossy().into());
        app.shelf_covers = covers;
        app.library.books = (0..2000)
            .map(|n| reader_core::library::BookEntry {
                id: format!("{n}"),
                title: format!("Book {n}"),
                path: format!("book-{n}.epub"),
                last_chapter: 0,
                last_block: 0,
                last_char_offset: 0,
                last_chapter_title: None,
                last_opened: 0,
            })
            .collect();
        let ctx = egui::Context::default();
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1080., 720.),
            )),
            ..Default::default()
        };
        let started = std::time::Instant::now();
        let _ = ctx.run(input(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.render_library(ui));
        });
        println!(
            "2000-book first egui shelf frame, all cover work blocked: {:?}",
            started.elapsed()
        );
        assert!(
            app.shelf_covers.touched.len() <= 18,
            "offscreen covers requested"
        );
        assert!(
            app.book_presentation.covers.len() <= 18,
            "offscreen cards laid out"
        );
        app.library_query = "Book 1999".into();
        let _ = ctx.run(input(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.render_library(ui));
        });
        assert!(app.book_presentation.covers.contains_key("book-1999.epub"));
        for _ in 0..8 {
            let _ = release.send(());
        }
    }
}
