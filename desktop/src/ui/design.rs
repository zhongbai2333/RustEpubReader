//! Application chrome. Reader typography and paper preferences remain independent.
use eframe::egui::{self, Color32, CornerRadius, Stroke};

#[derive(Clone, Copy)]
pub struct Palette {
    pub canvas: Color32,
    pub surface: Color32,
    pub raised: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
}
impl Palette {
    pub fn new(dark: bool) -> Self {
        if dark {
            Self {
                canvas: Color32::from_rgb(18, 22, 28),
                surface: Color32::from_rgb(27, 33, 41),
                raised: Color32::from_rgb(37, 45, 56),
                border: Color32::from_rgb(53, 63, 76),
                text: Color32::from_rgb(233, 237, 244),
                muted: Color32::from_rgb(167, 178, 194),
                accent: Color32::from_rgb(140, 181, 255),
            }
        } else {
            Self {
                canvas: Color32::from_rgb(244, 246, 249),
                surface: Color32::WHITE,
                raised: Color32::from_rgb(234, 239, 247),
                border: Color32::from_rgb(219, 226, 236),
                text: Color32::from_rgb(31, 41, 55),
                muted: Color32::from_rgb(94, 109, 129),
                accent: Color32::from_rgb(47, 100, 207),
            }
        }
    }
}
pub const BUTTON_BLUE: Color32 = Color32::from_rgb(47, 100, 207);

/// Chrome geometry, mirrored by Android's ReaderGeometry in logical dp.
pub struct Geometry;
impl Geometry {
    pub const RADIUS: u8 = 12;
    pub const NAV_RADIUS: u8 = 8;
    pub const NAV_HEIGHT: f32 = 40.;
    pub const DIALOG_RADIUS: u8 = 16;
    pub const PAGE_INSET: i8 = 24;
    pub const CARD_INSET: i8 = 16;
    pub const GAP: f32 = 8.;
    pub const BLOCK_GAP: f32 = 16.;
    pub const SECTION_GAP: f32 = 24.;
    pub const CONTROL_HEIGHT: f32 = 40.;
    pub const FIELD_MARGIN: egui::Margin = egui::Margin::symmetric(16, 8);
    pub const BUTTON_PADDING: egui::Vec2 = egui::vec2(16., 8.);
}

/// Menus share one surface; rows are compact, aligned and only tinted on hover.
pub fn menu_button<R>(
    ui: &mut egui::Ui,
    button: egui::Button<'_>,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    let response = ui.add(button);
    let popup_id = response.id.with("action_popup");
    if response.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup_id));
    }
    let direction = if ui.layout().main_dir() == egui::Direction::BottomUp {
        egui::AboveOrBelow::Above
    } else {
        egui::AboveOrBelow::Below
    };
    let inner = egui::popup::popup_above_or_below_widget(
        ui,
        popup_id,
        &response,
        direction,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            menu_style(ui);
            contents(ui)
        },
    );
    egui::InnerResponse::new(inner, response)
}

pub fn menu_style(ui: &mut egui::Ui) {
    ui.set_width(176.);
    ui.spacing_mut().item_spacing = egui::vec2(4., 2.);
    ui.spacing_mut().interact_size.y = 34.;
    ui.spacing_mut().button_padding = egui::vec2(10., 6.);
    let widgets = &mut ui.visuals_mut().widgets;
    for w in [
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
    ] {
        w.corner_radius = CornerRadius::same(4);
    }
}
pub fn menu_item(
    ui: &mut egui::Ui,
    label: &str,
    selected: bool,
    destructive: bool,
) -> egui::Response {
    let r = ui.add_sized(
        [ui.available_width(), 34.],
        egui::Button::new("").frame(false),
    );
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    if r.clicked() {
        ui.memory_mut(|m| m.close_popup());
    }
    let label = label.trim_start_matches(['📤', '📌', ' ']);
    let c = if destructive {
        ui.visuals().error_fg_color
    } else {
        ui.visuals().text_color()
    };
    let mut job = egui::text::LayoutJob::simple(
        label.to_owned(),
        egui::FontId::proportional(14.),
        c,
        r.rect.width() - 42.,
    );
    job.wrap.max_rows = 1;
    let galley = ui.fonts(|f| f.layout_job(job));
    ui.painter().galley(
        r.rect.left_center() + egui::vec2(10., -galley.size().y / 2.),
        galley,
        c,
    );
    if selected {
        let c = r.rect.right_center() - egui::vec2(16., 0.);
        ui.painter().add(egui::Shape::line(
            vec![
                c + egui::vec2(-4., 0.),
                c + egui::vec2(-1., 3.),
                c + egui::vec2(5., -4.),
            ],
            Stroke::new(1.6_f32, ui.visuals().selection.stroke.color),
        ));
    }
    r
}
pub fn close_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let r = ui
        .scope(|ui| {
            ui.spacing_mut().interact_size = egui::vec2(28., 28.);
            ui.add_sized([28., 28.], egui::Button::new("").frame(false))
        })
        .inner;
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let c = r.rect.center();
    let stroke = Stroke::new(1.5_f32, ui.visuals().text_color());
    ui.painter()
        .line_segment([c + egui::vec2(-5., -5.), c + egui::vec2(5., 5.)], stroke);
    ui.painter()
        .line_segment([c + egui::vec2(-5., 5.), c + egui::vec2(5., -5.)], stroke);
    r.on_hover_text(label)
}

