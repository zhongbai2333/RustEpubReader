use crate::app::ReaderApp;
use eframe::egui;

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
                    "epub",
                    "MIT",
                    "https://crates.io/crates/epub",
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
