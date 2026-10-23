//! UI for contributing to the Chinese Spelling Correction dataset.
use crate::app::ReaderApp;
use eframe::egui;

/// Minimum number of accepted/rejected corrections before first prompt.
const MIN_CORRECTIONS_TO_PROMPT: usize = 30;

/// After first prompt, how many additional resolved corrections before prompting again.
const PROMPT_INCREMENT: usize = 50;

/// A single training sample: input (with error) and output (corrected).
pub type CscSample = reader_core::csc::contribution::Sample;

/// Result of an async contribution operation.
pub enum ContributeResult {
    Success { pr_url: String },
    Error(String),
}

impl ReaderApp {
    /// Check if we should prompt the user to contribute corrections.
    /// Called periodically (e.g. on chapter change or popup close).
    pub fn csc_check_contribution_prompt(&mut self) {
        // Don't prompt if already shown this session, or user dismissed this session
        if self.csc_contribute_prompted || self.csc_contribute_dismissed {
            return;
        }
        let cfg = match &self.book_config {
            Some(cfg) => cfg,
            None => return,
        };
        let resolved_count = cfg
            .corrections
            .iter()
            .filter(|r| r.status == "accepted" || r.status == "rejected")
            .count();
        let last_prompted = cfg.last_contribute_prompt_count;
        // First prompt at MIN threshold; subsequent prompts require INCREMENT more
        let threshold = if last_prompted == 0 {
            MIN_CORRECTIONS_TO_PROMPT
        } else {
            last_prompted + PROMPT_INCREMENT
        };
        if resolved_count >= threshold {
            self.csc_contribute_prompted = true;
            self.show_csc_contribute_dialog = true;
            // Persist the count so we don't prompt again at this level
            if let Some(cfg) = &mut self.book_config {
                cfg.last_contribute_prompt_count = resolved_count;
                if let Err(error) = cfg.save(&self.data_dir) {
                    self.error_msg = Some(error.to_string());
                };
            }
        }
    }

    /// Extract training samples from all corrections in current book.
    /// Returns Vec of (input_with_error, output_corrected) pairs.
    pub fn csc_collect_samples(&self) -> Vec<CscSample> {
        match (&self.book, &self.book_config) {
            (Some(book), Some(cfg)) => {
                reader_core::csc::contribution::samples(book, cfg).unwrap_or_default()
            }
            _ => Vec::new(),
        }
    }

    /// Format samples as JSONL string.
    pub fn csc_format_jsonl(samples: &[CscSample]) -> String {
        let mut buf = String::new();
        for s in samples {
            if let Ok(line) = serde_json::to_string(s) {
                buf.push_str(&line);
                buf.push('\n');
            }
        }
        buf
    }

    /// Submit corrections to RustEpubReader-Model via GitHub API.
    /// Spawns a background thread that: forks the repo, creates a file, opens a PR.
    pub fn csc_submit_contribution(&mut self) {
        let token = match &self.github_token {
            Some(t) => t.clone(),
            None => {
                self.csc_contribute_status = self.i18n.t("csc.contribute_need_login").to_string();
                return;
            }
        };
        let username = match &self.github_username {
            Some(u) => u.clone(),
            None => {
                self.csc_contribute_status = self.i18n.t("csc.contribute_need_login").to_string();
                return;
            }
        };

        let samples = self
            .csc_contribution_samples
            .clone()
            .unwrap_or_else(|| self.csc_collect_samples());
        if samples.is_empty() {
            self.csc_contribute_status = self.i18n.t("csc.contribute_no_data").to_string();
            return;
        }

        let jsonl = Self::csc_format_jsonl(&samples);
        let sample_count = samples.len();
        let timestamp = reader_core::now_secs();
        let filename = format!("{}_{}.jsonl", username, timestamp);

        self.csc_contribute_status = self.i18n.t("csc.contribute_submitting").to_string();
        self.csc_contribute_in_progress = true;

        let ctx = self.last_egui_ctx.clone();
        let (tx, rx) = std::sync::mpsc::channel::<ContributeResult>();

        self.push_feedback_log(format!(
            "[CSC-Contribute] starting submission: {} samples, file={}",
            sample_count, filename
        ));

        std::thread::spawn(move || {
            let result = csc_contribute_worker(&token, &username, &filename, &jsonl);
            let _ = tx.send(result);
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        });

        self.csc_contribute_rx = Some(rx);
    }

    /// Poll contribution result (called each frame).
    pub fn csc_poll_contribution(&mut self) {
        let result = self
            .csc_contribute_rx
            .as_ref()
            .and_then(|rx| rx.try_recv().ok());

        if let Some(result) = result {
            self.csc_contribute_rx = None;
            self.csc_contribute_in_progress = false;
            match result {
                ContributeResult::Success { pr_url } => {
                    self.push_feedback_log(format!("[CSC-Contribute] PR created: {}", pr_url));
                    self.csc_contribute_status = self.i18n.tf1("csc.contribute_success", &pr_url);
                    self.csc_contribute_pr_url = Some(pr_url);
                }
                ContributeResult::Error(e) => {
                    self.push_feedback_log(format!("[CSC-Contribute] error: {}", e));
                    self.csc_contribute_status = self.i18n.tf1("csc.contribute_error", &e);
                }
            }
        }
    }

