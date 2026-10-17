//! UI component for performing textual searches within the book.
use crate::app::ReaderApp;
use eframe::egui;

pub(crate) struct SearchTask {
    path: Option<String>,
    query: String,
    receiver: std::sync::mpsc::Receiver<Result<Vec<reader_core::search::SearchResult>, String>>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Drop for SearchTask {
    fn drop(&mut self) {
        self.cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
impl ReaderApp {
    /// Render the search side-panel.
    pub fn render_search_panel(&mut self, ctx: &egui::Context) {
        if let Some(task) = &self.search_task {
            if task.path != self.book_path {
                self.search_task = None;
            } else if let Ok(result) = task.receiver.try_recv() {
                if task.query == self.search_query {
                    match result {
                        Ok(results) => {
                            self.search_results = results;
                            self.search_selected = None;
                        }
                        Err(error) => self.error_msg = Some(error),
                    }
                }
                self.search_task = None;
            }
        }
        let collapse_label = self.i18n.t("common.collapse").to_owned();
        if super::design::animated_side_panel(
            ctx,
            "search_panel",
            false,
            320.,
            self.show_search && self.view == crate::app::AppView::Reader,
            &collapse_label,
            |ui| {
                super::design::sidebar_title(ui, self.i18n.t("search.title"));
                ui.add_space(4.0);

                let mut run_search = false;
                ui.horizontal(|ui| {
                    let resp = ui.add(
                        super::design::text_edit(&mut self.search_query)
                            .hint_text(self.i18n.t("search.placeholder"))
                            .desired_width(ui.available_width() - 100.0),
                    );
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        run_search = true;
                    }
                    if ui.button(self.i18n.t("search.go")).clicked() {
                        run_search = true;
                    }
                });

                if run_search && !self.search_query.is_empty() {
                    if let Some(book) = &self.book {
                        drop(self.search_task.take());
                        let book = book.clone();
                        let query = self.search_query.clone();
                        let worker_query = query.clone();
                        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                        let worker_cancel = cancel.clone();
                        let (sender, receiver) = std::sync::mpsc::channel();
                        let ctx = ctx.clone();
                        std::thread::spawn(move || {
                            let result = reader_core::search::search_book_checked(
                                &book,
                                &worker_query,
                                false,
                                || worker_cancel.load(std::sync::atomic::Ordering::Relaxed),
                            );
                            let _ = sender.send(result);
                            ctx.request_repaint();
                        });
                        self.search_task = Some(SearchTask {
                            path: self.book_path.clone(),
                            query,
                            receiver,
                            cancel,
                        });
                        self.search_selected = None;
                    }
                }

                super::design::block_gap(ui);
                if !self.search_results.is_empty() {
                    ui.label(
                        self.i18n
                            .count("search.results_count", self.search_results.len()),
                    );
                    ui.add_space(4.0);

                    egui::ScrollArea::vertical()
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            let results = self.search_results.clone();
                            for (idx, result) in results.iter().enumerate() {
                                let selected = self.search_selected == Some(idx);
                                let resp = ui.selectable_label(
                                    selected,
                                    format!("[{}] {}", result.chapter_title, result.context),
                                );
                                if resp.clicked() {
                                    self.search_selected = Some(idx);
                                    self.navigate_reader(
                                        super::reader_navigation::ReaderLocation {
                                            chapter: result.chapter_index,
                                            block: result.block_index,
                                            char_offset: 0,
                                        },
                                        super::reader_navigation::NavigationKind::Local,
                                    );
                                }
                            }
                        });
                } else if !self.search_query.is_empty() {
                    ui.label(self.i18n.t("search.no_results"));
                }
            },
        ) {
            self.show_search = false;
        }
    }
}
