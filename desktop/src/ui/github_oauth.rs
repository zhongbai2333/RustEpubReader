//! GitHub OAuth authentication flow and UI integration.
use crate::app::ReaderApp;

/// GitHub OAuth App Client ID.
/// Can be overridden via the `EPUB_READER_GITHUB_CLIENT_ID` environment variable.
fn github_client_id() -> &'static str {
    // Allow overriding via environment for different deployments/forks
    static CLIENT_ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    CLIENT_ID.get_or_init(|| {
        std::env::var("EPUB_READER_GITHUB_CLIENT_ID")
            .unwrap_or_else(|_| "Ov23liG7iXNfGOTAXXxx".to_string())
    })
}

pub enum PollResult {
    Success { token: String, username: String },
    Pending,
    SlowDown,
    Expired,
    Denied,
    Error(String),
}

impl ReaderApp {
    /// Start GitHub Device Flow: request device + user codes.
    pub fn github_start_device_flow(&mut self) {
        self.push_feedback_log("[GitHub] start_device_flow: requesting device code");
        self.github_oauth_polling = false;
        self.github_oauth_status = self.i18n.t("csc.requesting_code").to_string();

        let ctx = self.last_egui_ctx.clone();
        let logs = self.feedback_logs.clone();
        let (tx, rx) = std::sync::mpsc::channel::<crate::app::DeviceCodeResult>();

        std::thread::spawn(move || {
            let result: crate::app::DeviceCodeResult = (|| {
                let client = reqwest::blocking::Client::new();
                crate::app::dbg_log(&logs, "[GitHub] POST https://github.com/login/device/code");
                let resp = client
                    .post("https://github.com/login/device/code")
                    .header("Accept", "application/json")
                    .form(&[("client_id", github_client_id()), ("scope", "public_repo")])
                    .send()
                    .map_err(|e| {
                        crate::app::dbg_log(
                            &logs,
                            format!("[GitHub] device_code request failed: {}", e),
                        );
                        e.to_string()
                    })?;

                let status = resp.status();
                crate::app::dbg_log(
                    &logs,
                    format!("[GitHub] device_code response HTTP {}", status),
                );
                let body: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
                let device_code = body["device_code"]
                    .as_str()
                    .ok_or_else(|| {
                        crate::app::dbg_log(
                            &logs,
                            format!("[GitHub] missing device_code in response: {}", body),
                        );
                        "missing device_code".to_string()
                    })?
                    .to_string();
                let user_code = body["user_code"]
                    .as_str()
                    .ok_or_else(|| "missing user_code".to_string())?
                    .to_string();
                let expires_in = body["expires_in"].as_u64().unwrap_or(900);
                let interval = body["interval"].as_u64().unwrap_or(5);

                crate::app::dbg_log(
                    &logs,
                    format!(
                        "[GitHub] got user_code={}, expires_in={}s, interval={}s",
                        user_code, expires_in, interval
                    ),
                );
                Ok((device_code, user_code, expires_in, interval))
            })();

            let _ = tx.send(result);
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        });

        self.github_pending_device_code = Some(rx);
    }

    /// Poll for device code response (called each frame).
    pub fn github_poll_device_code(&mut self) {
        let result = self
            .github_pending_device_code
            .as_ref()
            .and_then(|rx| rx.try_recv().ok());

        if let Some(result) = result {
            self.github_pending_device_code = None;
            match result {
                Ok((device_code, user_code, expires_in, interval)) => {
                    self.push_feedback_log(format!(
                        "[GitHub] device_code received, user_code={}, expires={}s",
                        user_code, expires_in
                    ));
                    self.github_device_code = Some(device_code);
                    self.github_user_code = Some(user_code);
                    self.github_oauth_interval = interval;
                    self.github_oauth_expires_at = Some(
                        std::time::Instant::now() + std::time::Duration::from_secs(expires_in),
                    );
                    self.github_oauth_polling = true;
                    self.github_oauth_status.clear();
                    self.github_last_poll = None;
                    let _ = open::that("https://github.com/login/device");
                }
                Err(e) => {
                    self.push_feedback_log(format!("[GitHub] device_code error: {}", e));
                    self.github_oauth_status = format!("Error: {}", e);
                }
            }
        }
    }