pub fn sidebar_title(ui: &mut egui::Ui, title: &str) {
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 28.),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.add(egui::Label::new(egui::RichText::new(title).size(17.).strong()).truncate());
        },
    );
}

/// A short vertical control entirely inside the panel's edge facing the reader.
/// Call only while the panel is present: collapsed sidebars leave no hit target.
pub fn sidebar_collapse_button(
    ui: &mut egui::Ui,
    panel: egui::Rect,
    left: bool,
    label: &str,
) -> egui::Response {
    let x = if left {
        panel.right() - 14.
    } else {
        panel.left() + 14.
    };
    let y = panel.center().y;
    let rect = egui::Rect::from_center_size(egui::pos2(x, y), egui::vec2(24., 72.));
    let response = ui.interact(rect, ui.id().with("collapse_sidebar"), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    let visuals = ui.style().interact(&response);
    let painter = ui.painter().with_clip_rect(ui.clip_rect().intersect(panel));
    painter.rect(
        rect,
        8,
        visuals.weak_bg_fill,
        visuals.bg_stroke,
        egui::StrokeKind::Inside,
    );
    let c = rect.center();
    let direction = if left { -1. } else { 1. };
    painter.add(egui::Shape::line(
        vec![
            c + egui::vec2(-direction * 3., -5.),
            c + egui::vec2(direction * 3., 0.),
            c + egui::vec2(-direction * 3., 5.),
        ],
        Stroke::new(1.6_f32, visuals.fg_stroke.color),
    ));
    response.on_hover_text(label)
}

pub fn settings_slider(ui: &mut egui::Ui, slider: egui::Slider<'_>) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = 20.;
        ui.spacing_mut().button_padding = egui::vec2(6., 2.);
        ui.spacing_mut().slider_width = (ui.available_width() - 64.).max(80.);
        ui.spacing_mut().slider_rail_height = 3.;
        let dark = ui.visuals().dark_mode;
        let palette = Palette::new(dark);
        ui.visuals_mut().widgets.inactive.bg_fill = palette.border;
        ui.visuals_mut().widgets.inactive.corner_radius = CornerRadius::same(2);
        ui.visuals_mut().widgets.inactive.fg_stroke = Stroke::new(1.0_f32, palette.muted);
        ui.visuals_mut().slider_trailing_fill = true;
        ui.visuals_mut().selection.bg_fill = palette.accent;
        ui.add(slider)
    })
    .inner
}

