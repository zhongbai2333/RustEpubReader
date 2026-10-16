use super::{
    page_curl::{self, CurlGeometry, Motion, PagePosition},
    reader_block::render_content_layout,
    reader_state::*,
};
use crate::app::ReaderApp;
use eframe::egui::{
    self,
    epaint::{ClippedShape, Mesh, Primitive},
    Color32, Rect, UiBuilder,
};
use std::hash::{Hash, Hasher};

#[derive(Default)]
pub(crate) struct PageCurlState {
    pending: Option<(bool, Option<usize>)>,
    active: Option<CurlTurn>,
    held_after_commit: bool,
}
impl PageCurlState {
    pub fn clear(&mut self) {
        self.pending = None;
        self.active = None;
        self.held_after_commit = false;
    }
    pub fn source(&self) -> Option<PagePosition> {
        self.active.as_ref().map(|turn| turn.source)
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some() || self.active.is_some()
    }
    pub fn request(&mut self, forward: bool, page: Option<usize>) {
        // Keep one latest request while the current sheet is settling.
        self.pending = Some((forward, page));
    }
}
struct PageImage {
    rect: Rect,
    shapes: Vec<ClippedShape>,
    meshes: Vec<(Rect, Mesh)>,
    atlas_size: [usize; 2],
    _textures: Vec<egui::TextureHandle>,
}
impl PageImage {
    fn refresh(&mut self, ctx: &egui::Context) {
        let size = ctx.fonts(|f| f.font_image_size());
        if self.meshes.is_empty() || self.atlas_size != size {
            self.meshes = ctx
                .tessellate(self.shapes.clone(), ctx.pixels_per_point())
                .into_iter()
                .filter_map(|p| match p.primitive {
                    Primitive::Mesh(m) => Some((p.clip_rect.intersect(self.rect), m)),
                    _ => None,
                })
                .collect();
            self.atlas_size = size;
        }
    }
    fn paint(&self, painter: &egui::Painter, paper: Color32) {
        painter.rect_filled(self.rect, 0., paper);
        for (clip, mesh) in &self.meshes {
            painter
                .with_clip_rect(*clip)
                .add(egui::Shape::mesh(mesh.clone()));
        }
    }
}
struct CurlTurn {
    source: PagePosition,
    target: PagePosition,
    target_ranges: super::reader_pagination::PageRanges,
    source_ranges: super::reader_pagination::PageRanges,
    image_revision: u64,
    forward: bool,
    dual: bool,
    images: Vec<PageImage>, // source left/right, target left/right (or source/target)
    signature: u64,
    progress: f32,
    tilt: f32,
    anchor_y: f32,
    motion: Motion,
}

impl ReaderApp {
    pub(crate) fn request_curl_turn(&mut self, forward: bool, page: Option<usize>) {
        self.page_curl.request(forward, page);
    }

    pub(crate) fn turn_reader_page(&mut self, forward: bool) {
        if self.reader_page_animation == "Realistic" && !self.scroll_mode {
            self.request_curl_turn(forward, None);
            return;
        }
        if self.is_dual_column && !self.scroll_mode {
            if forward {
                if self.current_page + 2 < self.total_pages {
                    self.trigger_page_animation_to(self.current_page + 2, 1.);
                } else if self.current_chapter + 1 < self.total_chapters() {
                    self.capture_cross_chapter_snapshot();
                    self.next_chapter();
                    self.start_cross_chapter_animation(1.);
                }
            } else if self.current_page >= 2 {
                self.trigger_page_animation_to(self.current_page - 2, -1.);
            } else if self.current_chapter > 0 {
                self.capture_cross_chapter_snapshot();
                self.prev_chapter();
                self.current_page = usize::MAX;
                self.start_cross_chapter_animation(-1.);
            }
        } else if forward {
            self.next_page();
        } else {
            self.prev_page();
        }
    }

