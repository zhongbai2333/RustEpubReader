use super::design::{self, Icon, Palette};
use crate::app::ReaderApp;
use eframe::egui;
use reader_core::epub::EpubBook;
impl ReaderApp {
    pub fn render_library(&mut self, ui: &mut egui::Ui) {
        let mut action_open_path = None;
        let mut action_remove_path = None;
        let mut action_export_path = None;
        let mut action_open_dialog = false;
        let p = Palette::new(self.dark_mode);
        self.library_selected
            .retain(|id| self.library.books.iter().any(|e| &e.id == id));
        let mut sorted = self.library.sorted_indices_by_recent();
        let recent = sorted
            .iter()
            .copied()
            .find(|&i| self.library.books[i].last_opened > 0);
        if self.library_sort_title {
            sorted.sort_by_key(|&i| self.library.books[i].title.to_lowercase());
        }
        sorted.retain(|&i| {
            self.library.books[i]
                .title
                .to_lowercase()
                .contains(&self.library_query.to_lowercase())
        });
        let ctx = ui.ctx().clone();
        let mut loaded = 0;
        for &idx in &sorted {
            let path = self.library.books[idx].path.clone();
            if let std::collections::hash_map::Entry::Vacant(e) =
                self.cover_textures.entry(path.clone())
            {
                if loaded >= 2 {
                    ctx.request_repaint();
                    break;
                }
                loaded += 1;
                let texture = (|| {
                    let book = EpubBook::open(&path).ok()?;
                    let img = image::load_from_memory(book.cover_data.as_ref()?)
                        .ok()?
                        .to_rgba8();
                    Some(ctx.load_texture(
                        format!("cover_{path}"),
                        egui::ColorImage::from_rgba_unmultiplied(
                            [img.width() as usize, img.height() as usize],
                            img.as_raw(),
                        ),
                        egui::TextureOptions::LINEAR,
                    ))
                })();
                e.insert(texture);
            }
        }
        egui::Frame::default()
            .inner_margin(design::Geometry::PAGE_INSET)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.heading(self.i18n.t("library.title"));
                            if self.cloud_sync.is_syncing() {
                                ui.add_space(4.0);
                                ui.add(egui::Spinner::new().size(14.0));
                                ui.label(
                                    egui::RichText::new(self.i18n.t("cloud.syncing_indicator"))
                                        .small()
                                        .color(p.muted),
                                );
                            }
                        });
                        ui.label(
                            egui::RichText::new(
                                self.i18n
                                    .tf1("ui.library_count", &self.library.books.len().to_string()),
                            )
                            .color(p.muted),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(self.i18n.t(if self.library_manage {
                                "manage.done"
                            } else {
                                "manage.books"
                            }))
                            .clicked()
                        {
                            self.library_manage = !self.library_manage;
                            if !self.library_manage {
                                self.library_selected.clear();
                            }
                        }
                        if ui
                            .add(design::primary(
                                egui::RichText::new(self.i18n.t("ui.import"))
                                    .color(egui::Color32::WHITE),
                            ))
                            .clicked()
                        {
                            action_open_dialog = true;
                        }
                    });
                });
                design::section_gap(ui);
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [
                            ((ui.available_width() - 168.).max(100.)),
                            design::Geometry::CONTROL_HEIGHT,
                        ],
                        super::design::text_edit(&mut self.library_query)
                            .hint_text(self.i18n.t("ui.search_books"))
                            .vertical_align(egui::Align::Center)
                            .desired_width((ui.available_width() - 168.).max(100.)),
                    );
                    egui::ComboBox::from_id_salt("library_sort")
                        .selected_text(self.i18n.t(if self.library_sort_title {
                            "ui.sort_title"
                        } else {
                            "ui.sort_recent"
                        }))
                        .show_ui(ui, |ui| {
                            super::design::menu_style(ui);
                            ui.selectable_value(
                                &mut self.library_sort_title,
                                false,
                                self.i18n.t("ui.sort_recent"),
                            );
                            ui.selectable_value(
                                &mut self.library_sort_title,
                                true,
                                self.i18n.t("ui.sort_title"),
                            );
                        });
                });
                design::section_gap(ui);
                if self.library_manage {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(
                            self.i18n
                                .tf1("manage.count", &self.library_selected.len().to_string()),
                        );
                        if ui.button(self.i18n.t("manage.all")).clicked() {
                            self.library_selected
                                .extend(sorted.iter().map(|&i| self.library.books[i].id.clone()));
                        }
                        if ui.button(self.i18n.t("manage.clear")).clicked() {
                            self.library_selected.clear();
                        }
                        if ui
                            .add_enabled(
                                !self.library_selected.is_empty(),
                                egui::Button::new(self.i18n.t("manage.export")),
                            )
                            .clicked()
                        {
                            self.export_selected_books();
                        }
                        if ui
                            .add_enabled(
                                !self.library_selected.is_empty(),
                                egui::Button::new(self.i18n.t("manage.delete")),
                            )
                            .clicked()
                        {
                            self.library_confirm_delete = true;
                        }
                    });
                    design::block_gap(ui);
                }
                let mut content_clip = ui.clip_rect();
                let scroll = egui::ScrollArea::vertical().show(ui, |ui| {
                    content_clip = ui.clip_rect();
                    if self.library_query.is_empty() && !self.library_manage {
                        if let Some(idx) = recent {
                            let e = &self.library.books[idx];
                            egui::Frame::default()
                                .fill(p.surface)
                                .stroke(egui::Stroke::new(1.0_f32, p.border))
                                .corner_radius(super::design::Geometry::RADIUS)
                                .inner_margin(design::Geometry::CARD_INSET)
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        ui.allocate_ui_with_layout(
                                            egui::vec2((ui.available_width() - 140.).max(80.), 64.),
                                            egui::Layout::top_down(egui::Align::LEFT),
                                            |ui| {
                                                ui.small(self.i18n.t("ui.recent"));
                                                ui.label(
                                                    egui::RichText::new(&e.title)
                                                        .size(19.)
                                                        .strong(),
                                                );
                                                ui.label(
                                                    egui::RichText::new(
                                                        e.last_chapter_title
                                                            .clone()
                                                            .unwrap_or_else(|| {
                                                                self.i18n.tf1(
                                                                    "ui.chapter",
                                                                    &(e.last_chapter + 1)
                                                                        .to_string(),
                                                                )
                                                            }),
                                                    )
                                                    .color(p.muted),
                                                );
                                            },
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui
                                                    .add(design::primary(
                                                        egui::RichText::new(
                                                            self.i18n.t("ui.continue"),
                                                        )
                                                        .color(egui::Color32::WHITE),
                                                    ))
                                                    .clicked()
                                                {
                                                    action_open_path =
                                                        Some((e.path.clone(), e.last_chapter));
                                                }
                                            },
                                        );
                                    });
                                });
                            design::section_gap(ui);
                        }
                    }
                    if sorted.is_empty() {
                        ui.add_space(56.);
                        ui.vertical_centered(|ui| {
                            ui.heading(self.i18n.t(if self.library.books.is_empty() {
                                "library.empty"
                            } else {
                                "ui.no_results"
                            }));
                            if self.library.books.is_empty() {
                                ui.label(self.i18n.t("ui.empty_hint"));
                            }
                            if !self.library_query.is_empty()
                                && ui.button(self.i18n.t("ui.clear_search")).clicked()
                            {
                                self.library_query.clear();
                            }
                        });
                    } else {
                        ui.strong(self.i18n.t("ui.all_books"));
                        design::block_gap(ui);
                        let width = (if ui.available_width() < 400. {
                            148.0_f32
                        } else {
                            160.0_f32
                        })
                        .min(ui.available_width());
                        let cols = ((ui.available_width() + design::Geometry::BLOCK_GAP)
                            / (width + design::Geometry::BLOCK_GAP))
                            .floor()
                            .max(1.) as usize;
                        for row in sorted.chunks(cols) {
                            ui.horizontal(|ui| {
                                for &idx in row {
                                    let e = &self.library.books[idx];
                                    ui.push_id(&e.path, |ui| {
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(width, width * 1.5 + 72.),
                                            egui::Layout::top_down(egui::Align::LEFT),
                                            |ui| {
                                                let (rect, response) = ui.allocate_exact_size(
                                                    egui::vec2(width, width * 1.5),
                                                    egui::Sense::click(),
                                                );
                                                response.widget_info(|| {
                                                    egui::WidgetInfo::labeled(
                                                        egui::WidgetType::Button,
                                                        true,
                                                        &e.title,
                                                    )
                                                });
                                                if let Some(tex) = self
                                                    .cover_textures
                                                    .get(&e.path)
                                                    .and_then(|t| t.as_ref())
                                                {
                                                    let aspect =
                                                        tex.size_vec2().x / tex.size_vec2().y;
                                                    let target = rect.width() / rect.height();
                                                    let fraction = if aspect > target {
                                                        egui::vec2(target / aspect, 1.)
                                                    } else {
                                                        egui::vec2(1., aspect / target)
                                                    };
                                                    let uv = egui::Rect::from_center_size(
                                                        egui::pos2(0.5, 0.5),
                                                        fraction,
                                                    );
                                                    egui::Image::new(tex)
                                                        .uv(uv)
                                                        .corner_radius(
                                                            super::design::Geometry::RADIUS,
                                                        )
                                                        .paint_at(ui, rect);
                                                } else {
                                                    ui.painter().rect_filled(
                                                        rect,
                                                        design::Geometry::RADIUS,
                                                        p.raised,
                                                    );
                                                    design::paint_icon(
                                                        ui.painter(),
                                                        egui::Rect::from_center_size(
                                                            rect.center() - egui::vec2(0., 20.),
                                                            egui::vec2(32., 32.),
                                                        ),
                                                        Icon::Book,
                                                        p.muted,
                                                    );
                                                    ui.painter().text(
                                                        rect.center() + egui::vec2(0., 20.),
                                                        egui::Align2::CENTER_CENTER,
                                                        self.i18n.t("ui.cover_missing"),
                                                        egui::FontId::proportional(13.),
                                                        p.muted,
                                                    );
                                                }
                                                if self.library_manage {
                                                    let selected =
                                                        self.library_selected.contains(&e.id);
                                                    let check = egui::Rect::from_min_size(
                                                        rect.right_top() + egui::vec2(-32., 8.),
                                                        egui::vec2(24., 24.),
                                                    );
                                                    ui.painter().circle_filled(
                                                        check.center(),
                                                        12.,
                                                        if selected {
                                                            design::BUTTON_BLUE
                                                        } else {
                                                            p.surface
                                                        },
                                                    );
                                                    if selected {
                                                        let c = check.center();
                                                        ui.painter().line_segment(
                                                            [
                                                                c + egui::vec2(-5., 0.),
                                                                c + egui::vec2(-1., 4.),
                                                            ],
                                                            egui::Stroke::new(
                                                                2.0_f32,
                                                                egui::Color32::WHITE,
                                                            ),
                                                        );
                                                        ui.painter().line_segment(
                                                            [
                                                                c + egui::vec2(-1., 4.),
                                                                c + egui::vec2(6., -5.),
                                                            ],
                                                            egui::Stroke::new(
                                                                2.0_f32,
                                                                egui::Color32::WHITE,
                                                            ),
                                                        );
                                                    }
                                                }
                                                if response.clicked() {
                                                    if self.library_manage {
                                                        if !self
                                                            .library_selected
                                                            .insert(e.id.clone())
                                                        {
                                                            self.library_selected.remove(&e.id);
                                                        }
                                                    } else {
                                                        action_open_path =
                                                            Some((e.path.clone(), e.last_chapter));
                                                    }
                                                }
                                                ui.horizontal(|ui| {
                                                    let mut title = egui::text::LayoutJob::simple(
                                                        e.title.clone(),
                                                        egui::FontId::proportional(15.),
                                                        p.text,
                                                        width - 56.,
                                                    );
                                                    title.wrap.max_rows = 2;
                                                    ui.add_sized(
                                                        [width - 56., 40.],
                                                        egui::Label::new(title),
                                                    );
                                                    if !self.library_manage {
                                                        super::design::menu_button(
                                                            ui,
                                                            egui::Button::new("⋯")
                                                                .frame(false)
                                                                .min_size(egui::vec2(32., 32.)),
                                                            |ui| {
                                                                design::menu_style(ui);
                                                                if design::menu_item(
                                                                    ui,
                                                                    self.i18n.t("toolbar.export"),
                                                                    false,
                                                                    false,
                                                                )
                                                                .clicked()
                                                                {
                                                                    action_export_path =
                                                                        Some(e.path.clone());
                                                                    ui.memory_mut(|m| {
                                                                        m.close_popup()
                                                                    });
                                                                }
                                                                if design::menu_item(
                                                                    ui,
                                                                    self.i18n.t("library.delete"),
                                                                    false,
                                                                    true,
                                                                )
                                                                .clicked()
                                                                {
                                                                    action_remove_path =
                                                                        Some(e.path.clone());
                                                                    ui.memory_mut(|m| {
                                                                        m.close_popup()
                                                                    });
                                                                }
                                                            },
                                                        );
                                                    }
                                                });
                                                ui.add(
                                                    egui::Label::new(
                                                        egui::RichText::new(
                                                            e.last_chapter_title
                                                                .clone()
                                                                .unwrap_or_else(|| {
                                                                    self.i18n.tf1(
                                                                        "ui.chapter",
                                                                        &(e.last_chapter + 1)
                                                                            .to_string(),
                                                                    )
                                                                }),
                                                        )
                                                        .small()
                                                        .color(p.muted),
                                                    )
                                                    .truncate(),
                                                );
                                            },
                                        );
                                    });
                                    ui.add_space(
                                        design::Geometry::BLOCK_GAP - ui.spacing().item_spacing.x,
                                    );
                                }
                            });
                            design::section_gap(ui);
                        }
                    }
                });
                design::scroll_edge_fades(
                    ui.painter(),
                    // ScrollArea expands its content clip to allow widget strokes.
                    // Cover that same vertical extent so no unfaded strip leaks out.
                    egui::Rect::from_min_max(
                        egui::pos2(scroll.inner_rect.left(), content_clip.top()),
                        egui::pos2(scroll.inner_rect.right(), content_clip.bottom()),
                    ),
                    p.canvas,
                    scroll.state.offset.y > 1.,
                    scroll.state.offset.y + scroll.inner_rect.height() + 1. < scroll.content_size.y,
                );
            });
        if self.library_confirm_delete {
            egui::Window::new(self.i18n.t("manage.confirm"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0., 0.])
                .show(ui.ctx(), |ui| {
                    ui.set_max_width(420.);
                    ui.label(self.i18n.t("manage.delete_note"));
                    ui.horizontal(|ui| {
                        if ui.button(self.i18n.t("manage.delete")).clicked() {
                            let paths: Vec<_> = self
                                .library
                                .books
                                .iter()
                                .filter(|e| self.library_selected.contains(&e.id))
                                .map(|e| e.path.clone())
                                .collect();
                            for path in paths {
                                self.cover_textures.remove(&path);
                                self.library.remove_by_path(&self.data_dir, &path);
                            }
                            self.library_selected.clear();
                            self.library_confirm_delete = false;
                        }
                        if ui.button(self.i18n.t("settings.cancel")).clicked() {
                            self.library_confirm_delete = false;
                        }
                    });
                });
        }
        if action_open_dialog {
            self.open_file_dialog();
        }
        if let Some(path) = action_remove_path {
            self.push_feedback_log(format!("[Library] remove book: {}", path));
            self.cover_textures.remove(&path);
            self.library.remove_by_path(&self.data_dir, &path);
            if self.book_path.as_deref() == Some(path.as_str()) {
                self.book_path = None;
                self.current_book_hash = None;
                self.last_synced_chapter = None;
            }
        } else if let Some((path, chapter)) = action_open_path {
            self.open_book_from_path(&path, Some(chapter));
        }
        if let Some(path) = action_export_path {
            self.export_library_path = Some(path);
            self.show_export_dialog = true;
        }
    }
    pub fn render_book_opening(&mut self, ctx: &egui::Context) {
        let Some(started) = self.opening_started else {
            return;
        };
        let title = self
            .pending_book_open
            .as_ref()
            .and_then(|(path, _, _)| self.library.books.iter().find(|e| &e.path == path))
            .map(|e| e.title.clone())
            .unwrap_or_default();
        let response = egui::Modal::new(egui::Id::new("book_opening")).show(ctx, |ui| {
            ui.set_width(260.);
            ui.vertical_centered(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(132., 160.), egui::Sense::hover());
                let motion = ctx.style().animation_time > 0.;
                let phase = if motion {
                    (started.elapsed().as_secs_f32() * 2.4).sin() * 0.5 + 0.5
                } else {
                    0.
                };
                let page = rect.shrink2(egui::vec2(10., 4.));
                ui.painter()
                    .rect_filled(page, 4., p_surface(self.dark_mode));
                for line in 0..9 {
                    let y = page.top() + 18. + line as f32 * 13.;
                    ui.painter().line_segment(
                        [
                            egui::pos2(page.left() + 12., y),
                            egui::pos2(page.right() - 12., y),
                        ],
                        egui::Stroke::new(1.0_f32, egui::Color32::GRAY),
                    );
                }
                let width = page.width() * (0.70 - phase * 0.32);
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(page.min, egui::vec2(width, page.height())),
                    4.,
                    design::BUTTON_BLUE,
                );
                ui.label(egui::RichText::new(title).strong());
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(self.i18n.t("manage.opening"));
                });
            });
        });
        if response.should_close() {
            self.pending_book_open = None;
            self.opening_started = None;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
    fn export_selected_books(&mut self) {
        let Some(directory) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        for e in self
            .library
            .books
            .iter()
            .filter(|e| self.library_selected.contains(&e.id))
        {
            let target = directory.join(format!(
                "{}-{}.epub",
                reader_core::sanitize_filename(&e.title),
                e.id
            ));
            let result = (|| -> std::io::Result<()> {
                let mut source = std::fs::File::open(&e.path)?;
                let mut output = std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&target)?;
                std::io::copy(&mut source, &mut output)?;
                output.sync_all()
            })();
            if let Err(error) = result {
                self.error_msg = Some(error.to_string());
                break;
            }
        }
    }
}

fn p_surface(dark: bool) -> egui::Color32 {
    Palette::new(dark).surface
}