pub fn panels_animating(ctx: &egui::Context) -> bool {
    ctx.data(|d| {
        d.get_temp::<bool>(egui::Id::new("panels_animating"))
            .unwrap_or(false)
    })
}
/// Slide full-width content through a clipped panel, avoiding compressed text during motion.
pub fn animated_side_panel(
    ctx: &egui::Context,
    id: &'static str,
    left: bool,
    width: f32,
    expanded: bool,
    collapse_label: &str,
    contents: impl FnOnce(&mut egui::Ui),
) -> bool {
    let progress = ctx.animate_bool_with_time(egui::Id::new(id).with("slide"), expanded, 0.18);
    let moving = progress > 0. && progress < 1.;
    if moving {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("panels_animating"), true));
        ctx.request_repaint();
    }
    if progress <= 0. {
        return false;
    }
    let panel = if left {
        egui::SidePanel::left(id)
    } else {
        egui::SidePanel::right(id)
    };
    panel
        .resizable(false)
        .min_width(0.)
        .exact_width(width * progress)
        .frame(egui::Frame::default().fill(ctx.style().visuals.panel_fill))
        .show(ctx, |ui| {
            let r = ui.max_rect();
            let x = if left {
                r.right() - width + 16.
            } else {
                r.left() + 32.
            };
            let rect = egui::Rect::from_min_size(
                egui::pos2(x, r.top() + 16.),
                egui::vec2(width - 48., (r.height() - 32.).max(0.)),
            );
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt("sliding_contents")
                    .max_rect(rect)
                    .layout(egui::Layout::top_down(egui::Align::LEFT)),
            );
            child.set_clip_rect(ui.clip_rect().intersect(r));
            if moving {
                child.disable();
            }
            contents(&mut child);
            if moving {
                ui.disable();
            }
            sidebar_collapse_button(ui, r, left, collapse_label).clicked()
        })
        .inner
}

pub fn animated_top_panel(
    ctx: &egui::Context,
    id: &'static str,
    height: f32,
    expanded: bool,
    fill: Color32,
    contents: impl FnOnce(&mut egui::Ui),
) {
    let p = ctx.animate_bool_with_time(egui::Id::new(id).with("slide"), expanded, 0.18);
    if p > 0. && p < 1. {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("panels_animating"), true));
        ctx.request_repaint();
    }
    if p <= 0. {
        return;
    }
    egui::TopBottomPanel::top(id)
        .resizable(false)
        .exact_height(height * p)
        .frame(egui::Frame::default().fill(fill))
        .show(ctx, |ui| {
            let r = ui.max_rect();
            let rect = egui::Rect::from_min_size(
                egui::pos2(r.left() + 16., r.bottom() - height + 8.),
                egui::vec2((r.width() - 32.).max(0.), height - 16.),
            );
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt("sliding_contents")
                    .max_rect(rect),
            );
            child.set_clip_rect(ui.clip_rect().intersect(r));
            if p < 1. {
                child.disable();
            }
            contents(&mut child);
        });
}

pub fn text_edit(text: &mut dyn egui::TextBuffer) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(text)
        .margin(Geometry::FIELD_MARGIN)
        .min_size(egui::vec2(0., Geometry::CONTROL_HEIGHT))
        .vertical_align(egui::Align::Center)
}

// egui already inserts GAP between widgets; add only the remainder.
pub fn section_gap(ui: &mut egui::Ui) {
    ui.add_space(Geometry::SECTION_GAP - ui.spacing().item_spacing.y);
}
pub fn block_gap(ui: &mut egui::Ui) {
    ui.add_space(Geometry::BLOCK_GAP - ui.spacing().item_spacing.y);
}

pub fn paint_navigation_item(
    ui: &egui::Ui,
    response: &egui::Response,
    icon: Icon,
    label: &str,
    selected: bool,
) {
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let color = if selected {
        ui.visuals().selection.stroke.color
    } else {
        ui.visuals().text_color()
    };
    paint_icon(
        ui.painter(),
        egui::Rect::from_center_size(
            response.rect.left_center() + egui::vec2(23., 0.),
            egui::vec2(22., 22.),
        ),
        icon,
        color,
    );
    let mut job = egui::text::LayoutJob::simple(
        label.to_owned(),
        egui::TextStyle::Button.resolve(ui.style()),
        color,
        response.rect.width() - 58.,
    );
    job.wrap.max_rows = 1;
    let galley = ui.fonts(|fonts| fonts.layout_job(job));
    ui.painter().galley(
        response.rect.left_center() + egui::vec2(46., -galley.size().y / 2.),
        galley,
        color,
    );
}

pub fn navigation_item(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    selected: bool,
) -> egui::Response {
    let response = ui.add_sized(
        [ui.available_width(), Geometry::NAV_HEIGHT],
        egui::Button::new("")
            .selected(selected)
            .corner_radius(Geometry::NAV_RADIUS)
            .stroke(Stroke::NONE),
    );
    paint_navigation_item(ui, &response, icon, label, selected);
    response.on_hover_text(label)
}