    /// Poll GitHub for token exchange (called each frame while github_oauth_polling).
    pub fn github_poll_token(&mut self) {
        if !self.github_oauth_polling {
            return;
        }
        // Already have a pending poll in flight
        if self.github_pending_token_poll.is_some() {
            return;
        }

        // Check expiry
        if let Some(expires_at) = self.github_oauth_expires_at {
            if std::time::Instant::now() > expires_at {
                self.push_feedback_log("[GitHub] token poll: device code expired");
                self.github_oauth_polling = false;
                self.github_oauth_status = self.i18n.t("csc.code_expired").to_string();
                return;
            }
        }

        // Rate-limit polling to interval
        let now = std::time::Instant::now();
        if let Some(last) = self.github_last_poll {
            if now.duration_since(last).as_secs() < self.github_oauth_interval {
                return;
            }
        }
        self.github_last_poll = Some(now);

        let device_code = match &self.github_device_code {
            Some(c) => c.clone(),
            None => return,
        };

        let ctx = self.last_egui_ctx.clone();
        let logs = self.feedback_logs.clone();
        let (tx, rx) = std::sync::mpsc::channel::<Result<PollResult, String>>();

        std::thread::spawn(move || {
            let result = (|| -> Result<PollResult, String> {
                let client = reqwest::blocking::Client::new();
                crate::app::dbg_log(&logs, "[GitHub] polling token exchange...");
                let resp = client
                    .post("https://github.com/login/oauth/access_token")
                    .header("Accept", "application/json")
                    .form(&[
                        ("client_id", github_client_id()),
                        ("device_code", device_code.as_str()),
                        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ])
                    .send()
                    .map_err(|e| e.to_string())?;

                let body: serde_json::Value = resp.json().map_err(|e| e.to_string())?;

                if let Some(token) = body["access_token"].as_str() {
                    crate::app::dbg_log(
                        &logs,
                        "[GitHub] access_token received, fetching user info",
                    );
                    let user_resp = client
                        .get("https://api.github.com/user")
                        .header("Authorization", format!("Bearer {}", token))
                        .header("User-Agent", "RustEpubReader")
                        .send()
                        .map_err(|e| {
                            crate::app::dbg_log(
                                &logs,
                                format!("[GitHub] user info request failed: {}", e),
                            );
                            e.to_string()
                        })?;
                    let user: serde_json::Value = user_resp.json().map_err(|e| e.to_string())?;
                    let username = user["login"].as_str().unwrap_or("unknown").to_string();
                    crate::app::dbg_log(
                        &logs,
                        format!("[GitHub] login success: user={}", username),
                    );
                    return Ok(PollResult::Success {
                        token: token.to_string(),
                        username,
                    });
                }

                if let Some(error) = body["error"].as_str() {
                    crate::app::dbg_log(&logs, format!("[GitHub] poll response error={}", error));
                    return match error {
                        "authorization_pending" => Ok(PollResult::Pending),
                        "slow_down" => Ok(PollResult::SlowDown),
                        "expired_token" => Ok(PollResult::Expired),
                        "access_denied" => Ok(PollResult::Denied),
                        _ => Ok(PollResult::Error(error.to_string())),
                    };
                }

                Ok(PollResult::Pending)
            })();

            let _ = tx.send(result);
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        });

        self.github_pending_token_poll = Some(rx);
    }

    /// Check token poll result (called each frame).
    pub fn github_poll_token_result(&mut self) {
        let result = self
            .github_pending_token_poll
            .as_ref()
            .and_then(|rx| rx.try_recv().ok());

        if let Some(result) = result {
            self.github_pending_token_poll = None;
            match result {
                Ok(PollResult::Success { token, username }) => {
                    self.push_feedback_log(format!("[GitHub] OAuth success: user={}", username));
                    // Save token to OS credential store
                    match reader_core::sharing::keystore::store_github_token(&token) {
                        Ok(()) => {
                            self.push_feedback_log("[GitHub] token saved to OS credential store")
                        }
                        Err(e) => self.push_feedback_log(format!(
                            "[GitHub] WARN: failed to save token to keystore: {}",
                            e
                        )),
                    }
                    self.github_token = Some(token);
                    self.github_username = Some(username);
                    self.github_oauth_polling = false;
                    self.github_device_code = None;
                    self.github_user_code = None;
                    self.github_oauth_status.clear();
                }
                Ok(PollResult::Pending) => {}
                Ok(PollResult::SlowDown) => {
                    self.push_feedback_log(format!(
                        "[GitHub] slow_down, increasing interval to {}s",
                        self.github_oauth_interval + 5
                    ));
                    self.github_oauth_interval += 5;
                }
                Ok(PollResult::Expired) => {
                    self.push_feedback_log("[GitHub] token poll: expired");
                    self.github_oauth_polling = false;
                    self.github_oauth_status = self.i18n.t("csc.code_expired").to_string();
                }
                Ok(PollResult::Denied) => {
                    self.push_feedback_log("[GitHub] token poll: access denied");
                    self.github_oauth_polling = false;
                    self.github_oauth_status = self.i18n.t("csc.access_denied").to_string();
                }
                Ok(PollResult::Error(e)) => {
                    self.push_feedback_log(format!("[GitHub] token poll error: {}", e));
                    self.github_oauth_status = format!("Error: {}", e);
                }
                Err(e) => {
                    self.push_feedback_log(format!("[GitHub] token poll channel error: {}", e));
                    self.github_oauth_status = format!("Error: {}", e);
                }
            }
        }
    }
}
