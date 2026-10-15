//! Application sharing entry and explicit system export.
use crate::app::ReaderApp;
use eframe::egui;
use reader_core::export::ExportMode;

impl ReaderApp {
    pub fn render_export_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_export_dialog {
            return;
        }
        self.book_share.initialize(&self.data_dir);
        let paths = if !self.share_paths.is_empty() {
            self.share_paths.clone()
        } else {
            self.export_library_path
                .clone()
                .or(self.book_path.clone())
                .into_iter()
                .collect::<Vec<_>>()
        };
        let title = paths
            .first()
            .and_then(|p| {
                self.library
                    .books
                    .iter()
                    .find(|b| &b.path == p)
                    .map(|b| b.title.clone())
            })
            .or_else(|| self.book_config.as_ref().map(|b| b.title.clone()))
            .unwrap_or_default();
        let cover = paths
            .first()
            .and_then(|p| self.cover_textures.get(p))
            .and_then(|t| t.clone());
        let palette = super::design::Palette::new(ctx.style().visuals.dark_mode);
        let width = if self.book_share.stage == 1
            || (self.book_share.stage == 2 && self.book_share.shows_qr())
        {
            480.
        } else {
            400.
        };
        let mut close = false;
        let mut completed = false;
        egui::Window::new(self.i18n.t("book.share_title"))
            .id(egui::Id::new("book_share_native"))
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .auto_sized()
            .frame(
                egui::Frame::new()
                    .fill(palette.surface)
                    .stroke(egui::Stroke::new(1_f32, palette.border))
                    .corner_radius(16)
                    .inner_margin(20),
            )
            .anchor(egui::Align2::CENTER_CENTER, [0., 0.])
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.horizontal(|ui| {
                    if self.book_share.stage != 0
                        && super::design::icon_button(
                            ui,
                            super::design::Icon::Back,
                            self.i18n.t("settings.back"),
                            false,
                        )
                        .clicked()
                    {
                        self.book_share.back(&self.data_dir);
                    }
                    ui.label(
                        egui::RichText::new(self.i18n.t(match self.book_share.stage {
                            1 => "share_ui.nearby",
                            2 => "transfer.github",
                            3 => "share_ui.export_file",
                            _ => "book.share_title",
                        }))
                        .size(18.)
                        .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        close = super::design::close_button(ui, self.i18n.t("settings.close"))
                            .clicked();
                    });
                });
                ui.add_space(14.);
                ui.horizontal(|ui| {
                    if let Some(texture) = &cover {
                        ui.add(
                            egui::Image::new((texture.id(), egui::vec2(32., 48.))).corner_radius(4),
                        );
                    } else {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(32., 48.), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 4., palette.raised);
                        let book =
                            egui::Rect::from_center_size(rect.center(), egui::vec2(16., 20.));
                        ui.painter().rect_stroke(
                            book,
                            2.,
                            egui::Stroke::new(1.4_f32, palette.muted),
                            egui::StrokeKind::Inside,
                        );
                        ui.painter().line_segment(
                            [
                                book.left_top() + egui::vec2(4., 1.),
                                book.left_bottom() + egui::vec2(4., -1.),
                            ],
                            egui::Stroke::new(1_f32, palette.muted),
                        );
                    }
                    ui.add_space(4.);
                    ui.vertical(|ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new(&title).size(16.).strong())
                                .truncate(),
                        );
                        ui.label(
                            egui::RichText::new(
                                self.i18n.tf1("manage.count", &paths.len().to_string()),
                            )
                            .size(12.)
                            .color(palette.muted),
                        );
                    });
                });
                ui.add_space(12.);
                if self.book_share.stage == 3 {
                    egui::ScrollArea::vertical()
                        .max_height((ctx.screen_rect().height() - 240.).max(100.))
                        .show(ui, |ui| {
                            ui.spacing_mut().interact_size.y = 28.;
                            if paths.len() == 1 {
                                for (mode, key) in [
                                    (ExportMode::Original, "export.mode_original"),
                                    (ExportMode::WithCorrections, "export.mode_corrections"),
                                    (ExportMode::WithAnnotations, "export.mode_annotations"),
                                    (ExportMode::Full, "export.mode_full"),
                                ] {
                                    ui.radio_value(&mut self.share_mode, mode, self.i18n.t(key));
                                }
                            } else {
                                ui.label(self.i18n.t("export.mode_original"));
                            }
                        });
                    ui.add_space(12.);
                    if ui
                        .add_enabled(
                            !paths.is_empty(),
                            super::design::primary(self.i18n.t("book.export")),
                        )
                        .clicked()
                    {
                        completed = if paths.len() > 1 {
                            self.export_selected_books()
                        } else {
                            self.do_export(self.share_mode)
                        };
                    }
                } else if self.book_share.stage == 1
                    || (self.book_share.stage == 2 && self.book_share.shows_qr())
                {
                    self.render_book_share_actions(ui, &paths);
                } else {
                    egui::ScrollArea::vertical()
                        .max_height((ctx.screen_rect().height() - 170.).max(220.))
                        .show(ui, |ui| {
                            self.render_book_share_actions(ui, &paths);
                            if paths.len() > 1 {
                                ui.collapsing(self.i18n.t("share_ui.selected"), |ui| {
                                    for path in &paths {
                                        if let Some(book) =
                                            self.library.books.iter().find(|b| &b.path == path)
                                        {
                                            ui.label(&book.title);
                                        }
                                    }
                                });
                            }
                        });
                }
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.book_share.stage == 0 {
                close = true
            } else {
                self.book_share.back(&self.data_dir);
            }
        }
        if close || completed {
            self.book_share.cancel(&self.data_dir);
            self.book_share.stage = 0;
            self.show_export_dialog = false;
            self.export_library_path = None;
            self.share_paths.clear();
        }
    }

    fn do_export(&mut self, mode: ExportMode) -> bool {
        let Some(book_path) = self
            .share_paths
            .first()
            .cloned()
            .or(self.export_library_path.clone())
            .or(self.book_path.clone())
        else {
            return false;
        };
        // Library export must load the selected book's configuration, not the open reader's.
        let entry = self.library.books.iter().find(|e| e.path == book_path);
        let config = entry
            .and_then(|e| reader_core::library::Library::read_book_config(&self.data_dir, &e.id))
            .or_else(|| {
                self.book_config
                    .clone()
                    .filter(|c| c.epub_path == book_path)
            });
        let Some(config) = config else {
            self.error_msg = Some(
                self.i18n
                    .tf1("book.export_failed", "Cannot read book configuration"),
            );
            return false;
        };
        let Some(path) = rfd::FileDialog::new()
            .set_title(self.i18n.t("export.title"))
            .set_file_name(reader_core::export::book_filename(&config.title))
            .add_filter("EPUB", &["epub"])
            .save_file()
        else {
            return false;
        };
        let path = if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("epub"))
        {
            path
        } else {
            path.with_extension("epub")
        };
        if std::fs::canonicalize(&path)
            .ok()
            .is_some_and(|p| std::fs::canonicalize(&book_path).ok().as_ref() == Some(&p))
        {
            self.error_msg = Some(self.i18n.tf1(
                "book.export_failed",
                "Choose a destination outside the library source file",
            ));
            return false;
        }
        match reader_core::export::export_book(&book_path, &path.to_string_lossy(), &config, mode) {
            Ok(()) => true,
            Err(error) => {
                self.error_msg = Some(self.i18n.tf1("book.export_failed", &error));
                false
            }
        }
    }

    pub fn render_rename_dialog(&mut self, ctx: &egui::Context) {
        let Some((id, mut title)) = self.rename_book.take() else {
            return;
        };
        let mut open = true;
        let mut finished = false;
        egui::Window::new(self.i18n.t("book.rename"))
            .id(egui::Id::new("rename_book"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(360.)
            .anchor(egui::Align2::CENTER_CENTER, [0., 0.])
            .show(ctx, |ui| {
                ui.set_max_width(360.);
                ui.label(self.i18n.t("book.rename_description"));
                ui.add_space(8.);
                ui.add(
                    egui::TextEdit::singleline(&mut title)
                        .desired_width(340.)
                        .hint_text(self.i18n.t("book.rename_hint")),
                );
                ui.horizontal(|ui| {
                    if ui.button(self.i18n.t("settings.cancel")).clicked() {
                        finished = true;
                    }
                    if ui
                        .add_enabled(
                            !title.trim().is_empty(),
                            super::design::primary(
                                egui::RichText::new(self.i18n.t("error.ok"))
                                    .color(egui::Color32::WHITE),
                            ),
                        )
                        .clicked()
                    {
                        match self.library.rename_book(&self.data_dir, &id, &title) {
                            Ok(entry) => {
                                if let Some(cfg) = self.book_config.as_mut().filter(|c| c.id == id)
                                {
                                    cfg.title = entry.title.clone();
                                }
                                if self.book_path.as_deref() == Some(&entry.path) {
                                    if let Some(book) = &mut self.book {
                                        book.title = entry.title;
                                    }
                                }
                                finished = true;
                            }
                            Err(error) => self.error_msg = Some(error),
                        }
                    }
                });
            });
        if open && !finished {
            self.rename_book = Some((id, title));
        }
    }
}
