//! User Interface for viewing and managing book annotations and highlights.
use crate::app::ReaderApp;
use eframe::egui;
use reader_core::library::HighlightColor;

#[derive(Default)]
pub(crate) struct PanelState {
    book: Option<String>,
    tab: Option<usize>,
    details: Option<String>,
    previews: std::collections::HashMap<String, String>,
    order: std::collections::VecDeque<String>,
}

enum PanelAction {
    Jump(super::reader_navigation::ReaderLocation),
    Edit(String),
    Save(String),
    CancelEdit,
    DeleteHighlight(String),
    DeleteBookmark(usize, usize),
    Details(String),
    Revert(usize, usize, usize),
}

fn bounded_label(
    ui: &mut egui::Ui,
    text: &str,
    lines: usize,
    small: bool,
    clickable: bool,
) -> egui::Response {
    let style = if small {
        egui::TextStyle::Small
    } else {
        egui::TextStyle::Body
    };
    let mut job = egui::text::LayoutJob::simple(
        text.into(),
        ui.style().text_styles[&style].clone(),
        if small {
            ui.visuals().weak_text_color()
        } else {
            ui.visuals().text_color()
        },
        ui.available_width(),
    );
    job.wrap.max_rows = lines;
    job.wrap.overflow_character = Some('…');
    ui.add(egui::Label::new(job).sense(if clickable {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    }))
}

// A bounded excerpt always occupies two lines, including when an offscreen row
// has no cached text yet. Scrolling must not depend on estimated row heights.
fn excerpt_label(ui: &mut egui::Ui, text: &str, small: bool, clickable: bool) -> egui::Response {
    let style = if small {
        egui::TextStyle::Small
    } else {
        egui::TextStyle::Body
    };
    let size = egui::vec2(ui.available_width(), ui.text_style_height(&style) * 2.);
    ui.allocate_ui_with_layout(size, egui::Layout::top_down(egui::Align::LEFT), |ui| {
        ui.set_min_size(size);
        bounded_label(ui, text, 2, small, clickable)
    })
    .inner
}

// Allocate the same geometry for visible and skipped rows. Only visible rows
// create widgets or read chapter text, so large lists retain a bounded AX tree.
fn annotation_row(ui: &mut egui::Ui, key: impl std::hash::Hash, height: f32) -> Option<egui::Ui> {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return None;
    }
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(key)
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::LEFT)),
    );
    child.set_clip_rect(ui.clip_rect().intersect(rect.expand2(egui::vec2(6., 0.))));
    Some(child)
}

fn block_text(block: &reader_core::epub::ContentBlock) -> String {
    match block {
        reader_core::epub::ContentBlock::Paragraph { spans, .. }
        | reader_core::epub::ContentBlock::Heading { spans, .. } => {
            spans.iter().map(|s| s.text.as_str()).collect()
        }
        reader_core::epub::ContentBlock::Image { alt, .. } => alt.clone().unwrap_or_default(),
        _ => String::new(),
    }
}

fn excerpt_parts(
    parts: impl IntoIterator<Item = (usize, String)>,
    first: usize,
    last: usize,
    start: usize,
    end: usize,
) -> String {
    let mut result = String::new();
    let mut remaining = 240;
    for (index, text) in parts {
        if index < first || index > last {
            continue;
        }
        let count = text.chars().count();
        let a = if index == first { start.min(count) } else { 0 };
        let b = if index == last { end.min(count) } else { count };
        if a >= b {
            continue;
        }
        if !result.is_empty() {
            result.push(' ');
        }
        let take = (b - a).min(remaining);
        result.extend(text.chars().skip(a).take(take));
        remaining -= take;
        if remaining == 0 {
            break;
        }
    }
    result
}

fn note_editor(ui: &mut egui::Ui, buffer: &mut String, save_label: &str) -> egui::Response {
    ui.vertical(|ui| {
        let width = ui.available_width();
        let height = (ui.text_style_height(&egui::TextStyle::Body) * 4.).max(80.);
        ui.add_sized(
            [width, height],
            egui::TextEdit::multiline(buffer)
                .desired_rows(3)
                .desired_width(width),
        );
        ui.add(egui::Button::new(save_label).wrap())
    })
    .inner
}