    /// Render the contribution prompt dialog.
    pub fn render_csc_contribute_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_csc_contribute_dialog {
            self.csc_contribution_samples = None;
            return;
        }
        if self.csc_contribution_samples.is_none() {
            self.csc_contribution_samples = Some(self.csc_collect_samples());
        }

        let mut open = true;
        egui::Window::new(self.i18n.t("csc.contribute_title"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_max_width(400.0);

                if !self.csc_contribute_in_progress && self.csc_contribute_pr_url.is_none() {
                    // Initial prompt
                    ui.label(self.i18n.t("contribution.public"));
                    ui.add_space(8.0);

                    // Show sample count
                    let samples = self.csc_contribution_samples.clone().unwrap_or_default();
                    ui.label(
                        egui::RichText::new(
                            self.i18n
                                .count("csc.contribute_sample_count", samples.len()),
                        )
                        .strong(),
                    );

                    // Preview first few samples
                    if !samples.is_empty() {
                        ui.add_space(4.0);
                        ui.group(|ui| {
                            ui.label(
                                egui::RichText::new(self.i18n.t("csc.contribute_preview"))
                                    .small()
                                    .color(egui::Color32::GRAY),
                            );
                            let mut remove = None;
                            egui::ScrollArea::vertical()
                                .max_height(240.)
                                .show(ui, |ui| {
                                    for (index, sample) in samples.iter().enumerate() {
                                        ui.horizontal_wrapped(|ui| {
                                            ui.vertical(|ui| {
                                                ui.label(&sample.input);
                                                ui.label(
                                                    egui::RichText::new(&sample.output).strong(),
                                                );
                                            });
                                            if ui.small_button("×").clicked() {
                                                remove = Some(index);
                                            }
                                        });
                                        ui.separator();
                                    }
                                });
                            if let Some(index) = remove {
                                self.csc_contribution_samples
                                    .as_mut()
                                    .unwrap()
                                    .remove(index);
                            }
                        });
                    }

                    ui.add_space(8.0);

                    ui.small(self.i18n.t("account.contribution_hint"));
                    // Check if logged in
                    if self.github_token.is_none() {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 160, 50),
                            self.i18n.t("csc.contribute_need_login"),
                        );
                        ui.add_space(4.0);
                    } else {
                        ui.horizontal(|ui| {
                            ui.label("✓");
                            ui.label(self.i18n.tf1(
                                "csc.contribute_logged_in",
                                self.github_username.as_deref().unwrap_or("?"),
                            ));
                        });
                    }

                    if ui.button(self.i18n.t("account.manage")).clicked() {
                        self.show_github_login = true;
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let can_submit = self.github_token.is_some() && !samples.is_empty();
                        if ui
                            .add_enabled(
                                can_submit,
                                egui::Button::new(self.i18n.t("csc.contribute_submit")),
                            )
                            .clicked()
                        {
                            self.csc_submit_contribution();
                        }
                        if ui.button(self.i18n.t("csc.contribute_later")).clicked() {
                            self.show_csc_contribute_dialog = false;
                            self.csc_contribute_dismissed = true;
                        }
                    });
                } else if self.csc_contribute_in_progress {
                    // Submitting
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(self.i18n.message(&self.csc_contribute_status));
                    });
                } else if let Some(pr_url) = &self.csc_contribute_pr_url.clone() {
                    // Success
                    ui.label(
                        egui::RichText::new("✅")
                            .size(24.0)
                            .color(egui::Color32::from_rgb(60, 180, 80)),
                    );
                    ui.label(self.i18n.message(&self.csc_contribute_status));
                    ui.add_space(4.0);
                    if ui.link(pr_url).clicked() {
                        ctx.open_url(egui::OpenUrl::new_tab(pr_url));
                    }
                    ui.add_space(8.0);
                    if ui.button(self.i18n.t("csc.contribute_close")).clicked() {
                        self.show_csc_contribute_dialog = false;
                    }
                }

                // Show error status
                if !self.csc_contribute_status.is_empty()
                    && !self.csc_contribute_in_progress
                    && self.csc_contribute_pr_url.is_none()
                {
                    ui.add_space(4.0);
                    ui.colored_label(
                        egui::Color32::from_rgb(220, 60, 50),
                        &self.csc_contribute_status,
                    );
                }
            });

        if !open {
            self.show_csc_contribute_dialog = false;
            self.csc_contribute_dismissed = true;
        }
    }
}

/// Background worker: fork repo → create file on fork → open PR to upstream.
fn csc_contribute_worker(
    token: &str,
    _username: &str,
    _filename: &str,
    jsonl: &str,
) -> ContributeResult {
    let samples = jsonl
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<Vec<reader_core::csc::contribution::Sample>, _>>();
    match samples
        .map_err(|e| e.to_string())
        .and_then(|samples| reader_core::csc::contribution::submit(token, &samples))
    {
        Ok(pr_url) => ContributeResult::Success { pr_url },
        Err(error) => ContributeResult::Error(error),
    }
}
