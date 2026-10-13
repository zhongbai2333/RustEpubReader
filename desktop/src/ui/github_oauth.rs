//! GitHub OAuth authentication flow and UI integration.
use crate::app::ReaderApp;

/// GitHub OAuth App Client ID.
const GITHUB_CLIENT_ID: &str = "Ov23liG7iXNfGOTAXXxx";

pub enum PollResult {
    Success {
        token: String,
        username: String,
        repo_authorized: bool,
    },
    Pending,
    SlowDown,
    Expired,
    Denied,
    Error(String),
}

impl ReaderApp {
    pub fn github_start_repository_login(&mut self) {
        if self.github_auth_busy() {
            return;
        }
        self.github_enable_sync_after_login = false;
        self.github_oauth_scope = "repo".into();
        self.github_start_device_flow_with_scope();
    }
    pub fn github_start_sync_device_flow(&mut self) {
        if self.github_auth_busy() {
            return;
        }
        self.github_enable_sync_after_login = true;
        self.github_oauth_scope = "repo".into();
        self.github_start_device_flow_with_scope();
    }

    pub fn github_auth_busy(&self) -> bool {
        self.github_pending_device_code.is_some()
            || self.github_pending_token_poll.is_some()
            || self.github_oauth_polling
    }

    pub fn github_cancel_login(&mut self) {
        // Dropping result channels prevents a cancelled request from replacing credentials.
        self.github_pending_device_code = None;
        self.github_pending_token_poll = None;
        self.github_oauth_polling = false;
        self.github_device_code = None;
        self.github_user_code = None;
        self.github_enable_sync_after_login = false;
        self.github_oauth_status.clear();
    }

    pub fn github_logout(&mut self) {
        self.github_cancel_login();
        self.cloud_set_enabled(false);
        reader_core::sharing::keystore::delete_github_token();
        self.github_token = None;
        self.github_username = None;
        self.github_repo_authorized = false;
        self.cloud_sync.clear_account();
    }

