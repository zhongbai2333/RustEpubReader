use crate::app::ReaderApp;
use eframe::egui;
use serde_json::{json, Value};

#[derive(Default)]
pub struct CategoryUi {
    pub filter: Option<String>,
    pub managing: bool,
    pub targets: Vec<String>,
    draft: Option<(String, String)>,
    deleting: Option<String>,
    failure: Option<String>,
}

impl ReaderApp {
    pub(super) fn begin_classification(&mut self, books: Vec<String>) {
        self.category_ui.targets = books;
        self.category_ui.managing = false;
        self.category_ui.draft = None;
        self.category_ui.deleting = None;
        self.category_ui.failure = None;
    }

    fn change_categories(&mut self, request: Value) -> bool {
        match reader_core::library::categories::command(&self.data_dir, &request) {
            Ok(_) => {
                self.reload_library();
                self.category_ui.failure = None;
                true
            }
            Err(error) => {
                self.category_ui.failure = Some(if error.starts_with("category.error_") {
                    self.i18n.t(&error).to_owned()
                } else {
                    error
                });
                false
            }
        }
    }

    pub(super) fn category_filter(&mut self, ui: &mut egui::Ui) {
        let state = self.library.classification.clone();
        if self
            .category_ui
            .filter
            .as_ref()
            .is_some_and(|id| !id.is_empty() && !state.categories.iter().any(|c| &c.id == id))
        {
            self.category_ui.filter = None;
        }
        let previous = self.category_ui.filter.clone();
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 120.).max(100.);
            ui.allocate_ui_with_layout(
                egui::vec2(width, 40.),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    egui::ScrollArea::horizontal()
                        .id_salt("category-tabs")
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.selectable_value(
                                    &mut self.category_ui.filter,
                                    None,
                                    self.i18n.t("ui.all_books"),
                                );
                                ui.selectable_value(
                                    &mut self.category_ui.filter,
                                    Some(String::new()),
                                    self.i18n.t("category.uncategorized"),
                                );
                                for category in &state.categories {
                                    let count = self
                                        .library
                                        .books
                                        .iter()
                                        .filter(|b| state.matches(&b.id, Some(&category.id)))
                                        .count();
                                    ui.selectable_value(
                                        &mut self.category_ui.filter,
                                        Some(category.id.clone()),
                                        format!("{} ({count})", category.name),
                                    );
                                }
                            });
                        });
                },
            );
            if ui.button(self.i18n.t("category.manage")).clicked() {
                self.category_ui.managing = !self.category_ui.managing;
                self.category_ui.targets.clear();
                self.category_ui.failure = None;
            }
        });
        if previous != self.category_ui.filter {
            self.library_selected.clear();
            self.category_ui.targets.clear();
            self.category_ui.managing = false;
            self.category_ui.draft = None;
        }
        self.category_panel(ui);
    }

    fn category_panel(&mut self, ui: &mut egui::Ui) {
        let state = self.library.classification.clone();
        let mut request = None;
        let mut cancel_name = false;
        if self.category_ui.managing || !self.category_ui.targets.is_empty() {
            let p = super::design::Palette::new(self.dark_mode);
            egui::Frame::default().fill(p.surface).corner_radius(12.).inner_margin(16.).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.strong(self.i18n.t(if self.category_ui.managing {"category.manage"} else {"category.assign"}));
                    if !self.category_ui.targets.is_empty() {ui.small(self.i18n.tf1("manage.count", &self.category_ui.targets.len().to_string()));}
                    if self.category_ui.managing && ui.button(self.i18n.t("category.create")).clicked() { self.category_ui.draft = Some((String::new(), String::new())); self.category_ui.deleting = None; }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(self.i18n.t("manage.done")).clicked() {
                            if self.category_ui.managing { self.category_ui.managing = false; self.category_ui.draft = None; }
                            else { self.category_ui.targets.clear(); }
                        }
                    });
                });
                if self.category_ui.managing {
                    ui.small(self.i18n.t("category.note"));
                    if let Some((id, name)) = &mut self.category_ui.draft {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(self.i18n.t(if id.is_empty() {"category.create"} else {"category.rename"}));
                            ui.add(egui::TextEdit::singleline(name).id_salt("category-name").hint_text(self.i18n.t("category.name")).char_limit(40).desired_width(220.));
                            if ui.add_enabled(!name.trim().is_empty(), egui::Button::new(self.i18n.t("error.ok"))).clicked() { request = Some(json!({"action":if id.is_empty() {"create"} else {"rename"},"id":id,"name":name})); }
                            if ui.button(self.i18n.t("settings.cancel")).clicked() { cancel_name = true; }
                        });
                    }
                    egui::ScrollArea::vertical().id_salt("category-management").max_height((ui.ctx().screen_rect().height() * 0.25).min(200.)).show(ui, |ui| {
                        for category in &state.categories {
                            ui.push_id(&category.id, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.add(egui::Label::new(&category.name).wrap());
                                    if ui.button(self.i18n.t("book.rename")).clicked() { self.category_ui.draft = Some((category.id.clone(), category.name.clone())); self.category_ui.deleting = None; }
                                    if ui.button(self.i18n.t("category.delete")).clicked() { self.category_ui.deleting = Some(category.id.clone()); self.category_ui.draft = None; }
                                });
                                if self.category_ui.deleting.as_deref() == Some(&category.id) {
                                    ui.small(self.i18n.t("category.delete_note"));
                                    ui.horizontal(|ui| {
                                        if ui.button(self.i18n.t("category.delete")).clicked() { request = Some(json!({"action":"delete","id":category.id})); }
                                        if ui.button(self.i18n.t("settings.cancel")).clicked() { self.category_ui.deleting = None; }
                                    });
                                }
                                ui.separator();
                            });
                        }
                    });
                } else {
                    let common = self.category_ui.targets.first().and_then(|id| state.assignments.get(id));
                    let same = self.category_ui.targets.iter().all(|id| state.assignments.get(id) == common);
                    egui::ScrollArea::horizontal().id_salt("category-assignment").show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.selectable_label(same && common.is_none(), self.i18n.t("category.uncategorized")).clicked() { request = Some(json!({"action":"assign","id":"","books":self.category_ui.targets})); }
                            for category in &state.categories {
                                if ui.selectable_label(same && common == Some(&category.id), &category.name).clicked() { request = Some(json!({"action":"assign","id":category.id,"books":self.category_ui.targets})); }
                            }
                        });
                    });
                }
                if let Some(failure) = &self.category_ui.failure { ui.colored_label(p.muted, failure); }
            });
        }
        if cancel_name {
            self.category_ui.draft = None;
        }
        if let Some(request) = request {
            if self.change_categories(request.clone()) {
                match request["action"].as_str() {
                    Some("assign") => self.category_ui.targets.clear(),
                    Some("create" | "rename") => self.category_ui.draft = None,
                    Some("delete") => {
                        self.category_ui.deleting = None;
                        if self.category_ui.filter.as_deref() == request["id"].as_str() {
                            self.category_ui.filter = None;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_save_after_external_archive_keeps_shelf_indices_valid() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let mut library = reader_core::library::Library::default();
        library
            .add_or_update_from_bytes(root, "Kept".into(), b"kept", 0, None)
            .unwrap();
        let removed = library
            .add_or_update_from_bytes(root, "Archived".into(), b"archived", 0, None)
            .unwrap();
        let mut app = ReaderApp::from_data_dir(root.into());
        app.category_ui.managing = true;
        app.category_ui.draft = Some((String::new(), "Fiction".into()));
        let confirm = app.i18n.t("error.ok").to_owned();
        let ctx = egui::Context::default();
        let input = |events| egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100., 800.),
            )),
            focused: true,
            events,
            ..Default::default()
        };
        let output = ctx.run(input(vec![]), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.render_library(ui));
        });
        let button = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == confirm => {
                    Some(text.pos + text.galley.rect.center().to_vec2())
                }
                _ => None,
            })
            .expect("inline category confirmation button");
        // The displayed snapshot still has two books when another writer hides one.
        library.hide_by_path(root, &removed.path).unwrap();
        assert_eq!(app.library.books.len(), 2);
        let click = |pressed| egui::Event::PointerButton {
            pos: button,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let _ = ctx.run(
            input(vec![
                egui::Event::PointerMoved(button),
                click(true),
                click(false),
            ]),
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.render_library(ui));
            },
        );
        assert_eq!(app.library.classification.categories.len(), 1);
        assert_eq!(app.library.books.len(), 1);
        assert_eq!(app.library.books[0].title, "Kept");
    }
}
