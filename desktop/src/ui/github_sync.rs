//! Optional GitHub sync UI and main-thread application of background results.
use crate::{
    app::ReaderApp,
    github_sync::{self, LocalBook, SyncPhase, SyncResult, TransferFailure},
};
use eframe::egui;
use reader_core::{
    cloud_sync::{CloudBook, MAX_DOCUMENT_BYTES},
    sharing::ProgressEntry,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

type Journal = reader_core::cloud_sync::SyncJournal;
#[derive(Default)]
pub struct CloudSyncState {
    journal: Option<Journal>,
    pending: Option<mpsc::Receiver<Result<(SyncResult, usize), String>>>,
    cancel: Arc<AtomicBool>,
    last_attempt: Option<Instant>,
    dirty_at: Option<Instant>,
    retry_delay: u64,
    throttled: bool,
    phase: Option<SyncPhase>,
    retry_failed: bool,
    failed: Vec<TransferFailure>,
    pub needs_auth: bool,
    pub status: String,
    pub offered: Option<ProgressEntry>,
    pub completed_at: Option<u64>,
    account: Option<String>,
}
impl CloudSyncState {
    pub fn is_syncing(&self) -> bool {
        self.pending.is_some()
    }

    pub fn clear_account(&mut self) {
        self.stop();
        self.completed_at = None;
        self.status.clear();
        self.retry_delay = 300;
        self.failed.clear();
    }
    pub fn stop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        self.pending = None;
        self.offered = None;
        self.last_attempt = None;
        self.account = None;
        self.phase = None;
    }
}
impl Drop for CloudSyncState {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}
impl ReaderApp {
    pub fn cloud_set_enabled(&mut self, enabled: bool) {
        self.github_sync_enabled = enabled;
        if enabled {
            self.cloud_sync.needs_auth = self.github_token.is_none();
            self.cloud_sync.last_attempt = None;
            self.cloud_sync.account = self.github_username.clone();
        } else {
            self.cloud_sync.stop();
        }
    }

