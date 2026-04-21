//! UI for contributing to the Chinese Spelling Correction dataset.
use crate::app::ReaderApp;
use eframe::egui;
use reader_core::epub::ContentBlock;

const MODEL_REPO_OWNER: &str = "zhongbai2333";
const MODEL_REPO_NAME: &str = "RustEpubReader-Model";
const SUBMISSION_DIR: &str = "datasets/submissions";

/// Minimum number of accepted/rejected corrections before first prompt.
const MIN_CORRECTIONS_TO_PROMPT: usize = 30;

/// After first prompt, how many additional resolved corrections before prompting again.
const PROMPT_INCREMENT: usize = 50;

/// Max context chars on each side of the corrected character.
const CONTEXT_RADIUS: usize = 10;

/// Max chars per line (validation constraint).
const MAX_LINE_LEN: usize = 50;

/// A single training sample: input (with error) and output (corrected).
#[derive(Debug, Clone, serde::Serialize)]
pub struct CscSample {
    pub input: String,
    pub output: String,
}

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
                cfg.save(&self.data_dir);
            }
        }
    }

    /// Extract training samples from all corrections in current book.
    /// Returns Vec of (input_with_error, output_corrected) pairs.
    pub fn csc_collect_samples(&self) -> Vec<CscSample> {
        let corrections = match &self.book_config {
            Some(cfg) => &cfg.corrections,
            None => return vec![],
        };
        let book = match &self.book {
            Some(b) => b,
            None => return vec![],
        };

        // Only take accepted corrections (user confirmed the replacement)
        let accepted: Vec<_> = corrections
            .iter()
            .filter(|r| r.status == "accepted")
            .collect();

        let mut samples = Vec::new();

        for rec in &accepted {
            let chapter = match book.chapters.get(rec.chapter) {
                Some(ch) => ch,
                None => continue,
            };
            let block = match chapter.blocks.get(rec.block_idx) {
                Some(b) => b,
                None => continue,
            };

            // Extract full block text
            let block_text: String = match block {
                ContentBlock::Paragraph { spans, .. } => {
                    spans.iter().map(|s| s.text.as_str()).collect()
                }
                ContentBlock::Heading { spans, .. } => {
                    spans.iter().map(|s| s.text.as_str()).collect()
                }
                _ => continue,
            };

            let chars: Vec<char> = block_text.chars().collect();
            if rec.char_offset >= chars.len() {
                continue;
            }

            // Extract context window around the correction
            let start = rec.char_offset.saturating_sub(CONTEXT_RADIUS);
            let end = (rec.char_offset + 1 + CONTEXT_RADIUS).min(chars.len());
            let context_chars = &chars[start..end];

            // Build output (corrected) string from context
            let mut output_chars: Vec<char> = context_chars.to_vec();
            let local_offset = rec.char_offset - start;

            // Build input (with error) string — replace the corrected char with original
            let mut input_chars = output_chars.clone();
            if let Some(orig_ch) = rec.original.chars().next() {
                input_chars[local_offset] = orig_ch;
            }
            if let Some(corr_ch) = rec.corrected.chars().next() {
                output_chars[local_offset] = corr_ch;
            }

            let input: String = input_chars.iter().collect();
            let output: String = output_chars.iter().collect();

            // Validation: same length, different content, within limits
            if input.len() == output.len()
                && input != output
                && input.chars().count() <= MAX_LINE_LEN
            {
                samples.push(CscSample { input, output });
            }
        }

        samples
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

        let samples = self.csc_collect_samples();
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
        let logs = self.feedback_logs.clone();
        let (tx, rx) = std::sync::mpsc::channel::<ContributeResult>();

        self.push_feedback_log(format!(
            "[CSC-Contribute] starting submission: {} samples, file={}",
            sample_count, filename
        ));

        std::thread::spawn(move || {
            let result = csc_contribute_worker(&token, &username, &filename, &jsonl, &logs);
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
            return;
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
                    ui.label(self.i18n.t("csc.contribute_desc"));
                    ui.add_space(8.0);

                    // Show sample count
                    let samples = self.csc_collect_samples();
                    ui.label(
                        egui::RichText::new(
                            self.i18n
                                .tf1("csc.contribute_sample_count", &samples.len().to_string()),
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
                            for s in samples.iter().take(3) {
                                ui.horizontal(|ui| {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(220, 60, 50),
                                        &s.input,
                                    );
                                    ui.label("→");
                                    ui.colored_label(
                                        egui::Color32::from_rgb(60, 180, 80),
                                        &s.output,
                                    );
                                });
                            }
                            if samples.len() > 3 {
                                ui.label(
                                    egui::RichText::new(format!("... +{}", samples.len() - 3))
                                        .small()
                                        .color(egui::Color32::GRAY),
                                );
                            }
                        });
                    }

                    ui.add_space(8.0);

                    // Check if logged in
                    if self.github_token.is_none() {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 160, 50),
                            self.i18n.t("csc.contribute_need_login"),
                        );
                        ui.add_space(4.0);
                        if ui.button(self.i18n.t("csc.login_github")).clicked() {
                            self.show_github_login = true;
                        }
                    } else {
                        ui.horizontal(|ui| {
                            ui.label("✓");
                            ui.label(self.i18n.tf1(
                                "csc.contribute_logged_in",
                                self.github_username.as_deref().unwrap_or("?"),
                            ));
                        });
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
                        ui.label(&self.csc_contribute_status);
                    });
                } else if let Some(pr_url) = &self.csc_contribute_pr_url.clone() {
                    // Success
                    ui.label(
                        egui::RichText::new("✅")
                            .size(24.0)
                            .color(egui::Color32::from_rgb(60, 180, 80)),
                    );
                    ui.label(&self.csc_contribute_status);
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
    username: &str,
    filename: &str,
    jsonl: &str,
    logs: &std::sync::Arc<std::sync::Mutex<Vec<String>>>,
) -> ContributeResult {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::blocking::Client::new());

    let auth = format!("Bearer {}", token);
    let ua = "RustEpubReader";

    // Step 1: Fork the model repo (idempotent — if already forked, GitHub returns the existing fork)
    crate::app::dbg_log(
        logs,
        format!(
            "[CSC-Contribute] step 1: forking {}/{}",
            MODEL_REPO_OWNER, MODEL_REPO_NAME
        ),
    );
    let fork_resp = client
        .post(format!(
            "https://api.github.com/repos/{}/{}/forks",
            MODEL_REPO_OWNER, MODEL_REPO_NAME
        ))
        .header("Authorization", &auth)
        .header("User-Agent", ua)
        .header("Accept", "application/vnd.github+json")
        .send();

    let fork_resp = match fork_resp {
        Ok(r) => r,
        Err(e) => {
            return ContributeResult::Error(format!("Fork request failed: {}", e));
        }
    };

    let fork_status = fork_resp.status();
    if !fork_status.is_success() && fork_status.as_u16() != 202 {
        let body = fork_resp.text().unwrap_or_default();
        crate::app::dbg_log(
            logs,
            format!(
                "[CSC-Contribute] fork failed: HTTP {} body={}",
                fork_status, body
            ),
        );
        return ContributeResult::Error(format!("Fork failed: HTTP {}", fork_status));
    }
    crate::app::dbg_log(logs, "[CSC-Contribute] fork OK (or already exists)");

    // Brief pause to let GitHub process the fork
    std::thread::sleep(std::time::Duration::from_secs(3));

    // Step 2: Get default branch SHA from user's fork
    crate::app::dbg_log(
        logs,
        format!(
            "[CSC-Contribute] step 2: getting default branch of {}/{}",
            username, MODEL_REPO_NAME
        ),
    );
    let repo_resp = client
        .get(format!(
            "https://api.github.com/repos/{}/{}",
            username, MODEL_REPO_NAME
        ))
        .header("Authorization", &auth)
        .header("User-Agent", ua)
        .header("Accept", "application/vnd.github+json")
        .send();

    let default_branch = match repo_resp {
        Ok(r) if r.status().is_success() => {
            let body: serde_json::Value = r.json().unwrap_or_default();
            body["default_branch"]
                .as_str()
                .unwrap_or("main")
                .to_string()
        }
        Ok(r) => {
            let status = r.status();
            let body = r.text().unwrap_or_default();
            crate::app::dbg_log(
                logs,
                format!(
                    "[CSC-Contribute] get fork info failed: HTTP {} {}",
                    status, body
                ),
            );
            return ContributeResult::Error(format!("Cannot access fork: HTTP {}", status));
        }
        Err(e) => {
            return ContributeResult::Error(format!("Get fork info failed: {}", e));
        }
    };
    crate::app::dbg_log(
        logs,
        format!("[CSC-Contribute] default_branch={}", default_branch),
    );

    // Step 3: Create file on user's fork via Contents API
    let file_path = format!("{}/{}", SUBMISSION_DIR, filename);
    crate::app::dbg_log(
        logs,
        format!(
            "[CSC-Contribute] step 3: creating file {} on {}/{}",
            file_path, username, MODEL_REPO_NAME
        ),
    );

    use base64::Engine;
    let content_b64 = base64::engine::general_purpose::STANDARD.encode(jsonl.as_bytes());

    let commit_body = serde_json::json!({
        "message": format!("Add correction data from {}", username),
        "content": content_b64,
        "branch": default_branch,
    });

    let create_resp = client
        .put(format!(
            "https://api.github.com/repos/{}/{}/contents/{}",
            username, MODEL_REPO_NAME, file_path
        ))
        .header("Authorization", &auth)
        .header("User-Agent", ua)
        .header("Accept", "application/vnd.github+json")
        .json(&commit_body)
        .send();

    match create_resp {
        Ok(r) if r.status().is_success() || r.status().as_u16() == 201 => {
            crate::app::dbg_log(logs, "[CSC-Contribute] file created OK");
        }
        Ok(r) => {
            let status = r.status();
            let body = r.text().unwrap_or_default();
            crate::app::dbg_log(
                logs,
                format!(
                    "[CSC-Contribute] create file failed: HTTP {} body={}",
                    status, body
                ),
            );
            // If file already exists (409), try updating with SHA
            if status.as_u16() == 422 || status.as_u16() == 409 {
                crate::app::dbg_log(
                    logs,
                    "[CSC-Contribute] file may exist, trying to get SHA for update",
                );
                // Get existing file SHA
                let get_resp = client
                    .get(format!(
                        "https://api.github.com/repos/{}/{}/contents/{}?ref={}",
                        username, MODEL_REPO_NAME, file_path, default_branch
                    ))
                    .header("Authorization", &auth)
                    .header("User-Agent", ua)
                    .header("Accept", "application/vnd.github+json")
                    .send();

                if let Ok(gr) = get_resp {
                    if gr.status().is_success() {
                        let gbody: serde_json::Value = gr.json().unwrap_or_default();
                        if let Some(sha) = gbody["sha"].as_str() {
                            let update_body = serde_json::json!({
                                "message": format!("Update correction data from {}", username),
                                "content": content_b64,
                                "sha": sha,
                                "branch": default_branch,
                            });
                            let update_resp = client
                                .put(format!(
                                    "https://api.github.com/repos/{}/{}/contents/{}",
                                    username, MODEL_REPO_NAME, file_path
                                ))
                                .header("Authorization", &auth)
                                .header("User-Agent", ua)
                                .header("Accept", "application/vnd.github+json")
                                .json(&update_body)
                                .send();

                            match update_resp {
                                Ok(ur)
                                    if ur.status().is_success() || ur.status().as_u16() == 200 =>
                                {
                                    crate::app::dbg_log(logs, "[CSC-Contribute] file updated OK");
                                }
                                Ok(ur) => {
                                    let s = ur.status();
                                    let b = ur.text().unwrap_or_default();
                                    return ContributeResult::Error(format!(
                                        "Update file failed: HTTP {} {}",
                                        s, b
                                    ));
                                }
                                Err(e) => {
                                    return ContributeResult::Error(format!(
                                        "Update file request failed: {}",
                                        e
                                    ));
                                }
                            }
                        }
                    }
                }
            } else {
                return ContributeResult::Error(format!("Create file failed: HTTP {}", status));
            }
        }
        Err(e) => {
            return ContributeResult::Error(format!("Create file request failed: {}", e));
        }
    }

    // Step 4: Create Pull Request from user's fork to upstream
    crate::app::dbg_log(logs, "[CSC-Contribute] step 4: creating pull request");

    let pr_body = serde_json::json!({
        "title": format!("CSC correction data from {}", username),
        "body": format!(
            "Automatically submitted correction data from RustEpubReader.\n\n- User: @{}\n- Samples: {}\n- File: `{}`",
            username,
            jsonl.lines().count(),
            file_path,
        ),
        "head": format!("{}:{}", username, default_branch),
        "base": default_branch,
    });

    let pr_resp = client
        .post(format!(
            "https://api.github.com/repos/{}/{}/pulls",
            MODEL_REPO_OWNER, MODEL_REPO_NAME
        ))
        .header("Authorization", &auth)
        .header("User-Agent", ua)
        .header("Accept", "application/vnd.github+json")
        .json(&pr_body)
        .send();

    match pr_resp {
        Ok(r) => {
            let status = r.status();
            let body: serde_json::Value = r.json().unwrap_or_default();
            if status.is_success() || status.as_u16() == 201 {
                let pr_url = body["html_url"].as_str().unwrap_or("").to_string();
                crate::app::dbg_log(logs, format!("[CSC-Contribute] PR created: {}", pr_url));
                ContributeResult::Success { pr_url }
            } else if status.as_u16() == 422 {
                // PR already exists — try to find it
                crate::app::dbg_log(logs, "[CSC-Contribute] PR may already exist, searching...");
                let search_resp = client
                    .get(format!(
                        "https://api.github.com/repos/{}/{}/pulls?head={}:{}&state=open",
                        MODEL_REPO_OWNER, MODEL_REPO_NAME, username, default_branch
                    ))
                    .header("Authorization", &auth)
                    .header("User-Agent", ua)
                    .header("Accept", "application/vnd.github+json")
                    .send();
                if let Ok(sr) = search_resp {
                    let prs: serde_json::Value = sr.json().unwrap_or_default();
                    if let Some(first) = prs.as_array().and_then(|a| a.first()) {
                        let url = first["html_url"].as_str().unwrap_or("").to_string();
                        crate::app::dbg_log(
                            logs,
                            format!("[CSC-Contribute] found existing PR: {}", url),
                        );
                        return ContributeResult::Success { pr_url: url };
                    }
                }
                ContributeResult::Success {
                    pr_url: format!(
                        "https://github.com/{}/{}/pulls",
                        MODEL_REPO_OWNER, MODEL_REPO_NAME
                    ),
                }
            } else {
                crate::app::dbg_log(
                    logs,
                    format!(
                        "[CSC-Contribute] PR creation failed: HTTP {} body={}",
                        status, body
                    ),
                );
                ContributeResult::Error(format!("Create PR failed: HTTP {}", status))
            }
        }
        Err(e) => ContributeResult::Error(format!("Create PR request failed: {}", e)),
    }
}
