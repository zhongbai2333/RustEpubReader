//! Shared account entry for library, reader settings and correction contributions.
use crate::app::ReaderApp;
use eframe::egui;

#[derive(Clone, Default)]
struct LoginFeedback {
    code: Option<String>,
    copied_until: Option<std::time::Instant>,
    opened: bool,
    error: Option<&'static str>,
}

#[allow(clippy::too_many_arguments)]
fn device_login(
    ui: &mut egui::Ui,
    i18n: &reader_core::i18n::I18n,
    dark: bool,
    code: Option<&str>,
    repository: bool,
    status: &str,
    preview: bool,
) -> bool {
    let id = ui.id().with("github_login_feedback");
    let mut feedback = ui
        .ctx()
        .data(|d| d.get_temp::<LoginFeedback>(id))
        .unwrap_or_default();
    if feedback.code.as_deref() != code {
        feedback = LoginFeedback {
            code: code.map(str::to_owned),
            ..Default::default()
        };
    }
    let copied = feedback
        .copied_until
        .is_some_and(|until| until > std::time::Instant::now());
    if copied {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }
    let palette = super::design::Palette::new(dark);
    let mut cancel = false;
    egui::Frame::default()
        .fill(palette.surface)
        .corner_radius(12)
        .inner_margin(20)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                egui::RichText::new(i18n.t("account.auth_title"))
                    .strong()
                    .size(19.),
            );
            ui.add_space(4.);
            ui.label(egui::RichText::new(i18n.t("account.auth_instruction")).color(palette.muted));
            ui.add_space(16.);
            egui::Frame::default()
                .fill(palette.raised)
                .corner_radius(8)
                .inner_margin(12)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        egui::RichText::new(i18n.t("sync.auth_code_label"))
                            .size(13.)
                            .color(palette.muted),
                    );
                    ui.add_space(4.);
                    let copy_button = |ui: &mut egui::Ui| {
                        ui.add_enabled(
                            code.is_some(),
                            egui::Button::new(i18n.t(if copied {
                                "sync.code_copied"
                            } else {
                                "csc.copy_code"
                            })),
                        )
                        .clicked()
                    };
                    let copy = if ui.available_width() < 330. {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(code.unwrap_or("••••-••••"))
                                    .monospace()
                                    .size(28.),
                            )
                            .selectable(code.is_some()),
                        );
                        copy_button(ui)
                    } else {
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(code.unwrap_or("••••-••••"))
                                        .monospace()
                                        .size(28.),
                                )
                                .selectable(code.is_some()),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                copy_button,
                            )
                            .inner
                        })
                        .inner
                    };
                    if copy {
                        ui.ctx().copy_text(code.unwrap_or_default().to_owned());
                        feedback.copied_until =
                            Some(std::time::Instant::now() + std::time::Duration::from_secs(2));
                        feedback.error = None;
                    }
                });
            ui.add_space(16.);
            if ui
                .add_enabled(
                    code.is_some(),
                    super::design::primary(i18n.t(if feedback.opened {
                        "sync.reopen_github"
                    } else {
                        "account.open_auth"
                    }))
                    .min_size(egui::vec2(220_f32.min(ui.available_width()), 40.)),
                )
                .clicked()
            {
                let result = if preview {
                    Ok(())
                } else {
                    open::that("https://github.com/login/device")
                };
                if result.is_ok() {
                    feedback.opened = true;
                    feedback.error = None;
                } else {
                    feedback.error = Some("sync.auth_browser_failed");
                }
            }
            ui.add_space(12.);
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(16.));
                ui.label(
                    egui::RichText::new(i18n.t(if code.is_some() {
                        "account.waiting"
                    } else {
                        "csc.requesting_code"
                    }))
                    .size(13.)
                    .color(palette.muted),
                );
            });
            if let Some(error) = feedback.error {
                ui.colored_label(egui::Color32::from_rgb(210, 80, 70), i18n.t(error));
                ui.hyperlink_to("github.com/login/device", "https://github.com/login/device");
            } else if feedback.opened {
                ui.label(
                    egui::RichText::new(i18n.t("sync.browser_opened"))
                        .size(13.)
                        .color(palette.muted),
                );
            }
            if !status.is_empty() && status != i18n.t("csc.requesting_code") {
                ui.label(i18n.message(status));
            }
            ui.add_space(4.);
            cancel = ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(i18n.t("account.cancel_auth")).color(palette.accent),
                    )
                    .frame(false),
                )
                .clicked();
        });
    ui.add_space(12.);
    ui.label(
        egui::RichText::new(i18n.t(if repository {
            "sync.auth_purpose"
        } else {
            "account.basic_permissions"
        }))
        .size(13.)
        .color(palette.muted),
    );
    if repository {
        sync_permission_details(ui, i18n);
    }
    if cancel {
        feedback = LoginFeedback::default();
    }
    ui.ctx().data_mut(|d| d.insert_temp(id, feedback));
    cancel
}

