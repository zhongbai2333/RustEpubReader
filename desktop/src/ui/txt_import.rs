//! Native TXT heading review. Disk work never runs in the frame loop.
use crate::app::{ReaderApp, TxtConvertSlot, TxtImportPage};
use eframe::egui;
use serde_json::json;
use std::sync::{Arc, Mutex};
impl crate::app::TxtImportState {
    fn request(&self, action: &str) -> serde_json::Value {
        json!({"action":action,"path":self.txt_path,"query":self.source_query,"custom_regex":self.custom_regex,"use_heuristic":self.use_heuristic,"excluded_lines":self.excluded,"included_lines":self.included})
    }
    fn work(&mut self, action: &str, ctx: &egui::Context) {
        if self.pending.is_some() {
            return;
        }
        let v = self.request(action);
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        self.pending = Some((action.into(), rx));
        self.error = None;
        std::thread::spawn(move || {
            let _ = tx.send(reader_core::txt::command(&v));
            ctx.request_repaint();
        });
    }
}
impl ReaderApp {
    pub fn render_txt_import(&mut self, ctx: &egui::Context) {
        let Some(state) = self.txt_import.as_mut() else {
            return;
        };
        let converted = state
            .result_slot
            .as_ref()
            .and_then(|slot| slot.try_lock().ok()?.take());
        if let Some(result) = converted {
            match result {
                Ok(r) => {
                    let path = r.epub_path.to_string_lossy().into_owned();
                    self.txt_import = None;
                    self.open_book_from_path(&path, None);
                }
                Err(e) => {
                    state.converting = false;
                    state.error = Some(e);
                    state.result_slot = None;
                }
            }
            return;
        }
        if !state.analyzed && state.pending.is_none() {
            state.analyzed = true;
            state.work("analyze", ctx);
        }
        let done = state
            .pending
            .as_ref()
            .and_then(|(_, rx)| rx.try_recv().ok());
        if let Some(result) = done {
            let (kind, _) = state.pending.take().unwrap();
            if kind != "analyze" || state.analyzed {
                match result {
                    Ok(value) => {
                        if kind == "analyze" {
                            state.heading_count =
                                value["heading_count"].as_u64().unwrap_or(0) as usize;
                            state.chapter_count =
                                value["chapter_count"].as_u64().unwrap_or(0) as usize;
                        }
                        let rows = serde_json::from_value::<Vec<reader_core::txt::ChapterPreview>>(
                            if kind == "lines" {
                                value
                            } else {
                                value["chapters"].clone()
                            },
                        );
                        match rows {
                            Ok(rows) => {
                                if kind == "lines" {
                                    state.source_hits = rows.into_iter().take(80).collect();
                                } else {
                                    // Keep unchecked candidates so they can be selected again.
                                    for row in rows {
                                        if let Some(old) = state
                                            .previews
                                            .iter_mut()
                                            .find(|r| r.line_start == row.line_start)
                                        {
                                            if !state.excluded.contains(&row.line_start) {
                                                *old = row;
                                            }
                                        } else {
                                            state.previews.push(row);
                                        }
                                    }
                                    state.previews.sort_by_key(|r| r.line_start);
                                }
                            }
                            Err(e) => state.error = Some(e.to_string()),
                        }
                    }
                    Err(e) => state.error = Some(e),
                }
            }
        }
        let mut open = true;
        let mut convert = false;
        let mut cancel = false;
        let screen = ctx.screen_rect();
        egui::Window::new(self.i18n.t("txt.import_title"))
            .id(egui::Id::new("txt-import-review"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_CENTER, [0., 0.])
            .default_size([(screen.width() - 64.).min(780.), (screen.height() * 0.82).min(720.)])
            .min_width((screen.width() - 48.).min(420.))
            .min_height((screen.height() * 0.7).min(420.))
            .max_width(screen.width() - 32.)
            .max_height(screen.height() * 0.9)
            .show(ctx, |ui| {
                let state = self.txt_import.as_mut().unwrap();
                let busy = state.pending.is_some() || state.converting;
                ui.spacing_mut().item_spacing = egui::vec2(12., 12.);
                ui.spacing_mut().interact_size.y = 32.;
                ui.spacing_mut().button_padding = egui::vec2(12., 8.);
                ui.spacing_mut().scroll.floating = false;
                ui.spacing_mut().scroll.foreground_color = false;
                let review = matches!(state.page, TxtImportPage::Review);
                // Footer is allocated first, so neither a long list nor an error can hide it.
                egui::TopBottomPanel::bottom("txt-review-footer")
                    .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(4, 12)))
                    .show_inside(ui, |ui| {
                        ui.horizontal(|ui| {
                            if busy { ui.spinner(); ui.small(self.i18n.t(if state.converting {"txt.converting"} else {"txt.analyzing"})); }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if review {
                                    convert = ui.add_enabled(!busy && state.error.is_none() && !state.previews.is_empty(),
                                        egui::Button::new(egui::RichText::new(self.i18n.t("txt.convert_import")).color(egui::Color32::WHITE))
                                            .fill(super::design::BUTTON_BLUE).min_size(egui::vec2(132., 38.))).clicked();
                                    if ui.add_enabled(!state.converting, egui::Button::new(self.i18n.t("common.cancel"))).clicked() { cancel = true; }
                                }
                            });
                        });
                    });
                egui::CentralPanel::default().frame(egui::Frame::NONE.inner_margin(8)).show_inside(ui, |ui| {
                    ui.add_enabled_ui(!state.converting, |ui| {
                        if !review {
                            ui.horizontal(|ui| {
                                if ui.button(format!("‹ {}", self.i18n.t("settings.back"))).clicked() {state.page=TxtImportPage::Review;}
                                ui.strong(self.i18n.t(match state.page { TxtImportPage::Rules=>"txt.advanced", TxtImportPage::Source=>"txt.add_heading", _=>"txt.excerpt" }));
                            });
                            ui.separator();
                        }
                        egui::ScrollArea::vertical().id_salt(match state.page { TxtImportPage::Review => "txt-review-body", TxtImportPage::Source => "txt-source-body", TxtImportPage::Rules => "txt-rules-body", TxtImportPage::Excerpt(_) => "txt-excerpt-body" })
                            .auto_shrink([false, false]).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            if let Some(e) = &state.error {
                                ui.colored_label(ui.visuals().error_fg_color,
                                    reader_core::txt::error_key(e).map(|k| self.i18n.t(k)).unwrap_or(e));
                                if ui.add_enabled(!busy, egui::Button::new(self.i18n.t("txt.retry"))).clicked() {state.work("analyze",ctx);}
                                ui.separator();
                            }
                            match state.page.clone() {
                                TxtImportPage::Review => {
                                    ui.strong(self.i18n.t("txt.book_info"));
                                    egui::Frame::group(ui.style()).inner_margin(16).corner_radius(12).show(ui, |ui| {
                                        egui::Grid::new("txt-metadata").num_columns(2).spacing([20.,12.]).show(ui, |ui| {
                                            ui.label(self.i18n.t("txt.book_title"));
                                            ui.add(egui::TextEdit::singleline(&mut state.title).desired_width(ui.available_width()));ui.end_row();
                                            ui.label(self.i18n.t("txt.author_optional"));
                                            ui.add(egui::TextEdit::singleline(&mut state.author).desired_width(ui.available_width()));ui.end_row();
                                        });
                                    });
                                    ui.small(self.i18n.t("txt.source_preserved"));
                                    ui.horizontal(|ui| {
                                        if ui.button(self.i18n.t("txt.add_heading")).clicked() {state.page=TxtImportPage::Source;if state.source_hits.is_empty(){state.work("lines",ctx);}}
                                        if ui.button(self.i18n.t("txt.advanced")).clicked() {state.page=TxtImportPage::Rules;}
                                    });
                                    ui.add_space(4.);
                                    ui.horizontal(|ui| {
                                        ui.strong(self.i18n.t("txt.preview_title"));
                                        ui.weak(self.i18n.count("txt.detected_count",state.heading_count));
                                        if !state.excluded.is_empty() && ui.add_enabled(!busy,egui::Button::new(self.i18n.t("txt.restore_titles")).small()).clicked(){state.excluded.clear();state.work("analyze",ctx);}
                                    });
                                    ui.small(self.i18n.t("txt.review_hint"));
                                    if state.chapter_count>500 {ui.small(self.i18n.count("txt.preview_limit",500));}
                                    if state.heading_count==0 && !busy && state.error.is_none(){ui.small(self.i18n.t("txt.fallback_hint"));}
                                    let mut changed=false;
                                    let mut detail=None;
                                    egui::Frame::group(ui.style()).inner_margin(egui::Margin::symmetric(12,4)).corner_radius(12).show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        for row in &state.previews {
                                            ui.push_id(row.line_start, |ui| {
                                                ui.horizontal(|ui| {
                                                    let mut selected=!state.excluded.contains(&row.line_start);
                                                    if (row.is_heading || state.included.contains(&row.line_start) || state.excluded.contains(&row.line_start))
                                                        && ui.add_enabled(!busy,egui::Checkbox::without_text(&mut selected)).on_hover_text(self.i18n.t(if selected {"txt.included"} else {"txt.excluded"})).changed() {
                                                        if selected {state.excluded.remove(&row.line_start);}else{state.excluded.insert(row.line_start);}changed=true;
                                                    }
                                                    let label=egui::RichText::new(&row.title).color(if selected {ui.visuals().text_color()}else{ui.visuals().weak_text_color()});
                                                    if ui.allocate_ui_with_layout(egui::vec2(ui.available_width()-64.,36.), egui::Layout::left_to_right(egui::Align::Center), |ui| { ui.set_min_width(ui.available_width()); ui.add(egui::Button::new(label).frame(false).wrap()) }).inner.on_hover_text(self.i18n.t("txt.excerpt")).clicked(){detail=Some(row.clone());}
                                                    if ui.button("›").on_hover_text(self.i18n.t("txt.excerpt")).clicked(){detail=Some(row.clone());}
                                                });
                                                ui.separator();
                                            });
                                        }
                                    });
                                    if let Some(row)=detail{state.page=TxtImportPage::Excerpt(row);}
                                    if changed {state.work("analyze",ctx);}
                                }
                                TxtImportPage::Rules => {
                                    let mut changed=ui.checkbox(&mut state.use_heuristic,self.i18n.t("txt.mode_heuristic")).changed();
                                    ui.weak(self.i18n.t("txt.heuristic_hint"));
                                    ui.add_space(8.);
                                    ui.label(self.i18n.t("txt.mode_regex"));
                                    changed |= ui.add(egui::TextEdit::multiline(&mut state.custom_regex).code_editor().desired_rows(3).desired_width(f32::INFINITY).hint_text(self.i18n.t("txt.rule_placeholder"))).changed();
                                    if changed {state.previews.clear();state.excluded.clear();state.included.clear();state.analyzed=false;}
                                    if ui.add_enabled(!busy,egui::Button::new(self.i18n.t("txt.retry"))).clicked(){state.work("analyze",ctx);}
                                }
                                TxtImportPage::Source => {
                                    ui.weak(self.i18n.t("txt.add_hint"));
                                    ui.horizontal(|ui| {
                                        ui.add(egui::TextEdit::singleline(&mut state.source_query).desired_width((ui.available_width()-130.).max(100.)).hint_text(self.i18n.t("txt.search_source")));
                                        if ui.add_enabled(!busy,egui::Button::new(self.i18n.t("txt.preview"))).clicked(){state.work("lines",ctx);}
                                    });
                                    let mut add=None;
                                    for row in &state.source_hits {
                                        egui::Frame::group(ui.style()).inner_margin(12).corner_radius(8).show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            ui.horizontal(|ui| {
                                                ui.add_sized([ui.available_width()-48., 24.], egui::Label::new(egui::RichText::new(&row.title).strong()).halign(egui::Align::Min).truncate());
                                                if ui.add_enabled(!busy,egui::Button::new("+")).on_hover_text(self.i18n.t("txt.select_heading")).clicked(){add=Some(row.line_start);}
                                            });
                                            ui.weak(&row.excerpt);
                                        });
                                    }
                                    if state.source_hits.is_empty() && !busy {ui.weak(self.i18n.t("txt.no_matches"));}
                                    if state.source_hits.len()>=80{ui.small(self.i18n.t("txt.search_limit"));}
                                    if let Some(line)=add{state.included.insert(line);state.excluded.remove(&line);state.page=TxtImportPage::Review;state.work("analyze",ctx);}
                                }
                                TxtImportPage::Excerpt(row) => {
                                    ui.heading(&row.title);
                                    egui::Frame::group(ui.style()).inner_margin(20).corner_radius(12).show(ui, |ui| {ui.set_width(ui.available_width());ui.label(&row.excerpt);});
                                }
                            }
                        });
                    });
                });
            });
        if convert {
            let state = self.txt_import.as_mut().unwrap();
            state.converting = true;
            let path = state.txt_path.clone();
            let directory = std::path::PathBuf::from(&self.data_dir).join("books");
            let options = reader_core::txt::ConvertOptions {
                title: Some(state.title.clone()),
                author: Some(state.author.clone()),
                custom_regex: (!state.custom_regex.is_empty()).then(|| state.custom_regex.clone()),
                use_heuristic: state.use_heuristic,
                ..Default::default()
            };
            let excluded = state.excluded.iter().copied().collect::<Vec<_>>();
            let included = state.included.iter().copied().collect::<Vec<_>>();
            let slot: TxtConvertSlot = Arc::new(Mutex::new(None));
            state.result_slot = Some(slot.clone());
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let result = std::fs::create_dir_all(&directory)
                    .map_err(|e| e.to_string())
                    .and_then(|_| {
                        reader_core::txt::convert_txt_to_epub_selected(
                            &path, &directory, &options, &excluded, &included,
                        )
                        .map_err(|e| e.to_string())
                    });
                *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
                ctx.request_repaint();
            });
        }
        if (!open || cancel) && !self.txt_import.as_ref().is_some_and(|s| s.converting) {
            self.txt_import = None;
        }
    }
}
