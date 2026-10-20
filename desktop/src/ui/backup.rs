//! Native file dialogs and background package work; no book bytes enter UI messages.
use crate::app::{AppView, ReaderApp};
use eframe::egui;
use reader_core::backup::BackupSummary;
use std::{path::PathBuf, sync::mpsc, time::Duration};

fn readable_bytes(bytes: u64) -> String {
    if bytes < 1000 {
        format!("{bytes} B")
    } else if bytes < 1_000_000 {
        format!("{:.1} KB", bytes as f64 / 1000.)
    } else if bytes < 1_000_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.)
    } else {
        format!("{:.1} GB", bytes as f64 / 1_000_000_000.)
    }
}

#[derive(Debug)]
struct Completed {
    action: &'static str,
    path: PathBuf,
    summary: BackupSummary,
}
#[derive(Default)]
pub(crate) struct BackupState {
    pub show: bool,
    pending: Option<mpsc::Receiver<Result<Option<Completed>, String>>>,
    selected: Option<(PathBuf, BackupSummary)>,
    exported: Option<PathBuf>,
    message: Option<&'static str>,
    error: Option<String>,
    operation: Option<&'static str>,
}

/// Describe only errors whose cause is actually present in the core/OS result.
pub(crate) fn failure_hint(error: &str) -> &'static str {
    let error = error.to_ascii_lowercase();
    if error.contains("backup changed after verification") {
        "backup.error_changed"
    } else if error.contains("unsupported library backup version") {
        "backup.error_version"
    } else if error.contains("no space left") || error.contains("os error 28") {
        "backup.error_space"
    } else if error.contains("permission denied") || error.contains("access is denied") {
        "backup.error_permission"
    } else {
        "backup.error_hint"
    }
}