    pub fn github_prepare_sync(&mut self) {
        if self.github_token.is_some() && self.github_repo_authorized && !self.cloud_sync.needs_auth
        {
            self.cloud_set_enabled(true);
            self.cloud_start_sync();
        } else {
            self.github_start_sync_device_flow();
        }
    }
    pub(crate) fn cloud_journal_init(&mut self) {
        if self.cloud_sync.journal.is_some() {
            return;
        }
        let device_id = self
            .peer_store
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .device_id
            .clone();
        let mut journal = Journal::load(&self.data_dir, &device_id);
        for entry in &self.library.books {
            journal.seed_entry(&self.data_dir, entry);
        }
        self.cloud_sync.journal = Some(journal);
    }
    fn cloud_save_journal(&mut self) {
        if let Some(journal) = &self.cloud_sync.journal {
            let result = journal.save(&self.data_dir);
            if result.is_err() {
                self.cloud_sync.status = self.i18n.t("cloud.save_failed").into();
            }
        }
    }
    pub fn cloud_record_progress(&mut self, p: ProgressEntry) {
        self.cloud_journal_init();
        if self.cloud_sync.journal.as_mut().unwrap().record(p) {
            self.cloud_sync.dirty_at = Some(Instant::now());
            self.cloud_save_journal();
        }
    }
    pub fn cloud_start_sync(&mut self) {
        if self.book_share.receive && self.book_share.busy {
            return;
        }
        self.cloud_start_phase(SyncPhase::Progress, false);
    }
    fn cloud_start_phase(&mut self, phase: SyncPhase, retry_failed: bool) {
        if !self.github_sync_enabled
            || self.cloud_sync.pending.is_some()
            || (phase == SyncPhase::Progress
                && self.cloud_sync.throttled
                && self.cloud_sync.last_attempt.is_some_and(|time| {
                    time.elapsed() < Duration::from_secs(self.cloud_sync.retry_delay)
                }))
        {
            return;
        }
        let (Some(token), Some(owner)) = (self.github_token.clone(), self.github_username.clone())
        else {
            self.cloud_sync.needs_auth = true;
            return;
        };
        self.cloud_journal_init();
        let journal = self.cloud_sync.journal.as_mut().unwrap();
        if let Ok(activity) =
            reader_core::reading_activity::export(&self.data_dir, &journal.own.device_id)
        {
            for (hash, value) in activity {
                journal.own.activity.entry(hash).or_default().merge(&value);
            }
        }
        let own = journal.own.clone();
        let seen = journal.seen.clone();
        let entries = self.library.books.clone();
        self.cloud_save_journal();
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        self.cloud_sync.cancel = cancel.clone();
        self.cloud_sync.account = Some(owner.clone());
        self.cloud_sync.pending = Some(rx);
        self.cloud_sync.phase = Some(phase);
        self.cloud_sync.retry_failed = retry_failed;
        self.cloud_sync.last_attempt = Some(Instant::now());
        if phase == SyncPhase::Progress {
            self.cloud_sync.dirty_at = None;
        }
        self.cloud_sync.status = self
            .i18n
            .t(if phase == SyncPhase::Progress {
                "cloud.progress_syncing"
            } else {
                "cloud.books_syncing"
            })
            .into();
        let ctx = self.last_egui_ctx.clone();
        let count_contributions = self.github_sync_contributions;
        let data_dir = self.data_dir.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let mut own = own;
                let mut books = Vec::new();
                let mut skipped = 0;
                for entry in entries {
                    if cancel.load(Ordering::SeqCst) {
                        return Err("Sync cancelled".into());
                    }
                    if phase == SyncPhase::Progress {
                        if let Some(hash) =
                            reader_core::library::Library::read_book_config(&data_dir, &entry.id)
                                .and_then(|c| c.file_hash)
                        {
                            if !seen.contains(&hash) {
                                own.progress
                                    .entry(hash.clone())
                                    .or_insert_with(|| ProgressEntry {
                                        book_hash: hash,
                                        title: entry.title,
                                        chapter: entry.last_chapter,
                                        block: entry.last_block,
                                        char_offset: entry.last_char_offset,
                                        chapter_title: entry.last_chapter_title,
                                        timestamp: if (
                                            entry.last_chapter,
                                            entry.last_block,
                                            entry.last_char_offset,
                                        ) == (0, 0, 0)
                                        {
                                            0
                                        } else {
                                            entry.last_opened
                                        },
                                    });
                            }
                        }
                        continue;
                    }
                    let Ok(metadata) = std::fs::metadata(&entry.path) else {
                        skipped += 1;
                        continue;
                    };
                    if metadata.len() > MAX_DOCUMENT_BYTES {
                        skipped += 1;
                        continue;
                    }
                    let Some(hash) =
                        reader_core::library::Library::read_book_config(&data_dir, &entry.id)
                            .and_then(|c| c.file_hash)
                            .or_else(|| reader_core::epub::EpubBook::file_hash(&entry.path).ok())
                    else {
                        skipped += 1;
                        continue;
                    };
                    if !seen.contains(&hash) {
                        own.progress
                            .entry(hash.clone())
                            .or_insert_with(|| ProgressEntry {
                                book_hash: hash.clone(),
                                title: entry.title.clone(),
                                chapter: entry.last_chapter,
                                block: entry.last_block,
                                char_offset: entry.last_char_offset,
                                chapter_title: entry.last_chapter_title.clone(),
                                timestamp: if (
                                    entry.last_chapter,
                                    entry.last_block,
                                    entry.last_char_offset,
                                ) == (0, 0, 0)
                                {
                                    0
                                } else {
                                    entry.last_opened
                                },
                            });
                    }
                    books.push(LocalBook {
                        info: CloudBook {
                            hash,
                            title: entry.title,
                            size: metadata.len(),
                        },
                        path: PathBuf::from(entry.path),
                    });
                }
                let device_id = own.device_id.clone();
                github_sync::sync_phase(
                    token,
                    owner.clone(),
                    own,
                    books,
                    cancel.clone(),
                    count_contributions,
                    phase,
                    &data_dir,
                    retry_failed,
                )
                .and_then(|mut result| {
                    let mut validated = Vec::new();
                    for book in std::mem::take(&mut result.downloaded) {
                        if cancel.load(Ordering::SeqCst) {
                            return Err("Sync cancelled".into());
                        }
                        let checked = (|| {
                            let temporary = tempfile::NamedTempFile::new()
                                .map_err(|_| "Cannot stage downloaded book")?;
                            let path = if let Some(path) = &book.path {
                                path.as_path()
                            } else {
                                book.save_to(temporary.path())?;
                                temporary.path()
                            };
                            reader_core::epub::EpubBook::open_lazy(path)
                                .map_err(|_| "Downloaded file is not a valid EPUB")?;
                            Ok::<_, String>(())
                        })();
                        match checked {
                            Ok(()) => validated.push(book),
                            Err(error) => {
                                result.failed = github_sync::record_transfer_failure(
                                    &data_dir, &owner, &device_id, "download", &book.info, error,
                                )?;
                                result.next_retry_after = Some(30);
                            }
                        }
                    }
                    result.downloaded = validated;
                    Ok((result, skipped))
                })
            })();
            let _ = tx.send(result);
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        });
    }
    pub fn cloud_poll(&mut self, ctx: &egui::Context) {
        if !self.github_sync_enabled
            || self.cloud_sync.account.is_some() && self.cloud_sync.account != self.github_username
        {
            self.cloud_sync.stop();
            return;
        }
        let result = self
            .cloud_sync
            .pending
            .as_ref()
            .and_then(|rx| rx.try_recv().ok());
        if let Some(result) = result {
            let progress_phase = self.cloud_sync.phase == Some(SyncPhase::Progress);
            self.cloud_sync.pending = None;
            self.cloud_sync.last_attempt = Some(Instant::now());
            match result {
                Err(error) => {
                    self.cloud_sync.throttled = error.starts_with("RETRY_AFTER:");
                    self.cloud_sync.retry_delay = error
                        .strip_prefix("RETRY_AFTER:")
                        .and_then(|s| s.split(':').next())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(300);
                    self.cloud_sync.needs_auth =
                        error.starts_with("REAUTHORIZE:") || error.contains("login expired");
                    if self.cloud_sync.needs_auth {
                        self.github_repo_authorized = false;
                    }
                    let display_error = error
                        .strip_prefix("RETRY_AFTER:")
                        .and_then(|s| s.split_once(':').map(|(_, message)| message))
                        .unwrap_or(&error);
                    self.cloud_sync.status = if self.cloud_sync.needs_auth {
                        self.i18n.t("cloud.need_auth").into()
                    } else {
                        format!("{}: {display_error}", self.i18n.t("cloud.failed"))
                    };
                }
                Ok((mut result, skipped)) => {
                    self.cloud_sync.retry_delay = result.next_retry_after.unwrap_or(300);
                    self.cloud_sync.throttled = false;
                    self.cloud_sync.needs_auth = false;
                    let journal = self.cloud_sync.journal.as_mut().unwrap();
                    // Reading may have continued while network work was in flight.
                    let _ = journal.own.merge_own(&result.own);
                    if let Err(error) = reader_core::reading_activity::import(
                        &self.data_dir,
                        &journal.own.device_id,
                        &result.snapshots,
                    ) {
                        self.error_msg = Some(error);
                    }
                    journal.seen.extend(result.own.progress.keys().cloned());
                    let latest_records = reader_core::cloud_progress::latest_records(
                        &result.snapshots,
                        &journal.own,
                    );
                    let latest = latest_records
                        .iter()
                        .map(|(h, r)| (h.clone(), r.progress.clone()))
                        .collect::<std::collections::BTreeMap<_, _>>();
                    self.cloud_sync.offered = None;
                    if let Err(error) = reader_core::library::archive::ArchiveStore::apply_verified(
                        &self.data_dir,
                        &mut self.library,
                        &result.snapshots,
                        &journal.own,
                        &result.confirmed_archives,
                    ) {
                        self.error_msg = Some(error);
                    }
                    let mut imported = 0;
                    for book in result.downloaded {
                        if reader_core::library::archive::ArchiveStore::load(&self.data_dir)
                            .map(|a| a.hidden(&book.info.hash))
                            .unwrap_or(true)
                        {
                            continue;
                        }

                        // Recheck the live library: an import may have happened during the request.
                        if self.library.books.iter().any(|e| {
                            reader_core::epub::EpubBook::file_hash(&e.path)
                                .ok()
                                .as_ref()
                                == Some(&book.info.hash)
                        }) {
                            match github_sync::complete_transfer(
                                &self.data_dir,
                                self.github_username.as_deref().unwrap_or(""),
                                &journal.own.device_id,
                                "download",
                                &book.info.hash,
                            ) {
                                Ok(failed) => result.failed = failed,
                                Err(error) => result.terminal_error = Some(error),
                            }
                            continue;
                        }
                        let import = (|| {
                            let checked = tempfile::NamedTempFile::new()
                                .map_err(|_| "Cannot stage downloaded book")?;
                            book.save_to(checked.path())
                                .map_err(|_| "Cannot stage downloaded book")?;
                            // Worker already validated EPUB structure off the UI thread.
                            let p = latest.get(&book.info.hash);
                            let previous = self.library.clone();
                            let entry = if let Some(restored) =
                                reader_core::library::archive::restore_download_file(
                                    &self.data_dir,
                                    &mut self.library,
                                    &book.info.hash,
                                    checked.path(),
                                )
                                .map_err(|_| "Cannot restore archived book")?
                            {
                                restored
                            } else {
                                self.library.add_or_update(
                                    &self.data_dir,
                                    book.info.title.clone(),
                                    checked.path().to_string_lossy().into_owned(),
                                    p.map_or(0, |p| p.chapter),
                                    p.and_then(|p| p.chapter_title.clone()),
                                )
                            };
                            if !std::path::Path::new(&entry.path)
                                .starts_with(std::path::Path::new(&self.data_dir).join("books"))
                                || reader_core::epub::EpubBook::file_hash(&entry.path)
                                    .ok()
                                    .as_ref()
                                    != Some(&book.info.hash)
                            {
                                self.library = previous;
                                self.library.save_to(&self.data_dir);
                                return Err("Cannot save downloaded book");
                            }
                            Ok(())
                        })();
                        match import {
                            Ok(()) => {
                                match github_sync::complete_transfer(
                                    &self.data_dir,
                                    self.github_username.as_deref().unwrap_or(""),
                                    &journal.own.device_id,
                                    "download",
                                    &book.info.hash,
                                ) {
                                    Ok(failed) => result.failed = failed,
                                    Err(error) => result.terminal_error = Some(error),
                                }
                                journal.seen.insert(book.info.hash);
                                imported += 1;
                            }
                            Err(error) => {
                                match github_sync::record_transfer_failure(
                                    &self.data_dir,
                                    self.github_username.as_deref().unwrap_or(""),
                                    &journal.own.device_id,
                                    "download",
                                    &book.info,
                                    error.into(),
                                ) {
                                    Ok(failed) => result.failed = failed,
                                    Err(error) => result.terminal_error = Some(error),
                                }
                                result.next_retry_after = Some(30);
                            }
                        }
                    }
                    let mut paths_by_hash =
                        std::collections::BTreeMap::<String, Vec<String>>::new();
                    for entry in &self.library.books {
                        if let Some(hash) = reader_core::library::Library::read_book_config(
                            &self.data_dir,
                            &entry.id,
                        )
                        .and_then(|c| c.file_hash)
                        .or_else(|| reader_core::epub::EpubBook::file_hash(&entry.path).ok())
                        {
                            paths_by_hash
                                .entry(hash)
                                .or_default()
                                .push(entry.path.clone());
                        }
                    }
                    for (hash, record) in latest_records {
                        let p = &record.progress;
                        let paths = paths_by_hash.get(&hash).cloned().unwrap_or_default();
                        let mut accepted = false;
                        for path in &paths {
                            let changed = self
                                .library
                                .books
                                .iter()
                                .find(|e| &e.path == path)
                                .is_some_and(|e| {
                                    (e.last_chapter, e.last_block, e.last_char_offset)
                                        != (p.chapter, p.block, p.char_offset)
                                });
                            if changed
                                && !reader_core::epub::EpubBook::open_lazy(path).is_ok_and(|book| {
                                    reader_core::cloud_progress::validate_position(&book, p)
                                })
                            {
                                continue;
                            }
                            accepted = true;
                            if self.current_book_hash.as_ref() == Some(&hash)
                                && self.book.is_some()
                                && changed
                            {
                                self.pending_sync_updates
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .push(p.clone());
                            }
                            if changed {
                                self.library.update_position(
                                    &self.data_dir,
                                    path,
                                    p.chapter,
                                    p.chapter_title.clone(),
                                    p.block,
                                    p.char_offset,
                                );
                            }
                        }
                        if accepted {
                            journal.accept(&record);
                            journal.applied.insert(hash, p.timestamp);
                        }
                    }
                    if !progress_phase {
                        self.cloud_sync.completed_at = Some(reader_core::now_secs());
                    }
                    self.github_repo_authorized = true;
                    self.cloud_sync.status = format!(
                        "{} (↑ {}, ↓ {}, {}: {})",
                        self.i18n.t("cloud.done"),
                        result.uploaded,
                        imported,
                        self.i18n.t("cloud.skipped"),
                        skipped
                    );
                    self.cloud_sync.retry_delay = result.next_retry_after.unwrap_or(300);
                    self.cloud_sync.failed = std::mem::take(&mut result.failed);
                    if !self.cloud_sync.failed.is_empty() {
                        self.cloud_sync.status.push_str(&format!(
                            " · {}",
                            self.i18n.tf1(
                                "cloud.files_failed",
                                &self.cloud_sync.failed.len().to_string()
                            )
                        ));
                    }
                    if let Some(error) = result.terminal_error {
                        self.cloud_sync.throttled = error.starts_with("RETRY_AFTER:");
                        self.cloud_sync.retry_delay = error
                            .strip_prefix("RETRY_AFTER:")
                            .and_then(|e| e.split(':').next())
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(300);
                        self.cloud_sync.needs_auth =
                            error.starts_with("REAUTHORIZE:") || error.contains("login expired");
                        self.cloud_sync.status =
                            format!("{}: {error}", self.i18n.t("cloud.failed"));
                    }
                    self.cloud_save_journal();
                    if self.book_path.as_ref().is_some_and(|path| {
                        !self.library.books.iter().any(|entry| &entry.path == path)
                    }) {
                        self.close_book_presentation();
                        self.book_path = None;
                        self.current_book_hash = None;
                        self.book = None;
                    }
                    if progress_phase {
                        self.cloud_start_phase(SyncPhase::Books, self.cloud_sync.retry_failed);
                    }
                }
            }
        }
        if self.cloud_sync.pending.is_none()
            && !self.cloud_sync.needs_auth
            && self.cloud_sync.last_attempt.is_none_or(|time| {
                time.elapsed() >= Duration::from_secs(self.cloud_sync.retry_delay.max(1))
                    || (!self.cloud_sync.throttled
                        && self
                            .cloud_sync
                            .dirty_at
                            .is_some_and(|t| t.elapsed() >= Duration::from_secs(2))
                        && time.elapsed() >= Duration::from_secs(5))
            })
        {
            self.cloud_start_sync();
        }
        if self.cloud_sync.pending.is_some() {
            ctx.request_repaint_after(Duration::from_secs(1));
        } else {
            ctx.request_repaint_after(Duration::from_secs(if self.cloud_sync.dirty_at.is_some() {
                1
            } else {
                30
            }));
        }
    }
    pub fn render_sync_meter(&self, ui: &mut egui::Ui) {
        if !self.cloud_sync.is_syncing() {
            return;
        }
        let meter = github_sync::sync_meter(&self.data_dir);
        let key = match meter.phase.as_str() {
            "progress" => "cloud.phase_progress",
            "uploading" => "cloud.phase_uploading",
            "downloading" => "cloud.phase_downloading",
            "deleting" => "cloud.phase_deleting",
            "saving" => "cloud.phase_saving",
            _ => "cloud.phase_preparing",
        };
        ui.small(self.i18n.t(key));
        if meter.total > 0 {
            let text = self.i18n.tf(
                "cloud.processed",
                &[
                    &if meter.bytes {
                        format!("{:.1} MiB", meter.completed as f64 / 1048576.0)
                    } else {
                        meter.completed.to_string()
                    },
                    &if meter.bytes {
                        format!("{:.1} MiB", meter.total as f64 / 1048576.0)
                    } else {
                        meter.total.to_string()
                    },
                ],
            );
            ui.add(
                egui::ProgressBar::new(meter.completed as f32 / meter.total as f32)
                    .desired_width(190.)
                    .text(text),
            );
        }
        if !meter.title.is_empty() {
            ui.small(&meter.title);
        }
    }
    pub fn render_github_sync_controls(&mut self, ui: &mut egui::Ui) {
        self.render_sync_meter(ui);
        let enabled_before = self.github_sync_enabled;
        let authorized = self.github_token.is_some()
            && self.github_repo_authorized
            && !self.cloud_sync.needs_auth;
        ui.horizontal(|ui| {
            ui.label(self.i18n.t("cloud.enable"));
            ui.allocate_ui_with_layout(
                egui::vec2(
                    ui.available_width(),
                    super::design::Geometry::CONTROL_HEIGHT,
                ),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    super::design::sync_toggle(
                        ui,
                        &mut self.github_sync_enabled,
                        authorized,
                        self.i18n.t("cloud.enable"),
                    );
                },
            );
        });
        if self.github_sync_enabled != enabled_before {
            if self.github_sync_enabled {
                self.github_prepare_sync();
            } else {
                self.cloud_set_enabled(false);
            }
        }
        ui.horizontal(|ui| {
            ui.label(self.i18n.t("sync.contributions"));
            ui.allocate_ui_with_layout(
                egui::vec2(
                    ui.available_width(),
                    super::design::Geometry::CONTROL_HEIGHT,
                ),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    super::design::sync_toggle(
                        ui,
                        &mut self.github_sync_contributions,
                        self.cloud_sync.pending.is_none(),
                        self.i18n.t("sync.contributions"),
                    );
                },
            );
        });
        ui.label(
            egui::RichText::new(self.i18n.t("sync.contributions_note"))
                .small()
                .color(super::design::Palette::new(self.dark_mode).muted),
        );
        ui.collapsing(self.i18n.t("sync.contributions_details"), |ui| {
            ui.small(self.i18n.t("sync.contributions_detail"));
        });
        ui.allocate_ui_with_layout(
            egui::vec2(
                ui.available_width(),
                super::design::Geometry::CONTROL_HEIGHT,
            ),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                if ui
                    .add_enabled(
                        self.github_sync_enabled && self.cloud_sync.pending.is_none(),
                        egui::Button::new(self.i18n.t("cloud.now")),
                    )
                    .clicked()
                {
                    self.cloud_start_sync();
                }
                if self.cloud_sync.pending.is_some() {
                    ui.spinner();
                }
            },
        );
        if self.cloud_sync.needs_auth {
            ui.small(self.i18n.t("cloud.need_auth"));
        }
        if self.github_sync_enabled {
            if !self.cloud_sync.status.is_empty() {
                ui.label(self.i18n.message(&self.cloud_sync.status));
            }
            if !self.cloud_sync.failed.is_empty() {
                ui.collapsing(
                    self.i18n.tf1(
                        "cloud.files_failed",
                        &self.cloud_sync.failed.len().to_string(),
                    ),
                    |ui| {
                        for failure in &self.cloud_sync.failed {
                            ui.label(&failure.title);
                            ui.small(&failure.error);
                        }
                    },
                );
                if ui
                    .add_enabled(
                        self.cloud_sync.pending.is_none() && !self.cloud_sync.throttled,
                        egui::Button::new(self.i18n.t("cloud.retry_files")),
                    )
                    .clicked()
                {
                    self.cloud_start_phase(SyncPhase::Progress, true);
                }
            }
            let offer = self
                .cloud_sync
                .offered
                .clone()
                .filter(|p| self.current_book_hash.as_ref() == Some(&p.book_hash));
            if let Some(p) = offer {
                ui.label(format!("{} · {} / {}", p.title, p.chapter + 1, p.block + 1));
                if ui.button(self.i18n.t("cloud.apply")).clicked() {
                    // Never navigate beyond the parsed chapter list.
                    if self
                        .book
                        .as_ref()
                        .is_some_and(|b| p.chapter < b.chapters.len())
                    {
                        self.pending_sync_updates
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .push(p.clone());

                        self.cloud_sync.offered = None;
                    }
                }
            }
        }
        if let Some(time) = self.cloud_sync.completed_at {
            ui.small(self.i18n.tf1(
                "cloud.last_sync",
                &((reader_core::now_secs().saturating_sub(time)) / 60).to_string(),
            ));
            if let Some(owner) = &self.github_username {
                ui.hyperlink_to(
                    self.i18n.t("cloud.repository"),
                    format!("https://github.com/{owner}/{}", github_sync::REPOSITORY),
                );
            }
        }
        if self.cloud_sync.completed_at.is_none() {
            super::github_account::sync_value_row(
                ui,
                self.i18n.t("sync.last_sync"),
                self.i18n.t("sync.never_synced"),
            );
        }
    }
}
