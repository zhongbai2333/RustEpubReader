use crate::app::ReaderApp;
use eframe::egui;
use serde_json::{json, Value};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

struct Event {
    op: String,
    result: Result<Value, String>,
    state: Value,
}
pub struct ShareUI {
    pub receive: bool,
    pub link: String,
    pub stage: usize,
    state: Value,
    pub(crate) busy: bool,
    error: Option<String>,
    result: Option<(u64, u64)>,
    operation: String,
    receiver: Option<mpsc::Receiver<Event>>,
    root: Option<String>,
    qr: Option<(String, egui::TextureHandle)>,
    poll: Instant,
    delete: Option<String>,
    copied: Option<Instant>,
}
impl Default for ShareUI {
    fn default() -> Self {
        Self {
            receive: false,
            link: String::new(),
            stage: 0,
            state: json!({}),
            busy: false,
            error: None,
            result: None,
            operation: String::new(),
            receiver: None,
            root: None,
            qr: None,
            poll: Instant::now(),
            delete: None,
            copied: None,
        }
    }
}
impl Drop for ShareUI {
    fn drop(&mut self) {
        if let Some(root) = &self.root {
            reader_core::book_share::stop(root);
        }
    }
}
impl ShareUI {
    pub fn cancel_authenticated_work(&mut self, root: &str) {
        if self.busy && matches!(self.operation.as_str(), "publish" | "delete_share") {
            self.cancel(root);
        }
    }

    pub fn initialize(&mut self, root: &str) {
        if self.root.is_none() {
            self.root = Some(root.into());
            self.state = reader_core::book_share::state(root);
        }
    }
    pub fn open_receive(&mut self, root: &str) {
        self.receive = true;
        self.initialize(root);
    }
    fn start(&mut self, root: &str, mut request: Value, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        let op = request["op"].as_str().unwrap_or("").to_string();
        if matches!(
            op.as_str(),
            "start_lan" | "receive" | "publish" | "delete_share"
        ) {
            match reader_core::book_share::dispatch(root, json!({"op":"arm","operation":op})) {
                Ok(armed) => request["job_id"] = armed["job_id"].clone(),
                Err(error) => {
                    self.error = Some(error);
                    return;
                }
            }
        }
        self.root = Some(root.into());
        self.state = reader_core::book_share::state(root);
        self.copied = None;
        self.operation = op.clone();
        self.busy = true;
        self.error = None;
        self.result = None;
        let root = root.to_owned();
        let ctx = ctx.clone();
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        std::thread::spawn(move || {
            let result = reader_core::book_share::dispatch(&root, request);
            let state = reader_core::book_share::state(&root);
            let _ = sender.send(Event { op, result, state });
            ctx.request_repaint();
        });
    }
    pub(super) fn cancel(&mut self, root: &str) {
        reader_core::book_share::stop(root);
        self.receiver = None;
        self.busy = false;
        self.qr = None;
        self.state = reader_core::book_share::state(root);
        self.error = None;
    }
    pub(super) fn phase(&self) -> &str {
        self.state["progress"]["phase"].as_str().unwrap_or("idle")
    }
    pub(super) fn active_lan(&self) -> bool {
        self.state["outgoing"]["kind"].as_str() == Some("lan")
            && matches!(self.phase(), "waiting" | "transferring")
    }
    pub(super) fn shows_qr(&self) -> bool {
        matches!(self.phase(), "waiting" | "published") && self.state["outgoing"].is_object()
    }
    pub(super) fn copy_link(&mut self, ctx: &egui::Context) {
        if let Some(link) = self.state["outgoing"]["link"].as_str() {
            ctx.copy_text(link.into());
            self.copied = Some(Instant::now());
            ctx.request_repaint_after(Duration::from_millis(2600));
        }
    }