pub fn sync_toggle(
    ui: &mut egui::Ui,
    value: &mut bool,
    enabled: bool,
    label: &str,
) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(
        egui::vec2(44., Geometry::CONTROL_HEIGHT),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    if enabled && response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, enabled, *value, label)
    });
    let p = Palette::new(ui.visuals().dark_mode);
    let track = egui::Rect::from_center_size(rect.center(), egui::vec2(44., 24.));
    let progress = ui.ctx().animate_bool(response.id, *value);
    ui.painter().rect_filled(
        track,
        12.,
        if *value && enabled {
            BUTTON_BLUE
        } else {
            p.border
        },
    );
    ui.painter().circle_filled(
        egui::pos2(track.left() + 12. + progress * 20., track.center().y),
        9.,
        if enabled { Color32::WHITE } else { p.muted },
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            track.expand(2.),
            14.,
            Stroke::new(1.5_f32, p.accent),
            egui::StrokeKind::Outside,
        );
    }
    response
}

pub fn apply(ctx: &egui::Context, dark: bool) {
    let p = Palette::new(dark);
    let mut style = (*ctx.style()).clone();
    style.visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    style.visuals.panel_fill = p.surface;
    style.visuals.window_fill = p.surface;
    style.visuals.extreme_bg_color = p.canvas;
    style.visuals.faint_bg_color = p.raised;
    style.visuals.override_text_color = Some(p.text);
    style.visuals.window_corner_radius = CornerRadius::same(Geometry::DIALOG_RADIUS);
    style.visuals.menu_corner_radius = CornerRadius::same(8);
    style.visuals.window_stroke = Stroke::new(1.0_f32, p.border);
    style.visuals.selection.bg_fill = if dark {
        Color32::from_rgb(37, 63, 106)
    } else {
        Color32::from_rgb(221, 232, 252)
    };
    style.visuals.selection.stroke = Stroke::new(1.0_f32, p.accent);
    style.visuals.hyperlink_color = p.accent;
    for (style_id, size) in [
        (egui::TextStyle::Heading, 24.0),
        (egui::TextStyle::Body, 15.0),
        (egui::TextStyle::Button, 14.0),
        (egui::TextStyle::Small, 12.0),
        (egui::TextStyle::Monospace, 14.0),
    ] {
        style.text_styles.insert(
            style_id.clone(),
            egui::FontId::new(
                size,
                if style_id == egui::TextStyle::Monospace {
                    egui::FontFamily::Monospace
                } else {
                    egui::FontFamily::Proportional
                },
            ),
        );
    }
    style.animation_time = 0.18;
    style.visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 3],
        blur: 12,
        spread: 0,
        color: Color32::from_black_alpha(28),
    };
    style.spacing.item_spacing = egui::vec2(Geometry::GAP, Geometry::GAP);
    style.spacing.button_padding = Geometry::BUTTON_PADDING;
    style.spacing.interact_size.y = Geometry::CONTROL_HEIGHT;
    style.spacing.window_margin = egui::Margin::same(Geometry::CARD_INSET);
    style.spacing.menu_margin = egui::Margin::same(8);
    for widgets in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        widgets.corner_radius = CornerRadius::same(Geometry::RADIUS);
        widgets.fg_stroke = Stroke::new(1.4_f32, p.text);
        widgets.bg_stroke = Stroke::new(1.0_f32, p.border);
    }
    style.visuals.widgets.inactive.bg_fill = p.raised;
    style.visuals.widgets.inactive.weak_bg_fill = p.surface;
    style.visuals.widgets.hovered.bg_fill = p.raised;
    style.visuals.widgets.hovered.weak_bg_fill = p.raised;
    style.visuals.widgets.noninteractive.corner_radius = CornerRadius::same(Geometry::RADIUS);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.text);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.border);
    ctx.set_style(style);
}