    fn curl_signature(&self, rect: Rect, dual: bool) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.book_path.hash(&mut h);
        self.current_chapter.hash(&mut h);
        self.current_page.hash(&mut h);
        for v in [
            rect.left(),
            rect.top(),
            rect.width(),
            rect.height(),
            self.font_size,
            self.line_spacing,
            self.para_spacing,
            self.title_font_scale,
            self.reader_bg_image_alpha,
        ] {
            v.to_bits().hash(&mut h);
        }
        self.text_indent.hash(&mut h);
        self.reader_bg_color.hash(&mut h);
        self.reader_font_color.hash(&mut h);
        self.reader_font_family.hash(&mut h);
        self.reader_cjk_font_family.hash(&mut h);
        self.reader_bg_image_path.hash(&mut h);
        self.defer_custom_font_for_frame.hash(&mut h);
        dual.hash(&mut h);
        h.finish()
    }
    fn curl_ranges(
        &self,
        ui: &mut egui::Ui,
        chapter: usize,
        height: f32,
        width: f32,
    ) -> super::reader_pagination::PageRanges {
        self.measured_chapter_pages(ui, chapter, height, width)
    }
    fn curl_target(
        &self,
        ui: &mut egui::Ui,
        forward: bool,
        dual: bool,
        height: f32,
        width: f32,
    ) -> Option<(PagePosition, super::reader_pagination::PageRanges)> {
        let from = PagePosition {
            chapter: self.current_chapter,
            page: self.current_page,
        };
        let previous = if !forward && from.page < if dual { 2 } else { 1 } && from.chapter > 0 {
            self.curl_ranges(ui, from.chapter - 1, height, width)
        } else {
            Default::default()
        };
        let target = page_curl::destination(
            from,
            self.total_pages,
            previous.len(),
            self.total_chapters(),
            dual,
            forward,
        )?;
        let ranges = if target.chapter == from.chapter {
            self.page_block_ranges.clone()
        } else if !forward {
            previous
        } else {
            self.curl_ranges(ui, target.chapter, height, width)
        };
        Some((target, ranges))
    }
    #[allow(clippy::too_many_arguments)]
    fn draw_curl_page(
        &self,
        ui: &mut egui::Ui,
        physical: Rect,
        content: Rect,
        chapter: usize,
        page: usize,
        ranges: &super::reader_pagination::PageRanges,
        text_width: f32,
        h_margin: f32,
        font: &str,
        link: &mut Option<String>,
    ) {
        let Some(ch) = self.book.as_ref().and_then(|b| b.chapters.get(chapter)) else {
            return;
        };
        // Keep the selected paper image on both sides of a spread, including blank pages.
        if let Some(texture) = &self.reader_bg_texture {
            let full = self.paging_page_rect.unwrap_or(physical);
            let mut image = page_curl::paper_mesh(
                physical,
                Color32::from_white_alpha((self.reader_bg_image_alpha * 255.) as u8),
            );
            image.texture_id = texture.id();
            for vertex in &mut image.vertices {
                vertex.uv = egui::pos2(
                    (vertex.pos.x - full.left()) / full.width(),
                    (vertex.pos.y - full.top()) / full.height(),
                );
            }
            ui.painter()
                .with_clip_rect(physical)
                .add(egui::Shape::mesh(image));
        }
        // The odd last spread has a genuine blank facing page.
        if page >= ranges.len().max(1) {
            return;
        }
        let (start, end) = ranges.get(page).copied().unwrap_or((0, 0));
        let highlights = self
            .book_config
            .as_ref()
            .map(|cfg| {
                let mut map = std::collections::HashMap::<
                    usize,
                    Vec<(usize, usize, reader_core::library::HighlightColor)>,
                >::new();
                for h in cfg.highlights.iter().filter(|h| h.chapter == chapter) {
                    map.entry(h.start_block).or_default().push((
                        h.start_offset,
                        h.end_offset,
                        h.color.clone(),
                    ));
                }
                map
            })
            .unwrap_or_default();
        let mut child = ui.new_child(
            UiBuilder::new()
                .id_salt(("curl_page", chapter, page))
                .max_rect(content)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(physical.intersect(ui.clip_rect()));
        render_content_layout(
            &mut child,
            h_margin,
            text_width,
            &ch.title,
            &ch.blocks,
            start.min(ch.blocks.len()),
            end.min(ch.blocks.len()),
            page == 0,
            ranges.fragment(page),
            self.font_size,
            self.title_font_scale,
            self.reader_bg_color,
            chapter,
            self.reader_font_color,
            font,
            &self.i18n,
            link,
            &highlights,
        );
    }
    #[allow(clippy::too_many_arguments)]
    fn capture_curl_page(
        &self,
        ui: &mut egui::Ui,
        physical: Rect,
        content: Rect,
        chapter: usize,
        page: usize,
        ranges: &super::reader_pagination::PageRanges,
        text_width: f32,
        h_margin: f32,
        font: &str,
    ) -> PageImage {
        let ctx = ui.ctx().clone();
        let layer = egui::LayerId::new(egui::Order::Background, egui::Id::new("curl_capture"));
        let mut child = ui.new_child(
            UiBuilder::new()
                .id_salt(("curl_capture", chapter, page))
                .layer_id(layer)
                .max_rect(physical),
        );
        child.set_clip_rect(physical);
        let old_galleys = BLOCK_GALLEYS.with(|g| std::mem::take(&mut *g.borrow_mut()));
        let old_csc = CSC_RECTS.with(|g| std::mem::take(&mut *g.borrow_mut()));
        let old_corrections = CSC_CORRECTIONS.with(|g| std::mem::take(&mut *g.borrow_mut()));
        CSC_CORRECTIONS.with(|g| {
            if self.csc_mode != reader_core::csc::CorrectionMode::None {
                *g.borrow_mut() = self
                    .csc_cache
                    .iter()
                    .filter(|((ch, _), _)| *ch == chapter)
                    .map(|((ch, block), corrections)| {
                        (BlockKey::new(*ch, *block), corrections.clone())
                    })
                    .collect();
            }
        });
        self.draw_curl_page(
            &mut child, physical, content, chapter, page, ranges, text_width, h_margin, font,
            &mut None,
        );
        // The gutter is ink on each physical sheet, not a screen-space overlay.
        // Capturing it here lets the turned sheet carry/occlude its own half.
        if self.is_dual_column {
            if let Some(spread) = self.paging_page_rect {
                paint_spine(child.painter(), spread);
            }
        }
        BLOCK_GALLEYS.with(|g| *g.borrow_mut() = old_galleys);
        CSC_RECTS.with(|g| *g.borrow_mut() = old_csc);
        CSC_CORRECTIONS.with(|g| *g.borrow_mut() = old_corrections);
        let shapes: Vec<ClippedShape> = ctx
            .graphics_mut(|g| std::mem::take(g.entry(layer)))
            .all_entries()
            .cloned()
            .collect();
        let textures = super::reader_images::retain_textures(&ctx, &shapes);
        PageImage {
            _textures: textures,
            rect: physical,
            shapes,
            meshes: Vec::new(),
            atlas_size: [0, 0],
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_curl_reader(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        dual: bool,
        text_width: f32,
        h_margin: f32,
        font: &str,
        link: &mut Option<String>,
    ) -> bool {
        let ctx = ui.ctx().clone();
        let now = ui.input(|i| i.time);
        let selecting_text = ui.input(|i| i.modifiers.shift);
        let pressed_at = ui
            .input(|i| {
                i.events.iter().find_map(|event| match event {
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        ..
                    } => Some(*pos),
                    _ => None,
                })
            })
            .filter(|p| {
                !selecting_text
                    && rect.contains(*p)
                    && ctx
                        .layer_id_at(*p)
                        .is_none_or(|layer| layer.order <= ui.layer_id().order)
            });
        let signature = self.curl_signature(rect, dual);
        if self
            .page_curl
            .active
            .as_ref()
            .is_some_and(|t| t.signature != signature)
        {
            self.page_curl.clear();
        }
        let blocked = self.show_stats
            || self.show_export_dialog
            || self.show_review_panel
            || self.text_selection.is_some()
            || self.clicked_highlight_id.is_some()
            || self.csc_popup.is_some()
            || self.csc_custom_replace_active;
        let escape = ui.input(|i| i.key_pressed(egui::Key::Escape) || !i.focused);
        if blocked {
            self.page_curl.clear();
        }
        let mut consumed = self.page_curl.held_after_commit;
        if self.page_curl.held_after_commit && !ui.input(|i| i.pointer.primary_down()) {
            self.page_curl.held_after_commit = false;
        }
        if let Some(mut turn) = self.page_curl.active.take() {
            consumed = true;
            if matches!(turn.motion, Motion::Settle { .. }) && !escape {
                // Sample before grabbing so the sheet stays exactly where it was.
                turn.progress = turn.motion.sample(now).0;
                if let Some(origin) = pressed_at {
                    turn.motion = Motion::Drag {
                        origin,
                        last: origin,
                        moved: true,
                        base_progress: turn.progress,
                        base_tilt: turn.tilt,
                    };
                    self.page_curl.pending = None;
                } else if self
                    .page_curl
                    .pending
                    .is_some_and(|(forward, _)| forward != turn.forward)
                {
                    self.page_curl.pending = None;
                    turn.motion =
                        Motion::settle(turn.progress, false, now, self.reader_page_animation_speed);
                }
            }
            if escape {
                turn.motion =
                    Motion::settle(turn.progress, false, now, self.reader_page_animation_speed);
                if !ui.input(|i| i.focused) {
                    turn.progress = 0.;
                    turn.motion = Motion::Settle {
                        from: 0.,
                        to: 0.,
                        start: now - 1.,
                        duration: 0.1,
                    };
                }
            }
            if let Motion::Drag {
                origin,
                last,
                moved,
                base_progress,
                base_tilt,
            } = turn.motion
            {
                let pos = ui.input(|i| i.pointer.interact_pos()).unwrap_or(last);
                let width = rect.width();
                turn.tilt = base_tilt
                    + ((origin.y - pos.y) / width).clamp(-0.4, 0.4)
                        * if turn.forward { 1. } else { -1. };
                let sheet = if turn.dual { turn.images[1].rect } else { rect };
                turn.progress = drag_progress(
                    sheet,
                    turn.forward,
                    turn.dual,
                    turn.anchor_y,
                    base_progress,
                    base_tilt,
                    turn.tilt,
                    pos.x - origin.x,
                );
                let moved = moved || (pos - origin).length() > 5.;
                if moved
                    && (if turn.forward {
                        pos.x <= rect.left() + 8.
                    } else {
                        pos.x >= rect.right() - 8.
                    })
                {
                    turn.progress = 1.;
                }
                turn.motion = Motion::Drag {
                    origin,
                    last: pos,
                    moved,
                    base_progress,
                    base_tilt,
                };
                if ui.input(|i| i.pointer.primary_released()) {
                    let v = ui.input(|i| i.pointer.velocity().x) / width
                        * if turn.forward { -1. } else { 1. };
                    turn.motion = Motion::settle(
                        turn.progress,
                        page_curl::release_commits(turn.progress, moved, v),
                        now,
                        self.reader_page_animation_speed,
                    );
                } else if !ui.input(|i| i.pointer.primary_down()) {
                    turn.motion =
                        Motion::settle(turn.progress, false, now, self.reader_page_animation_speed);
                }
            }
            let finished = if turn.progress >= 0.9999
                && matches!(turn.motion, Motion::Drag { moved: true, .. })
            {
                self.page_curl.held_after_commit = ui.input(|i| i.pointer.primary_down());
                Some(true)
            } else if matches!(turn.motion, Motion::Settle { .. }) {
                let (p, end) = turn.motion.sample(now);
                turn.progress = p;
                end
            } else {
                None
            };
            if let Some(commit) = finished {
                ctx.request_repaint();
                if commit
                    && turn.source
                        == (PagePosition {
                            chapter: self.current_chapter,
                            page: self.current_page,
                        })
                {
                    if turn.target.chapter > self.current_chapter {
                        self.next_chapter();
                    } else if turn.target.chapter < self.current_chapter {
                        self.prev_chapter();
                    }
                    self.current_page = turn.target.page;
                    self.page_block_ranges = turn.target_ranges;
                    self.total_pages = self.page_block_ranges.len().max(1);
                    self.pages_dirty = false;
                    self.tts_detach_view();
                    self.sel_press_origin = None;
                    let block = self
                        .page_block_ranges
                        .get(self.current_page)
                        .map_or(0, |r| r.0);
                    let offset = self
                        .page_block_ranges
                        .fragment(self.current_page)
                        .map_or(0, |f| f.char_offset);
                    self.schedule_position_save_at(self.current_chapter, block, offset);
                }
            } else {
                self.page_curl.active = Some(turn);
            }
        }
        let half = rect.width() / 2.;
        let physical = if dual {
            vec![
                Rect::from_min_max(rect.min, egui::pos2(rect.left() + half, rect.bottom())),
                Rect::from_min_max(egui::pos2(rect.left() + half, rect.top()), rect.max),
            ]
        } else {
            vec![rect]
        };
        let contents: Vec<Rect> = physical
            .iter()
            .enumerate()
            .map(|(i, r)| {
                if !dual {
                    *r
                } else if i == 0 {
                    Rect::from_min_max(
                        r.min,
                        egui::pos2(r.right() - DUAL_COLUMN_GAP / 2., r.bottom()),
                    )
                } else {
                    Rect::from_min_max(egui::pos2(r.left() + DUAL_COLUMN_GAP / 2., r.top()), r.max)
                }
            })
            .collect();
        // A decoder completion refreshes captured paper without resetting the
        // user's drag/settle progress. Source/target ranges survive chapter commit.
        if self
            .page_curl
            .active
            .as_ref()
            .is_some_and(|turn| turn.image_revision != super::reader_images::revision())
        {
            let captured_revision = super::reader_images::revision();
            let mut turn = self.page_curl.active.take().unwrap();
            let mut images = Vec::new();
            for (pos, ranges) in [
                (turn.source, &turn.source_ranges),
                (turn.target, &turn.target_ranges),
            ] {
                for (i, physical) in physical.iter().enumerate() {
                    images.push(self.capture_curl_page(
                        ui,
                        *physical,
                        contents[i],
                        pos.chapter,
                        pos.page + i,
                        ranges,
                        text_width,
                        h_margin,
                        font,
                    ));
                }
            }
            for image in &mut images {
                image.refresh(&ctx);
            }
            turn.images = images;
            turn.image_revision = captured_revision;
            self.page_curl.active = Some(turn);
        }
        if self.page_curl.active.is_none() {
            for (i, r) in physical.iter().enumerate() {
                ui.painter().rect_filled(*r, 0., self.reader_bg_color);
                self.draw_curl_page(
                    ui,
                    *r,
                    contents[i],
                    self.current_chapter,
                    self.current_page + i,
                    &self.page_block_ranges,
                    text_width,
                    h_margin,
                    font,
                    link,
                );
            }
        }
        let edge = (rect.width() * if dual { 0.14 } else { 0.28 }).max(48.);
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let edge_direction = pointer
            .filter(|p| {
                rect.contains(*p)
                    && ctx
                        .layer_id_at(*p)
                        .is_none_or(|layer| layer.order <= ui.layer_id().order)
            })
            .and_then(|p| {
                if p.x < rect.left() + edge {
                    Some(false)
                } else if p.x > rect.right() - edge {
                    Some(true)
                } else {
                    None
                }
            });
        if !blocked
            && !selecting_text
            && self.page_curl.active.is_none()
            && edge_direction.is_some()
        {
            ctx.set_cursor_icon(egui::CursorIcon::Grab);
        }
        // A fast drag may deliver press, motion and release in one frame. Its direction
        // belongs to the press position, never to the final pointer position.
        let drag = if !consumed && !blocked && !escape {
            pressed_at
                .filter(|p| {
                    rect.contains(*p)
                        && ctx
                            .layer_id_at(*p)
                            .is_none_or(|layer| layer.order <= ui.layer_id().order)
                })
                .and_then(|origin| {
                    if origin.x < rect.left() + edge {
                        Some((false, origin))
                    } else if origin.x > rect.right() - edge {
                        Some((true, origin))
                    } else {
                        None
                    }
                })
        } else {
            None
        };
        let request = if self.page_curl.active.is_none() {
            self.page_curl.pending.take()
        } else {
            None
        };
        if !blocked && !escape && self.page_curl.active.is_none() {
            if let Some((forward, origin, page_override)) = drag
                .map(|(d, p)| (d, Some(p), None))
                .or_else(|| request.map(|(d, p)| (d, None, p)))
            {
                let target = page_override
                    .filter(|&p| p != self.current_page && p < self.total_pages)
                    .map(|p| {
                        (
                            PagePosition {
                                chapter: self.current_chapter,
                                page: if dual { p / 2 * 2 } else { p },
                            },
                            self.page_block_ranges.clone(),
                        )
                    })
                    .or_else(|| {
                        if page_override.is_none() {
                            self.curl_target(ui, forward, dual, rect.height(), text_width)
                        } else {
                            None
                        }
                    });
                if let Some((target, target_ranges)) = target {
                    let source = PagePosition {
                        chapter: self.current_chapter,
                        page: self.current_page,
                    };
                    let captured_revision = super::reader_images::revision();
                    let mut images = Vec::new();
                    for (pos, ranges) in
                        [(source, &self.page_block_ranges), (target, &target_ranges)]
                    {
                        for (i, r) in physical.iter().enumerate() {
                            images.push(self.capture_curl_page(
                                ui,
                                *r,
                                contents[i],
                                pos.chapter,
                                pos.page + i,
                                ranges,
                                text_width,
                                h_margin,
                                font,
                            ));
                        }
                    }
                    // Tessellate only after all four pages have populated the shared font atlas.
                    for image in &mut images {
                        image.refresh(&ctx);
                    }
                    let mut initial_progress = 0.;
                    let mut initial_tilt = 0.12;
                    let motion = if let Some(origin) = origin {
                        let pos = pointer.unwrap_or(origin);
                        let width = rect.width();
                        initial_tilt = ((origin.y - pos.y) / width).clamp(-0.4, 0.4)
                            * if forward { 1. } else { -1. };
                        initial_progress = drag_progress(
                            if dual { physical[1] } else { rect },
                            forward,
                            dual,
                            origin.y - rect.top(),
                            0.,
                            0.,
                            initial_tilt,
                            pos.x - origin.x,
                        );
                        let moved = (pos - origin).length() > 5.;
                        if ui.input(|i| i.pointer.primary_released()) {
                            let velocity = ui.input(|i| i.pointer.velocity().x) / width
                                * if forward { -1. } else { 1. };
                            Motion::settle(
                                initial_progress,
                                page_curl::release_commits(initial_progress, moved, velocity),
                                now,
                                self.reader_page_animation_speed,
                            )
                        } else {
                            Motion::Drag {
                                origin,
                                last: pos,
                                moved,
                                base_progress: 0.,
                                base_tilt: 0.,
                            }
                        }
                    } else {
                        Motion::settle(0., true, now, self.reader_page_animation_speed)
                    };
                    self.page_curl.active = Some(CurlTurn {
                        source,
                        target,
                        source_ranges: self.page_block_ranges.clone(),
                        image_revision: captured_revision,
                        target_ranges,
                        forward,
                        dual,
                        images,
                        signature,
                        progress: initial_progress,
                        tilt: initial_tilt,
                        anchor_y: origin.map_or(rect.height(), |p| p.y - rect.top()),
                        motion,
                    });
                    self.sel_press_origin = None;
                    consumed = true;
                } else if drag.is_some() {
                    consumed = true;
                    self.sel_press_origin = None;
                }
            }
        }
        if let Some(turn) = &mut self.page_curl.active {
            for image in &mut turn.images {
                image.refresh(&ctx);
            }
            paint_turn(
                ui.painter().with_clip_rect(rect),
                rect,
                turn,
                self.reader_bg_color,
            );
            if matches!(turn.motion, Motion::Drag { .. }) {
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
        }
        if dual && self.page_curl.active.is_none() {
            paint_spine(ui.painter(), rect);
        }
        // Page counters belong to committed state; previews never publish progress.
        let pages = if dual && self.current_page + 1 < self.total_pages {
            format!(
                "{}–{} / {}",
                self.current_page + 1,
                (self.current_page + 2).min(self.total_pages),
                self.total_pages
            )
        } else {
            format!("{} / {}", self.current_page + 1, self.total_pages)
        };
        ui.painter().text(
            egui::pos2(rect.right() - 20., rect.top() + 8.),
            egui::Align2::RIGHT_TOP,
            pages,
            egui::FontId::proportional(13.),
            Color32::GRAY,
        );
        consumed
    }
}

/// Follow the visible paper edge, including curvature and perspective. A grabbed
/// animation keeps its initial edge-to-pointer offset instead of jumping to the cursor.
fn drag_edge_x(g: CurlGeometry, forward: bool, dual: bool) -> f32 {
    let y = g.rect.top() + g.anchor_y;
    if !forward && !dual {
        // In a single-page backward turn the outer tip starts offscreen. Follow
        // the entering fold instead, so dragging immediately reveals the page.
        g.shadow_point(y).x
    } else {
        g.project(egui::pos2(g.rect.right(), y), false).0.x
    }
}
#[allow(clippy::too_many_arguments)]
fn drag_progress(
    sheet: Rect,
    forward: bool,
    dual: bool,
    anchor_y: f32,
    base_progress: f32,
    base_tilt: f32,
    tilt: f32,
    delta_x: f32,
) -> f32 {
    let geometry = |p, tilt| CurlGeometry {
        rect: sheet,
        progress: if forward { p } else { 1. - p },
        tilt,
        anchor_y,
    };
    let target = drag_edge_x(geometry(base_progress, base_tilt), forward, dual) + delta_x;
    let mut low = 0.;
    let mut high = 1.;
    for _ in 0..20 {
        let t = (low + high) / 2.;
        let x = drag_edge_x(geometry(t, tilt), forward, dual);
        if (x > target) == forward {
            low = t;
        } else {
            high = t;
        }
    }
    (low + high) / 2.
}
fn paint_spine(painter: &egui::Painter, rect: Rect) {
    painter.add(egui::Shape::mesh(spine_mesh(rect)));
}
fn spine_mesh(rect: Rect) -> Mesh {
    super::design::horizontal_gradient_mesh(
        Rect::from_min_max(
            egui::pos2(rect.center().x - 5., rect.top()),
            egui::pos2(rect.center().x + 5., rect.bottom()),
        ),
        Color32::TRANSPARENT,
        Color32::from_black_alpha(10),
        Color32::TRANSPARENT,
    )
}
fn paint_paper_edge(painter: &egui::Painter, g: CurlGeometry, back: bool, border: Color32) {
    for y in [g.rect.top(), g.rect.bottom()] {
        let left = g.face_distance(egui::pos2(g.rect.left(), y), false);
        let right = g.face_distance(egui::pos2(g.rect.right(), y), false);
        let split = egui::lerp(g.rect.x_range(), (-left / (right - left)).clamp(0., 1.));
        let range = if back {
            split..=g.rect.right()
        } else {
            g.rect.left()..=split
        };
        if range.end() - range.start() <= 0.001 {
            continue;
        }
        let points = (0..=64)
            .map(|i| {
                g.project(
                    egui::pos2(egui::lerp(range.clone(), i as f32 / 64.), y),
                    false,
                )
                .0
            })
            .collect();
        painter.add(egui::Shape::line(
            points,
            egui::Stroke::new(0.6_f32, border),
        ));
    }
}
fn paint_turn(painter: egui::Painter, rect: Rect, turn: &CurlTurn, paper: Color32) {
    let dual = turn.dual;
    let (front, back, base_left, base_right) = if dual {
        if turn.forward {
            (
                &turn.images[1],
                Some(&turn.images[2]),
                Some(&turn.images[0]),
                &turn.images[3],
            )
        } else {
            (
                &turn.images[3],
                Some(&turn.images[0]),
                Some(&turn.images[2]),
                &turn.images[1],
            )
        }
    } else if turn.forward {
        (&turn.images[0], None, None, &turn.images[1])
    } else {
        (&turn.images[1], None, None, &turn.images[0])
    };
    if let Some(left) = base_left {
        left.paint(&painter, paper);
    }
    base_right.paint(&painter, paper);
    let progress = if turn.forward {
        turn.progress
    } else {
        1. - turn.progress
    };
    let g = CurlGeometry {
        rect: front.rect,
        progress,
        tilt: turn.tilt,
        anchor_y: turn.anchor_y,
    };
    let strength = (std::f32::consts::PI * progress).sin().max(0.);
    // Soft cast shadow is painted underneath the turning paper, not over its text.
    let shadow_width = (front.rect.width() * 0.055).clamp(12., 36.) * strength;
    let mut shadow = Mesh::default();
    for row in 0..=32 {
        let y = egui::lerp(rect.y_range(), row as f32 / 32.);
        let center = g.shadow_point(y);
        for offset in [-1., 0., 1.] {
            shadow.vertices.push(egui::epaint::Vertex {
                pos: center + egui::vec2(offset * shadow_width, 0.),
                uv: egui::epaint::WHITE_UV,
                color: Color32::from_black_alpha(if offset == 0. {
                    (32. * strength) as u8
                } else {
                    0
                }),
            });
        }
        if row > 0 {
            let i = (row * 3) as u32;
            for column in 0..2 {
                let a = i - 3 + column;
                shadow
                    .indices
                    .extend([a, a + 1, a + 3, a + 1, a + 4, a + 3]);
            }
        }
    }
    painter.add(egui::Shape::mesh(shadow));
    let surface = page_curl::paper_mesh(front.rect, paper);
    painter.add(egui::Shape::mesh(page_curl::bend_mesh(
        &surface, front.rect, g, false, 1.,
    )));
    for (clip, mesh) in &front.meshes {
        painter.add(egui::Shape::mesh(page_curl::bend_mesh(
            mesh, *clip, g, false, 1.,
        )));
    }
    let border = Color32::from_black_alpha((32. * strength) as u8);
    // Front edge is behind the back face wherever the two projections overlap.
    paint_paper_edge(&painter, g, false, border);
    painter.add(egui::Shape::mesh(page_curl::bend_mesh(
        &surface,
        front.rect,
        g,
        true,
        1. - 0.03 * strength,
    )));
    if let Some(back) = back {
        let shift = egui::vec2(front.rect.left() - back.rect.left(), 0.);
        for (clip, mesh) in &back.meshes {
            let mut shifted = mesh.clone();
            for v in &mut shifted.vertices {
                v.pos += shift;
            }
            painter.add(egui::Shape::mesh(page_curl::bend_mesh(
                &shifted,
                clip.translate(shift),
                g,
                true,
                1. - 0.03 * strength,
            )));
        }
    }
    paint_paper_edge(&painter, g, true, border);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spine_matches_idle_at_endpoints_and_is_occluded_by_lifted_paper() {
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200., 800.));
        let paper = Color32::from_rgb(250, 246, 238);
        let render = |motion: Option<(bool, f32)>, gutter: bool| {
            let ctx = egui::Context::default();
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(rect),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            if let Some((forward, progress)) = motion {
                                let left = Rect::from_min_max(rect.min, egui::pos2(600., 800.));
                                let right = Rect::from_min_max(egui::pos2(600., 0.), rect.max);
                                let turn = CurlTurn {
                                    source: PagePosition {
                                        chapter: 0,
                                        page: 0,
                                    },
                                    target: PagePosition {
                                        chapter: 0,
                                        page: 2,
                                    },
                                    target_ranges: Default::default(),
                                    source_ranges: Default::default(),
                                    image_revision: 0,
                                    forward,
                                    dual: true,
                                    images: [left, right, left, right]
                                        .into_iter()
                                        .map(|rect| PageImage {
                                            rect,
                                            shapes: vec![],
                                            meshes: if gutter {
                                                vec![(
                                                    rect,
                                                    spine_mesh(Rect::from_min_size(
                                                        egui::Pos2::ZERO,
                                                        egui::vec2(1200., 800.),
                                                    )),
                                                )]
                                            } else {
                                                vec![]
                                            },
                                            atlas_size: [0, 0],
                                            _textures: vec![],
                                        })
                                        .collect(),
                                    signature: 0,
                                    progress,
                                    tilt: 0.3,
                                    anchor_y: 500.,
                                    motion: Motion::settle(progress, true, 0., 0.14),
                                };
                                paint_turn(ui.painter().clone(), rect, &turn, paper);
                            } else {
                                ui.painter().rect_filled(rect, 0., paper);
                                if gutter {
                                    paint_spine(ui.painter(), rect);
                                }
                            }
                        });
                },
            );
            ctx.tessellate(output.shapes, 1.)
        };
        // Composite actual egui triangle colors at several gutter pixels. This
        // catches paper/backside shading or ordering changes at the endpoints.
        let sample = |shapes: &[egui::ClippedPrimitive], point: egui::Pos2| {
            let mut pixel = [0_f32; 4];
            for shape in shapes.iter().filter(|s| s.clip_rect.contains(point)) {
                let Primitive::Mesh(mesh) = &shape.primitive else {
                    continue;
                };
                for tri in mesh.indices.as_chunks::<3>().0 {
                    let [a, b, c] = [
                        mesh.vertices[tri[0] as usize],
                        mesh.vertices[tri[1] as usize],
                        mesh.vertices[tri[2] as usize],
                    ];
                    let cross = |a: egui::Vec2, b: egui::Vec2| a.x * b.y - a.y * b.x;
                    let area = cross(b.pos - a.pos, c.pos - a.pos);
                    if area.abs() < 0.0001 {
                        continue;
                    }
                    let wb = cross(point - a.pos, c.pos - a.pos) / area;
                    let wc = cross(b.pos - a.pos, point - a.pos) / area;
                    let wa = 1. - wb - wc;
                    if wa < 0. || wb < 0. || wc < 0. {
                        continue;
                    }
                    let colors = [a.color.to_array(), b.color.to_array(), c.color.to_array()];
                    let color: [f32; 4] = std::array::from_fn(|i| {
                        (colors[0][i] as f32 * wa
                            + colors[1][i] as f32 * wb
                            + colors[2][i] as f32 * wc)
                            / 255.
                    });
                    for i in 0..4 {
                        pixel[i] = color[i] + pixel[i] * (1. - color[3]);
                    }
                }
            }
            pixel
        };
        let idle = render(None, true);
        for forward in [true, false] {
            for progress in [0., 1.] {
                let endpoint = render(Some((forward, progress)), true);
                for x in [590.3, 597.4, 602.6, 610.3] {
                    let point = egui::pos2(x, 400.7);
                    let before = sample(&idle, point);
                    let after = sample(&endpoint, point);
                    for i in 0..4 {
                        assert!((before[i] - after[i]).abs() < 0.001,
                        "gutter pixel changed: forward={forward} progress={progress} x={x}: {before:?} -> {after:?}");
                    }
                }
            }
        }
        for (forward, progress) in [(true, 0.8), (false, 0.2)] {
            let with_gutter = render(Some((forward, progress)), true);
            let without_gutter = render(Some((forward, progress)), false);
            for x in [597.4, 602.6] {
                for y in [300.7, 500.7] {
                    let point = egui::pos2(x, y);
                    let occluded = sample(&with_gutter, point);
                    // The flat back face has uniform tone here. Compare with a
                    // pixel away from the stationary gutter as well, so an
                    // erroneous final overlay cannot contaminate both renders
                    // and make this regression pass by coincidence.
                    let plain_paper = sample(&without_gutter, egui::pos2(632.3, y));
                    for i in 0..4 {
                        assert!((occluded[i] - plain_paper[i]).abs() < 0.001,
                            "stationary gutter leaked through lifted sheet: forward={forward} at {point:?}");
                    }
                }
            }
        }
    }
    #[test]
    fn dragging_tracks_projected_edge_in_pixels_in_both_layouts_and_directions() {
        for dual in [false, true] {
            let rect = Rect::from_min_size(
                egui::pos2(40., 20.),
                egui::vec2(if dual { 600. } else { 900. }, 800.),
            );
            for forward in [false, true] {
                for (base, base_tilt, tilt) in [(0., 0., 0.), (0.25, 0.12, -0.18), (0.7, -0.2, 0.3)]
                {
                    let geometry = |p, tilt| CurlGeometry {
                        rect,
                        progress: if forward { p } else { 1. - p },
                        tilt,
                        anchor_y: 330.,
                    };
                    let start = drag_edge_x(geometry(base, base_tilt), forward, dual);
                    for target_progress in [0., 0.05, 0.2, 0.5, 0.8, 1.] {
                        let expected_x =
                            drag_edge_x(geometry(target_progress, tilt), forward, dual);
                        let solved = drag_progress(
                            rect,
                            forward,
                            dual,
                            330.,
                            base,
                            base_tilt,
                            tilt,
                            expected_x - start,
                        );
                        let actual_x = drag_edge_x(geometry(solved, tilt), forward, dual);
                        assert!((actual_x-expected_x).abs()<0.01, "dual={dual} forward={forward} p={target_progress}: {actual_x} vs {expected_x}");
                    }
                    let unchanged =
                        drag_progress(rect, forward, dual, 330., base, base_tilt, base_tilt, 0.);
                    assert!((unchanged - base).abs() < 0.00001);
                }
            }
        }
    }
    #[test]
    fn keeps_the_latest_queued_request() {
        let mut state = PageCurlState::default();
        state.request(true, None);
        state.request(false, None);
        assert_eq!(state.pending, Some((false, None)));
        state.clear();
        assert!(!state.busy());
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use eframe::egui::Pos2;
    use reader_core::epub::{Chapter, ContentBlock, EpubBook};
    fn setup() -> (tempfile::TempDir, ReaderApp, egui::Context) {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let dir = tempfile::tempdir().unwrap();
        let mut app = ReaderApp::from_data_dir(dir.path().to_string_lossy().into_owned());
        app.reader_page_animation = "Realistic".into();
        app.view = crate::app::AppView::Reader;
        app.book = Some(EpubBook {
            chapter_store: None,
            title: "Turn regression".into(),
            chapters: (0..3)
                .map(|i| Chapter {
                    native_document: Default::default(),
                    title: format!("Chapter {i}"),
                    blocks: (0..11 + i)
                        .map(|n| ContentBlock::Paragraph {
                            native_style: Default::default(),
                            spans: vec![reader_core::epub::TextSpan {
                                text: format!(
                                    "{i}.{n} {}",
                                    "A readable paragraph verifies the source and destination. "
                                        .repeat(8)
                                ),
                                style: reader_core::epub::InlineStyle::Normal,
                                link_url: None,
                                correction: None,
                            }],
                            anchor_id: None,
                        })
                        .collect(),
                    source_href: None,
                })
                .collect(),
            toc: vec![],
            cover_data: None,
            fonts: vec![],
            supplemental_images: Default::default(),
            chapter_reviews: Default::default(),
            review_chapter_indices: Default::default(),
        });
        let ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        let chain = fonts.families[&egui::FontFamily::Proportional].clone();
        fonts
            .families
            .insert(egui::FontFamily::Name("ReaderFont".into()), chain.clone());
        fonts
            .families
            .insert(egui::FontFamily::Name("ReaderCjk".into()), chain);
        ctx.set_fonts(fonts);
        (dir, app, ctx)
    }
    fn frame(
        app: &mut ReaderApp,
        ctx: &egui::Context,
        t: f64,
        size: egui::Vec2,
        events: Vec<egui::Event>,
        focused: bool,
    ) {
        let _ = ctx.run(
            egui::RawInput {
                time: Some(t),
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                events,
                focused,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.render_reader(ui));
            },
        );
    }
    fn pos(app: &ReaderApp) -> (usize, usize, usize) {
        (app.current_chapter, app.current_page, app.current_block)
    }
    fn pointer_button(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }
    #[test]
    fn center_click_toggles_tools_once_without_turning_or_selecting() {
        let (_dir, mut app, ctx) = setup();
        let size = egui::vec2(800., 800.);
        frame(&mut app, &ctx, 0., size, vec![], true);
        let source = pos(&app);
        let point = app.paging_page_rect.unwrap().center();
        for (time, visible) in [(1., true), (2., false)] {
            frame(
                &mut app,
                &ctx,
                time,
                size,
                vec![
                    egui::Event::PointerMoved(point),
                    pointer_button(point, true),
                    pointer_button(point, false),
                ],
                true,
            );
            assert_eq!(app.reader_chrome.visible, visible);
            assert_eq!(pos(&app), source);
            assert!(!app.page_curl.busy());
            assert!(app.text_selection.is_none());
        }
    }
    #[test]
    fn broad_grip_tracks_holds_reverses_and_cancels_in_both_layouts() {
        for size in [egui::vec2(800., 800.), egui::vec2(1600., 800.)] {
            let (_dir, mut app, ctx) = setup();
            frame(&mut app, &ctx, 0., size, vec![], true);
            let rect = app.paging_page_rect.unwrap();
            let origin = egui::pos2(rect.right() - 100., rect.center().y);
            let partial = origin - egui::vec2(rect.width() * 0.3, 0.);
            let source = pos(&app);
            frame(
                &mut app,
                &ctx,
                1.,
                size,
                vec![
                    egui::Event::PointerMoved(origin),
                    pointer_button(origin, true),
                ],
                true,
            );
            assert!(matches!(
                app.page_curl.active.as_ref().unwrap().motion,
                Motion::Drag { .. }
            ));
            frame(
                &mut app,
                &ctx,
                1.1,
                size,
                vec![egui::Event::PointerMoved(partial)],
                true,
            );
            let turn = app.page_curl.active.as_ref().unwrap();
            let sheet = if turn.dual { turn.images[1].rect } else { rect };
            let edge = |p| {
                drag_edge_x(
                    CurlGeometry {
                        rect: sheet,
                        progress: p,
                        tilt: 0.,
                        anchor_y: origin.y - rect.top(),
                    },
                    true,
                    turn.dual,
                )
            };
            assert!((edge(turn.progress) - edge(0.) - (partial.x - origin.x)).abs() < 0.01);
            let progress = app.page_curl.active.as_ref().unwrap().progress;
            assert!(progress > 0.2 && progress < 0.7);
            frame(&mut app, &ctx, 4., size, vec![], true);
            assert_eq!(app.page_curl.active.as_ref().unwrap().progress, progress);
            assert_eq!(pos(&app), source);
            frame(
                &mut app,
                &ctx,
                4.1,
                size,
                vec![egui::Event::PointerMoved(origin)],
                true,
            );
            assert!(app.page_curl.active.as_ref().unwrap().progress < 0.001);
            frame(
                &mut app,
                &ctx,
                4.2,
                size,
                vec![pointer_button(origin, false)],
                true,
            );
            frame(&mut app, &ctx, 5., size, vec![], true);
            assert_eq!(pos(&app), source);
            assert!(!app.page_curl.busy());
        }
    }
    #[test]
    fn settling_sheet_can_be_grabbed_without_a_jump_and_dragged_back() {
        for size in [egui::vec2(800., 800.), egui::vec2(1600., 800.)] {
            let (_dir, mut app, ctx) = setup();
            frame(&mut app, &ctx, 0., size, vec![], true);
            let source = pos(&app);
            app.turn_reader_page(true);
            frame(&mut app, &ctx, 1., size, vec![], true);
            let expected = app.page_curl.active.as_ref().unwrap().motion.sample(1.15).0;
            let rect = app.paging_page_rect.unwrap();
            let grip = rect.center();
            frame(
                &mut app,
                &ctx,
                1.15,
                size,
                vec![egui::Event::PointerMoved(grip), pointer_button(grip, true)],
                true,
            );
            let turn = app.page_curl.active.as_ref().unwrap();
            assert!(matches!(turn.motion, Motion::Drag { .. }));
            assert!((turn.progress - expected).abs() < 0.001);
            frame(&mut app, &ctx, 4., size, vec![], true);
            assert!((app.page_curl.active.as_ref().unwrap().progress - expected).abs() < 0.001);
            assert_eq!(pos(&app), source);
            let back = grip + egui::vec2(rect.width() * 0.4, 0.);
            frame(
                &mut app,
                &ctx,
                4.1,
                size,
                vec![egui::Event::PointerMoved(back)],
                true,
            );
            frame(
                &mut app,
                &ctx,
                4.2,
                size,
                vec![pointer_button(back, false)],
                true,
            );
            frame(&mut app, &ctx, 5., size, vec![], true);
            assert_eq!(pos(&app), source);
            assert!(!app.page_curl.busy());
        }
    }
    #[test]
    fn dragging_to_the_other_edge_commits_while_held_including_chapter_crossing() {
        for size in [egui::vec2(800., 800.), egui::vec2(1600., 800.)] {
            let (_dir, mut app, ctx) = setup();
            frame(&mut app, &ctx, 0., size, vec![], true);
            let step = if app.is_dual_column { 2 } else { 1 };
            app.current_page = (app.total_pages - 1) / step * step;
            frame(&mut app, &ctx, 0.5, size, vec![], true);
            let rect = app.paging_page_rect.unwrap();
            let origin = egui::pos2(rect.right() - 5., rect.center().y);
            let end = egui::pos2(rect.left() + 2., rect.center().y);
            frame(
                &mut app,
                &ctx,
                1.,
                size,
                vec![
                    egui::Event::PointerMoved(origin),
                    pointer_button(origin, true),
                ],
                true,
            );
            frame(
                &mut app,
                &ctx,
                1.1,
                size,
                vec![egui::Event::PointerMoved(end)],
                true,
            );
            assert_eq!((app.current_chapter, app.current_page), (1, 0));
            assert!(app.page_curl.held_after_commit);
            frame(&mut app, &ctx, 2., size, vec![], true);
            assert_eq!((app.current_chapter, app.current_page), (1, 0));
            frame(
                &mut app,
                &ctx,
                2.1,
                size,
                vec![pointer_button(end, false)],
                true,
            );
            assert!(app.text_selection.is_none());
            assert_eq!((app.current_chapter, app.current_page), (1, 0));
        }
    }
    #[test]
    fn both_layouts_cross_chapters_in_both_directions_after_settling() {
        for size in [egui::vec2(800., 800.), egui::vec2(1600., 800.)] {
            let (_dir, mut app, ctx) = setup();
            frame(&mut app, &ctx, 0., size, vec![], true);
            let step = if app.is_dual_column { 2 } else { 1 };
            app.current_page = (app.total_pages - 1) / step * step;
            app.current_block = app.page_block_ranges[app.current_page].0;
            let source = pos(&app);
            app.turn_reader_page(true);
            frame(&mut app, &ctx, 1., size, vec![], true);
            assert_eq!(pos(&app), source);
            frame(&mut app, &ctx, 1.1, size, vec![], true);
            assert_eq!(pos(&app), source);
            frame(&mut app, &ctx, 2., size, vec![], true);
            assert_eq!((app.current_chapter, app.current_page), (1, 0));
            app.turn_reader_page(false);
            frame(&mut app, &ctx, 3., size, vec![], true);
            assert_eq!((app.current_chapter, app.current_page), (1, 0));
            frame(&mut app, &ctx, 4., size, vec![], true);
            assert_eq!(pos(&app), source);
            assert!(!app.page_curl.busy());
        }
    }
    #[test]
    fn short_drag_and_focus_loss_do_not_publish_destination() {
        let (_dir, mut app, ctx) = setup();
        let size = egui::vec2(1600., 800.);
        frame(&mut app, &ctx, 0., size, vec![], true);
        let r = app.paging_page_rect.unwrap();
        let origin = egui::pos2(r.right() - 5., r.center().y);
        let moved = origin - egui::vec2(22., 0.);
        let source = pos(&app);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &mut app,
            &ctx,
            1.,
            size,
            vec![egui::Event::PointerMoved(origin), button(origin, true)],
            true,
        );
        assert!(app.page_curl.busy());
        frame(
            &mut app,
            &ctx,
            1.2,
            size,
            vec![egui::Event::PointerMoved(moved)],
            true,
        );
        frame(&mut app, &ctx, 1.4, size, vec![button(moved, false)], true);
        assert_eq!(pos(&app), source);
        frame(&mut app, &ctx, 2., size, vec![], true);
        assert_eq!(pos(&app), source);
        assert!(!app.page_curl.busy());
        app.turn_reader_page(true);
        frame(&mut app, &ctx, 3., size, vec![], true);
        assert!(app.page_curl.busy());
        frame(&mut app, &ctx, 3.1, size, vec![], false);
        assert_eq!(pos(&app), source);
        assert!(!app.page_curl.busy());
    }
    #[test]
    fn resize_cancels_preview_and_book_boundaries_remain_still() {
        let (_dir, mut app, ctx) = setup();
        let size = egui::vec2(800., 800.);
        frame(&mut app, &ctx, 0., size, vec![], true);
        let source = pos(&app);
        app.turn_reader_page(false);
        frame(&mut app, &ctx, 1., size, vec![], true);
        assert_eq!(pos(&app), source);
        assert!(!app.page_curl.busy());
        app.turn_reader_page(true);
        frame(&mut app, &ctx, 2., size, vec![], true);
        assert!(app.page_curl.busy());
        frame(&mut app, &ctx, 2.1, egui::vec2(1600., 800.), vec![], true);
        assert!(!app.page_curl.busy());
        assert_eq!(app.current_chapter, 0);
        app.current_chapter = 2;
        app.pages_dirty = true;
        frame(&mut app, &ctx, 3., size, vec![], true);
        app.current_page = app.total_pages - 1;
        app.current_block = app.page_block_ranges[app.current_page].0;
        let source = pos(&app);
        app.turn_reader_page(true);
        frame(&mut app, &ctx, 4., size, vec![], true);
        assert_eq!(pos(&app), source);
        assert!(!app.page_curl.busy());
    }
    #[test]
    fn sliding_panels_preserve_the_source_block_through_reflow() {
        let (_dir, mut app, ctx) = setup();
        let size = egui::vec2(1400., 800.);
        frame(&mut app, &ctx, 0., size, vec![], true);
        app.current_page = 2.min(app.total_pages - 1);
        app.current_block = app.page_block_ranges[app.current_page].0;
        let source_block = app.current_block;
        app.turn_reader_page(true);
        frame(&mut app, &ctx, 1., size, vec![], true);
        assert!(app.page_curl.busy());
        for (i, expanded) in [false, true, true, true, false, false, false, true, true]
            .into_iter()
            .enumerate()
        {
            let _ = ctx.run(
                egui::RawInput {
                    time: Some(1.1 + i as f64 * 0.1),
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ctx| {
                    ctx.data_mut(|d| d.insert_temp(egui::Id::new("panels_animating"), false));
                    super::super::design::animated_side_panel(
                        ctx,
                        "regression_toc",
                        true,
                        280.,
                        expanded,
                        "Collapse",
                        |ui| {
                            ui.label("Contents");
                        },
                    );
                    let moving = super::super::design::panels_animating(ctx);
                    if moving && app.panel_reflow_anchor.is_none() {
                        app.panel_reflow_anchor = Some((app.current_chapter, app.current_block));
                        app.page_curl.clear();
                    }
                    egui::CentralPanel::default().show(ctx, |ui| app.render_reader(ui));
                    if !moving {
                        app.panel_reflow_anchor = None;
                    }
                },
            );
            assert_eq!(app.current_chapter, 0);
            assert_eq!(app.current_block, source_block);
            assert!(app.visible_page_contains_block(source_block));
        }
        assert!(!app.page_curl.busy());
    }
    #[test]
    fn coalesced_mouse_events_keep_the_press_direction_and_allow_cancellation() {
        for size in [egui::vec2(800., 800.), egui::vec2(1600., 800.)] {
            let (_dir, mut app, ctx) = setup();
            frame(&mut app, &ctx, 0., size, vec![], true);
            let source = pos(&app);
            let r = app.paging_page_rect.unwrap();
            let press = egui::pos2(r.right() - 5., r.center().y);
            let release = egui::pos2(r.left() + 15., r.center().y - 50.);
            let button = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            let events = |from, to| {
                vec![
                    egui::Event::PointerMoved(from),
                    button(from, true),
                    egui::Event::PointerMoved(to),
                    button(to, false),
                ]
            };
            frame(&mut app, &ctx, 1., size, events(press, release), true);
            assert_eq!(pos(&app), source);
            frame(&mut app, &ctx, 2., size, vec![], true);
            assert_eq!(
                (app.current_chapter, app.current_page),
                (0, if app.is_dual_column { 2 } else { 1 })
            );
            frame(&mut app, &ctx, 3., size, events(release, press), true);
            frame(&mut app, &ctx, 4., size, vec![], true);
            assert_eq!(pos(&app), source);
            frame(
                &mut app,
                &ctx,
                5.,
                size,
                events(press, press - egui::vec2(22., 0.)),
                true,
            );
            frame(&mut app, &ctx, 6., size, vec![], true);
            assert_eq!(pos(&app), source);
        }
    }
    #[test]
    fn clicking_a_menu_over_the_paper_edge_does_not_turn_the_page() {
        let (_dir, mut app, ctx) = setup();
        let size = egui::vec2(800., 800.);
        frame(&mut app, &ctx, 0., size, vec![], true);
        let source = pos(&app);
        let r = app.paging_page_rect.unwrap();
        let pointer = egui::pos2(r.right() - 5., 120.);
        for step in 0..3 {
            let events = if step == 1 {
                vec![
                    egui::Event::PointerMoved(pointer),
                    egui::Event::PointerButton {
                        pos: pointer,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                    egui::Event::PointerButton {
                        pos: pointer,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            } else {
                vec![]
            };
            let _ = ctx.run(
                egui::RawInput {
                    time: Some(1. + step as f64),
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::Area::new(egui::Id::new("menu_overlay_regression"))
                        .order(egui::Order::Foreground)
                        .fixed_pos(egui::pos2(r.right() - 80., 100.))
                        .show(ctx, |ui| {
                            ui.set_min_size(egui::vec2(80., 160.));
                            let _ = ui.button("Menu action");
                        });
                    egui::CentralPanel::default().show(ctx, |ui| app.render_reader(ui));
                },
            );
            assert_eq!(pos(&app), source);
            assert!(
                !app.page_curl.busy(),
                "step={step}, layer={:?}",
                ctx.layer_id_at(pointer)
            );
        }
    }
}
