//! User Interface for viewing and managing book annotations and highlights.
use crate::app::ReaderApp;
use eframe::egui;
use reader_core::library::HighlightColor;

/// Return a display-friendly (egui Color32, label) pair for each highlight color.
fn hl_color_info(color: &HighlightColor) -> (egui::Color32, &'static str) {
    match color {
        HighlightColor::Yellow => (egui::Color32::from_rgb(255, 245, 140), "Y"),
        HighlightColor::Green => (egui::Color32::from_rgb(144, 238, 144), "G"),
        HighlightColor::Blue => (egui::Color32::from_rgb(135, 206, 250), "B"),
        HighlightColor::Pink => (egui::Color32::from_rgb(255, 182, 193), "P"),
    }
}

impl ReaderApp {
    /// Render the bookmarks / highlights / notes side-panel.
    pub fn render_annotations_panel(&mut self, ctx: &egui::Context) {
        if !self.show_annotations {
            return;
        }

        egui::SidePanel::right("annotations_panel")
            .default_width(320.0)
            .min_width(260.0)
            .show(ctx, |ui| {
                ui.heading(self.i18n.t("annotations.title"));
                ui.add_space(4.0);
                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    // ── Bookmarks list ──
                    ui.collapsing(self.i18n.t("annotations.bookmarks"), |ui| {
                        let bookmarks = self
                            .book_config
                            .as_ref()
                            .map(|c| c.bookmarks.clone())
                            .unwrap_or_default();
                        if bookmarks.is_empty() {
                            ui.label(self.i18n.t("annotations.no_bookmarks"));
                        }
                        let mut to_remove: Option<usize> = None;
                        for (idx, bm) in bookmarks.iter().enumerate() {
                            ui.horizontal(|ui| {
                                let title = self
                                    .book
                                    .as_ref()
                                    .and_then(|b| b.chapters.get(bm.chapter))
                                    .map(|c| c.title.clone())
                                    .unwrap_or_else(|| format!("Ch.{}", bm.chapter));
                                if ui.link(&title).clicked() {
                                    self.current_chapter = bm.chapter;
                                    self.pages_dirty = true;
                                    self.current_page = 0;
                                }
                                if ui.small_button("x").clicked() {
                                    to_remove = Some(idx);
                                }
                            });
                        }
                        if let Some(idx) = to_remove {
                            if let Some(cfg) = &mut self.book_config {
                                cfg.bookmarks.remove(idx);
                                cfg.save(&self.data_dir);
                            }
                        }
                    });

                    ui.add_space(4.0);

                    // ── Highlights grouped by chapter ──
                    ui.collapsing(self.i18n.t("annotations.highlights"), |ui| {
                        let highlights = self
                            .book_config
                            .as_ref()
                            .map(|c| c.highlights.clone())
                            .unwrap_or_default();
                        if highlights.is_empty() {
                            ui.label(self.i18n.t("annotations.no_highlights"));
                            return;
                        }

                        // Group by chapter
                        let mut by_chapter: std::collections::BTreeMap<
                            usize,
                            Vec<(usize, reader_core::library::Highlight)>,
                        > = std::collections::BTreeMap::new();
                        for (idx, hl) in highlights.iter().enumerate() {
                            by_chapter
                                .entry(hl.chapter)
                                .or_default()
                                .push((idx, hl.clone()));
                        }

                        let mut to_remove: Option<usize> = None;
                        for (chapter, entries) in &by_chapter {
                            let ch_title = self
                                .book
                                .as_ref()
                                .and_then(|b| b.chapters.get(*chapter))
                                .map(|c| c.title.clone())
                                .unwrap_or_else(|| format!("Ch.{}", chapter));

                            ui.collapsing(&ch_title, |ui| {
                                for (global_idx, hl) in entries {
                                    let (dot_color, _label) = hl_color_info(&hl.color);

                                    // Extract highlighted text preview from book
                                    let preview = self
                                        .book
                                        .as_ref()
                                        .and_then(|b| b.chapters.get(hl.chapter))
                                        .and_then(|ch| ch.blocks.get(hl.start_block))
                                        .map(|block| {
                                            let full: String = match block {
                                                reader_core::epub::ContentBlock::Paragraph {
                                                    spans, ..
                                                } => {
                                                    spans.iter().map(|s| s.text.as_str()).collect()
                                                }
                                                reader_core::epub::ContentBlock::Heading {
                                                    spans,
                                                    ..
                                                } => {
                                                    spans.iter().map(|s| s.text.as_str()).collect()
                                                }
                                                _ => String::new(),
                                            };
                                            let chars: Vec<char> = full.chars().collect();
                                            let start = hl.start_offset.min(chars.len());
                                            let end = hl.end_offset.min(chars.len());
                                            if start < end {
                                                chars[start..end].iter().collect::<String>()
                                            } else {
                                                chars.iter().take(40).collect::<String>()
                                            }
                                        })
                                        .unwrap_or_default();

                                    ui.horizontal(|ui| {
                                        let dot = egui::Button::new(" ")
                                            .fill(dot_color)
                                            .min_size(egui::vec2(12.0, 12.0));
                                        ui.add(dot);

                                        // Truncate preview
                                        let display: String = preview.chars().take(30).collect();
                                        let label_text = if preview.chars().count() > 30 {
                                            format!("{}...", display)
                                        } else {
                                            display
                                        };
                                        if ui.link(&label_text).clicked() {
                                            self.current_chapter = hl.chapter;
                                            self.pages_dirty = true;
                                            self.current_page = 0;
                                        }
                                        if ui.small_button("x").clicked() {
                                            to_remove = Some(*global_idx);
                                        }
                                    });

                                    // Note display / edit
                                    let existing_note = self.book_config.as_ref().and_then(|c| {
                                        c.notes.iter().find(|n| n.highlight_id == hl.id).cloned()
                                    });

                                    let is_editing =
                                        self.editing_note_id.as_deref() == Some(&hl.id);

                                    ui.indent(format!("note_{}", hl.id), |ui| {
                                        if is_editing {
                                            ui.horizontal(|ui| {
                                                ui.text_edit_singleline(&mut self.editing_note_buf);
                                                if ui.small_button("✓").clicked() {
                                                    let buf = self.editing_note_buf.clone();
                                                    if let Some(cfg) = &mut self.book_config {
                                                        if let Some(note) = cfg
                                                            .notes
                                                            .iter_mut()
                                                            .find(|n| n.highlight_id == hl.id)
                                                        {
                                                            note.content = buf;
                                                            note.updated_at =
                                                                reader_core::now_secs();
                                                        } else if !self
                                                            .editing_note_buf
                                                            .trim()
                                                            .is_empty()
                                                        {
                                                            cfg.notes.push(
                                                                reader_core::library::Note {
                                                                    highlight_id: hl.id.clone(),
                                                                    content: self
                                                                        .editing_note_buf
                                                                        .clone(),
                                                                    created_at:
                                                                        reader_core::now_secs(),
                                                                    updated_at:
                                                                        reader_core::now_secs(),
                                                                },
                                                            );
                                                        }
                                                        cfg.save(&self.data_dir);
                                                    }
                                                    self.editing_note_id = None;
                                                    self.editing_note_buf.clear();
                                                }
                                            });
                                        } else if let Some(note) = &existing_note {
                                            let resp = ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(&note.content)
                                                        .small()
                                                        .color(egui::Color32::GRAY),
                                                )
                                                .sense(egui::Sense::click()),
                                            );
                                            if resp.clicked() {
                                                self.editing_note_id = Some(hl.id.clone());
                                                self.editing_note_buf = note.content.clone();
                                            }
                                        } else {
                                            let resp = ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new("+ 添加备注")
                                                        .small()
                                                        .color(egui::Color32::DARK_GRAY),
                                                )
                                                .sense(egui::Sense::click()),
                                            );
                                            if resp.clicked() {
                                                self.editing_note_id = Some(hl.id.clone());
                                                self.editing_note_buf.clear();
                                            }
                                        }
                                    });
                                }
                            });
                        }

                        if let Some(idx) = to_remove {
                            if let Some(cfg) = &mut self.book_config {
                                let hl_id = cfg.highlights[idx].id.clone();
                                cfg.notes.retain(|n| n.highlight_id != hl_id);
                                cfg.highlights.remove(idx);
                                cfg.save(&self.data_dir);
                            }
                        }
                    });

                    ui.add_space(4.0);

                    // ── CSC Corrections list ──
                    ui.collapsing(self.i18n.t("annotations.corrections"), |ui| {
                        let corrections = self
                            .book_config
                            .as_ref()
                            .map(|c| c.corrections.clone())
                            .unwrap_or_default();
                        let resolved: Vec<_> = corrections
                            .iter()
                            .filter(|r| r.status == "accepted" || r.status == "rejected")
                            .collect();
                        if resolved.is_empty() {
                            ui.label(self.i18n.t("annotations.no_corrections"));
                            return;
                        }

                        // Group by chapter
                        let mut by_chapter: std::collections::BTreeMap<
                            usize,
                            Vec<&reader_core::library::CorrectionRecord>,
                        > = std::collections::BTreeMap::new();
                        for rec in &resolved {
                            by_chapter.entry(rec.chapter).or_default().push(rec);
                        }

                        let mut to_revert: Option<(usize, usize, usize)> = None; // (chapter, block_idx, char_offset)
                        for (chapter, entries) in &by_chapter {
                            let ch_title = self
                                .book
                                .as_ref()
                                .and_then(|b| b.chapters.get(*chapter))
                                .map(|c| c.title.clone())
                                .unwrap_or_else(|| format!("Ch.{}", chapter));

                            ui.collapsing(&ch_title, |ui| {
                                for rec in entries {
                                    ui.horizontal(|ui| {
                                        let (icon, color) = if rec.status == "accepted" {
                                            ("✓", egui::Color32::from_rgb(60, 180, 80))
                                        } else {
                                            ("✗", egui::Color32::from_rgb(220, 60, 50))
                                        };
                                        ui.colored_label(color, icon);
                                        ui.label(
                                            egui::RichText::new(&rec.original)
                                                .strikethrough()
                                                .color(egui::Color32::GRAY),
                                        );
                                        ui.label("→");
                                        ui.label(
                                            egui::RichText::new(&rec.corrected)
                                                .color(egui::Color32::from_rgb(60, 180, 80)),
                                        );
                                        if ui
                                            .small_button("↩")
                                            .on_hover_text(
                                                self.i18n.t("annotations.revert_correction"),
                                            )
                                            .clicked()
                                        {
                                            to_revert =
                                                Some((rec.chapter, rec.block_idx, rec.char_offset));
                                        }
                                    });
                                }
                            });
                        }

                        // Process revert
                        if let Some((ch, blk, off)) = to_revert {
                            // Remove from config
                            if let Some(cfg) = &mut self.book_config {
                                cfg.corrections.retain(|r| {
                                    !(r.chapter == ch && r.block_idx == blk && r.char_offset == off)
                                });
                                cfg.save(&self.data_dir);
                            }
                            // Reset in csc_cache to Pending
                            if let Some(corrs) = self.csc_cache.get_mut(&(ch, blk)) {
                                if let Some(c) = corrs.iter_mut().find(|c| c.char_offset == off) {
                                    c.status = reader_core::epub::CorrectionStatus::Pending;
                                }
                            }
                        }
                    });
                });
            });
    }
}
