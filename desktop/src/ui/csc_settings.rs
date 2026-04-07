use crate::app::ReaderApp;
use eframe::egui;
use reader_core::csc::{CorrectionMode, CscThreshold, ModelStatus};

impl ReaderApp {
    pub fn render_csc_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading(self.i18n.t("csc.title"));
        ui.add_space(8.0);

        // ── Correction mode ──
        ui.label(self.i18n.t("csc.mode"));
        ui.horizontal(|ui| {
            if ui
                .selectable_label(
                    self.csc_mode == CorrectionMode::None,
                    self.i18n.t("csc.mode_none"),
                )
                .clicked()
            {
                self.csc_mode = CorrectionMode::None;
            }
            if ui
                .selectable_label(
                    self.csc_mode == CorrectionMode::ReadOnly,
                    self.i18n.t("csc.mode_readonly"),
                )
                .clicked()
            {
                self.csc_mode = CorrectionMode::ReadOnly;
            }
            if ui
                .selectable_label(
                    self.csc_mode == CorrectionMode::ReadWrite,
                    self.i18n.t("csc.mode_readwrite"),
                )
                .clicked()
            {
                self.csc_mode = CorrectionMode::ReadWrite;
            }
        });
        ui.add_space(4.0);

        // ── Threshold (only when CSC enabled) ──
        if self.csc_mode != CorrectionMode::None {
            ui.label(self.i18n.t("csc.threshold"));
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(
                        self.csc_threshold == CscThreshold::Conservative,
                        self.i18n.t("csc.conservative"),
                    )
                    .clicked()
                {
                    self.csc_threshold = CscThreshold::Conservative;
                }
                if ui
                    .selectable_label(
                        self.csc_threshold == CscThreshold::Standard,
                        self.i18n.t("csc.standard"),
                    )
                    .clicked()
                {
                    self.csc_threshold = CscThreshold::Standard;
                }
                if ui
                    .selectable_label(
                        self.csc_threshold == CscThreshold::Aggressive,
                        self.i18n.t("csc.aggressive"),
                    )
                    .clicked()
                {
                    self.csc_threshold = CscThreshold::Aggressive;
                }
            });
            ui.add_space(8.0);

            // ── Model status ──
            ui.separator();
            ui.label(self.i18n.t("csc.model_management"));
            ui.add_space(4.0);

            match &self.csc_model_status {
                ModelStatus::NotDownloaded => {
                    ui.horizontal(|ui| {
                        ui.label("⚠");
                        ui.label(self.i18n.t("csc.model_not_downloaded"));
                    });
                    if ui.button(self.i18n.t("csc.download_model")).clicked() {
                        self.csc_start_download();
                    }
                }
                ModelStatus::Downloading { progress } => {
                    let progress = *progress;
                    ui.horizontal(|ui| {
                        ui.label("⏳");
                        ui.label(self.i18n.t("csc.downloading"));
                    });
                    let bar =
                        egui::ProgressBar::new(progress).text(format!("{:.0}%", progress * 100.0));
                    ui.add(bar);
                }
                ModelStatus::Downloaded => {
                    ui.horizontal(|ui| {
                        ui.label("✓");
                        ui.label(self.i18n.t("csc.model_downloaded"));
                    });
                    #[cfg(feature = "csc")]
                    if ui.button(self.i18n.t("csc.load_model")).clicked() {
                        self.csc_load_model();
                    }
                    #[cfg(not(feature = "csc"))]
                    {
                        ui.label(self.i18n.t("csc.model_downloaded"));
                    }
                }
                ModelStatus::Loading => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(self.i18n.t("csc.loading_model"));
                    });
                }
                ModelStatus::Ready => {
                    ui.horizontal(|ui| {
                        ui.label("✓");
                        ui.colored_label(
                            egui::Color32::from_rgb(80, 200, 120),
                            self.i18n.t("csc.model_ready"),
                        );
                    });
                }
                ModelStatus::Error(msg) => {
                    ui.horizontal(|ui| {
                        ui.label("✗");
                        ui.colored_label(egui::Color32::from_rgb(255, 100, 100), msg.as_str());
                    });
                    if ui.button(self.i18n.t("csc.retry")).clicked() {
                        self.csc_model_status = ModelStatus::NotDownloaded;
                    }
                }
            }
        }

        ui.add_space(12.0);
        ui.separator();

        // ── GitHub account (for data contribution) ──
        ui.add_space(8.0);
        ui.label(self.i18n.t("csc.github_account"));
        ui.add_space(4.0);

        if let Some(username) = &self.github_username {
            ui.horizontal(|ui| {
                ui.label("✓");
                ui.label(format!("{}: {}", self.i18n.t("csc.logged_in_as"), username));
            });
            if ui.button(self.i18n.t("csc.logout")).clicked() {
                self.push_feedback_log(format!("[GitHub] logout user={}", username));
                // Remove token from OS credential store
                reader_core::sharing::keystore::delete_github_token();
                self.github_token = None;
                self.github_username = None;
            }
        } else if self.github_oauth_polling {
            // Device Flow — show user code
            if let Some(user_code) = &self.github_user_code {
                ui.label(self.i18n.t("csc.visit_github"));
                ui.hyperlink("https://github.com/login/device");
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(self.i18n.t("csc.enter_code"));
                    ui.monospace(user_code);
                    if ui
                        .small_button("📋")
                        .on_hover_text(self.i18n.t("csc.copy_code"))
                        .clicked()
                    {
                        ui.ctx().copy_text(user_code.clone());
                    }
                });
            }
            if !self.github_oauth_status.is_empty() {
                ui.label(&self.github_oauth_status);
            }
            if ui.button(self.i18n.t("csc.cancel")).clicked() {
                self.github_oauth_polling = false;
                self.github_device_code = None;
                self.github_user_code = None;
                self.github_oauth_status.clear();
            }
        } else if ui.button(self.i18n.t("csc.login_github")).clicked() {
            self.github_start_device_flow();
        }
    }

    /// Check model status on startup.
    pub fn csc_check_model_status(&mut self) {
        let model_dir = reader_core::csc::model::model_dir(&self.data_dir);
        self.push_feedback_log(format!(
            "[CSC] check_model_status: data_dir={}, model_dir={}",
            self.data_dir,
            model_dir.display()
        ));
        let available = reader_core::csc::model::is_model_available(&self.data_dir);
        self.push_feedback_log(format!("[CSC] is_model_available={}", available));
        if available {
            let verified = reader_core::csc::model::verify_model(&self.data_dir);
            self.push_feedback_log(format!("[CSC] verify_model={}", verified));
            if verified {
                self.csc_model_status = ModelStatus::Downloaded;
            } else {
                self.csc_model_status = ModelStatus::Error("Model integrity check failed".into());
            }
        } else {
            self.csc_model_status = ModelStatus::NotDownloaded;
            // List what files exist in model_dir for debugging
            if model_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&model_dir) {
                    let names: Vec<String> = entries
                        .filter_map(|e| e.ok())
                        .map(|e| {
                            format!(
                                "{}({}B)",
                                e.file_name().to_string_lossy(),
                                e.metadata().map(|m| m.len()).unwrap_or(0)
                            )
                        })
                        .collect();
                    self.push_feedback_log(format!(
                        "[CSC] model_dir contents: [{}]",
                        names.join(", ")
                    ));
                }
            } else {
                self.push_feedback_log("[CSC] model_dir does not exist");
            }
        }
    }

    /// Start downloading model files in background.
    fn csc_start_download(&mut self) {
        self.push_feedback_log("[CSC] start_download: begin");
        self.csc_model_status = ModelStatus::Downloading { progress: 0.0 };
        let data_dir = self.data_dir.clone();
        let progress = self.csc_download_progress.clone();
        let ctx = self.last_egui_ctx.clone();
        let logs = self.feedback_logs.clone();

        std::thread::spawn(move || {
            let dir = reader_core::csc::model::model_dir(&data_dir);
            let _ = std::fs::create_dir_all(&dir);
            let files = reader_core::csc::model::required_files();
            let total = files.len() as f32;
            crate::app::dbg_log(
                &logs,
                format!(
                    "[CSC] download thread: {} files to fetch, dir={}",
                    files.len(),
                    dir.display()
                ),
            );

            for (i, (url, filename)) in files.iter().enumerate() {
                *progress.lock().unwrap() = i as f32 / total;
                if let Some(ctx) = &ctx {
                    ctx.request_repaint();
                }

                let dest = dir.join(filename);
                crate::app::dbg_log(
                    &logs,
                    format!(
                        "[CSC] downloading [{}/{}] {} → {}",
                        i + 1,
                        files.len(),
                        url,
                        dest.display()
                    ),
                );
                match reqwest::blocking::get(*url) {
                    Ok(resp) => {
                        let status = resp.status();
                        if status.is_success() {
                            match resp.bytes() {
                                Ok(bytes) => {
                                    crate::app::dbg_log(
                                        &logs,
                                        format!(
                                            "[CSC] downloaded {} ({} bytes)",
                                            filename,
                                            bytes.len()
                                        ),
                                    );
                                    if let Err(e) = std::fs::write(&dest, &bytes) {
                                        crate::app::dbg_log(
                                            &logs,
                                            format!("[CSC] ERROR writing {}: {}", filename, e),
                                        );
                                    }
                                }
                                Err(e) => {
                                    crate::app::dbg_log(
                                        &logs,
                                        format!(
                                            "[CSC] ERROR reading response body for {}: {}",
                                            filename, e
                                        ),
                                    );
                                }
                            }
                        } else {
                            crate::app::dbg_log(
                                &logs,
                                format!("[CSC] ERROR HTTP {} for {}", status, url),
                            );
                        }
                    }
                    Err(e) => {
                        crate::app::dbg_log(
                            &logs,
                            format!("[CSC] ERROR request failed for {}: {}", url, e),
                        );
                    }
                }
            }
            *progress.lock().unwrap() = 1.0;
            crate::app::dbg_log(&logs, "[CSC] download thread: all files done");
            if let Some(ctx) = &ctx {
                ctx.request_repaint();
            }
        });
    }

    /// Poll download progress each frame.
    pub fn csc_poll_download(&mut self) {
        if let ModelStatus::Downloading { .. } = &self.csc_model_status {
            let p = *self.csc_download_progress.lock().unwrap();
            if p >= 1.0 {
                // Download complete — verify
                let available = reader_core::csc::model::is_model_available(&self.data_dir);
                self.push_feedback_log(format!(
                    "[CSC] poll_download complete: is_available={}",
                    available
                ));
                if available {
                    let verified = reader_core::csc::model::verify_model(&self.data_dir);
                    self.push_feedback_log(format!("[CSC] poll_download verify={}", verified));
                    if verified {
                        self.csc_model_status = ModelStatus::Downloaded;
                    } else {
                        self.csc_model_status =
                            ModelStatus::Error("Model verification failed".into());
                    }
                } else {
                    self.csc_model_status = ModelStatus::Error("Download failed".into());
                }
            } else {
                self.csc_model_status = ModelStatus::Downloading { progress: p };
            }
        }
    }

    /// Load the CSC model (blocking — call from UI thread for now, will be moved to bg thread).
    #[cfg(feature = "csc")]
    pub fn csc_load_model(&mut self) {
        self.push_feedback_log("[CSC] load_model: begin");
        self.csc_model_status = ModelStatus::Loading;
        let mut engine =
            reader_core::csc::CscEngine::new(self.csc_mode.clone(), self.csc_threshold.clone());
        let t0 = std::time::Instant::now();
        match engine.load(&self.data_dir) {
            Ok(()) => {
                let elapsed = t0.elapsed();
                self.push_feedback_log(format!(
                    "[CSC] load_model: OK in {:.1}s",
                    elapsed.as_secs_f64()
                ));
                *self.csc_engine.lock().unwrap() = Some(engine);
                self.csc_model_status = ModelStatus::Ready;
                // Spawn background worker and trigger current chapter
                self.csc_spawn_worker();
                self.csc_trigger_chapter(self.current_chapter);
                if let Some(book) = &self.book {
                    if self.current_chapter + 1 < book.chapters.len() {
                        self.csc_trigger_chapter(self.current_chapter + 1);
                    }
                }
            }
            Err(e) => {
                self.push_feedback_log(format!("[CSC] load_model: ERROR {}", e));
                self.csc_model_status = ModelStatus::Error(format!("{}", e));
            }
        }
    }
}
