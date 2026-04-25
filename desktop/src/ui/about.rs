//! About dialog and application information UI.
use crate::app::{ReaderApp, UpdateState};
use crate::self_update;
use eframe::egui;
use std::sync::{Arc, Mutex};

const GITHUB_URL: &str = "https://github.com/zhongbai2333/RustEpubReader";
const ICON_BYTES: &[u8] = include_bytes!("../../../icon/ReaderIcon2.png");

impl ReaderApp {
    pub fn render_about(&mut self, ctx: &egui::Context) {
        // 惰性加载 logo 纹理
        if self.about_icon_texture.is_none() {
            if let Ok(img) = image::load_from_memory(ICON_BYTES) {
                let rgba = img.into_rgba8();
                let (w, h) = rgba.dimensions();
                let ci = egui::ColorImage::from_rgba_unmultiplied(
                    [w as usize, h as usize],
                    rgba.as_raw(),
                );
                self.about_icon_texture =
                    Some(ctx.load_texture("about_app_icon", ci, egui::TextureOptions::LINEAR));
            }
        }

        let mut open = self.show_about;
        egui::Window::new(self.i18n.t("about.title"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .min_width(380.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_min_width(360.0);
                ui.add_space(12.0);

                // ── App logo + title block ──
                ui.vertical_centered(|ui| {
                    if let Some(tex) = &self.about_icon_texture {
                        ui.add(
                            egui::Image::new(tex)
                                .fit_to_exact_size(egui::Vec2::splat(96.0))
                                .corner_radius(18.0),
                        );
                        ui.add_space(10.0);
                    }
                    ui.label(
                        egui::RichText::new(self.i18n.t("about.app_name"))
                            .size(22.0)
                            .strong(),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                            .size(13.0)
                            .color(egui::Color32::GRAY),
                    );
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(self.i18n.t("about.author_line"))
                            .size(12.0)
                            .color(egui::Color32::GRAY),
                    );
                });

                ui.add_space(14.0);
                ui.separator();
                ui.add_space(10.0);

                // ── GitHub button ──
                ui.vertical_centered(|ui| {
                    if ui
                        .button(egui::RichText::new(self.i18n.t("about.github_repo")).size(13.0))
                        .clicked()
                    {
                        let _ = open_url(GITHUB_URL);
                    }
                });

                ui.add_space(10.0);

                // ── Check for updates ──
                ui.vertical_centered(|ui| match &self.update_state {
                    UpdateState::Idle => {
                        if ui
                            .button(egui::RichText::new(self.i18n.t("update.check")).size(13.0))
                            .clicked()
                        {
                            self.update_state = UpdateState::Checking;
                            let ctx = ui.ctx().clone();
                            let state_slot: Arc<Mutex<Option<UpdateState>>> =
                                Arc::new(Mutex::new(None));
                            let slot = state_slot.clone();
                            std::thread::spawn(move || {
                                let result = match self_update::check_latest_version() {
                                    Some((tag, _name)) => UpdateState::Available(tag),
                                    None => UpdateState::UpToDate,
                                };
                                if let Ok(mut s) = slot.lock() {
                                    *s = Some(result);
                                }
                                ctx.request_repaint();
                            });
                            self._update_check_slot = Some(state_slot);
                        }
                    }
                    UpdateState::Checking => {
                        let mut resolved = None;
                        if let Some(ref slot) = self._update_check_slot {
                            if let Ok(s) = slot.lock() {
                                resolved = s.clone();
                            }
                        }
                        if let Some(state) = resolved {
                            self.update_state = state;
                            self._update_check_slot = None;
                        } else {
                            ui.spinner();
                            ui.label(
                                egui::RichText::new(self.i18n.t("update.checking")).size(12.0),
                            );
                            ui.ctx().request_repaint();
                        }
                    }
                    UpdateState::Available(tag) => {
                        let tag = tag.clone();
                        ui.label(
                            egui::RichText::new(self.i18n.tf1("update.new_version", &tag))
                                .size(13.0)
                                .color(egui::Color32::from_rgb(80, 200, 120)),
                        );
                        ui.add_space(4.0);
                        if ui
                            .button(
                                egui::RichText::new(self.i18n.t("update.download_update"))
                                    .size(13.0),
                            )
                            .clicked()
                        {
                            self.update_state = UpdateState::Downloading;
                            let ctx = ui.ctx().clone();
                            let done_slot: Arc<Mutex<Option<UpdateState>>> =
                                Arc::new(Mutex::new(None));
                            let slot = done_slot.clone();
                            let progress_for_ui: Arc<Mutex<f32>> = Arc::new(Mutex::new(0.0));
                            let progress_writer = progress_for_ui.clone();
                            std::thread::spawn(move || {
                                let cb_ctx = ctx.clone();
                                let pw = progress_writer;
                                let result = self_update::perform_update(Some(Box::new(
                                    move |downloaded, total| {
                                        if total > 0 {
                                            let pct = downloaded as f32 / total as f32;
                                            if let Ok(mut p) = pw.lock() {
                                                *p = pct;
                                            }
                                            cb_ctx.request_repaint();
                                        }
                                    },
                                )));
                                let state = match result {
                                    Ok(self_update::UpdateOutcome::UpdateLaunched) => {
                                        UpdateState::Restarting
                                    }
                                    Ok(_) => UpdateState::UpToDate,
                                    Err(e) => UpdateState::Failed(e.to_string()),
                                };
                                if let Ok(mut s) = slot.lock() {
                                    *s = Some(state);
                                }
                                ctx.request_repaint();
                            });
                            self._update_download_slot = Some(done_slot);
                            self._update_progress = Some(progress_for_ui);
                        }
                    }
                    UpdateState::Downloading => {
                        let mut resolved = None;
                        if let Some(ref slot) = self._update_download_slot {
                            if let Ok(s) = slot.lock() {
                                resolved = s.clone();
                            }
                        }
                        if let Some(state) = resolved {
                            self.update_state = state;
                            self._update_download_slot = None;
                            self._update_progress = None;
                        } else {
                            let pct = self
                                ._update_progress
                                .as_ref()
                                .and_then(|p| p.lock().ok().map(|v| *v))
                                .unwrap_or(0.0);
                            ui.label(
                                egui::RichText::new(self.i18n.t("update.downloading")).size(12.0),
                            );
                            ui.add(egui::ProgressBar::new(pct).show_percentage());
                            ui.ctx().request_repaint();
                        }
                    }
                    UpdateState::UpToDate => {
                        ui.label(
                            egui::RichText::new(self.i18n.t("update.up_to_date"))
                                .size(12.0)
                                .color(egui::Color32::GRAY),
                        );
                    }
                    UpdateState::Failed(msg) => {
                        let msg = msg.clone();
                        ui.label(
                            egui::RichText::new(self.i18n.tf1("update.failed", &msg))
                                .size(12.0)
                                .color(egui::Color32::from_rgb(255, 100, 100)),
                        );
                        ui.add_space(4.0);
                        if ui
                            .small_button(
                                egui::RichText::new(self.i18n.t("update.check")).size(11.0),
                            )
                            .clicked()
                        {
                            self.update_state = UpdateState::Idle;
                        }
                    }
                    UpdateState::Restarting => {
                        ui.label(
                            egui::RichText::new(self.i18n.t("update.restarting"))
                                .size(13.0)
                                .color(egui::Color32::from_rgb(80, 200, 120)),
                        );
                    }
                });

                ui.add_space(10.0);

                // ── Export logs button ──
                ui.vertical_centered(|ui| {
                    if ui
                        .button(egui::RichText::new(self.i18n.t("feedback.export_logs")).size(13.0))
                        .clicked()
                    {
                        match self.export_feedback_log() {
                            Ok(path) => {
                                self.last_exported_feedback_log = Some(path.clone());
                                self.push_feedback_log(format!(
                                    "export feedback log success: {path}"
                                ));
                            }
                            Err(e) => {
                                self.push_feedback_log(format!("export feedback log failed: {e}"));
                            }
                        }
                    }

                    if let Some(ref path) = self.last_exported_feedback_log.clone() {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(self.i18n.tf1("feedback.export_success", path))
                                .size(11.0)
                                .color(egui::Color32::GRAY),
                        );
                    }
                });

                ui.add_space(14.0);
                ui.separator();
                ui.add_space(8.0);

                // ── Open source notices ──
                ui.label(
                    egui::RichText::new(self.i18n.t("about.open_source"))
                        .size(13.0)
                        .strong(),
                );
                ui.add_space(6.0);

                open_source_row(
                    ui,
                    "pagecurl",
                    "Apache 2.0",
                    "https://github.com/oleksandrbalan/pagecurl",
                    self.i18n.t("about.view_license"),
                );
                ui.add_space(4.0);
                open_source_row(
                    ui,
                    "egui / eframe",
                    "MIT / Apache 2.0",
                    "https://github.com/emilk/egui",
                    self.i18n.t("about.view_license"),
                );
                ui.add_space(4.0);
                open_source_row(
                    ui,
                    "rbook",
                    "Apache 2.0",
                    "https://crates.io/crates/rbook",
                    self.i18n.t("about.view_license"),
                );

                ui.add_space(12.0);
            });

        self.show_about = open;
    }
}

fn open_source_row(ui: &mut egui::Ui, name: &str, license: &str, url: &str, view_label: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(name).size(12.0).strong());
        ui.label(
            egui::RichText::new(format!("({})", license))
                .size(11.0)
                .color(egui::Color32::GRAY),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .small_button(egui::RichText::new(view_label).size(11.0))
                .clicked()
            {
                let _ = open_url(url);
            }
        });
    });
}

fn open_url(url: &str) -> Result<(), std::io::Error> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", "", url])
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(url).spawn()?;
    }
    Ok(())
}