#[derive(Clone, Copy)]
pub enum Icon {
    Book,
    Sync,
    Search,
    Toc,
    Notes,
    Settings,
    Back,
    Return,
    Audio,
    Close,
    Bookmark,
    More,
    Correction,
    Export,
    Import,
    Sun,
    Moon,
}
pub fn paint_icon(p: &egui::Painter, rect: egui::Rect, icon: Icon, color: Color32) {
    let c = rect.center();
    let s = Stroke::new(1.6_f32, color);
    let pt = |x: f32, y: f32| c + egui::vec2(x, y);
    let line = |points: &[(f32, f32)]| {
        p.add(egui::Shape::line(
            points.iter().map(|&(x, y)| pt(x, y)).collect(),
            s,
        ));
    };
    match icon {
        Icon::Export | Icon::Import => {
            line(&[(-8., 1.), (-8., 8.), (8., 8.), (8., 1.)]);
            if matches!(icon, Icon::Export) {
                line(&[(0., 3.), (0., -9.)]);
                line(&[(-4., -5.), (0., -9.), (4., -5.)]);
            } else {
                line(&[(0., -9.), (0., 3.)]);
                line(&[(-4., -1.), (0., 3.), (4., -1.)]);
            }
        }
        Icon::Book => {
            line(&[
                (-8., -8.),
                (-3., -8.),
                (0., -6.),
                (3., -8.),
                (8., -8.),
                (8., 6.),
                (3., 6.),
                (0., 8.),
                (-3., 6.),
                (-8., 6.),
                (-8., -8.),
            ]);
            line(&[(0., -6.), (0., 7.)]);
        }
        Icon::Sun => {
            p.circle_stroke(c, 4., s);
            for (x, y) in [
                (1f32, 0f32),
                (0., 1.),
                (-1., 0.),
                (0., -1.),
                (0.707, 0.707),
                (-0.707, 0.707),
                (0.707, -0.707),
                (-0.707, -0.707),
            ] {
                p.line_segment([c + egui::vec2(x, y) * 7., c + egui::vec2(x, y) * 10.], s);
            }
        }
        Icon::Moon => {
            line(&[
                (4., -9.),
                (-3., -8.),
                (-8., -3.),
                (-8., 4.),
                (-3., 8.),
                (4., 8.),
                (9., 3.),
                (3., 4.),
                (-1., 1.),
                (-2., -4.),
                (0., -8.),
                (4., -9.),
            ]);
        }
        Icon::Sync => {
            line(&[(-8., -1.), (-7., -6.), (-3., -8.), (3., -8.), (8., -3.)]);
            line(&[(8., -8.), (8., -3.), (3., -3.)]);
            line(&[(8., 1.), (7., 6.), (3., 8.), (-3., 8.), (-8., 3.)]);
            line(&[(-8., 8.), (-8., 3.), (-3., 3.)]);
        }
        Icon::Search => {
            p.circle_stroke(pt(-2., -2.), 6.0, s);
            line(&[(3., 3.), (9., 9.)]);
        }
        Icon::Toc => {
            for y in [-6., 0., 6.] {
                line(&[(-4., y), (9., y)]);
                p.circle_filled(pt(-8., y), 1.0, color);
            }
        }
        Icon::Notes => {
            line(&[(-8., -9.), (7., -9.), (7., 9.), (-8., 9.), (-8., -9.)]);
            for y in [-4., 1., 6.] {
                line(&[(-4., y), (3., y)]);
            }
        }
        Icon::Settings => {
            for (y, x) in [(-6., -3.), (0., 4.), (6., -1.)] {
                line(&[(-9., y), (9., y)]);
                p.circle_filled(pt(x, y), 3., color);
            }
        }
        Icon::Return => {
            line(&[(-1., -8.), (-8., -1.), (-1., 6.)]);
            let points = [
                pt(-8., -1.),
                pt(7., -1.),
                pt(9., 3.),
                pt(9., 7.),
                pt(5., 10.),
                pt(-1., 10.),
            ];
            p.add(egui::epaint::CubicBezierShape::from_points_stroke(
                [points[0], points[1], points[2], points[3]],
                false,
                Color32::TRANSPARENT,
                s,
            ));
            p.add(egui::epaint::CubicBezierShape::from_points_stroke(
                [points[3], points[4], points[5], pt(-3., 10.)],
                false,
                Color32::TRANSPARENT,
                s,
            ));
        }
        Icon::Back => {
            line(&[(-1., -7.), (-8., 0.), (-1., 7.)]);
            line(&[(-8., 0.), (9., 0.)]);
        }
        Icon::Close => {
            line(&[(-5., -5.), (5., 5.)]);
            line(&[(-5., 5.), (5., -5.)]);
        }
        Icon::Bookmark => {
            line(&[
                (-6., -9.),
                (6., -9.),
                (6., 9.),
                (0., 5.),
                (-6., 9.),
                (-6., -9.),
            ]);
        }
        Icon::More => {
            for x in [-6., 0., 6.] {
                p.circle_filled(pt(x, 0.), 1.5, color);
            }
        }
        Icon::Correction => {
            line(&[(-8., -8.), (2., -8.), (2., 2.), (-8., 2.), (-8., -8.)]);
            line(&[(-5., -4.), (-1., -4.)]);
            line(&[(-1., 6.), (3., 10.), (10., 2.)]);
        }
        Icon::Audio => {
            line(&[
                (-8., -3.),
                (-4., -3.),
                (1., -8.),
                (1., 8.),
                (-4., 3.),
                (-8., 3.),
                (-8., -3.),
            ]);
            line(&[(5., -5.), (8., 0.), (5., 5.)]);
        }
    }
}
pub fn icon_button(ui: &mut egui::Ui, icon: Icon, label: &str, active: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(Geometry::CONTROL_HEIGHT, Geometry::CONTROL_HEIGHT),
        egui::Sense::click(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    let fill = if active {
        ui.visuals().selection.bg_fill
    } else if response.hovered() {
        ui.visuals().widgets.hovered.bg_fill
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, Geometry::RADIUS, fill);
    paint_icon(
        ui.painter(),
        rect,
        icon,
        if active {
            ui.visuals().selection.stroke.color
        } else {
            ui.visuals().text_color()
        },
    );
    response.on_hover_text(label)
}
pub fn primary(label: impl Into<egui::WidgetText>) -> egui::Button<'static> {
    egui::Button::new(label)
        .fill(BUTTON_BLUE)
        .stroke(Stroke::NONE)
        .corner_radius(Geometry::RADIUS)
        .min_size(egui::vec2(0., Geometry::CONTROL_HEIGHT))
}

