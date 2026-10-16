//! Crossfade the previous drawing without keeping a second reader or layout alive.
use eframe::egui::{self, epaint::ClippedShape};

#[derive(Default)]
pub(crate) struct AppearanceTransition {
    key: Option<(bool, bool)>,
    previous: Vec<ClippedShape>,
    fading: Vec<ClippedShape>,
    started: Option<f64>,
    viewport: Option<egui::Rect>,
    retained_paper: Option<egui::TextureHandle>,
    previous_paper: Option<egui::TextureHandle>,
}
impl AppearanceTransition {
    pub(crate) fn begin(&mut self, ctx: &egui::Context, key: (bool, bool)) {
        let resized = self.viewport.is_some_and(|rect| rect != ctx.screen_rect());
        if resized {
            self.previous.clear();
            self.fading.clear();
            self.started = None;
        }
        if self.key.is_some_and(|old| old != key) && !resized && ctx.style().animation_time > 0. {
            self.fading = std::mem::take(&mut self.previous);
            self.retained_paper = self.previous_paper.take();
            self.started = Some(ctx.input(|i| i.time));
        }
        self.key = Some(key);
        self.viewport = Some(ctx.screen_rect());
    }
    pub(crate) fn finish(&mut self, ctx: &egui::Context, paper: Option<egui::TextureHandle>) {
        self.previous = ctx.graphics(|g| g.clone().drain(&[], &Default::default()));
        self.previous_paper = paper;
        let Some(started) = self.started else { return };
        let t = ((ctx.input(|i| i.time) - started) / 0.6).clamp(0., 1.) as f32;
        if t >= 1. {
            self.started = None;
            self.fading.clear();
            self.retained_paper = None;
            return;
        }
        let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("appearance_crossfade"));
        let mut painter = ctx.layer_painter(layer);
        painter.set_opacity(1. - t * t * (3. - 2. * t));
        for clipped in &self.fading {
            painter
                .with_clip_rect(clipped.clip_rect)
                .add(clipped.shape.clone());
        }
        ctx.request_repaint();
    }
}
