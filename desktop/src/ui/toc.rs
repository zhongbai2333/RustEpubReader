//! Table of Contents (TOC) side-panel UI component.
use crate::app::ReaderApp;
use eframe::egui;

impl ReaderApp {
    pub fn render_toc(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().interact_size.y = 32.;
        ui.spacing_mut().button_padding = egui::vec2(10., 5.);
        ui.add_space(2.0);
        super::design::sidebar_title(ui, self.i18n.t("toc.title"));
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);

        // "Locate current chapter" button
        if ui
            .button(egui::RichText::new(self.i18n.t("toc.locate_current")).size(13.0))
            .clicked()
        {
            self.scroll_toc_to_current = true;
        }
        ui.add_space(4.0);

        if let Some(book) = &self.book {
            let toc = book.toc.clone();
            let should_scroll = self.scroll_toc_to_current;
            self.scroll_toc_to_current = false;
            egui::ScrollArea::vertical()
                .id_salt("toc_scroll")
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4., 2.);
                    ui.spacing_mut().interact_size.y = 28.;
                    for entry in &toc {
                        let is_current = entry.chapter_index == self.current_chapter;
                        let ch_bookmarked = self.book_config.as_ref().is_some_and(|cfg| {
                            cfg.bookmarks
                                .iter()
                                .any(|b| b.chapter == entry.chapter_index)
                        });

                        ui.push_id(entry.chapter_index, |ui| {
                            let (row, _) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 32.),
                                egui::Sense::hover(),
                            );
                            let title_rect =
                                egui::Rect::from_min_max(row.min, row.max - egui::vec2(30., 0.));
                            let label = ui.interact(
                                title_rect,
                                ui.id().with("chapter"),
                                egui::Sense::click(),
                            );
                            label.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::WidgetType::SelectableLabel,
                                    true,
                                    is_current,
                                    &entry.title,
                                )
                            });
                            if is_current || label.hovered() {
                                ui.painter().rect_filled(
                                    row,
                                    4,
                                    if is_current {
                                        ui.visuals().selection.bg_fill
                                    } else {
                                        ui.visuals().widgets.hovered.weak_bg_fill
                                    },
                                );
                            }
                            if is_current {
                                ui.painter().rect_filled(
                                    egui::Rect::from_min_size(
                                        row.min + egui::vec2(0., 8.),
                                        egui::vec2(2., 16.),
                                    ),
                                    1,
                                    ui.visuals().selection.stroke.color,
                                );
                            }
                            let color = if is_current {
                                ui.visuals().selection.stroke.color
                            } else {
                                ui.visuals().text_color()
                            };
                            let mut job = egui::text::LayoutJob::simple(
                                entry.title.clone(),
                                egui::FontId::proportional(14.),
                                color,
                                title_rect.width() - 18.,
                            );
                            job.wrap.max_rows = 1;
                            let galley = ui.fonts(|f| f.layout_job(job));
                            ui.painter().galley(
                                row.left_center() + egui::vec2(10., -galley.size().y / 2.),
                                galley,
                                color,
                            );
                            let label = label.on_hover_text(&entry.title);
                            if is_current && should_scroll {
                                label.scroll_to_me(Some(egui::Align::Center));
                            }
                            if label.clicked() && entry.chapter_index != self.current_chapter {
                                self.navigate_reader(
                                    super::reader_navigation::ReaderLocation {
                                        chapter: entry.chapter_index,
                                        block: 0,
                                        char_offset: 0,
                                    },
                                    super::reader_navigation::NavigationKind::Local,
                                );
                            }

                            // Bookmark toggle at right edge
                            let bm_icon = if ch_bookmarked { "★" } else { "☆" };
                            let bm_color = if ch_bookmarked {
                                egui::Color32::from_rgb(255, 200, 0)
                            } else {
                                egui::Color32::GRAY
                            };
                            let chapter_idx = entry.chapter_index;
                            let bookmark_rect = egui::Rect::from_min_max(
                                egui::pos2(row.right() - 28., row.top()),
                                row.max,
                            );
                            let bookmark = ui.interact(
                                bookmark_rect,
                                ui.id().with("bookmark"),
                                egui::Sense::click(),
                            );
                            bookmark.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Button,
                                    true,
                                    if ch_bookmarked {
                                        self.i18n.t("reader.remove_bookmark_label")
                                    } else {
                                        self.i18n.t("context.bookmark")
                                    },
                                )
                            });
                            if ch_bookmarked || label.hovered() || bookmark.hovered() {
                                ui.painter().text(
                                    bookmark_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    bm_icon,
                                    egui::FontId::proportional(14.),
                                    bm_color,
                                );
                            }
                            if bookmark
                                .on_hover_text(if ch_bookmarked {
                                    self.i18n.t("reader.remove_bookmark_label")
                                } else {
                                    self.i18n.t("context.bookmark")
                                })
                                .clicked()
                            {
                                if let Some(cfg) = &mut self.book_config {
                                    if ch_bookmarked {
                                        cfg.bookmarks.retain(|b| b.chapter != chapter_idx);
                                    } else {
                                        cfg.bookmarks.push(reader_core::library::Bookmark {
                                            chapter: chapter_idx,
                                            block: 0,
                                            created_at: reader_core::now_secs(),
                                        });
                                    }
                                    cfg.save(&self.data_dir);
                                }
                            }
                        });
                    }
                });
        }
    }
}