    pub(super) fn back(&mut self, root: &str) {
        if self.stage == 1 || self.busy {
            self.cancel(root);
        }
        self.stage = 0;
    }
}
impl ReaderApp {
    pub fn tick_book_share(&mut self, ctx: &egui::Context) {
        let event = self
            .book_share
            .receiver
            .as_ref()
            .and_then(|r| r.try_recv().ok());
        if let Some(event) = event {
            self.book_share.busy = false;
            self.book_share.receiver = None;
            self.book_share.state = event.state;
            match event.result {
                Ok(value) => {
                    if event.op == "accept" {
                        self.library = reader_core::library::Library::load_from(&self.data_dir);
                        self.book_share.result = Some((
                            value["imported"].as_u64().unwrap_or(0),
                            value["skipped"].as_u64().unwrap_or(0),
                        ));
                    }
                    if event.op == "decode_image" {
                        self.book_share.link = value["link"].as_str().unwrap_or("").into();
                        let link = self.book_share.link.clone();
                        self.book_share.start(
                            &self.data_dir,
                            json!({"op":"receive","link":link}),
                            ctx,
                        );
                    }
                }
                Err(error) => self.book_share.error = Some(error),
            }
        }
        if self.book_share.root.is_some()
            && self.book_share.poll.elapsed() > Duration::from_millis(500)
        {
            self.book_share.state = reader_core::book_share::state(&self.data_dir);
            self.book_share.poll = Instant::now();
        }
        // A visible QR window can remain useful while another desktop window has
        // keyboard focus. End the temporary share when this window is minimized.
        if self.book_share.active_lan() && ctx.input(|i| i.viewport().minimized) == Some(true) {
            self.book_share.cancel(&self.data_dir);
        }
        if self.book_share.busy || self.book_share.active_lan() {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
    }
    pub(super) fn render_book_share_actions(&mut self, ui: &mut egui::Ui, paths: &[String]) {
        let ctx = ui.ctx().clone();
        let ids = paths
            .iter()
            .filter_map(|path| {
                self.library
                    .books
                    .iter()
                    .find(|b| &b.path == path)
                    .map(|b| b.id.clone())
            })
            .collect::<Vec<_>>();
        let palette = super::design::Palette::new(ui.visuals().dark_mode);
        if self.book_share.stage == 0 {
            for (stage, title, subtitle) in [
                (1, "share_ui.nearby", "share_ui.nearby_hint"),
                (2, "transfer.github", "share_ui.cloud_hint"),
                (3, "share_ui.export_file", "share_ui.export_hint"),
            ] {
                if share_action_row(
                    ui,
                    self.i18n.t(title),
                    self.i18n.t(subtitle),
                    stage,
                    !paths.is_empty() && !self.book_share.busy,
                )
                .clicked()
                {
                    self.book_share.stage = stage;
                    if stage == 1 {
                        self.book_share.start(
                            &self.data_dir,
                            json!({"op":"start_lan","ids":ids}),
                            &ctx,
                        );
                    }
                }
            }
            ui.add_space(8.);
            ui.collapsing(self.i18n.t("share_ui.details"), |ui| {
                ui.small(self.i18n.t("transfer.limits"));
                ui.small(self.i18n.t("transfer.lan_hint"));
            });
            if self.book_share.state["history"]
                .as_array()
                .is_some_and(|h| !h.is_empty())
            {
                ui.collapsing(self.i18n.t("transfer.history").to_owned(), |ui| {
                    self.render_published_shares(ui)
                });
            }
            return;
        }
        if self.book_share.stage == 2 && !self.book_share.shows_qr() {
            ui.label(self.i18n.t("share_ui.public_hint"));
            ui.add_space(12.);
            if self.github_token.is_none() || !self.github_repo_authorized {
                if ui
                    .add_enabled(
                        !self.book_share.busy,
                        super::design::primary(self.i18n.t("transfer.login")),
                    )
                    .clicked()
                {
                    self.github_start_repository_login();
                    self.show_github_login = true;
                }
            } else if ui
                .add_enabled(
                    !self.book_share.busy && !ids.is_empty(),
                    super::design::primary(self.i18n.t("share_ui.create_link")),
                )
                .clicked()
            {
                self.book_share.start(
                    &self.data_dir,
                    json!({"op":"publish","ids":ids,"token":self.github_token}),
                    &ctx,
                );
            }
            if matches!(
                self.book_share.operation.as_str(),
                "publish" | "delete_share"
            ) {
                self.render_book_share_status(ui);
            }
            if self.book_share.busy && ui.small_button(self.i18n.t("transfer.cancel")).clicked() {
                self.book_share.cancel(&self.data_dir);
            }
            ui.add_space(12.);
            ui.collapsing(self.i18n.t("share_ui.repository_details"), |ui| {
                ui.small(self.i18n.t("transfer.github_hint"));
                ui.small(self.i18n.t("transfer.limits"));
            });
            if self.book_share.state["history"]
                .as_array()
                .is_some_and(|h| !h.is_empty())
            {
                ui.collapsing(self.i18n.t("transfer.history").to_owned(), |ui| {
                    self.render_published_shares(ui)
                });
            }
            return;
        }
        if self.book_share.shows_qr() {
            let encoded = self.book_share.state["outgoing"]["qr"]
                .as_str()
                .unwrap_or("")
                .to_string();
            if self
                .book_share
                .qr
                .as_ref()
                .is_none_or(|(value, _)| value != &encoded)
            {
                if let Some(texture) = (|| {
                    let bytes = reader_core::base64_decode(&encoded).ok()?;
                    let image = image::load_from_memory(&bytes).ok()?.to_rgba8();
                    Some(ctx.load_texture(
                        "book-share-qr",
                        egui::ColorImage::from_rgba_unmultiplied(
                            [image.width() as usize, image.height() as usize],
                            image.as_raw(),
                        ),
                        egui::TextureOptions::NEAREST,
                    ))
                })() {
                    self.book_share.qr = Some((encoded, texture));
                }
            }
            let texture = self.book_share.qr.as_ref().map(|(_, t)| t.clone());
            ui.horizontal_top(|ui| {
                if let Some(texture) = texture {
                    egui::Frame::new()
                        .fill(egui::Color32::WHITE)
                        .corner_radius(10)
                        .inner_margin(10)
                        .show(ui, |ui| {
                            ui.image((texture.id(), egui::vec2(184., 184.)));
                        });
                }
                ui.add_space(20.);
                ui.vertical(|ui| {
                    ui.set_width(224.);
                    self.render_book_share_status(ui);
                    if let Some(expires) = self.book_share.state["outgoing"]["expires"].as_u64() {
                        let seconds = expires.saturating_sub(reader_core::now_secs());
                        ui.label(
                            egui::RichText::new(self.i18n.tf1(
                                "transfer.remaining",
                                &format!("{:02}:{:02}", seconds / 60, seconds % 60),
                            ))
                            .monospace()
                            .color(palette.muted),
                        );
                        ui.small(self.i18n.t("share_ui.keep_open"));
                    }
                    ui.add_space(8.);
                    ui.label(
                        egui::RichText::new(self.i18n.t("transfer.credential_hint"))
                            .size(13.)
                            .color(palette.text),
                    );
                    ui.add_space(8.);
                    let copied = self
                        .book_share
                        .copied
                        .is_some_and(|at| at.elapsed() < Duration::from_millis(2500));
                    if ui
                        .add_sized(
                            [ui.available_width(), 38.],
                            super::design::primary(self.i18n.t(if copied {
                                "transfer.copied"
                            } else {
                                "transfer.copy_link"
                            })),
                        )
                        .clicked()
                    {
                        self.book_share.copy_link(&ctx);
                    }
                    if self.book_share.active_lan()
                        && ui
                            .button(
                                egui::RichText::new(self.i18n.t("share_ui.end"))
                                    .color(palette.muted),
                            )
                            .clicked()
                    {
                        self.book_share.cancel(&self.data_dir);
                        self.book_share.stage = 0;
                    }
                });
            });
            if self.book_share.phase() == "published" {
                ui.add_space(8.);
                ui.small(self.i18n.t("transfer.published_hint"));
            }
        } else {
            ui.add_space(8.);
            self.render_book_share_status(ui);
            if self.book_share.busy {
                ui.spinner();
            } else if self.book_share.stage == 1
                && ui
                    .add(super::design::primary(self.i18n.t("share_ui.retry")))
                    .clicked()
            {
                self.book_share
                    .start(&self.data_dir, json!({"op":"start_lan","ids":ids}), &ctx);
            }
            if self.book_share.busy && ui.small_button(self.i18n.t("transfer.cancel")).clicked() {
                self.book_share.cancel(&self.data_dir);
            }
        }
    }
    fn render_book_share_status(&mut self, ui: &mut egui::Ui) {
        let phase = self.book_share.phase();
        if phase != "idle" {
            let key = if phase == "error" {
                "error".to_string()
            } else {
                format!("transfer.{phase}")
            };
            if phase != "error" {
                ui.label(egui::RichText::new(self.i18n.t(&key)).strong());
            }
        }
        let progress = &self.book_share.state["progress"];
        let total = progress["total"].as_u64().unwrap_or(0);
        let done = progress["done"].as_u64().unwrap_or(0);
        if total > 0 {
            ui.add(
                egui::ProgressBar::new((done as f32 / total as f32).clamp(0., 1.))
                    .show_percentage(),
            );
        }
        if let Some(error) = self
            .book_share
            .error
            .as_deref()
            .or(progress["error"].as_str())
        {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                self.i18n.tf1("transfer.failed", error),
            );
        }
        if let Some((imported, skipped)) = self.book_share.result {
            ui.label(self.i18n.tf2(
                "transfer.result",
                &imported.to_string(),
                &skipped.to_string(),
            ));
        }
    }
    fn render_published_shares(&mut self, ui: &mut egui::Ui) {
        let history = self.book_share.state["history"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if !history.is_empty() {
            ui.separator();
            ui.strong(self.i18n.t("transfer.history"));
        }
        for share in history.iter().rev() {
            if share["ready"].as_bool() == Some(false) && !self.book_share.busy {
                ui.small(self.i18n.t("transfer.unfinished"));
            }
            ui.horizontal(|ui| {
                ui.label(self.i18n.tf1(
                    "manage.count",
                    &share["count"].as_u64().unwrap_or(0).to_string(),
                ));
                ui.monospace(share["id"].as_str().unwrap_or("").get(..8).unwrap_or(""));
                if ui
                    .add_enabled(
                        !self.book_share.busy && self.github_token.is_some(),
                        egui::Button::new(self.i18n.t("transfer.delete")),
                    )
                    .clicked()
                {
                    self.book_share.delete = share["id"].as_str().map(str::to_owned);
                }
            });
        }
        if let Some(id) = self.book_share.delete.clone() {
            ui.label(self.i18n.t("transfer.delete_confirm"));
            ui.horizontal(|ui| {
                if ui.button(self.i18n.t("settings.cancel")).clicked() {
                    self.book_share.delete = None;
                }
                if ui.button(self.i18n.t("transfer.delete")).clicked() {
                    self.book_share.delete = None;
                    self.book_share.start(
                        &self.data_dir,
                        json!({"op":"delete_share","id":id,"token":self.github_token}),
                        ui.ctx(),
                    );
                }
            });
        }
    }
    pub fn render_receive_books(&mut self, ctx: &egui::Context) {
        if !self.book_share.receive {
            return;
        }
        let mut open = true;
        egui::Window::new(self.i18n.t("transfer.receive"))
            .id(egui::Id::new("receive-books"))
            .open(&mut open)
            .collapsible(false)
            .default_width(480.)
            .anchor(egui::Align2::CENTER_CENTER, [0., 0.])
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height((ctx.screen_rect().height() - 190.).max(130.))
                    .show(ui, |ui| {
                        if self.book_share.state["preview"].is_object() {
                            ui.label(self.i18n.t("transfer.import_hint"));
                            if let Some(books) =
                                self.book_share.state["preview"]["books"].as_array()
                            {
                                for book in books {
                                    ui.label(book["title"].as_str().unwrap_or(""));
                                }
                            }
                            if ui
                                .add_enabled(
                                    !self.book_share.busy,
                                    egui::Button::new(self.i18n.t("transfer.import")),
                                )
                                .clicked()
                            {
                                self.cloud_sync.stop();
                                self.book_share
                                    .start(&self.data_dir, json!({"op":"accept"}), ctx);
                            }
                        } else {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.book_share.link)
                                    .desired_rows(3)
                                    .hint_text(self.i18n.t("transfer.link")),
                            );
                            if ui
                                .add_enabled(
                                    !self.book_share.busy,
                                    egui::Button::new(self.i18n.t("transfer.image")),
                                )
                                .clicked()
                            {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("QR", &["png", "jpg", "jpeg", "webp"])
                                    .pick_file()
                                {
                                    self.book_share.start(
                                        &self.data_dir,
                                        json!({"op":"decode_image","path":path}),
                                        ctx,
                                    );
                                }
                            }
                            if ui
                                .add_enabled(
                                    !self.book_share.busy
                                        && !self.book_share.link.trim().is_empty(),
                                    egui::Button::new(self.i18n.t("transfer.connect")),
                                )
                                .clicked()
                            {
                                self.book_share.start(
                                    &self.data_dir,
                                    json!({"op":"receive","link":self.book_share.link}),
                                    ctx,
                                );
                            }
                        }
                        self.render_book_share_status(ui);
                        self.render_published_shares(ui);
                    });
                if ui.button(self.i18n.t("transfer.cancel")).clicked() {
                    self.book_share.cancel(&self.data_dir);
                }
            });
        if !open {
            self.book_share.receive = false;
            self.book_share.cancel(&self.data_dir);
        }
    }
}