pub(crate) enum AnnotationExportTask {
    Preparing(std::sync::mpsc::Receiver<Result<reader_core::annotations::MarkdownExport, String>>),
    Saving(std::sync::mpsc::Receiver<Result<(), String>>),
}
impl ReaderApp {
    fn start_annotation_export(&mut self, ctx: &egui::Context) {
        if self.annotation_export.is_some() {
            return;
        }
        let Some(config) = self.book_config.as_ref() else {
            return;
        };
        let (root, id, ctx) = (self.data_dir.clone(), config.id.clone(), ctx.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        self.annotation_export = Some(AnnotationExportTask::Preparing(rx));
        std::thread::spawn(move || {
            let _ = tx.send(reader_core::annotations::export_markdown(&root, &id));
            ctx.request_repaint();
        });
    }
    pub(crate) fn poll_annotation_export(&mut self, ctx: &egui::Context) {
        use std::sync::mpsc::TryRecvError;
        let Some(task) = self.annotation_export.as_ref() else {
            return;
        };
        match task {
            AnnotationExportTask::Preparing(rx) => match rx.try_recv() {
                Ok(Ok(document)) => {
                    self.annotation_export = None;
                    let Some(path) = rfd::FileDialog::new()
                        .set_title(self.i18n.t("annotations.export_markdown"))
                        .set_file_name(&document.filename)
                        .add_filter("Markdown", &["md"])
                        .save_file()
                    else {
                        return;
                    };
                    let path = path.with_extension("md");
                    let (tx, rx) = std::sync::mpsc::channel();
                    let ctx = ctx.clone();
                    self.annotation_export = Some(AnnotationExportTask::Saving(rx));
                    std::thread::spawn(move || {
                        let _ = tx.send(reader_core::storage::replace(
                            &path,
                            document.markdown.as_bytes(),
                        ));
                        ctx.request_repaint();
                    });
                }
                Ok(Err(error)) => {
                    self.annotation_export = None;
                    self.error_msg = Some(error);
                }
                Err(TryRecvError::Disconnected) => {
                    self.annotation_export = None;
                    self.error_msg = Some(self.i18n.t("annotations.export_failed").into());
                }
                Err(TryRecvError::Empty) => {}
            },
            AnnotationExportTask::Saving(rx) => match rx.try_recv() {
                Ok(result) => {
                    self.annotation_export = None;
                    if let Err(error) = result {
                        self.error_msg = Some(error);
                    }
                }
                Err(TryRecvError::Disconnected) => {
                    self.annotation_export = None;
                    self.error_msg = Some(self.i18n.t("annotations.export_failed").into());
                }
                Err(TryRecvError::Empty) => {}
            },
        }
        if self.annotation_export.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }
}

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
    fn annotation_preview(
        &mut self,
        key: String,
        chapter: usize,
        first: usize,
        last: usize,
        start: usize,
        end: usize,
    ) -> String {
        if let Some(value) = self.annotations_panel.previews.get(&key) {
            return value.clone();
        }
        let value = self
            .book
            .as_ref()
            .and_then(|book| book.chapter(chapter).ok())
            .map(|chapter| {
                excerpt_parts(
                    chapter
                        .blocks
                        .iter()
                        .enumerate()
                        .skip(first)
                        .take(last.saturating_sub(first) + 1)
                        .map(|(i, b)| (i, block_text(b))),
                    first,
                    last,
                    start,
                    end,
                )
            })
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| self.i18n.t("annotations.open_location").to_owned());
        if self.annotations_panel.previews.len() >= 128 {
            if let Some(old) = self.annotations_panel.order.pop_front() {
                self.annotations_panel.previews.remove(&old);
            }
        }
        self.annotations_panel.order.push_back(key.clone());
        self.annotations_panel.previews.insert(key, value.clone());
        value
    }
    fn annotation_menu(
        &self,
        ui: &mut egui::Ui,
        highlight: Option<&str>,
        bookmark: Option<(usize, usize)>,
        note: bool,
        action: &mut Option<PanelAction>,
    ) {
        let response = super::design::menu_button(
            ui,
            egui::Button::new("⋯")
                .frame(false)
                .min_size(egui::vec2(28., 26.)),
            |ui| {
                if let Some(id) = highlight {
                    if ui
                        .add_enabled(
                            self.editing_note_id.is_none()
                                || self.editing_note_id.as_deref() == Some(id),
                            egui::Button::new(self.i18n.t("annotations.edit_note")).frame(false),
                        )
                        .clicked()
                    {
                        *action = Some(PanelAction::Edit(id.to_owned()));
                        ui.memory_mut(|memory| memory.close_popup());
                    }
                    if ui
                        .button(self.i18n.t("annotations.details_action"))
                        .clicked()
                    {
                        *action = Some(PanelAction::Details(format!(
                            "{}:{id}",
                            if note { "n" } else { "h" }
                        )));
                        ui.memory_mut(|memory| memory.close_popup());
                    }
                    ui.separator();
                    if ui
                        .add_enabled(
                            self.editing_note_id.as_deref() != Some(id),
                            egui::Button::new(self.i18n.t("library.delete")),
                        )
                        .clicked()
                    {
                        *action = Some(PanelAction::DeleteHighlight(id.to_owned()));
                        ui.memory_mut(|memory| memory.close_popup());
                    }
                } else if let Some((chapter, block)) = bookmark {
                    if ui
                        .button(self.i18n.t("annotations.details_action"))
                        .clicked()
                    {
                        *action = Some(PanelAction::Details(format!("b:{chapter}:{block}")));
                        ui.memory_mut(|memory| memory.close_popup());
                    }
                    ui.separator();
                    if ui.button(self.i18n.t("library.delete")).clicked() {
                        *action = Some(PanelAction::DeleteBookmark(chapter, block));
                        ui.memory_mut(|memory| memory.close_popup());
                    }
                }
            },
        );
        response.response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                ui.is_enabled(),
                self.i18n.t("annotations.more_actions"),
            )
        });
        response
            .response
            .on_hover_text(self.i18n.t("annotations.more_actions"));
    }
    fn annotation_chapter(&self, chapter: usize) -> String {
        self.book
            .as_ref()
            .and_then(|b| b.chapters.get(chapter))
            .map(|c| c.title.clone())
            .unwrap_or_else(|| {
                self.i18n
                    .tf1("annotations.chapter", &(chapter + 1).to_string())
            })
    }
    pub fn render_annotations_panel(&mut self, ctx: &egui::Context) {
        let book = self.book_config.as_ref().map(|c| c.id.clone());
        if self.annotations_panel.book != book {
            self.annotations_panel = PanelState {
                book,
                ..Default::default()
            };
        }
        let counts = self
            .book_config
            .as_ref()
            .map(|c| {
                [
                    c.bookmarks.len(),
                    c.highlights.len(),
                    c.corrections
                        .iter()
                        .filter(|r| r.status == "accepted" || r.status == "rejected")
                        .count(),
                ]
            })
            .unwrap_or([0; 3]);
        let tab = self.annotations_panel.tab.unwrap_or(if counts[1] > 0 {
            1
        } else if counts[0] > 0 {
            0
        } else if counts[2] > 0 {
            2
        } else {
            0
        });
        let mut action = None;
        let collapse = self.i18n.t("common.collapse").to_owned();
        let width = (ctx.screen_rect().width() * 0.35).clamp(300., 360.);
        if super::design::animated_side_panel(
            ctx,
            "annotations_panel",
            false,
            width,
            self.show_annotations && self.view == crate::app::AppView::Reader,
            &collapse,
            |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(self.i18n.t("annotations.title"))
                            .size(17.)
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.annotation_export.is_some() {
                            ui.spinner();
                        } else if ui
                            .add_enabled_ui(counts[0] + counts[1] > 0, |ui| {
                                super::design::icon_button(
                                    ui,
                                    super::design::Icon::Export,
                                    self.i18n.t("annotations.export_all_hint"),
                                    false,
                                )
                            })
                            .inner
                            .clicked()
                        {
                            self.start_annotation_export(ctx);
                        }
                    });
                });
                ui.horizontal_wrapped(|ui| {
                    for (index, key) in [
                        "annotations.bookmarks",
                        "annotations.tab_highlights",
                        "annotations.corrections",
                    ]
                    .iter()
                    .enumerate()
                    {
                        if ui
                            .selectable_label(
                                tab == index,
                                format!("{} {}", self.i18n.t(key), counts[index]),
                            )
                            .clicked()
                        {
                            self.annotations_panel.tab = Some(index);
                        }
                    }
                });
                ui.add_space(6.);
                ui.separator();
                ui.add_space(4.);
                let row_font = ui.text_style_height(&egui::TextStyle::Body);
                let small = ui.text_style_height(&egui::TextStyle::Small);
                egui::ScrollArea::vertical().id_salt(("annotation_rows",tab)).auto_shrink([false,false]).show_viewport(ui,|ui,_viewport| {
                match tab {
                    0 => {
                        let mut entries=self.book_config.as_ref().map(|c|c.bookmarks.clone()).unwrap_or_default();
                        entries.sort_by_key(|b|(b.chapter,b.block));
                        if entries.is_empty() {ui.label(self.i18n.t("annotations.no_bookmarks"));}
                        for bookmark in entries {
                            let height=ui.spacing().interact_size.y.max(26.)+small*2.+24.;
                            let Some(mut row)=annotation_row(ui,("bookmark",bookmark.chapter,bookmark.block),height) else {continue;};
                            let ui=&mut row;
                            let title=self.annotation_chapter(bookmark.chapter);
                            let preview=self.annotation_preview(format!("b:{}:{}",bookmark.chapter,bookmark.block),bookmark.chapter,bookmark.block,bookmark.block,0,240);
                            ui.push_id((bookmark.chapter,bookmark.block),|ui| {
                                egui::Frame::default().inner_margin(egui::Margin::symmetric(0,6)).show(ui,|ui| {
                                    ui.horizontal(|ui| {
                                        let available=ui.available_width()-36.;
                                        ui.allocate_ui_with_layout(egui::vec2(available,row_font),egui::Layout::top_down(egui::Align::LEFT),|ui| {
                                            ui.set_min_width(available);
                                            if bounded_label(ui,&title,1,false,true).clicked() {action=Some(PanelAction::Jump(super::reader_navigation::ReaderLocation{chapter:bookmark.chapter,block:bookmark.block,char_offset:0}));}
                                        });
                                        self.annotation_menu(ui,None,Some((bookmark.chapter,bookmark.block)),false,&mut action);
                                    });
                                    excerpt_label(ui,&preview,true,false);
                                });
                            });
                        }
                    }
                    1 => {
                        let mut entries=self.book_config.as_ref().map(|c|c.highlights.clone()).unwrap_or_default();
                        entries.sort_by_key(|h|(h.chapter,h.start_block,h.start_offset));
                        let notes:std::collections::HashMap<_,_>=self.book_config.as_ref().map(|c|c.notes.iter().enumerate().map(|(i,n)|(n.highlight_id.clone(),i)).collect()).unwrap_or_default();
                        if entries.is_empty() {ui.label(self.i18n.t("annotations.no_highlights"));}
                        for highlight in entries {
                            let has_note=notes.contains_key(&highlight.id);
                            let editing=self.editing_note_id.as_deref()==Some(&highlight.id);
                            let menu_height=ui.spacing().interact_size.y.max(26.);
                            let height=row_font*2.+menu_height+24.+if editing {(row_font*4.).max(80.)+ui.spacing().interact_size.y.max(30.)+small.max(18.)+24.} else if has_note {row_font+4.} else {0.};
                            let Some(mut row)=annotation_row(ui,("highlight",&highlight.id),height) else {continue;};
                            let ui=&mut row;
                            let preview=self.annotation_preview(format!("h:{}:{}:{}:{}:{}:{}",highlight.id,highlight.chapter,highlight.start_block,highlight.end_block,highlight.start_offset,highlight.end_offset),highlight.chapter,highlight.start_block,highlight.end_block,highlight.start_offset,highlight.end_offset);
                            let title=self.annotation_chapter(highlight.chapter);
                            let note=notes.get(&highlight.id).and_then(|i|self.book_config.as_ref()?.notes.get(*i)).map(|n|n.content.chars().take(240).collect::<String>());
                            let concurrent=self.book_config.as_ref().and_then(|c|c.annotation_state.entries.get(&format!("n:{}",highlight.id))).is_some_and(|r|r.len()>1);
                            ui.push_id(&highlight.id,|ui| {
                                egui::Frame::default().inner_margin(egui::Margin::symmetric(0,6)).show(ui,|ui| {
                                    let start=ui.cursor().min;
                                    let color=hl_color_info(&highlight.color).0;
                                    ui.scope(|ui| {
                                        ui.spacing_mut().item_spacing.y=4.;
                                        if excerpt_label(ui,&preview,false,true).on_hover_text(self.i18n.t("annotations.open_location")).clicked() {action=Some(PanelAction::Jump(super::reader_navigation::ReaderLocation{chapter:highlight.chapter,block:highlight.start_block,char_offset:highlight.start_offset}));}
                                        if editing {
                                            let label=self.i18n.t(if concurrent {"annotations.save_merge"} else {"common.save"}).to_owned();
                                            if note_editor(ui,&mut self.editing_note_buf,&label).clicked() {action=Some(PanelAction::Save(highlight.id.clone()));}
                                            if ui.small_button(self.i18n.t("common.cancel")).clicked() {action=Some(PanelAction::CancelEdit);}
                                        } else if let Some(note)=&note {
                                            if bounded_label(ui,note,1,false,true).on_hover_text(self.i18n.t("annotations.edit_note")).clicked() && self.editing_note_id.is_none() {action=Some(PanelAction::Edit(highlight.id.clone()));}
                                        }
                                        ui.horizontal(|ui| {
                                            let metadata_width=(ui.available_width()-36.).max(40.);
                                            ui.allocate_ui_with_layout(egui::vec2(metadata_width,small+4.),egui::Layout::left_to_right(egui::Align::Center),|ui| {
                                                ui.set_width(metadata_width);
                                                ui.colored_label(color,"●");
                                                if concurrent {ui.small(egui::RichText::new(self.i18n.t("annotations.concurrent_versions")).color(ui.visuals().selection.stroke.color));}
                                                ui.add(egui::Label::new(egui::RichText::new(&title).small().color(ui.visuals().weak_text_color())).truncate()).on_hover_text(&title);
                                            });
                                            self.annotation_menu(ui,Some(&highlight.id),None,has_note,&mut action);
                                        });
                                    });
                                    // A thin accent identifies the highlight without another nested card.
                                    let end=ui.cursor().min;
                                    ui.painter().line_segment([start-egui::vec2(5.,0.),egui::pos2(start.x-5.,end.y-4.)],egui::Stroke::new(2_f32,color));
                                });
                            });
                        }
                    }
                    _ => {
                        let entries=self.book_config.as_ref().map(|c|c.corrections.clone()).unwrap_or_default();
                        let entries:Vec<_>=entries.into_iter().filter(|r|r.status=="accepted"||r.status=="rejected").collect();
                        if entries.is_empty() {ui.label(self.i18n.t("annotations.no_corrections"));}
                        for item in entries {
                            ui.push_id((item.chapter,item.block_idx,item.char_offset),|ui| {
                                let title=self.annotation_chapter(item.chapter);
                                ui.add_space(6.);ui.small(title);
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(egui::RichText::new(&item.original).strikethrough());ui.label("→");ui.label(&item.corrected);
                                    if ui.small_button("↩").on_hover_text(self.i18n.t("annotations.revert_correction")).clicked() {action=Some(PanelAction::Revert(item.chapter,item.block_idx,item.char_offset));}
                                });ui.add_space(6.);ui.separator();
                            });
                        }
                    }
                }
            });
            },
        ) {
            self.show_annotations = false;
        }
        if let Some(action) = action {
            self.apply_annotation_action(action);
        }
        self.annotation_details_modal(ctx);
    }
    fn apply_annotation_action(&mut self, action: PanelAction) {
        match action {
            PanelAction::Jump(location) => {
                self.navigate_reader(location, super::reader_navigation::NavigationKind::Local)
            }
            PanelAction::Details(key) => self.annotations_panel.details = Some(key),
            PanelAction::CancelEdit => {
                self.editing_note_id = None;
                self.editing_note_buf.clear();
                self.editing_note_original = None;
            }
            PanelAction::Edit(id) => {
                let original = self
                    .book_config
                    .as_ref()
                    .and_then(|c| c.notes.iter().find(|n| n.highlight_id == id))
                    .map(|n| n.content.clone());
                self.editing_note_buf = original.clone().unwrap_or_default();
                self.editing_note_original = original;
                self.editing_note_id = Some(id);
            }
            PanelAction::Save(id) => {
                if let Some(book) = self.book_config.as_ref().map(|c| c.id.clone()) {
                    match reader_core::annotations::save_note(
                        &self.data_dir,
                        &book,
                        &id,
                        &self.editing_note_buf,
                        Some(self.editing_note_original.as_deref()),
                    ) {
                        Ok(config) => {
                            self.book_config = Some(config);
                            self.editing_note_id = None;
                            self.editing_note_buf.clear();
                            self.editing_note_original = None;
                        }
                        Err(error) => self.error_msg = Some(error),
                    }
                }
            }
            other => {
                let Some(mut config) = self.book_config.clone() else {
                    return;
                };
                let mut reverted = None;
                match other {
                    PanelAction::DeleteHighlight(id) => {
                        config.highlights.retain(|h| h.id != id);
                        config.notes.retain(|n| n.highlight_id != id);
                    }
                    PanelAction::DeleteBookmark(chapter, block) => config
                        .bookmarks
                        .retain(|b| b.chapter != chapter || b.block != block),
                    PanelAction::Revert(chapter, block, offset) => {
                        config.corrections.retain(|r| {
                            !(r.chapter == chapter
                                && r.block_idx == block
                                && r.char_offset == offset)
                        });
                        reverted = Some((chapter, block, offset));
                    }
                    _ => unreachable!(),
                }
                match config.save(&self.data_dir) {
                    Ok(()) => {
                        self.book_config = Some(config);
                        if let Some((chapter, block, offset)) = reverted {
                            if let Some(correction) = self
                                .csc_cache
                                .get_mut(&(chapter, block))
                                .and_then(|rows| rows.iter_mut().find(|c| c.char_offset == offset))
                            {
                                correction.status = reader_core::epub::CorrectionStatus::Pending;
                            }
                        }
                    }
                    Err(error) => self.error_msg = Some(error),
                }
            }
        }
    }
    fn annotation_details_modal(&mut self, ctx: &egui::Context) {
        let Some(key) = self.annotations_panel.details.as_ref() else {
            return;
        };
        if !self.show_annotations {
            self.annotations_panel.details = None;
            return;
        }
        let versions = self
            .book_config
            .as_ref()
            .and_then(|c| c.annotation_state.entries.get(key));
        let mut close = false;
        let response = egui::Modal::new(egui::Id::new("annotation_details")).show(ctx, |ui| {
            ui.set_width((ctx.screen_rect().width() - 96.).clamp(240., 440.));
            ui.horizontal(|ui| {
                ui.strong(self.i18n.t("annotations.details_action"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close = super::design::close_button(ui, self.i18n.t("dialog.close")).clicked();
                });
            });
            ui.add_space(10.);
            egui::ScrollArea::vertical()
                .max_height((ctx.screen_rect().height() - 200.).max(120.))
                .show(ui, |ui| {
                    if let Some(versions) = versions {
                        if versions.len() > 1 {
                            ui.label(self.i18n.t("annotations.concurrent_versions"));
                            ui.add_space(8.);
                        }
                        for version in versions {
                            let source = if version.source.starts_with("legacy-") {
                                self.i18n.t("annotations.legacy_source").to_owned()
                            } else {
                                version.source.chars().take(12).collect()
                            };
                            let date = i64::try_from(version.edited_at)
                                .ok()
                                .filter(|t| *t > 0 && *t < 253402300800)
                                .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
                                .map(|t| {
                                    t.with_timezone(&chrono::Local)
                                        .format("%Y-%m-%d %H:%M")
                                        .to_string()
                                })
                                .unwrap_or_else(|| {
                                    self.i18n.t("annotations.unknown_time").to_owned()
                                });
                            ui.label(format!(
                                "{} · {}",
                                self.i18n.t("annotations.source_device"),
                                source
                            ))
                            .on_hover_text(&version.source);
                            ui.small(date);
                            if let Some(reader_core::annotations::Annotation::Note(note)) =
                                &version.value
                            {
                                ui.add_space(6.);
                                ui.add(egui::Label::new(&note.content).selectable(true).wrap());
                            }
                            ui.add_space(8.);
                            ui.separator();
                            ui.add_space(8.);
                        }
                    } else {
                        ui.label(self.i18n.t("annotations.legacy_source"));
                    }
                });
        });
        if response.should_close() || close {
            self.annotations_panel.details = None;
        }
    }
}