impl ReaderApp {
    pub fn render_github_account_content(&mut self, ui: &mut egui::Ui) {
        #[cfg(debug_assertions)]
        let preview = std::env::args().any(|a| a == "--ui-preview")
            && std::env::args().any(|a| a == "--github-login-preview")
            && !ui.ctx().data(|d| {
                d.get_temp::<bool>(egui::Id::new("github_login_preview_closed"))
                    .unwrap_or(false)
            });
        #[cfg(not(debug_assertions))]
        let preview = false;
        if self.github_auth_busy() || preview {
            let code = if preview {
                Some("DEMO-0000")
            } else {
                self.github_user_code.as_deref()
            };
            if device_login(
                ui,
                &self.i18n,
                self.dark_mode,
                code,
                preview || self.github_oauth_scope == "repo",
                &self.github_oauth_status,
                preview,
            ) {
                self.github_cancel_login();
                if preview {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::new("github_login_preview_closed"), true)
                    });
                }
            }
            return;
        }
        ui.ctx()
            .data_mut(|d| d.remove::<LoginFeedback>(ui.id().with("github_login_feedback")));
        let p = super::design::Palette::new(self.dark_mode);
        ui.strong(self.i18n.t("sync.account_section"));
        super::design::block_gap(ui);
        egui::Frame::default()
            .fill(p.surface)
            .corner_radius(8.)
            .inner_margin(16)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                sync_value_row(
                    ui,
                    self.i18n.t("sync.account"),
                    self.github_username
                        .as_deref()
                        .filter(|_| self.github_token.is_some())
                        .unwrap_or(self.i18n.t("account.signed_out")),
                );
                sync_value_row(ui, self.i18n.t("sync.repo"), "EpubReaderInfo");
                ui.allocate_ui_with_layout(
                    egui::vec2(
                        ui.available_width(),
                        super::design::Geometry::CONTROL_HEIGHT,
                    ),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        if self.github_token.is_some()
                            && ui.button(self.i18n.t("csc.logout")).clicked()
                        {
                            self.github_logout();
                        }
                        if ui
                            .button(self.i18n.t(if self.github_token.is_some() {
                                "account.reauthorize"
                            } else {
                                "account.login"
                            }))
                            .clicked()
                        {
                            self.github_start_repository_login();
                        }
                    },
                );
                if !self.github_oauth_status.is_empty() {
                    ui.colored_label(
                        egui::Color32::from_rgb(200, 80, 60),
                        &self.github_oauth_status,
                    );
                }
            });
        egui::Frame::default()
            .inner_margin(egui::Margin::symmetric(16, 0))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(self.i18n.t("sync.scope_note"))
                        .small()
                        .color(p.muted),
                );
                sync_permission_details(ui, &self.i18n);
            });
        super::design::section_gap(ui);
        ui.strong(self.i18n.t("sync.state_section"));
        super::design::block_gap(ui);
        egui::Frame::default()
            .fill(p.surface)
            .corner_radius(8.)
            .inner_margin(16)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.render_github_sync_controls(ui);
            });
        if self.github_token.is_none() {
            egui::Frame::default()
                .inner_margin(egui::Margin::symmetric(16, 0))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(self.i18n.t("sync.login_required"))
                            .small()
                            .color(p.muted),
                    );
                });
        }
        super::design::section_gap(ui);
        ui.strong(self.i18n.t("sync.contents"));
        super::design::block_gap(ui);
        egui::Frame::default()
            .fill(p.surface)
            .corner_radius(8.)
            .inner_margin(16)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(self.i18n.t("sync.content_note"));
                ui.label(
                    egui::RichText::new(self.i18n.t("sync.content_limit"))
                        .small()
                        .color(p.muted),
                );
                ui.collapsing(self.i18n.t("cloud.details"), |ui| {
                    ui.small(self.i18n.t("cloud.limits"));
                });
            });
    }
}

pub(super) fn sync_value_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(value)
                    .color(super::design::Palette::new(ui.visuals().dark_mode).muted),
            );
        });
    });
}
fn sync_permission_details(ui: &mut egui::Ui, i18n: &reader_core::i18n::I18n) {
    ui.collapsing(i18n.t("sync.permission_details"), |ui| {
        ui.small(i18n.t("sync.scope_detail"));
    });
}
