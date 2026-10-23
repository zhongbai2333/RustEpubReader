//! Book-opening worker lifecycle and publication into the reading session.
use super::{AppView, ReaderApp};
use reader_core::epub::EpubBook;

impl ReaderApp {
    pub fn open_book_from_path(&mut self, path: &str, chapter: Option<usize>) {
        if self.backup.busy() {
            return;
        }
        if self.pending_book_open.is_some() {
            return;
        }
        if let Err(error) = self.flush_reading_position_for_backup() {
            self.error_msg = Some(error);
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let source = path.to_owned();
        let ctx = self.last_egui_ctx.clone();
        let index_dir = std::path::Path::new(&self.data_dir).join(".cache/chapter-index");
        self.begin_book_presentation(path);
        self.opening_started = Some(std::time::Instant::now());
        self.pending_book_open = Some((path.to_owned(), chapter, rx));
        let cancel = reader_core::open_operation::Cancellation::default();
        self.pending_open_cancel = Some(cancel.clone());
        std::thread::spawn(move || {
            let result = cancel.run(|| {
                EpubBook::open_cached(&source, &index_dir).and_then(|mut book| {
                    book.load_embedded_fonts()?;
                    let at = chapter
                        .unwrap_or(0)
                        .min(book.chapters.len().saturating_sub(1));
                    book.ensure_chapter_window(
                        at.saturating_sub(1),
                        (at + 2).min(book.chapters.len()),
                    )?;
                    Ok((book, EpubBook::file_hash(&source).ok()))
                })
            });
            let _ = tx.send(result);
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        });
    }
    pub fn import_epub_batch(&mut self, paths: Vec<std::path::PathBuf>) {
        if self.pending_batch_import.is_some() {
            return;
        }
        let root = self.data_dir.clone();
        let ctx = self.last_egui_ctx.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.pending_batch_import = Some(rx);
        std::thread::spawn(move || {
            let mut errors = Vec::new();
            for path in paths {
                let result = (|| -> Result<(), String> {
                    if !path
                        .extension()
                        .is_some_and(|s| s.eq_ignore_ascii_case("epub"))
                    {
                        return Err("Select TXT files one at a time".into());
                    }
                    if std::fs::metadata(&path).map_err(|e| e.to_string())?.len()
                        > reader_core::cloud_sync::MAX_DOCUMENT_BYTES
                    {
                        return Err("Book exceeds 1 GiB".into());
                    }
                    let book = EpubBook::open_lazy(&path)?;
                    let mut library = reader_core::library::Library::load_from(&root)?;
                    library.add_or_update(
                        &root,
                        book.title,
                        path.to_string_lossy().into_owned(),
                        0,
                        book.chapters.first().map(|c| c.title.clone()),
                    )?;
                    Ok(())
                })();
                if let Err(error) = result {
                    errors.push(format!(
                        "{}: {error}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                }
            }
            let _ = tx.send(errors);
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        });
    }
    pub(super) fn poll_book_open(&mut self) {
        if let Some(errors) = self
            .pending_batch_import
            .as_ref()
            .and_then(|rx| rx.try_recv().ok())
        {
            self.pending_batch_import = None;
            self.reload_library();
            self.cloud_invalidate_journal();
            if !errors.is_empty() {
                self.error_msg = Some(errors.join("\n"));
            }
        }

        let result = self
            .pending_book_open
            .as_ref()
            .and_then(|(_, _, rx)| match rx.try_recv() {
                Ok(value) => Some(value),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some(Err("Cannot open EPUB".into()))
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
            });
        if let Some(result) = result {
            let (path, chapter, _) = self.pending_book_open.take().unwrap();
            self.pending_open_cancel = None;
            let failed = result.is_err();
            self.finish_book_open(&path, chapter, result);
            if failed {
                self.close_book_presentation();
            }
        }
    }
    pub(super) fn finish_book_open(
        &mut self,
        path: &str,
        chapter: Option<usize>,
        parsed: Result<(EpubBook, Option<String>), String>,
    ) {
        self.reader_layout_anchor = None;
        self.panel_reflow_anchor = None;
        self.page_curl.clear();
        self.reader_chrome = Default::default();
        self.push_feedback_log(format!(
            "[Book] open_book_from_path: path={}, chapter={:?}",
            path, chapter
        ));
        match parsed {
            Ok((mut book, hash)) => {
                self.push_feedback_log(format!(
                    "[Book] opened: title={}, chapters={}, fonts={}",
                    book.title,
                    book.chapters.len(),
                    book.fonts.len()
                ));
                let mut ch = self
                    .library
                    .books
                    .iter()
                    .find(|entry| entry.path == path)
                    .map(|entry| entry.last_chapter)
                    .or(chapter)
                    .unwrap_or(0);
                if !book.chapters.is_empty() {
                    ch = ch.min(book.chapters.len() - 1);
                }

                self.cloud_journal_init();
                let initial_title = book.title.clone();
                let initial_chapter_title = book.chapters.get(ch).map(|c| c.title.clone());
                let mut entry = match self.library.add_or_update(
                    &self.data_dir,
                    initial_title,
                    path.to_string(),
                    ch,
                    initial_chapter_title,
                ) {
                    Ok(entry) => entry,
                    Err(error) => {
                        self.error_msg = Some(error);
                        return;
                    }
                };

                // Registration preserves a fresh persisted locator, including when
                // the shelf snapshot was old. Normalize only an invalid historical
                // chapter here, retaining the previous open-time clamp behavior.
                if !book.chapters.is_empty() && entry.last_chapter >= book.chapters.len() {
                    let last = book.chapters.len() - 1;
                    if let Err(error) = self.library.update_position(
                        &self.data_dir,
                        &entry.path,
                        last,
                        Some(book.chapters[last].title.clone()),
                        0,
                        0,
                    ) {
                        self.error_msg = Some(error);
                        return;
                    }
                    let Some(current) = self
                        .library
                        .books
                        .iter()
                        .find(|e| e.id == entry.id)
                        .cloned()
                    else {
                        self.error_msg = Some("Book is no longer in the library".into());
                        return;
                    };
                    entry = current;
                    self.cloud_invalidate_journal();
                }

                if let Err(error) = self.library.set_last_opened(
                    &self.data_dir,
                    &entry.path,
                    reader_core::now_millis(),
                ) {
                    self.error_msg = Some(error.to_string());
                };
                self.cloud_record_recent();

                let font_names: Vec<String> =
                    book.fonts.iter().map(|(name, _)| name.clone()).collect();
                self.embedded_font_names = font_names;
                // Keep the registered system font atlas when the publication's
                // embedded fonts did not change. Rebuilding it on every open
                // blocks the navigation animation even for books without fonts.
                let fonts_changed = self
                    .book
                    .as_ref()
                    .map_or(!book.fonts.is_empty(), |old| old.fonts != book.fonts);
                self.embedded_fonts_registered &= !fonts_changed;
                book.title = entry.title.clone();
                self.book = Some(book);
                self.current_book_hash = hash.or_else(|| {
                    reader_core::library::Library::read_book_config(&self.data_dir, &entry.id)
                        .and_then(|config| config.file_hash)
                });
                self.book_path = Some(entry.path.clone());
                ch = entry.last_chapter;
                self.current_chapter = ch;
                self.position_save_due = None;
                self.position_save_modified_ms = 0;
                self.last_synced_chapter = Some(ch);
                self.last_synced_block = Some(entry.last_block);
                self.last_synced_char_offset = Some(entry.last_char_offset);
                self.current_block = entry.last_block;
                self.current_char_offset = entry.last_char_offset;
                self.pending_restore_block = Some(entry.last_block);
                self.scroll_to_top = entry.last_block == 0;
                self.pages_dirty = true;
                self.current_page = 0;
                self.view = AppView::Reader;
                self.error_msg = None;
                // Reset review panel when opening a new book
                self.show_review_panel = false;
                self.review_panel_chapter = None;
                self.review_panel_anchor = None;
                self.review_panel_just_opened = false;
                self.review_panel_scroll_offset = None;
                self.review_panel_show_all = true;
                // Load annotation config and start reading timer
                self.book_config =
                    reader_core::library::Library::read_book_config(&self.data_dir, &entry.id);
                self.load_reader_navigation();

                self.push_feedback_log(format!(
                    "[Book] ready: chapter={}/{}, embedded_fonts={}, path={}",
                    ch,
                    self.total_chapters(),
                    self.embedded_font_names.len(),
                    entry.path
                ));
                // Trigger CSC correction for current + adjacent chapters
                self.csc_cache.clear();
                self.csc_trigger_chapter(ch);
                let total = self.total_chapters();
                if ch > 0 {
                    self.csc_trigger_chapter(ch - 1);
                }
                if ch + 1 < total {
                    self.csc_trigger_chapter(ch + 1);
                }
            }
            Err(e) => {
                self.push_feedback_log(format!("[Book] ERROR opening {}: {}", path, e));
                self.error_msg = Some(e);
            }
        }
    }
}
