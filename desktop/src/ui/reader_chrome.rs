use crate::app::{AppView, ReaderApp};
use eframe::egui;

const BAR_HEIGHT: f32 = 56.;
const HIDE_AFTER: f64 = 3.;

#[derive(Default)]
pub(crate) struct ReaderChromeState {
    active: bool,
    pub(crate) visible: bool,
    last_engaged: f64,
    hover_since: Option<f64>,
    last_toggle: Option<f64>,
}

impl ReaderChromeState {
    pub(crate) fn toggle(&mut self, now: f64) {
        if self.last_toggle == Some(now) {
            return;
        }
        self.last_toggle = Some(now);
        self.active = true;
        self.visible = !self.visible;
        self.last_engaged = now;
    }
    fn update(&mut self, now: f64, active: bool, reveal: bool, hold: bool) {
        if !active {
            self.active = false;
            self.visible = false;
            self.hover_since = None;
            return;
        }
        if !self.active || reveal {
            self.visible = true;
            self.last_engaged = now;
        }
        self.active = true;
        if self.visible && hold {
            self.last_engaged = now;
        }
        if now - self.last_engaged >= HIDE_AFTER {
            self.visible = false;
        }
    }
}

pub(crate) fn center_click(rect: egui::Rect, pos: egui::Pos2) -> bool {
    rect.contains(pos)
        && (pos.x - rect.left()) / rect.width() > 0.32
        && (pos.x - rect.left()) / rect.width() < 0.68
}

