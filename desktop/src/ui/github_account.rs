//! Shared account entry for library, reader settings and correction contributions.
use crate::app::ReaderApp;
use eframe::egui;

impl ReaderApp {
    pub fn render_github_account_content(&mut self, ui: &mut egui::Ui) {
        if self.github_auth_busy() {
            ui.strong(self.i18n.t("account.auth_title"));
            ui.label(self.i18n.t("account.auth_instruction"));
            ui.small(self.i18n.t(if self.github_oauth_scope == "repo" {
                "cloud.permissions"
            } else {
                "account.basic_permissions"
            }));
            if self.github_oauth_scope == "repo" {
                sync_permission_details(ui, &self.i18n);
            }
            ui.add_space(8.0);
            if let Some(code) = &self.github_user_code {
                ui.label(egui::RichText::new(code).monospace().size(24.0));
                if ui.button(self.i18n.t("csc.copy_code")).clicked() {
                    ui.ctx().copy_text(code.clone());
                }
                ui.hyperlink_to(
                    self.i18n.t("account.open_auth"),
                    "https://github.com/login/device",
                );
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(self.i18n.t("account.waiting"));
                });
            } else {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(self.i18n.t("csc.requesting_code"));
                });
            }
            if !self.github_oauth_status.is_empty() {
                ui.label(self.i18n.message(&self.github_oauth_status));
            }
            if ui.button(self.i18n.t("account.cancel_auth")).clicked() {
                self.github_cancel_login();
            }
            return;
        }
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
