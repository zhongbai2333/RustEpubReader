use super::design::{self, Icon, Palette};
use crate::app::ReaderApp;
use eframe::egui;
enum LibraryListAction {
    Open(String, usize),
    Share(String),
    Rename(String, String),
    Remove(String),
}
impl ReaderApp {
    fn render_library_list(
        &mut self,
        ui: &mut egui::Ui,
        sorted: &[usize],
        p: Palette,
        cache: &std::path::Path,
    ) -> Option<LibraryListAction> {
        let mut action = None;
        let ctx = ui.ctx().clone();
        for &idx in sorted {
            let (slot, _) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 96.), egui::Sense::hover());
            let entry = &self.library.books[idx];
            if !ui.is_rect_visible(slot)
                && !self
                    .book_presentation
                    .reorders_from_visible(&entry.path, ui.clip_rect())
            {
                continue;
            }
            let row = self
                .book_presentation
                .shelf_card_rect(&entry.path, slot, &ctx);
            let mut builder = egui::UiBuilder::new().max_rect(row);
            if self.book_presentation.promoted_card(&entry.path) {
                let layer = egui::LayerId::new(
                    ui.layer_id().order,
                    egui::Id::new(("list_promoted", &entry.path)),
                );
                ctx.set_sublayer(ui.layer_id(), layer);
                builder = builder.layer_id(layer);
            }
            let mut row_ui = ui.new_child(builder);
            let ui = &mut row_ui;
            let cover =
                egui::Rect::from_min_size(row.min + egui::vec2(0., 8.), egui::vec2(48., 72.));
            self.book_presentation
                .covers
                .insert(entry.path.clone(), cover);
            if ui.is_rect_visible(cover) {
                self.shelf_covers
                    .request(&entry.path, cache, &ctx, &self.cover_textures);
            }
            if let Some(texture) = self
                .cover_textures
                .get(&entry.path)
                .and_then(|t| t.as_ref())
            {
                let aspect = texture.size_vec2().x / texture.size_vec2().y;
                let target = cover.width() / cover.height();
                let fraction = if aspect > target {
                    egui::vec2(target / aspect, 1.)
                } else {
                    egui::vec2(1., aspect / target)
                };
                egui::Image::new(texture)
                    .uv(egui::Rect::from_center_size(egui::pos2(0.5, 0.5), fraction))
                    .corner_radius(4.)
                    .paint_at(ui, cover);
            } else {
                ui.painter().rect_filled(cover, 4., p.raised);
                design::paint_icon(
                    ui.painter(),
                    egui::Rect::from_center_size(cover.center(), egui::vec2(24., 24.)),
                    Icon::Book,
                    p.muted,
                );
            }
            let main =
                egui::Rect::from_min_max(row.min, egui::pos2(row.right() - 48., row.bottom()));
            let response = ui
                .interact(
                    main,
                    ui.id().with(("list_book", &entry.id)),
                    egui::Sense::click(),
                )
                .on_hover_text(&entry.title);
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &entry.title)
            });
            let text_left = cover.right() + 16.;
            let text_width = (main.right() - text_left - 8.).max(1.);
            let mut title = egui::text::LayoutJob::simple(
                entry.title.clone(),
                egui::FontId::proportional(16.),
                p.text,
                text_width,
            );
            title.wrap.max_rows = 2;
            title.wrap.break_anywhere = true;
            let title = ui.fonts(|fonts| fonts.layout_job(title));
            ui.painter().galley(
                egui::pos2(text_left, row.top() + 14.),
                title.clone(),
                p.text,
            );
            let chapter = entry.last_chapter_title.clone().unwrap_or_else(|| {
                self.i18n
                    .tf1("ui.chapter", &(entry.last_chapter + 1).to_string())
            });
            let mut chapter = egui::text::LayoutJob::simple(
                chapter,
                egui::FontId::proportional(12.),
                p.muted,
                text_width,
            );
            chapter.wrap.max_rows = 1;
            let chapter = ui.fonts(|fonts| fonts.layout_job(chapter));
            ui.painter().galley(
                egui::pos2(text_left, row.top() + 20. + title.size().y),
                chapter,
                p.muted,
            );
            let mut toggle = response.clicked() && self.library_manage;
            if response.clicked() && !self.library_manage {
                action = Some(LibraryListAction::Open(
                    entry.path.clone(),
                    entry.last_chapter,
                ));
            }
            let menu = egui::Rect::from_center_size(
                egui::pos2(row.right() - 22., row.center().y),
                egui::vec2(44., 44.),
            );
            if self.library_manage {
                let mut selected = self.library_selected.contains(&entry.id);
                if ui
                    .put(menu, egui::Checkbox::without_text(&mut selected))
                    .changed()
                {
                    toggle = true;
                }
            } else {
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(menu), |ui| {
                    design::menu_button(
                        ui,
                        egui::Button::new("⋯")
                            .frame(false)
                            .min_size(egui::vec2(40., 40.)),
                        |ui| {
                            design::menu_style(ui);
                            if design::menu_item(ui, self.i18n.t("book.share"), false, false)
                                .clicked()
                            {
                                action = Some(LibraryListAction::Share(entry.path.clone()));
                                ui.memory_mut(|m| m.close_popup());
                            }
                            if design::menu_item(ui, self.i18n.t("book.rename"), false, false)
                                .clicked()
                            {
                                action = Some(LibraryListAction::Rename(
                                    entry.id.clone(),
                                    entry.title.clone(),
                                ));
                                ui.memory_mut(|m| m.close_popup());
                            }
                            if design::menu_item(ui, self.i18n.t("archive.action"), false, false)
                                .clicked()
                            {
                                action = Some(LibraryListAction::Remove(entry.path.clone()));
                                ui.memory_mut(|m| m.close_popup());
                            }
                        },
                    );
                });
            }
            if toggle && !self.library_selected.insert(entry.id.clone()) {
                self.library_selected.remove(&entry.id);
            }
            ui.painter().line_segment(
                [
                    egui::pos2(text_left, row.bottom() - 1.),
                    row.right_bottom() - egui::vec2(0., 1.),
                ],
                egui::Stroke::new(0.5_f32, p.border),
            );
        }
        action
    }
    pub fn render_library(&mut self, ui: &mut egui::Ui) {
        if self.book_share.receive {
            ui.disable();
        }
        let mut action_open_path = None;
        let mut action_remove_path = None;
        let mut action_export_path = None;
        let mut action_rename = None;
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
        let stable = self.book_presentation.shelf_order.resolve(
            sorted
                .iter()
                .map(|&i| self.library.books[i].id.clone())
                .collect(),
            self.library_sort_title,
        );
        let ranks: std::collections::HashMap<_, _> = stable
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect();
        sorted.sort_by_key(|&i| {
            ranks
                .get(self.library.books[i].id.as_str())
                .copied()
                .unwrap_or(usize::MAX)
        });
        sorted.retain(|&i| {
            self.library.books[i]
                .title
                .to_lowercase()
                .contains(&self.library_query.to_lowercase())
        });
        let ctx = ui.ctx().clone();
        self.book_presentation.covers.clear();
        self.shelf_covers
            .poll(&ctx, &mut self.cover_textures, &self.library.books);
        let cover_cache = std::path::Path::new(&self.data_dir).join(".cache/shelf-covers-v1");
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
                        self.render_sync_meter(ui);
                        ui.label(
                            egui::RichText::new(
                                self.i18n
                                    .count("ui.library_count", self.library.books.len()),
                            )
                            .color(p.muted),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(self.i18n.t("archive.title")).clicked() {
                            self.library_show_archive = true;
                        }
                        if ui.button(self.i18n.t("transfer.receive")).clicked() {
                            self.book_share.open_receive(&self.data_dir);
                        }
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
                    let sort_label = self
                        .i18n
                        .t(if self.library_sort_title {
                            "ui.sort_title"
                        } else {
                            "ui.sort_recent"
                        })
                        .to_owned();
                    let font = egui::TextStyle::Button.resolve(ui.style());
                    let color = ui.visuals().text_color();
                    let sort_width = ui.fonts(|fonts| {
                        fonts
                            .layout_no_wrap(sort_label.clone(), font, color)
                            .size()
                            .x
                    }) + 40.;
                    let search_width = (ui.available_width()
                        - sort_width
                        - 90.
                        - 2. * ui.spacing().item_spacing.x)
                        .max(80.);
                    ui.add_sized(
                        [search_width, design::Geometry::CONTROL_HEIGHT],
                        super::design::text_edit(&mut self.library_query)
                            .hint_text(self.i18n.t("ui.search_books"))
                            .vertical_align(egui::Align::Center)
                            .desired_width(search_width),
                    );
                    egui::ComboBox::from_id_salt("library_sort")
                        .width(sort_width)
                        .selected_text(sort_label)
                        .show_ui(ui, |ui| {
                            super::design::menu_style(ui);
                            if ui
                                .selectable_value(
                                    &mut self.library_sort_title,
                                    false,
                                    self.i18n.t("ui.sort_recent"),
                                )
                                .clicked()
                            {
                                self.book_presentation.shelf_order.reset();
                            }
                            if ui
                                .selectable_value(
                                    &mut self.library_sort_title,
                                    true,
                                    self.i18n.t("ui.sort_title"),
                                )
                                .clicked()
                            {
                                self.book_presentation.shelf_order.reset();
                            }
                        });
                    egui::ComboBox::from_id_salt("library_display")
                        .width(82.)
                        .selected_text(self.i18n.t(if self.library_list_mode {
                            "library.view_list"
                        } else {
                            "library.view_grid"
                        }))
                        .show_ui(ui, |ui| {
                            super::design::menu_style(ui);
                            if ui
                                .selectable_value(
                                    &mut self.library_list_mode,
                                    false,
                                    self.i18n.t("library.view_grid"),
                                )
                                .clicked()
                            {
                                ui.memory_mut(|m| m.close_popup());
                            }
                            if ui
                                .selectable_value(
                                    &mut self.library_list_mode,
                                    true,
                                    self.i18n.t("library.view_list"),
                                )
                                .clicked()
                            {
                                ui.memory_mut(|m| m.close_popup());
                            }
                        })
                        .response
                        .on_hover_text(self.i18n.t("library.display"));
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
                            self.share_paths = self
                                .library
                                .books
                                .iter()
                                .filter(|e| self.library_selected.contains(&e.id))
                                .map(|e| e.path.clone())
                                .collect();
                            self.export_library_path = None;
                            self.share_mode = reader_core::export::ExportMode::Original;
                            self.show_export_dialog = true;
                        }
                        if ui
                            .add_enabled(
                                !self.library_selected.is_empty(),
                                egui::Button::new(self.i18n.t("archive.action")),
                            )
                            .clicked()
                        {
                            self.library_confirm_delete = true;
                        }
                    });
                    design::block_gap(ui);
                }
                let mut content_clip = ui.clip_rect();
                let area = egui::ScrollArea::vertical();
                #[cfg(feature = "performance-checks")]
                let area = if let Some(offset) = self
                    .shelf_performance
                    .as_mut()
                    .and_then(|probe| probe.take_offset())
                {
                    area.vertical_scroll_offset(offset)
                } else {
                    area
                };
                let scroll = area.show(ui, |ui| {
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
                                                ui.add(
                                                    egui::Label::new(
                                                        egui::RichText::new(&e.title)
                                                            .size(19.)
                                                            .strong(),
                                                    )
                                                    .truncate(),
                                                )
                                                .on_hover_text(&e.title);
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
                                                        .color(p.muted),
                                                    )
                                                    .truncate(),
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
                        if self.library_list_mode {
                            if let Some(action) =
                                self.render_library_list(ui, &sorted, p, &cover_cache)
                            {
                                match action {
                                    LibraryListAction::Open(path, chapter) => {
                                        action_open_path = Some((path, chapter))
                                    }
                                    LibraryListAction::Share(path) => {
                                        action_export_path = Some(path)
                                    }
                                    LibraryListAction::Rename(id, title) => {
                                        action_rename = Some((id, title))
                                    }
                                    LibraryListAction::Remove(path) => {
                                        action_remove_path = Some(path)
                                    }
                                }
                            }
                        } else {
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
                                let row_size = egui::vec2(ui.available_width(), width * 1.5 + 72.);
                                let row_rect =
                                    egui::Rect::from_min_size(ui.next_widget_position(), row_size);
                                if !ui.is_rect_visible(row_rect)
                                    && !row.iter().any(|&idx| {
                                        self.book_presentation.reorders_from_visible(
                                            &self.library.books[idx].path,
                                            ui.clip_rect(),
                                        )
                                    })
                                {
                                    ui.allocate_exact_size(row_size, egui::Sense::hover());
                                    design::section_gap(ui);
                                    continue;
                                }
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = design::Geometry::BLOCK_GAP;
                                    for &idx in row {
                                        let e = &self.library.books[idx];
                                        ui.push_id(&e.path, |ui| {
                                            let (slot, _) = ui.allocate_exact_size(
                                                egui::vec2(width, width * 1.5 + 72.),
                                                egui::Sense::hover(),
                                            );
                                            let frame = self
                                                .book_presentation
                                                .shelf_card_rect(&e.path, slot, &ctx);
                                            let mut builder = egui::UiBuilder::new()
                                                .max_rect(frame)
                                                .layout(egui::Layout::top_down(egui::Align::LEFT));
                                            if self.book_presentation.promoted_card(&e.path) {
                                                let layer = egui::LayerId::new(
                                                    ui.layer_id().order,
                                                    egui::Id::new(("shelf_promoted", &e.path)),
                                                );
                                                ctx.set_sublayer(ui.layer_id(), layer);
                                                builder = builder.layer_id(layer);
                                            }
                                            let mut card_ui = ui.new_child(builder);
                                            {
                                                let ui = &mut card_ui;
                                                ui.set_min_size(egui::vec2(
                                                    width,
                                                    width * 1.5 + 72.,
                                                ));
                                                ui.spacing_mut().item_spacing.x =
                                                    design::Geometry::GAP;
                                                let (rect, response) = ui.allocate_exact_size(
                                                    egui::vec2(width, width * 1.5),
                                                    egui::Sense::click(),
                                                );
                                                self.book_presentation
                                                    .covers
                                                    .insert(e.path.clone(), rect);
                                                response.widget_info(|| {
                                                    egui::WidgetInfo::labeled(
                                                        egui::WidgetType::Button,
                                                        true,
                                                        &e.title,
                                                    )
                                                });
                                                if ui.is_rect_visible(rect) {
                                                    self.shelf_covers.request(
                                                        &e.path,
                                                        &cover_cache,
                                                        &ctx,
                                                        &self.cover_textures,
                                                    );
                                                }
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
                                                ui.horizontal_top(|ui| {
                                                    // Labels in a horizontal Ui otherwise default to
                                                    // Extend, overriding the LayoutJob's width.
                                                    ui.spacing_mut().button_padding =
                                                        egui::vec2(4., 4.);
                                                    let title_width = if self.library_manage {
                                                        width
                                                    } else {
                                                        width - 32. - ui.spacing().item_spacing.x
                                                    };
                                                    let mut title = egui::text::LayoutJob::simple(
                                                        e.title.clone(),
                                                        egui::FontId::proportional(15.),
                                                        p.text,
                                                        title_width,
                                                    );
                                                    title.wrap.max_rows = 2;
                                                    title.wrap.break_anywhere = true;
                                                    ui.allocate_ui_with_layout(
                                                        egui::vec2(title_width, 40.),
                                                        egui::Layout::top_down(egui::Align::LEFT),
                                                        |ui| {
                                                            ui.set_min_size(egui::vec2(
                                                                title_width,
                                                                40.,
                                                            ));
                                                            ui.add(egui::Label::new(title).wrap())
                                                                .on_hover_text(&e.title);
                                                        },
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
                                                                    self.i18n.t("book.share"),
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
                                                                    self.i18n.t("book.rename"),
                                                                    false,
                                                                    false,
                                                                )
                                                                .clicked()
                                                                {
                                                                    action_rename = Some((
                                                                        e.id.clone(),
                                                                        e.title.clone(),
                                                                    ));
                                                                    ui.memory_mut(|m| {
                                                                        m.close_popup()
                                                                    });
                                                                }
                                                                if design::menu_item(
                                                                    ui,
                                                                    self.i18n.t("archive.action"),
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
                                            }
                                        });
                                    }
                                });
                                design::section_gap(ui);
                            }
                        }
                    }
                });
                #[cfg(feature = "performance-checks")]
                if let Some(probe) = &mut self.shelf_performance {
                    probe.observe(
                        scroll.inner_rect,
                        scroll.content_size.y,
                        scroll.state.offset.y,
                    );
                }
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
        self.render_archive_window(ui.ctx());
        self.shelf_covers.trim(&mut self.cover_textures);
        if self.library_confirm_delete {
            egui::Window::new(self.i18n.t("archive.confirm"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0., 0.])
                .show(ui.ctx(), |ui| {
                    ui.set_max_width(420.);
                    ui.label(self.i18n.t("archive.note"));
                    ui.horizontal(|ui| {
                        if ui.button(self.i18n.t("archive.action")).clicked() {
                            let paths: Vec<_> = self
                                .library
                                .books
                                .iter()
                                .filter(|e| self.library_selected.contains(&e.id))
                                .map(|e| e.path.clone())
                                .collect();
                            if let Err(error) = reader_core::library::archive::ArchiveStore::archive(&self.data_dir,&mut self.library,&paths) { self.error_msg=Some(error); }
                            for path in paths { self.cover_textures.remove(&path); }
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
            if let Err(error) = reader_core::library::archive::ArchiveStore::archive(
                &self.data_dir,
                &mut self.library,
                std::slice::from_ref(&path),
            ) {
                self.error_msg = Some(error);
            }
            if self.book_path.as_deref() == Some(path.as_str()) {
                self.book_path = None;
                self.current_book_hash = None;
                self.last_synced_chapter = None;
            }
        } else if let Some((path, chapter)) = action_open_path {
            self.open_book_from_path(&path, Some(chapter));
        }
        if let Some(path) = action_export_path {
            self.share_paths.clear();
            self.share_mode = reader_core::export::ExportMode::Original;
            self.export_library_path = Some(path);
            self.show_export_dialog = true;
        }
        if let Some(rename) = action_rename {
            self.rename_book = Some(rename);
        }
    }
    pub(super) fn export_selected_books(&mut self) -> bool {
        let Some(directory) = rfd::FileDialog::new().pick_folder() else {
            return false;
        };
        let entries: Vec<_> = self
            .library
            .books
            .iter()
            .filter(|e| self.share_paths.contains(&e.path))
            .cloned()
            .collect();
        match reader_core::export::export_originals(&entries, &directory) {
            Ok(_) => true,
            Err(error) => {
                self.error_msg = Some(self.i18n.tf1("book.export_failed", &error));
                false
            }
        }
    }
}

impl ReaderApp {
    fn render_archive_window(&mut self, ctx: &egui::Context) {
        if self.library_show_archive {
            let result = reader_core::library::archive::ArchiveStore::load(&self.data_dir);
            let mut action = None;
            egui::Window::new(self.i18n.t("archive.title")).open(&mut self.library_show_archive)
                .default_width(540.).default_height(380.).resizable(true).show(ctx,|ui| {
                    match result {
                        Ok(store) => {
                            let mut entries=store.list(); entries.sort_by_key(|r|std::cmp::Reverse(r.lifecycle.revision));
                            if entries.is_empty() { ui.label(self.i18n.t("archive.empty")); }
                            egui::ScrollArea::vertical().max_height(430.).show(ui,|ui| {
                                for record in entries {
                                    let book=record.lifecycle.book;
                                    ui.push_id(&book.hash,|ui| {
                                        ui.strong(&book.title);
                                        ui.small(self.i18n.t(if record.entry.as_ref().is_some_and(|e|std::path::Path::new(&e.path).is_file()) {"archive.local"} else {"archive.cloud"}));
                                        ui.horizontal(|ui| {
                                            if ui.button(self.i18n.t("archive.restore")).clicked() { action=Some((book.hash.clone(),reader_core::library::archive::BookState::Active)); }
                                            if ui.button(self.i18n.t("archive.delete")).clicked() { self.library_archive_delete=Some(book.clone()); }
                                        });
                                        ui.separator();
                                    });
                                }
                            });
                        }
                        Err(error)=>{ui.label(error);}
                    }
                });
            if let Some((hash, state)) = action {
                match reader_core::library::archive::ArchiveStore::change(
                    &self.data_dir,
                    &hash,
                    state,
                ) {
                    Ok(()) => {
                        self.reload_library();
                        self.cloud_sync.stop();
                    }
                    Err(error) => self.error_msg = Some(error),
                }
            }
        }
        if let Some(book) = self.library_archive_delete.clone() {
            egui::Window::new(self.i18n.t("archive.delete_confirm"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0., 0.])
                .show(ctx, |ui| {
                    ui.set_max_width(440.);
                    ui.strong(book.title);
                    ui.label(self.i18n.t("archive.delete_note"));
                    ui.horizontal(|ui| {
                        if ui.button(self.i18n.t("archive.delete")).clicked() {
                            if let Err(error) = reader_core::library::archive::ArchiveStore::change(
                                &self.data_dir,
                                &book.hash,
                                reader_core::library::archive::BookState::Deleted,
                            ) {
                                self.error_msg = Some(error);
                            }
                            self.library_archive_delete = None;
                            self.cloud_sync.stop();
                        }
                        if ui.button(self.i18n.t("settings.cancel")).clicked() {
                            self.library_archive_delete = None;
                        }
                    });
                });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reader_core::library::BookEntry;

    fn shelf_shapes(
        shape: &egui::Shape,
        raised: egui::Color32,
        texture: egui::TextureId,
        covers: &mut Vec<egui::Rect>,
        text: &mut Vec<egui::Rect>,
    ) {
        match shape {
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    shelf_shapes(shape, raised, texture, covers, text);
                }
            }
            egui::Shape::Rect(shape)
                if shape.rect.height() > 60.
                    && ((shape.rect.height() / shape.rect.width()) - 1.5).abs() < 0.01 =>
            {
                if shape.fill == raised
                    || shape
                        .brush
                        .as_ref()
                        .is_some_and(|brush| brush.fill_texture_id == texture)
                {
                    covers.push(shape.rect);
                }
            }
            egui::Shape::Mesh(mesh) if mesh.texture_id == texture => {
                covers.push(mesh.calc_bounds());
            }
            egui::Shape::Text(shape)
                if shape.galley.job.text.starts_with("fixture-title:")
                    || shape.galley.job.text.starts_with("fixture-chapter:") =>
            {
                text.push(shape.visual_bounding_rect());
            }
            _ => {}
        }
    }

    #[test]
    fn long_book_names_keep_mixed_cover_rows_aligned_and_inside_the_viewport() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let directory = tempfile::tempdir().unwrap();
        let mut app = ReaderApp::from_data_dir(directory.path().to_string_lossy().into_owned());
        app.dark_mode = true;
        app.library.books = (0..37)
            .map(|i| BookEntry {
                id: format!("fixture-{i}"),
                title: format!(
                    "fixture-title:{i} {}",
                    match i % 3 {
                        0 => "LongUnbrokenBookFilename".repeat(16),
                        1 => "中文书名与英文混合 A long title with spaces ".repeat(8),
                        _ => "Short book".into(),
                    }
                ),
                path: format!("fixture-{i}.epub"),
                last_chapter: i,
                last_block: 0,
                last_char_offset: 0,
                last_chapter_title: Some(format!(
                    "fixture-chapter:{i} {}",
                    "long reading position ".repeat(15)
                )),
                last_opened: (100 - i) as u64,
            })
            .collect();
        for width in [420., 820., 1509.] {
            for scale in [1., 1.5, 2.] {
                for (manage, list_mode) in
                    [(false, false), (true, false), (false, true), (true, true)]
                {
                    let ctx = egui::Context::default();
                    ctx.set_pixels_per_point(scale);
                    design::apply(&ctx, true);
                    let texture = ctx.load_texture(
                        "fixture-cover",
                        egui::ColorImage::new([16, 24], egui::Color32::LIGHT_BLUE),
                        Default::default(),
                    );
                    app.cover_textures = app
                        .library
                        .books
                        .iter()
                        .enumerate()
                        .map(|(i, book)| (book.path.clone(), (i % 3 == 1).then(|| texture.clone())))
                        .collect();
                    app.library_manage = manage;
                    app.library_list_mode = list_mode;
                    let mut output = egui::FullOutput::default();
                    // Let pixel scaling and the ScrollArea's width settle before measuring.
                    for _ in 0..3 {
                        output = ctx.run(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(width, 14000.),
                                )),
                                ..Default::default()
                            },
                            |ctx| {
                                egui::CentralPanel::default()
                                    .show(ctx, |ui| app.render_library(ui));
                            },
                        );
                    }
                    let mut covers = Vec::new();
                    let mut text = Vec::new();
                    for shape in &output.shapes {
                        shelf_shapes(
                            &shape.shape,
                            Palette::new(true).raised,
                            texture.id(),
                            &mut covers,
                            &mut text,
                        );
                    }
                    let case =
                        format!("width={width}, scale={scale}, manage={manage}, list={list_mode}");
                    assert_eq!(covers.len(), 37, "missing/offscreen covers: {case}");
                    let first_row: Vec<_> = covers
                        .iter()
                        .filter(|r| (r.top() - covers[0].top()).abs() < 1.)
                        .collect();
                    for (index, rect) in covers.iter().enumerate() {
                        assert!(
                            rect.right() <= width - f32::from(design::Geometry::PAGE_INSET) + 1.,
                            "cover outside viewport: {case}, {rect:?}"
                        );
                        assert!(
                            (rect.left() - first_row[index % first_row.len()].left()).abs() < 1.,
                            "column drift: {case}, {rect:?}"
                        );
                        if let Some(next) = first_row.get(index % first_row.len() + 1) {
                            assert!(
                                (next.left()
                                    - first_row[index % first_row.len()].right()
                                    - design::Geometry::BLOCK_GAP)
                                    .abs()
                                    < 1.,
                                "uneven column gap: {case}"
                            );
                        }
                    }
                    if list_mode {
                        for pair in covers.windows(2) {
                            assert!(
                                pair[1].top() >= pair[0].bottom() + 8.,
                                "list rows overlap: {case}, {pair:?}"
                            );
                        }
                    }
                    for cover in &covers {
                        let captions: Vec<_> = text
                            .iter()
                            .filter(|rect| {
                                if list_mode {
                                    rect.top() >= cover.top()
                                        && rect.top() < cover.top() + 80.
                                        && rect.left() >= cover.right()
                                        && rect.left() < cover.right() + 20.
                                } else {
                                    rect.top() >= cover.bottom() - 1.
                                        && rect.top() < cover.bottom() + 72.
                                        && rect.left() >= cover.left() - 2.
                                        && rect.left() < cover.right()
                                }
                            })
                            .collect();
                        assert_eq!(
                            captions.len(),
                            2,
                            "missing title/progress: {case}, {cover:?}"
                        );
                        for rect in captions {
                            assert!(
                                rect.right()
                                    <= if list_mode {
                                        width - f32::from(design::Geometry::PAGE_INSET) + 2.
                                    } else {
                                        cover.right() + 2.
                                    },
                                "caption overlaps next book: {case}, {rect:?}"
                            );
                        }
                    }
                    for rect in text {
                        assert!(
                            rect.right() <= width - f32::from(design::Geometry::PAGE_INSET) + 2.,
                            "text outside viewport: {case}, {rect:?}"
                        );
                    }
                }
            }
        }
    }
}