impl ReaderApp {
    pub(crate) fn render_reader_chrome(&mut self, ctx: &egui::Context) {
        let (now, pointer, focused, reveal_key, pointer_down) = ctx.input(|i| {
            (
                i.time,
                i.pointer.hover_pos(),
                i.focused,
                i.key_pressed(egui::Key::F6),
                i.pointer.any_down(),
            )
        });
        if self.view != AppView::Reader
            || self.show_csc_settings
            || self.show_stats
            || self.show_export_dialog
            || self.show_language
            || self.show_about
        {
            self.reader_chrome.update(now, false, false, false);
            return;
        }

        // Overlay the full page, including its upper margin. No invisible panel
        // remains behind to shorten the paper or its page-turn mesh.
        let paper = self.paging_page_rect.unwrap_or_else(|| ctx.screen_rect());
        let slot = egui::Rect::from_min_size(paper.min, egui::vec2(paper.width(), BAR_HEIGHT));
        let at_top = focused
            && !pointer_down
            && !self.page_curl.busy()
            && pointer.is_some_and(|p| slot.contains(p));
        if at_top {
            self.reader_chrome.hover_since.get_or_insert(now);
            if !self.reader_chrome.visible {
                ctx.request_repaint_after(std::time::Duration::from_millis(120));
            }
        } else {
            self.reader_chrome.hover_since = None;
        }
        let hover_reveal = self
            .reader_chrome
            .hover_since
            .is_some_and(|t| now - t >= 0.12);
        let over_bar =
            focused && self.reader_chrome.visible && pointer.is_some_and(|p| slot.contains(p));
        let tools_busy = self.show_reader_introduction
            || ctx.memory(|m| m.any_popup_open())
            || ctx.wants_keyboard_input()
            || self.show_csc_settings
            || self.show_settings
            || self.show_search
            || self.show_annotations
            || self.show_review_panel
            || self.show_stats
            || self.show_export_dialog
            || self.show_language
            || self.show_about
            || self.text_selection.is_some()
            || self.csc_popup.is_some();
        self.reader_chrome.update(
            now,
            true,
            hover_reveal
                || reveal_key
                || ctx.memory(|m| m.is_popup_open(egui::Id::new("reader_quick_menu"))),
            over_bar || tools_busy || (focused && pointer_down),
        );
        if !tools_busy && (self.page_curl.busy() || self.page_anim_progress < 1.) {
            self.reader_chrome.visible = false;
            self.reader_chrome.hover_since = None;
        }
        if self.reader_chrome.visible {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                (HIDE_AFTER - (now - self.reader_chrome.last_engaged)).max(0.01),
            ));
        }
        let progress = ctx.animate_bool_with_time(
            egui::Id::new("reader_chrome_visibility"),
            self.reader_chrome.visible,
            0.18,
        );
        if progress <= 0.001 {
            return;
        }
        egui::Area::new(egui::Id::new("reader_chrome_overlay"))
            .order(egui::Order::Foreground)
            .fixed_pos(slot.min)
            .constrain(false)
            .fade_in(false)
            .show(ctx, |ui| {
                ui.set_clip_rect(slot);
                ui.set_opacity(progress);
                if !self.reader_chrome.visible {
                    ui.disable();
                }
                egui::Frame::NONE
                    .inner_margin(egui::Margin {
                        left: 16,
                        right: 16,
                        top: 8,
                        bottom: 8,
                    })
                    .show(ui, |ui| {
                        ui.set_min_size(egui::vec2((slot.width() - 32.).max(0.), 40.));
                        ui.set_max_width((slot.width() - 32.).max(0.));
                        self.render_toolbar(ui);
                    });
            });
        if self.view == AppView::Reader {
            let anchor = egui::pos2(paper.right() - 60., paper.bottom() - 60.);
            egui::Area::new(egui::Id::new("reader_menu_anchor"))
                .order(egui::Order::Foreground)
                .fixed_pos(anchor)
                .constrain(false)
                .fade_in(false)
                .show(ctx, |ui| {
                    ui.set_opacity(progress);
                    if !self.reader_chrome.visible {
                        ui.disable();
                    }
                    self.render_reader_menu_button(ui);
                });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_hide_reveal_and_return_from_library() {
        let mut state = ReaderChromeState::default();
        state.update(10., true, false, false);
        assert!(state.visible);
        state.update(12.9, true, false, false);
        assert!(state.visible);
        state.update(13., true, false, false);
        assert!(!state.visible);
        state.update(14., true, true, false);
        assert!(state.visible);
        state.update(20., false, false, false);
        state.update(30., true, false, false);
        assert!(state.visible);
    }

    #[test]
    fn menu_or_held_pointer_gets_a_fresh_idle_period_after_release() {
        let mut state = ReaderChromeState::default();
        state.update(0., true, false, false);
        state.update(20., true, false, true);
        assert!(state.visible);
        state.update(22.9, true, false, false);
        assert!(state.visible);
        state.update(23., true, false, false);
        assert!(!state.visible);
        // Reading gestures must not reveal the bar.
        state.update(24., true, false, true);
        assert!(!state.visible);
    }

    #[test]
    fn hiding_and_revealing_preserve_the_reading_viewport() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let dir = tempfile::tempdir().unwrap();
        let mut app = ReaderApp::from_data_dir(dir.path().to_string_lossy().into_owned());
        app.view = AppView::Reader;
        let ctx = egui::Context::default();
        let mut viewport = None;
        for (t, pointer, visible) in [
            (0., None, true),
            (4., None, false),
            (4.5, None, false),
            (5., Some(egui::pos2(600., 5.)), false),
            (5.3, Some(egui::pos2(600., 30.)), true),
            (5.6, Some(egui::pos2(600., 30.)), true),
        ] {
            let _ = ctx.run(
                egui::RawInput {
                    time: Some(t),
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200., 800.),
                    )),
                    events: pointer.map(egui::Event::PointerMoved).into_iter().collect(),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            let rect = ui.available_rect_before_wrap();
                            assert_eq!(
                                rect.top(),
                                0.,
                                "paper must reach the top of the content area"
                            );
                            assert_eq!(*viewport.get_or_insert(rect), rect);
                        });
                    app.render_reader_chrome(ctx);
                },
            );
            assert_eq!(app.reader_chrome.visible, visible);
        }
    }
    #[test]
    fn open_menu_revives_fading_controls_and_keeps_them_visible() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let dir = tempfile::tempdir().unwrap();
        let mut app = ReaderApp::from_data_dir(dir.path().to_string_lossy().into_owned());
        app.view = AppView::Reader;
        let ctx = egui::Context::default();
        let popup = egui::Id::new("reader_quick_menu");
        for (time, menu, visible) in [
            (0., false, true),
            (4., false, false),
            (4.1, true, true),
            (20., true, true),
        ] {
            if menu {
                ctx.memory_mut(|m| m.open_popup(popup));
            }
            let _ = ctx.run(
                egui::RawInput {
                    time: Some(time),
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200., 800.),
                    )),
                    events: vec![egui::Event::PointerMoved(egui::pos2(600., 400.))],
                    ..Default::default()
                },
                |ctx| app.render_reader_chrome(ctx),
            );
            assert_eq!(app.reader_chrome.visible, visible);
            if menu {
                assert!(ctx.memory(|m| m.is_popup_open(popup)));
            }
        }
    }
}
