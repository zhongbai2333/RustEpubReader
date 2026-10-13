//! Interface for configuring TXT file import and parsing rules.
use crate::app::{ReaderApp, TxtConvertSlot};
use eframe::egui;
use reader_core::txt::{ConvertOptions, SplitConfig};
use std::sync::{Arc, Mutex};

impl ReaderApp {
    pub fn render_txt_import(&mut self, ctx: &egui::Context) {
        let Some(state) = &mut self.txt_import else {
            return;
        };

        // 检查后台转换结果
        let mut convert_result = None;
        if let Some(slot) = &state.result_slot {
            if let Ok(mut guard) = slot.try_lock() {
                if let Some(result) = guard.take() {
                    convert_result = Some(result);
                }
            }
        }
        if let Some(result) = convert_result {
            match result {
                Ok(cr) => {
                    let epub_path = cr.epub_path.to_string_lossy().to_string();
                    self.txt_import = None;
                    self.open_book_from_path(&epub_path, None);
                    return;
                }
                Err(e) => {
                    if let Some(s) = &mut self.txt_import {
                        s.converting = false;
                        s.error = Some(e);
                        s.result_slot = None;
                    }
                }
            }
            return;
        }

        let mut open = true;
        let title = self.i18n.t("txt.import_title");
        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .min_width(420.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let state = self.txt_import.as_mut().unwrap();

                ui.add_space(8.0);

                // ── 书名 ──
                ui.horizontal(|ui| {
                    ui.label(self.i18n.t("txt.book_title"));
                    ui.add(super::design::text_edit(&mut state.title));
                });
                ui.add_space(4.0);

                // ── 作者 ──
                ui.horizontal(|ui| {
                    ui.label(self.i18n.t("txt.author"));
                    ui.add(super::design::text_edit(&mut state.author));
                });
                ui.add_space(8.0);

                // ── 章节检测模式 ──
                ui.label(self.i18n.t("txt.split_mode"));

                let mut need_refresh = false;
                ui.horizontal(|ui| {
                    if ui
                        .radio(
                            !state.use_heuristic && state.custom_regex.is_empty(),
                            self.i18n.t("txt.mode_auto"),
                        )
                        .clicked()
                    {
                        state.use_heuristic = false;
                        state.custom_regex.clear();
                        need_refresh = true;
                    }
                    if ui
                        .radio(state.use_heuristic, self.i18n.t("txt.mode_heuristic"))
                        .clicked()
                    {
                        state.use_heuristic = !state.use_heuristic;
                        need_refresh = true;
                    }
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(self.i18n.t("txt.mode_regex"));
                    let re = ui.add(super::design::text_edit(&mut state.custom_regex));
                    if re.lost_focus() {
                        need_refresh = true;
                    }
                });

                // ── 刷新预览 ──
                if need_refresh {
                    let config = SplitConfig {
                        min_chapter_chars: 100,
                        use_heuristic: state.use_heuristic,
                        custom_regex: if state.custom_regex.is_empty() {
                            None
                        } else {
                            Some(state.custom_regex.clone())
                        },
                    };
                    state.previews = reader_core::txt::preview_chapters(&state.txt_path, &config)
                        .unwrap_or_default();
                }

                ui.add_space(8.0);
                ui.separator();

                // ── 预览列表 ──
                let count_text = self
                    .i18n
                    .t("txt.chapter_count")
                    .replace("{}", &state.previews.len().to_string());
                ui.label(
                    egui::RichText::new(format!("{} — {}", self.i18n.t("txt.preview"), count_text))
                        .strong(),
                );

                if state.previews.is_empty() {
                    ui.label(self.i18n.t("txt.no_chapters"));
                } else {
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            let state = self.txt_import.as_ref().unwrap();
                            for (i, ch) in state.previews.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(format!("{}.", i + 1))
                                            .weak()
                                            .monospace(),
                                    );
                                    ui.label(&ch.title);
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                egui::RichText::new(format!("{}字", ch.char_count))
                                                    .weak(),
                                            );
                                        },
                                    );
                                });
                            }
                        });
                }

                ui.add_space(8.0);

                // ── 错误提示 ──
                if let Some(e) = &self.txt_import.as_ref().unwrap().error {
                    ui.colored_label(egui::Color32::RED, e);
                    ui.add_space(4.0);
                }

                // ── 转换按钮 ──
                let converting = self.txt_import.as_ref().unwrap().converting;
                ui.horizontal(|ui| {
                    let btn_text = if converting {
                        self.i18n.t("txt.converting")
                    } else {
                        self.i18n.t("txt.convert")
                    };
                    if ui
                        .add_enabled(!converting, egui::Button::new(btn_text))
                        .clicked()
                    {
                        let state = self.txt_import.as_mut().unwrap();
                        state.converting = true;
                        state.error = None;

                        let txt_path = state.txt_path.clone();
                        let output_dir = std::path::PathBuf::from(&self.data_dir).join("books");
                        let _ = std::fs::create_dir_all(&output_dir);

                        let options = ConvertOptions {
                            title: Some(state.title.clone()),
                            author: if state.author.is_empty() {
                                None
                            } else {
                                Some(state.author.clone())
                            },
                            custom_regex: if state.custom_regex.is_empty() {
                                None
                            } else {
                                Some(state.custom_regex.clone())
                            },
                            use_heuristic: state.use_heuristic,
                            ..Default::default()
                        };

                        let slot: TxtConvertSlot = Arc::new(Mutex::new(None));
                        let slot_clone = slot.clone();
                        state.result_slot = Some(slot);

                        std::thread::spawn(move || {
                            let result = reader_core::txt::convert_txt_to_epub(
                                &txt_path,
                                &output_dir,
                                &options,
                            )
                            .map_err(|e| e.to_string());
                            if let Ok(mut s) = slot_clone.lock() {
                                *s = Some(result);
                            }
                        });

                        ctx.request_repaint();
                    }
                });
            });

        if !open {
            self.txt_import = None;
        }
    }
}
