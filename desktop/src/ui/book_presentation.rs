//! Book navigation is independent of page turns: only a plain paper surface moves.
use crate::app::{AppView, ReaderApp};
use eframe::egui::{self, epaint::ClippedShape};
use std::{collections::HashMap, time::Instant};

#[derive(Default)]
pub(crate) struct BookPresentation {
    pub covers: HashMap<String, egui::Rect>,
    pub shelf_order: StableShelfOrder,
    reorder_from: HashMap<String, egui::Rect>,
    reorder_started: Option<Instant>,
    reorder_book: Option<String>,
    pub source: Option<egui::Rect>,
    viewport: Option<egui::Rect>,
    shelf: Vec<ClippedShape>,
    phase: Option<Phase>,
}
/// Live progress/last-opened updates cannot rearrange a visible shelf.
#[derive(Default)]
pub(crate) struct StableShelfOrder {
    ids: Vec<String>,
    by_title: bool,
}
impl StableShelfOrder {
    pub fn reset(&mut self) {
        self.ids.clear();
    }
    pub fn resolve(&mut self, ranked: Vec<String>, by_title: bool) -> Vec<String> {
        if ranked.is_empty() {
            return ranked;
        }
        if self.ids.is_empty() || by_title || self.by_title != by_title {
            self.ids = ranked;
        } else {
            let current: std::collections::HashSet<_> = ranked.iter().collect();
            self.ids.retain(|id| current.contains(id));
            let previous: std::collections::HashSet<_> = self.ids.iter().cloned().collect();
            self.ids
                .extend(ranked.into_iter().filter(|id| !previous.contains(id)));
        }
        self.by_title = by_title;
        self.ids.clone()
    }
}
enum Phase {
    Opening {
        started: Instant,
        ready: Option<Instant>,
    },
    Closing {
        started: Instant,
        from: f32,
        covering_reader: bool,
    },
}
fn ease(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}
impl BookPresentation {
    pub(crate) fn is_active(&self) -> bool {
        self.phase.is_some()
    }
    pub fn promoted_card(&self, path: &str) -> bool {
        self.reorder_started.is_some() && self.reorder_book.as_deref() == Some(path)
    }
    pub fn shelf_card_rect(
        &mut self,
        path: &str,
        target: egui::Rect,
        ctx: &egui::Context,
    ) -> egui::Rect {
        let Some(started) = self.reorder_started else {
            return target;
        };
        let t = started.elapsed().as_secs_f32() / 0.38;
        if t >= 1. || ctx.style().animation_time <= 0. {
            self.reorder_started = None;
            self.reorder_from.clear();
            return target;
        }
        let Some(old) = self.reorder_from.get(path) else {
            return target;
        };
        // A resize must not animate a card from stale viewport coordinates.
        if (old.width() - target.width()).abs() > 1. {
            return target;
        }
        ctx.request_repaint();
        egui::Rect::from_min_size(old.min.lerp(target.min, ease(t)), target.size())
    }
}
impl ReaderApp {
    pub(crate) fn begin_book_presentation(&mut self, path: &str) {
        self.book_presentation.source =
            self.book_presentation
                .covers
                .get(path)
                .copied()
                .filter(|r| {
                    self.last_egui_ctx
                        .as_ref()
                        .is_some_and(|ctx| ctx.screen_rect().contains_rect(*r))
                });
        self.book_presentation.shelf.clear();
        self.book_presentation.phase = Some(Phase::Opening {
            started: Instant::now(),
            ready: None,
        });
    }
    pub(crate) fn close_book_presentation(&mut self) {
        if matches!(self.book_presentation.phase, Some(Phase::Closing { .. })) {
            return;
        }
        self.pending_book_open = None;
        let (from, covering_reader) = match &self.book_presentation.phase {
            Some(Phase::Opening { started, .. }) => {
                (ease((started.elapsed().as_secs_f32() - 0.10) / 0.38), false)
            }
            _ => (1., true),
        };
        if !covering_reader {
            self.view = AppView::Library;
        }
        self.book_presentation.phase = Some(Phase::Closing {
            started: Instant::now(),
            from,
            covering_reader,
        });
        self.flush_reading_stats();
    }
    pub(crate) fn advance_book_presentation(&mut self) {
        if let Some(Phase::Closing { started, .. }) = &self.book_presentation.phase {
            if started.elapsed().as_secs_f32() >= 0.14 {
                self.view = AppView::Library;
            }
        }
    }
    pub fn render_book_opening(&mut self, ctx: &egui::Context) {
        let screen = ctx.screen_rect();
        let motion = ctx.style().animation_time > 0.;
        let state = &mut self.book_presentation;
        let Some(phase) = &mut state.phase else {
            return;
        };
        if state
            .viewport
            .is_some_and(|old| old.size() != screen.size())
        {
            state.source = None;
        }
        state.viewport = Some(screen);
        if matches!(phase, Phase::Opening { .. })
            && state.shelf.is_empty()
            && self.view != AppView::Reader
        {
            state.shelf = ctx.graphics(|g| g.clone().drain(&[], &Default::default()));
        }
        let (expansion, alpha, show_shelf, done, waiting) = match phase {
            Phase::Opening { started, ready } => {
                let elapsed = started.elapsed().as_secs_f32();
                let cover_fade = if motion && state.source.is_some() {
                    0.10
                } else {
                    0.
                };
                let duration = if motion && state.source.is_some() {
                    0.38
                } else {
                    0.
                };
                // Native preparation starts while the paper is expanding. The final
                // stretch is released by real content readiness, never by a timer alone.
                if self.view == AppView::Reader
                    && !self.defer_custom_font_for_frame
                    && super::reader_images::visible_images_ready(ctx)
                    && ready.is_none()
                {
                    *ready = Some(Instant::now());
                }
                let natural_end: f32 = duration + cover_fade;
                let completed_at = ready
                    .map(|at| natural_end.max(at.duration_since(*started).as_secs_f32() + 0.08));
                let raw = if duration > 0. {
                    ease((elapsed - cover_fade) / duration)
                } else {
                    1.
                };
                let expansion = match completed_at {
                    Some(end) if elapsed >= end => 1.,
                    Some(end) if end > natural_end => {
                        let at = ready.unwrap().duration_since(*started).as_secs_f32();
                        let held = if at < natural_end && duration > 0. {
                            ease((at - cover_fade) / duration).min(0.94)
                        } else {
                            0.94 + 0.05 * (1. - (-(at - natural_end).max(0.) / 0.5).exp())
                        };
                        held + (1. - held) * ease((elapsed - at) / 0.08)
                    }
                    _ if ready.is_none() && elapsed >= natural_end && duration > 0. => {
                        0.94 + 0.05 * (1. - (-(elapsed - natural_end) / 0.5).exp())
                    }
                    _ => raw.min(if ready.is_none() { 0.94 } else { 1. }),
                };
                let fade = completed_at.map_or(0., |end| ((elapsed - end) / 0.16).max(0.));
                (
                    expansion,
                    (if cover_fade > 0. {
                        ease(elapsed / cover_fade)
                    } else {
                        1.
                    }) * (1. - ease(fade)),
                    expansion < 1.,
                    fade >= 1.,
                    elapsed > 0.8 && ready.is_none(),
                )
            }
            Phase::Closing {
                started,
                from,
                covering_reader,
            } => {
                let elapsed = started.elapsed().as_secs_f32();
                let duration = if motion && state.source.is_some() {
                    0.30
                } else {
                    0.16
                };
                if let Some(path) = &self.book_path {
                    if self.view == AppView::Library {
                        state.source = state
                            .covers
                            .get(path)
                            .copied()
                            .filter(|r| screen.contains_rect(*r));
                    }
                }
                let delay = if *covering_reader { 0.14 } else { 0. };
                let cover_fade = if motion && state.source.is_some() {
                    0.10
                } else {
                    0.
                };
                (
                    *from * (1. - ease((elapsed - delay) / duration)),
                    if elapsed < delay {
                        ease(elapsed / delay)
                    } else if state.source.is_none() || !motion {
                        1. - ease((elapsed - delay) / duration)
                    } else {
                        1. - ease((elapsed - delay - duration) / cover_fade)
                    },
                    false,
                    elapsed >= delay + duration + cover_fade,
                    false,
                )
            }
        };
        if done {
            if matches!(state.phase, Some(Phase::Closing { .. })) {
                state.reorder_from = state.covers.clone();
                state.reorder_started = Some(Instant::now());
                state.reorder_book = self.book_path.clone();
                state.shelf_order.reset();
                ctx.request_repaint();
            }
            state.phase = None;
            state.shelf.clear();
            self.opening_started = None;
            return;
        }
        ctx.move_to_top(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("book_presentation_input"),
        ));
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("book_presentation_input"),
        ));
        if show_shelf {
            for clipped in &state.shelf {
                painter
                    .with_clip_rect(clipped.clip_rect)
                    .add(clipped.shape.clone());
            }
        }
        let source = if motion {
            state.source.filter(|r| screen.contains_rect(*r))
        } else {
            None
        };
        let rect = source.map_or(screen, |r| {
            egui::Rect::from_min_max(
                r.min.lerp(screen.min, expansion),
                r.max.lerp(screen.max, expansion),
            )
        });
        painter.rect_filled(
            rect,
            (12. * (1. - expansion)) as u8,
            self.reader_bg_color.gamma_multiply(alpha),
        );
        egui::Area::new(egui::Id::new("book_presentation_input"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                ui.allocate_exact_size(screen.size(), egui::Sense::click_and_drag());
                if waiting {
                    let spinner =
                        egui::Rect::from_center_size(screen.center(), egui::vec2(18., 18.));
                    ui.put(spinner, egui::Spinner::new());
                }
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && !matches!(state.phase, Some(Phase::Closing { .. }))
        {
            self.close_book_presentation();
        }
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod shelf_order_tests {
    use super::StableShelfOrder;
    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|x| x.to_string()).collect()
    }
    #[test]
    fn opened_right_cover_keeps_its_slot_until_close_releases_order() {
        let mut order = StableShelfOrder::default();
        assert_eq!(
            order.resolve(ids(&["left", "right"]), false),
            ids(&["left", "right"])
        );
        assert!(order.resolve(vec![], false).is_empty());
        assert_eq!(
            order.resolve(ids(&["right", "left"]), false),
            ids(&["left", "right"])
        );
        assert_eq!(
            order.resolve(ids(&["new", "right", "left"]), false),
            ids(&["left", "right", "new"])
        );
        assert_eq!(
            order.resolve(ids(&["new", "right"]), false),
            ids(&["right", "new"])
        );
        order.reset();
        assert_eq!(
            order.resolve(ids(&["new", "right"]), false),
            ids(&["new", "right"])
        );
    }
    #[test]
    fn shelf_movement_interpolates_whole_card_then_settles() {
        use eframe::egui::{self, pos2, vec2, Rect};
        use std::time::{Duration, Instant};
        let mut shelf = super::BookPresentation::default();
        let ctx = egui::Context::default();
        let old = Rect::from_min_size(pos2(200., 400.), vec2(160., 240.));
        let target = Rect::from_min_size(pos2(20., 40.), vec2(160., 312.));
        shelf.reorder_from.insert("book".into(), old);
        shelf.reorder_started = Some(Instant::now() - Duration::from_millis(190));
        let middle = shelf.shelf_card_rect("book", target, &ctx);
        assert!(middle.left() > target.left() && middle.left() < old.left());
        assert!(middle.top() > target.top() && middle.top() < old.top());
        assert_eq!(middle.size(), target.size());
        shelf.reorder_started = Some(Instant::now() - Duration::from_millis(400));
        assert_eq!(shelf.shelf_card_rect("book", target, &ctx), target);
        assert!(shelf.reorder_started.is_none());
    }
    #[test]
    fn changing_sort_modes_uses_the_current_ranks() {
        let mut order = StableShelfOrder::default();
        order.resolve(ids(&["left", "right"]), false);
        assert_eq!(
            order.resolve(ids(&["right", "left"]), true),
            ids(&["right", "left"])
        );
        assert_eq!(
            order.resolve(ids(&["left", "right"]), true),
            ids(&["left", "right"])
        );
        assert_eq!(
            order.resolve(ids(&["right", "left"]), false),
            ids(&["right", "left"])
        );
    }
}