    fn github_start_device_flow_with_scope(&mut self) {
        if self.github_pending_device_code.is_some()
            || self.github_pending_token_poll.is_some()
            || self.github_oauth_polling
        {
            return;
        }
        let scope = self.github_oauth_scope.clone();
        self.push_feedback_log("[GitHub] start_device_flow: requesting device code");
        self.github_oauth_polling = false;
        self.github_oauth_status = self.i18n.t("csc.requesting_code").to_string();

        let ctx = self.last_egui_ctx.clone();
        let logs = self.feedback_logs.clone();
        let (tx, rx) = std::sync::mpsc::channel::<crate::app::DeviceCodeResult>();

        std::thread::spawn(move || {
            let result: crate::app::DeviceCodeResult = (|| {
                let client = reqwest::blocking::Client::builder()
                    .timeout(std::time::Duration::from_secs(30))
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                    .map_err(|_| "Cannot initialize GitHub login")?;
                crate::app::dbg_log(&logs, "[GitHub] POST https://github.com/login/device/code");
                let resp = client
                    .post("https://github.com/login/device/code")
                    .header("Accept", "application/json")
                    .form(&[("client_id", GITHUB_CLIENT_ID), ("scope", scope.as_str())])
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
                        crate::app::dbg_log(&logs, "[GitHub] missing device_code in response");
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
                    self.github_oauth_polling = false;
                    self.github_oauth_status =
                        format!("{} {}", self.i18n.t("account.auth_failed"), e);
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
                let client = reqwest::blocking::Client::builder()
                    .timeout(std::time::Duration::from_secs(30))
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                    .map_err(|_| "Cannot initialize GitHub login")?;
                crate::app::dbg_log(&logs, "[GitHub] polling token exchange...");
                let resp = client
                    .post("https://github.com/login/oauth/access_token")
                    .header("Accept", "application/json")
                    .form(&[
                        ("client_id", GITHUB_CLIENT_ID),
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
                    if !user_resp.status().is_success() {
                        return Err("Cannot verify GitHub account".into());
                    }
                    let repo_authorized = crate::github_sync::has_repo_scope(
                        user_resp
                            .headers()
                            .get("x-oauth-scopes")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or(""),
                    );
                    let user: serde_json::Value = user_resp.json().map_err(|e| e.to_string())?;
                    let username = user["login"]
                        .as_str()
                        .ok_or("Missing GitHub account name")?
                        .to_string();
                    crate::app::dbg_log(
                        &logs,
                        format!("[GitHub] login success: user={}", username),
                    );
                    return Ok(PollResult::Success {
                        token: token.to_string(),
                        username,
                        repo_authorized,
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
                Ok(PollResult::Success {
                    token,
                    username,
                    repo_authorized,
                }) => {
                    self.push_feedback_log(format!("[GitHub] OAuth success: user={}", username));
                    // Save token to OS credential store
                    if let Err(error) = reader_core::sharing::keystore::store_github_token(&token) {
                        self.push_feedback_log(format!(
                            "[GitHub] cannot save credentials: {}",
                            error
                        ));
                        self.github_cancel_login();
                        self.github_oauth_status = self.i18n.t("account.auth_failed").into();
                        return;
                    }
                    self.cloud_sync.clear_account();
                    self.cloud_sync.needs_auth = self.github_sync_enabled && !repo_authorized;
                    self.github_repo_authorized = repo_authorized;
                    self.github_token = Some(token);
                    self.github_username = Some(username);
                    self.github_oauth_polling = false;
                    self.github_device_code = None;
                    self.github_user_code = None;
                    self.github_oauth_status.clear();
                    let enable = std::mem::take(&mut self.github_enable_sync_after_login);
                    if !enable {
                        self.cloud_set_enabled(false);
                    }
                    if enable && repo_authorized {
                        self.cloud_set_enabled(true);
                        self.cloud_start_sync();
                    } else if enable {
                        self.cloud_sync.needs_auth = true;
                        self.github_oauth_status = self.i18n.t("account.scope_missing").into();
                    }
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
                    self.github_oauth_polling = false;
                    self.github_oauth_status =
                        format!("{} {}", self.i18n.t("account.auth_failed"), e);
                }
                Err(e) => {
                    self.push_feedback_log(format!("[GitHub] token poll channel error: {}", e));
                    self.github_oauth_polling = false;
                    self.github_oauth_status =
                        format!("{} {}", self.i18n.t("account.auth_failed"), e);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelling_authorization_discards_late_replies_and_keeps_previous_session() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let temp = tempfile::tempdir().unwrap();
        let mut app = ReaderApp::from_data_dir(temp.path().to_string_lossy().into_owned());
        app.github_token = Some("test-old-session".into());
        app.github_username = Some("test-user".into());
        app.github_sync_enabled = false;
        app.github_enable_sync_after_login = true;
        app.github_oauth_polling = true;
        let (device_tx, device_rx) = std::sync::mpsc::channel();
        let (token_tx, token_rx) = std::sync::mpsc::channel();
        app.github_pending_device_code = Some(device_rx);
        app.github_pending_token_poll = Some(token_rx);
        app.github_cancel_login();
        assert!(!app.github_sync_enabled);
        assert!(!app.github_enable_sync_after_login);
        assert!(!app.github_auth_busy());
        assert_eq!(app.github_username.as_deref(), Some("test-user"));
        assert_eq!(app.github_token.as_deref(), Some("test-old-session"));
        assert!(device_tx.send(Err("late device code".into())).is_err());
        assert!(token_tx.send(Ok(PollResult::Pending)).is_err());
    }

    #[test]
    fn second_login_action_cannot_change_inflight_authorization_intent() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let temp = tempfile::tempdir().unwrap();
        let mut app = ReaderApp::from_data_dir(temp.path().to_string_lossy().into_owned());
        app.github_oauth_polling = true;
        app.github_oauth_scope = "repo".into();
        app.github_enable_sync_after_login = true;
        app.github_start_repository_login();
        assert_eq!(app.github_oauth_scope, "repo");
        assert!(app.github_enable_sync_after_login);
        app.github_oauth_scope = "public_repo".into();
        app.github_enable_sync_after_login = false;
        app.github_start_sync_device_flow();
        assert_eq!(app.github_oauth_scope, "public_repo");
        assert!(!app.github_enable_sync_after_login);
    }
    #[test]
    fn ordinary_login_does_not_enable_sync_even_if_token_has_repo_access() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let temp = tempfile::tempdir().unwrap();
        let mut app = ReaderApp::from_data_dir(temp.path().to_string_lossy().into_owned());
        // A saved enabled flag from an expired or different account must not start uploads.
        app.github_sync_enabled = true;
        let (tx, rx) = std::sync::mpsc::channel();
        app.github_pending_token_poll = Some(rx);
        tx.send(Ok(PollResult::Success {
            token: "test-token".into(),
            username: "test-user".into(),
            repo_authorized: true,
        }))
        .unwrap();
        app.github_poll_token_result();
        assert_eq!(app.github_username.as_deref(), Some("test-user"));
        assert!(app.github_repo_authorized);
        assert!(!app.github_sync_enabled);
    }

    #[test]
    fn missing_private_scope_cannot_enable_sync_after_login() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let temp = tempfile::tempdir().unwrap();
        let mut app = ReaderApp::from_data_dir(temp.path().to_string_lossy().into_owned());
        app.github_enable_sync_after_login = true;
        let (tx, rx) = std::sync::mpsc::channel();
        app.github_pending_token_poll = Some(rx);
        tx.send(Ok(PollResult::Success {
            token: "test-token".into(),
            username: "test-user".into(),
            repo_authorized: false,
        }))
        .unwrap();
        app.github_poll_token_result();
        assert!(!app.github_sync_enabled);
        assert!(!app.github_repo_authorized);
        assert!(app.cloud_sync.needs_auth);
        assert!(!app.github_auth_busy());
    }
}
