//! Keyboard dispatch stays behind the reader's focus and overlay guards.
use super::{AppView, ReaderApp};
use eframe::egui;

impl ReaderApp {
    pub(super) fn handle_reader_shortcuts(&mut self, ctx: &egui::Context) {
        if self.view == AppView::Reader
            && !self.show_reader_introduction
            && !self.show_sharing_panel
            && !self.show_review_panel
            && !self.show_csc_settings
            && !self.show_stats
            && !self.show_export_dialog
            && !self.show_language
            && !ctx.wants_keyboard_input()
            && !ctx.memory(|m| m.any_popup_open())
        {
            ctx.input(|i| {
                if i.key_pressed(egui::Key::ArrowLeft) {
                    if self.scroll_mode {
                        self.prev_chapter();
                    } else {
                        self.turn_reader_page(false);
                    }
                }
                if i.key_pressed(egui::Key::ArrowRight) {
                    if self.scroll_mode {
                        self.next_chapter();
                    } else {
                        self.turn_reader_page(true);
                    }
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> (tempfile::TempDir, ReaderApp) {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let directory = tempfile::tempdir().unwrap();
        let mut app = ReaderApp::from_data_dir(directory.path().to_string_lossy().into_owned());
        app.view = AppView::Reader;
        app.reader_introduction_seen = true;
        app.show_reader_introduction = false;
        app.reader_page_animation = "None".into();
        app.is_dual_column = false;
        app.scroll_mode = false;
        app.total_pages = 4;
        app.current_page = 1;
        (directory, app)
    }
    fn key(app: &mut ReaderApp, ctx: &egui::Context, key: egui::Key) {
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ctx| app.handle_reader_shortcuts(ctx),
        );
    }
    #[test]
    fn page_arrows_dispatch_and_return_through_the_real_reader_actions() {
        let (_root, mut app) = app();
        let ctx = egui::Context::default();
        key(&mut app, &ctx, egui::Key::ArrowRight);
        assert_eq!(app.current_page, 2);
        key(&mut app, &ctx, egui::Key::ArrowLeft);
        assert_eq!(app.current_page, 1);
    }
    #[test]
    fn scroll_arrows_move_between_actual_lazy_chapters() {
        let (_root, mut app) = app();
        let ctx = egui::Context::default();
        app.book = Some(
            reader_core::epub::EpubBook::open_lazy(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../ios/Tests/fixtures/native-reading-modes.epub"
            ))
            .unwrap(),
        );
        assert!(app.total_chapters() > 2);
        app.current_chapter = 1;
        app.scroll_mode = true;
        key(&mut app, &ctx, egui::Key::ArrowRight);
        assert_eq!(app.current_chapter, 2);
        key(&mut app, &ctx, egui::Key::ArrowLeft);
        assert_eq!(app.current_chapter, 1);
    }
    #[test]
    fn text_edit_focus_popup_and_reader_overlays_do_not_turn_the_page() {
        let (_root, mut app) = app();
        let ctx = egui::Context::default();
        let editor = egui::Id::new("owned-shortcut-editor");
        let mut draft = "Note draft".to_owned();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.add(egui::TextEdit::singleline(&mut draft).id(editor))
                    .request_focus();
            });
        });
        assert!(ctx.wants_keyboard_input());
        key(&mut app, &ctx, egui::Key::ArrowRight);
        assert_eq!(app.current_page, 1);
        ctx.memory_mut(|memory| memory.surrender_focus(editor));
        ctx.memory_mut(|memory| memory.open_popup(egui::Id::new("owned-shortcut-popup")));
        key(&mut app, &ctx, egui::Key::ArrowRight);
        assert_eq!(app.current_page, 1);
        ctx.memory_mut(|memory| memory.close_popup());
        app.show_sharing_panel = true;
        key(&mut app, &ctx, egui::Key::ArrowRight);
        assert_eq!(app.current_page, 1);
        app.show_sharing_panel = false;
        app.show_reader_introduction = true;
        key(&mut app, &ctx, egui::Key::ArrowRight);
        assert_eq!(app.current_page, 1);
        app.show_reader_introduction = false;
        app.view = AppView::Library;
        key(&mut app, &ctx, egui::Key::ArrowRight);
        assert_eq!(app.current_page, 1);
    }
}
