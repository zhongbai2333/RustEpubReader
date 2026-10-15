use crate::app::{AppView, ReaderApp};
use eframe::egui;

impl ReaderApp {
    pub(crate) fn render_reader_introduction(&mut self, ctx: &egui::Context) {
        if self.view != AppView::Reader {
            self.show_reader_introduction = false;
            return;
        }
        let can_show = !self.reader_introduction_seen
            && self.book.is_some()
            && self.pending_book_open.is_none()
            && !self.book_presentation.is_active()
            && !self.defer_custom_font_for_frame
            && ctx.input(|i| i.focused)
            && !self.show_settings
            && !self.show_search
            && !self.show_annotations
            && !self.show_review_panel
            && !self.show_stats
            && !self.show_export_dialog
            && !self.show_language
            && !self.show_about
            && !self.show_appearance_linkage
            && !ctx.memory(|m| m.any_popup_open());
        if can_show {
            self.show_reader_introduction = true;
        }
        if !self.show_reader_introduction {
            return;
        }
        let response = egui::Modal::new(egui::Id::new("reader_introduction")).show(ctx, |ui| {
            // Mark an actual presentation, so a restart never repeats an interruption.
            self.reader_introduction_seen = true;
            egui::Frame::NONE.inner_margin(16.).show(ui, |ui| {
                ui.set_width((ctx.screen_rect().width() - 80.).clamp(240., 420.));
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(self.i18n.t("reader.intro_title"))
                            .size(20.)
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if super::design::close_button(ui, self.i18n.t("settings.close")).clicked()
                        {
                            self.show_reader_introduction = false;
                        }
                    });
                });
                ui.add_space(18.);
                egui::ScrollArea::vertical()
                    .max_height((ctx.screen_rect().height() - 220.).clamp(100., 380.))
                    .show(ui, |ui| {
                        for (icon, title, body) in [
                            (
                                super::design::Icon::Book,
                                "reader.intro_turns_title",
                                "reader.intro_turns_desktop",
                            ),
                            (
                                super::design::Icon::More,
                                "reader.intro_menu_title",
                                "reader.intro_menu_desktop",
                            ),
                        ] {
                            ui.horizontal_top(|ui| {
                                ui.spacing_mut().item_spacing.x = 14.;
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(26., 26.),
                                    egui::Sense::hover(),
                                );
                                super::design::paint_icon(
                                    ui.painter(),
                                    rect,
                                    icon,
                                    ui.visuals().hyperlink_color,
                                );
                                ui.vertical(|ui| {
                                    ui.strong(self.i18n.t(title));
                                    ui.add_space(5.);
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(self.i18n.t(body))
                                                .color(ui.visuals().text_color()),
                                        )
                                        .wrap(),
                                    );
                                });
                            });
                            ui.add_space(20.);
                        }
                    });
                ui.add_space(4.);
                if ui
                    .add_sized(
                        [ui.available_width(), 44.],
                        egui::Button::new(self.i18n.t("reader.intro_start")).selected(true),
                    )
                    .clicked()
                {
                    self.show_reader_introduction = false;
                }
            });
        });
        if response.should_close() {
            self.show_reader_introduction = false;
        }
        if !self.show_reader_introduction {
            self.reader_chrome.visible = true;
        }
    }
}