fn task_row(
    ui: &mut egui::Ui,
    icon: super::design::Icon,
    title: &str,
    subtitle: &str,
) -> egui::Response {
    let p = super::design::Palette::new(ui.visuals().dark_mode);
    let width = ui.available_width();
    let body = ui.style().text_styles[&egui::TextStyle::Body].clone();
    let small = ui.style().text_styles[&egui::TextStyle::Small].clone();
    let title = ui.fonts(|f| f.layout(title.to_owned(), body, p.text, (width - 80.).max(80.)));
    let subtitle =
        ui.fonts(|f| f.layout(subtitle.to_owned(), small, p.muted, (width - 80.).max(80.)));
    let height = (title.size().y + subtitle.size().y + 30.).max(76.);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), title.text())
    });
    let opacity = if ui.is_enabled() { 1. } else { 0.45 };
    ui.painter().rect_filled(
        rect,
        12.,
        if response.hovered() {
            p.raised
        } else {
            p.surface
        },
    );
    super::design::paint_icon(
        ui.painter(),
        egui::Rect::from_center_size(
            rect.left_center() + egui::vec2(26., 0.),
            egui::vec2(24., 24.),
        ),
        icon,
        p.accent.gamma_multiply(opacity),
    );
    let origin = rect.min + egui::vec2(52., 14.);
    ui.painter()
        .galley(origin, title.clone(), p.text.gamma_multiply(opacity));
    ui.painter().galley(
        origin + egui::vec2(0., title.size().y + 4.),
        subtitle,
        p.muted.gamma_multiply(opacity),
    );
    let c = rect.right_center() - egui::vec2(17., 0.);
    ui.painter().add(egui::Shape::line(
        vec![
            c + egui::vec2(-3., -5.),
            c + egui::vec2(2., 0.),
            c + egui::vec2(-3., 5.),
        ],
        egui::Stroke::new(1.4_f32, p.muted),
    ));
    response
}
impl BackupState {
    pub(crate) fn busy(&self) -> bool {
        self.pending.is_some()
    }
}
impl ReaderApp {
    fn start_backup_task(&mut self, ctx: &egui::Context, action: &'static str) {
        if self.backup.busy()
            || self.view == AppView::Reader
            || self.pending_book_open.is_some()
            || self.cloud_sync.is_syncing()
        {
            return;
        }
        self.cloud_sync.stop();
        if let Err(error) = self.quiesce_activity_for_backup() {
            self.backup.error = Some(error);
            return;
        }
        if let Err(error) = self.flush_reading_position_for_backup() {
            self.backup.error = Some(error);
            return;
        }
        let selected = self.backup.selected.clone();
        let (tx, rx) = mpsc::channel();
        self.backup.pending = Some(rx);
        self.backup.operation = Some(action);
        self.backup.error = None;
        self.backup.message = None;
        if action == "inspect" {
            self.backup.selected = None;
        }
        let root = self.data_dir.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<Option<Completed>, String> {
                let (path, expected) = match action {
                    "export" => match rfd::FileDialog::new()
                        .add_filter("RustEpubReader backup", &["readerbackup"])
                        .set_file_name("RustEpubReader.readerbackup")
                        .save_file()
                    {
                        Some(path) => (path, None),
                        None => return Ok(None),
                    },
                    "inspect" => match rfd::FileDialog::new()
                        .add_filter("RustEpubReader backup", &["readerbackup", "zip"])
                        .pick_file()
                    {
                        Some(path) => (path, None),
                        None => return Ok(None),
                    },
                    "restore" => {
                        let (path, summary) = selected.ok_or("Choose and verify a backup first")?;
                        (path, Some(summary.manifest_sha256))
                    }
                    _ => return Err("Unknown backup operation".into()),
                };
                let request =
                    serde_json::json!({"action":action,"path":path,"expected_manifest":expected});
                let summary =
                    serde_json::from_value(reader_core::backup::command(&root, &request)?)
                        .map_err(|_| "Invalid backup response")?;
                Ok(Some(Completed {
                    action,
                    path,
                    summary,
                }))
            })();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    pub(crate) fn poll_backup(&mut self, ctx: &egui::Context) {
        let result = match self.backup.pending.as_ref().map(|rx| rx.try_recv()) {
            Some(Ok(result)) => Some(result),
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                Some(Err("Backup worker ended without a result".into()))
            }
            _ => None,
        };
        if let Some(result) = result {
            self.backup.pending = None;
            match result {
                Ok(Some(completed)) => match completed.action {
                    "export" => {
                        self.backup.exported = Some(completed.path);
                        self.backup.message = Some("backup.saved");
                    }
                    "inspect" => self.backup.selected = Some((completed.path, completed.summary)),
                    _ => {
                        self.backup.selected = None;
                        self.backup.message = Some("backup.done");
                        self.book = None;
                        self.book_config = None;
                        self.book_path = None;
                        self.current_book_hash = None;
                        self.position_save_due = None;
                        self.cloud_invalidate_journal();
                        self.reader_navigation = Default::default();
                        match reader_core::library::Library::load_from(&self.data_dir) {
                            Ok(library) => {
                                self.library = library;
                                self.library_selected.clear();
                            }
                            Err(error) => self.backup.error = Some(error),
                        }
                    }
                },
                Ok(None) => {}
                Err(error) => self.backup.error = Some(error),
            }
        }
        if self.backup.busy() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
    pub(crate) fn render_backup(&mut self, ctx: &egui::Context) {
        if !self.backup.show {
            return;
        }
        let busy = self.backup.busy();
        let available = !busy
            && self.view != AppView::Reader
            && self.pending_book_open.is_none()
            && !self.cloud_sync.is_syncing();
        let p = super::design::Palette::new(self.dark_mode);
        let mut action = None;
        let mut close = false;
        let mut home = false;
        let confirming = !busy && self.backup.error.is_none() && self.backup.selected.is_some();
        let result = !busy && self.backup.error.is_none() && self.backup.message.is_some();
        let modal = egui::Modal::new(egui::Id::new("library_backup"))
            .frame(
                egui::Frame::default()
                    .fill(p.surface)
                    .stroke(egui::Stroke::new(1_f32, p.border))
                    .corner_radius(16.)
                    .inner_margin(24.),
            )
            .show(ctx, |ui| {
                ui.set_width((ctx.screen_rect().width() - 96.).clamp(240., 560.));
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(self.i18n.t(if confirming {
                            "backup.confirm_restore"
                        } else {
                            "backup.title"
                        }))
                        .size(20.)
                        .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        close = ui
                            .add_enabled_ui(!busy, |ui| {
                                super::design::close_button(ui, self.i18n.t("dialog.close"))
                            })
                            .inner
                            .clicked();
                    });
                });
                ui.add_space(16.);
                egui::ScrollArea::vertical()
                    .id_salt("backup_body")
                    .max_height((ctx.screen_rect().height() - 250.).max(96.))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if busy {
                            let value = reader_core::backup::command(
                                &self.data_dir,
                                &serde_json::json!({"action":"status"}),
                            )
                            .unwrap_or_default();
                            let phase = value["phase"].as_str().unwrap_or("preparing");
                            let key = match phase {
                                "collecting" => "backup.collecting",
                                "packing" => "backup.packing",
                                "verifying" | "inspect" => "backup.verifying",
                                "merging" | "restore" => "backup.merging",
                                "publishing" => "backup.publishing",
                                _ => "backup.preparing",
                            };
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.strong(self.i18n.t(key));
                            });
                            ui.add_space(12.);
                            if let (Some(done), Some(total)) = (
                                value["completed"].as_u64(),
                                value["total"].as_u64().filter(|v| *v > 0),
                            ) {
                                ui.add(
                                    egui::ProgressBar::new(
                                        (done as f32 / total as f32).clamp(0., 1.),
                                    )
                                    .text(self.i18n.tf2(
                                        "cloud.processed",
                                        &done.to_string(),
                                        &total.to_string(),
                                    )),
                                );
                            }
                        } else if let Some(error) = &self.backup.error {
                            let title = match self.backup.operation {
                                Some("export") => "backup.error_create",
                                Some("restore") => "backup.error_restore",
                                _ => "backup.error_inspect",
                            };
                            ui.colored_label(
                                ui.visuals().error_fg_color,
                                egui::RichText::new(self.i18n.t(title)).strong(),
                            );
                            ui.add_space(8.);
                            ui.label(self.i18n.t(failure_hint(error)));
                            ui.add_space(8.);
                            ui.collapsing(self.i18n.t("backup.details"), |ui| {
                                ui.add(egui::Label::new(error).selectable(true).wrap());
                            });
                        } else if let Some((path, summary)) = &self.backup.selected {
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(
                                            path.file_name().unwrap_or_default().to_string_lossy(),
                                        )
                                        .strong(),
                                    )
                                    .wrap(),
                                );
                            });
                            ui.add_space(8.);
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("✓").color(p.accent));
                                ui.label(
                                    egui::RichText::new(self.i18n.t("backup.verified"))
                                        .color(p.text),
                                );
                            });
                            ui.add_space(10.);
                            ui.horizontal_wrapped(|ui| {
                                ui.label(self.i18n.tf1("backup.books", &summary.books.to_string()));
                                ui.label("·");
                                ui.label(
                                    self.i18n
                                        .tf1("backup.archived", &summary.archived.to_string()),
                                );
                                ui.label("·");
                                ui.label(readable_bytes(summary.file_bytes));
                            });
                            ui.add_space(12.);
                            ui.label(self.i18n.t("backup.rules"));
                            ui.add_space(8.);
                            if ui
                                .add_enabled(
                                    available,
                                    egui::Button::new(
                                        egui::RichText::new(self.i18n.t("backup.change_file"))
                                            .color(p.accent),
                                    )
                                    .frame(false),
                                )
                                .clicked()
                            {
                                action = Some("inspect");
                            }
                        } else if let Some(key) = self.backup.message {
                            ui.label(
                                egui::RichText::new(self.i18n.t(key))
                                    .strong()
                                    .color(p.accent),
                            );
                            if let Some(path) = self
                                .backup
                                .exported
                                .as_ref()
                                .filter(|_| key == "backup.saved")
                            {
                                ui.add_space(10.);
                                ui.add(
                                    egui::Label::new(
                                        path.file_name().unwrap_or_default().to_string_lossy(),
                                    )
                                    .selectable(true)
                                    .wrap(),
                                )
                                .on_hover_text(path.display().to_string());
                            }
                        } else {
                            ui.add_enabled_ui(available, |ui| {
                                if task_row(
                                    ui,
                                    super::design::Icon::Export,
                                    self.i18n.t("backup.create"),
                                    self.i18n.t("backup.create_hint"),
                                )
                                .clicked()
                                {
                                    action = Some("export");
                                }
                                ui.add_space(8.);
                                if task_row(
                                    ui,
                                    super::design::Icon::Import,
                                    self.i18n.t("backup.restore_title"),
                                    self.i18n.t("backup.restore_hint"),
                                )
                                .clicked()
                                {
                                    action = Some("inspect");
                                }
                            });
                            ui.add_space(16.);
                            ui.small(self.i18n.t("backup.privacy_short"));
                            ui.add_space(6.);
                            ui.collapsing(self.i18n.t("backup.contents"), |ui| {
                                ui.label(self.i18n.t("backup.scope"));
                                ui.add_space(6.);
                                ui.label(self.i18n.t("backup.excludes"));
                            });
                        }
                        if !available && !busy {
                            ui.add_space(8.);
                            ui.label(self.i18n.t("backup.wait"));
                        }
                    });
                if confirming || result || self.backup.error.is_some() {
                    ui.add_space(16.);
                    ui.separator();
                    ui.add_space(10.);
                    ui.horizontal(|ui| {
                        home = ui
                            .add_enabled(
                                !busy,
                                egui::Button::new(self.i18n.t("backup.back")).frame(false),
                            )
                            .clicked();
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let key = if confirming {
                                "backup.merge"
                            } else if self.backup.error.is_some() {
                                "backup.retry"
                            } else {
                                "manage.done"
                            };
                            if ui
                                .add_enabled(
                                    available,
                                    super::design::primary(self.i18n.t(key))
                                        .min_size(egui::vec2(140., 40.)),
                                )
                                .clicked()
                            {
                                if confirming {
                                    action = Some("restore");
                                } else if self.backup.error.is_some() {
                                    action = Some(self.backup.operation.unwrap_or("inspect"));
                                } else {
                                    close = true;
                                }
                            }
                        });
                    });
                }
            });
        if (modal.should_close() || close) && !busy {
            self.backup.show = false;
        }
        if home {
            self.backup.selected = None;
            self.backup.message = None;
            self.backup.error = None;
        }
        if let Some(action) = action {
            self.start_backup_task(ctx, action);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn error_hints_do_not_infer_causes_for_unknown_errors() {
        assert_eq!(
            failure_hint("Unsupported library backup version"),
            "backup.error_version"
        );
        assert_eq!(
            failure_hint("Backup changed after verification; select and verify it again"),
            "backup.error_changed"
        );
        assert_eq!(
            failure_hint("No space left on device (os error 28)"),
            "backup.error_space"
        );
        assert_eq!(
            failure_hint("Permission denied (os error 13)"),
            "backup.error_permission"
        );
        assert_eq!(
            failure_hint("Cannot read library backup: invalid Zip archive"),
            "backup.error_hint"
        );
    }
    #[test]
    fn successful_restore_releases_cached_reader_and_pending_position_before_reloading() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().to_str().unwrap();
        let mut library = reader_core::library::Library::load_from(directory).unwrap();
        let entry = library
            .add_or_update_from_bytes(
                directory,
                "Cached".into(),
                include_bytes!("../../../ios/Tests/fixtures/native-reading-modes.epub"),
                0,
                None,
            )
            .unwrap();
        let mut app = ReaderApp::from_data_dir(directory.into());
        app.book = Some(
            reader_core::epub::EpubBook::open_cached(&entry.path, &root.path().join("index"))
                .unwrap(),
        );
        app.book_path = Some(entry.path.clone());
        app.current_book_hash = Some("old cached hash".into());
        app.book_config =
            reader_core::library::Library::load_book_config(directory, &entry.id).unwrap();
        app.position_save_due = Some(std::time::Instant::now());
        let (tx, rx) = mpsc::channel();
        app.backup.pending = Some(rx);
        tx.send(Ok(Some(Completed {
            action: "restore",
            path: PathBuf::new(),
            summary: BackupSummary {
                manifest_sha256: "0".repeat(64),
                schema_version: 1,
                books: 1,
                archived: 0,
                files: 1,
                file_bytes: 1,
                created_at: 1,
            },
        })))
        .unwrap();
        app.poll_backup(&egui::Context::default());
        assert!(app.book.is_none() && app.book_path.is_none() && app.book_config.is_none());
        assert!(app.current_book_hash.is_none() && app.position_save_due.is_none());
        assert_eq!(app.library.books.len(), 1);
        assert!(app.backup.error.is_none());
    }
}