/// Native-looking action rows: one shared surface, aligned labels and quiet icons.
pub(super) fn share_action_row(
    ui: &mut egui::Ui,
    title: &str,
    subtitle: &str,
    kind: usize,
    enabled: bool,
) -> egui::Response {
    let palette = super::design::Palette::new(ui.visuals().dark_mode);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 56.),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, title));
    if response.hovered() && enabled {
        ui.painter().rect_filled(rect, 8., palette.raised);
    }
    let c = rect.left_center() + egui::vec2(24., 0.);
    let color = if enabled { palette.text } else { palette.muted };
    let stroke = egui::Stroke::new(1.6_f32, palette.muted);
    let painter = ui.painter();
    if kind == 1 {
        for radius in [6., 11.] {
            let points = (0..=16)
                .map(|i| {
                    let a = std::f32::consts::PI * (1.22 + i as f32 / 16. * 0.56);
                    c + egui::vec2(a.cos(), a.sin()) * radius
                })
                .collect();
            painter.add(egui::Shape::line(points, stroke));
        }
        painter.circle_filled(c + egui::vec2(0., 4.), 2., palette.muted);
    } else if kind == 2 {
        painter.rect_stroke(
            egui::Rect::from_center_size(c, egui::vec2(22., 18.)),
            4.,
            stroke,
            egui::StrokeKind::Inside,
        );
        painter.line_segment([c + egui::vec2(0., 5.), c - egui::vec2(0., 5.)], stroke);
        painter.add(egui::Shape::line(
            vec![
                c + egui::vec2(-4., -1.),
                c + egui::vec2(0., -5.),
                c + egui::vec2(4., -1.),
            ],
            stroke,
        ));
    } else {
        painter.line_segment([c - egui::vec2(0., 9.), c + egui::vec2(0., 3.)], stroke);
        painter.add(egui::Shape::line(
            vec![
                c + egui::vec2(-4., -1.),
                c + egui::vec2(0., 3.),
                c + egui::vec2(4., -1.),
            ],
            stroke,
        ));
        painter.add(egui::Shape::line(
            vec![
                c + egui::vec2(-9., 3.),
                c + egui::vec2(-9., 9.),
                c + egui::vec2(9., 9.),
                c + egui::vec2(9., 3.),
            ],
            stroke,
        ));
    }
    let x = rect.left() + 52.;
    painter.text(
        egui::pos2(x, rect.top() + 20.),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(15.),
        color,
    );
    let galley = ui.fonts(|f| {
        f.layout_job(egui::text::LayoutJob::simple(
            subtitle.into(),
            egui::FontId::proportional(12.),
            palette.muted,
            (rect.width() - 82.).max(40.),
        ))
    });
    painter.galley(egui::pos2(x, rect.top() + 34.), galley, palette.muted);
    let arrow = rect.right_center() - egui::vec2(16., 0.);
    painter.add(egui::Shape::line(
        vec![
            arrow + egui::vec2(-3., -4.),
            arrow + egui::vec2(1., 0.),
            arrow + egui::vec2(-3., 4.),
        ],
        stroke,
    ));
    response
}