#[cfg(test)]
mod editor_tests {
    use super::*;
    #[test]
    fn large_list_keeps_extent_stable_and_only_builds_visible_rows() {
        let ctx = egui::Context::default();
        let mut extents = Vec::new();
        for offset in [0., 400., 1800., 8000.] {
            for pass in 0..2 {
                let mut rendered = 0;
                let mut extent = 0.;
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(400., 400.),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let out = egui::ScrollArea::vertical()
                                .max_height(280.)
                                .vertical_scroll_offset(offset)
                                .show(ui, |ui| {
                                    for index in 0..200 {
                                        let height = if index % 3 == 0 { 80. } else { 100. };
                                        if let Some(mut row) = annotation_row(ui, index, height) {
                                            rendered += 1;
                                            row.label(format!("Annotation {index}"));
                                        }
                                    }
                                });
                            extent = out.content_size.y;
                        });
                    },
                );
                if pass == 1 {
                    assert!(
                        (1..=6).contains(&rendered),
                        "Built {rendered} rows at {offset}"
                    );
                    extents.push(extent);
                }
            }
        }
        for extent in &extents {
            assert!(
                (extent - extents[0]).abs() < 0.5,
                "Scroll changed the list extent: {extents:?}"
            );
        }
    }

    #[test]
    fn failed_delete_preserves_the_visible_annotation_and_note() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().to_str().unwrap();
        let mut library = reader_core::library::Library::load_from(directory).unwrap();
        let entry = library
            .add_or_update_from_bytes(
                directory,
                "Annotations".into(),
                include_bytes!("../../../ios/Tests/fixtures/native-reading-modes.epub"),
                0,
                None,
            )
            .unwrap();
        let mut config = reader_core::library::Library::load_book_config(directory, &entry.id)
            .unwrap()
            .unwrap();
        let id = reader_core::annotations::new_highlight_id();
        config.highlights.push(reader_core::library::Highlight {
            id: id.clone(),
            chapter: 0,
            start_block: 0,
            start_offset: 0,
            end_block: 0,
            end_offset: 2,
            color: HighlightColor::Yellow,
            created_at: 1,
        });
        config.notes.push(reader_core::library::Note {
            highlight_id: id.clone(),
            content: "Keep this draft".into(),
            created_at: 1,
            updated_at: 1,
        });
        let mut app = ReaderApp::from_data_dir(directory.into());
        app.book_config = Some(config);
        let blocked = root.path().join("blocked-root");
        std::fs::write(&blocked, b"not a directory").unwrap();
        app.data_dir = blocked.to_str().unwrap().into();
        app.apply_annotation_action(PanelAction::DeleteHighlight(id));
        assert!(app.error_msg.is_some());
        let config = app.book_config.as_ref().unwrap();
        assert_eq!(config.highlights.len(), 1);
        assert_eq!(config.notes[0].content, "Keep this draft");
    }
    #[test]
    fn preview_uses_unicode_scalar_offsets_across_blocks_and_bounds_large_selections() {
        let parts = vec![(1, "A😀中文".to_owned()), (2, "跨段尾部".to_owned())];
        assert_eq!(excerpt_parts(parts, 1, 2, 1, 2), "😀中文 跨段");
        let huge = vec![(0, "阅".repeat(100_000))];
        assert_eq!(excerpt_parts(huge, 0, 0, 0, 100_000).chars().count(), 240);
        assert_eq!(excerpt_parts(vec![(0, "短文".into())], 0, 0, 99, 100), "");
    }
    #[test]
    fn multiline_note_and_save_control_fit_narrow_panels_and_large_text_in_both_languages() {
        for width in [200., 240., 320.] {
            for scale in [1., 1.5, 2.] {
                for label in ["保存", "Save", "保存并合并", "Save and merge"] {
                    let ctx = egui::Context::default();
                    super::super::design::apply(&ctx, true);
                    ctx.style_mut(|style| {
                        for font in style.text_styles.values_mut() {
                            font.size *= scale;
                        }
                    });
                    let original = "Local 中文😀\n\n---\n\nRemote text";
                    let mut buffer = original.to_owned();
                    let _=ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(width+16.,400.))),..Default::default()},|ctx|{
                egui::CentralPanel::default().show(ctx,|ui|{
                    let available=ui.available_rect_before_wrap();let response=note_editor(ui,&mut buffer,label);
                    assert!(response.rect.max.x<=available.max.x+1.,"{label} width={width} scale={scale}: {:?} outside {available:?}",response.rect);
                    assert!(ui.clip_rect().contains_rect(response.rect));
                });
            });
                    assert_eq!(buffer, original);
                }
            }
        }
    }
}