pub fn horizontal_gradient_mesh(
    rect: egui::Rect,
    left: Color32,
    middle: Color32,
    right: Color32,
) -> egui::epaint::Mesh {
    let mut mesh = egui::epaint::Mesh::default();
    for (x, color) in [
        (rect.left(), left),
        (rect.center().x, middle),
        (rect.right(), right),
    ] {
        for y in [rect.top(), rect.bottom()] {
            mesh.vertices.push(egui::epaint::Vertex {
                pos: egui::pos2(x, y),
                uv: egui::epaint::WHITE_UV,
                color,
            });
        }
    }
    mesh.indices.extend([0, 1, 2, 1, 3, 2, 2, 3, 4, 3, 5, 4]);
    mesh
}

pub fn scroll_edge_fades(
    painter: &egui::Painter,
    rect: egui::Rect,
    color: Color32,
    above: bool,
    below: bool,
) {
    let painter = painter.with_clip_rect(rect);
    let height = 24.0_f32.min(rect.height() / 4.);
    for (enabled, top) in [(above, true), (below, false)] {
        if !enabled {
            continue;
        }
        let y = if top {
            rect.top()
        } else {
            rect.bottom() - height
        };
        let mut mesh = egui::epaint::Mesh::default();
        for (py, c) in [
            (y, if top { color } else { Color32::TRANSPARENT }),
            (y + height, if top { Color32::TRANSPARENT } else { color }),
        ] {
            for x in [rect.left(), rect.right()] {
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: egui::pos2(x, py),
                    uv: egui::epaint::WHITE_UV,
                    color: c,
                });
            }
        }
        mesh.indices.extend([0, 1, 2, 1, 3, 2]);
        painter.add(egui::Shape::mesh(mesh));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn action_menus_are_exclusive_across_different_ui_roots() {
        let ctx = egui::Context::default();
        let mut rects = [egui::Rect::NOTHING; 2];
        let mut ids = [egui::Id::NULL; 2];
        let mut run = |time, events| {
            let _ = ctx.run(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 600.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            for i in 0..2 {
                                if i == 1 {
                                    ui.add_space(350.);
                                }
                                ui.push_id(i, |ui| {
                                    let r = menu_button(
                                        ui,
                                        egui::Button::new(format!("Menu {i}")),
                                        |ui| {
                                            menu_item(ui, "Action", false, false);
                                        },
                                    );
                                    rects[i] = r.response.rect;
                                    ids[i] = r.response.id.with("action_popup");
                                });
                            }
                        });
                    });
                },
            );
            (rects, ids)
        };
        let (rects, _) = run(0., vec![]);
        let click = |pos| {
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        let (_, ids) = run(1., click(rects[0].center()));
        assert!(ctx.memory(|m| m.is_popup_open(ids[0])));
        run(2., vec![]);
        run(3., click(rects[1].center()));
        assert!(!ctx.memory(|m| m.is_popup_open(ids[0])));
        assert!(ctx.memory(|m| m.is_popup_open(ids[1])));
    }
}
