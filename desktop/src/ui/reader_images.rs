//! EPUB illustrations share sizing between pagination and painting.
use super::reader_state::{BlockKey, FRAME_MARGIN, TITLE_SPACING};
use eframe::egui;
use reader_core::epub::{parser::SupplementalImage, ContentBlock};
use std::{
    cell::RefCell,
    collections::HashMap,
    io::Cursor,
    sync::{mpsc, Arc, Mutex, OnceLock, Weak},
};

thread_local! {
    static MEDIA: RefCell<HashMap<BlockKey, Vec<SupplementalImage>>> = RefCell::new(HashMap::new());
    static VIEWPORT: std::cell::Cell<f32> = const { std::cell::Cell::new(f32::INFINITY) };
    static SCROLL: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
    static HEIGHT: std::cell::Cell<f32> = const { std::cell::Cell::new(f32::INFINITY) };
}

pub(crate) fn configure(
    book: Option<&reader_core::epub::EpubBook>,
    height: f32,
    scroll: bool,
    font: f32,
    title_scale: f32,
) {
    VIEWPORT.set(height);
    SCROLL.set(scroll);
    HEIGHT.set(if scroll {
        f32::INFINITY
    } else {
        page_image_limit(height, font, title_scale)
    });
    MEDIA.with(|media| {
        let mut media = media.borrow_mut();
        media.clear();
        if let Some(book) = book {
            for (&chapter, images) in &book.supplemental_images {
                for image in images {
                    media
                        .entry(BlockKey::new(chapter, image.block))
                        .or_default()
                        .push(image.clone());
                }
            }
        }
    });
}

pub(crate) fn page_image_limit(height: f32, font: f32, title_scale: f32) -> f32 {
    (height
        - FRAME_MARGIN
        - font * title_scale * 1.3 * super::reader_state::line_spacing().max(1.8)
        - font * 1.2
        - TITLE_SPACING
        - 16.)
        .max(32.)
}

pub(crate) fn page_limit_for(
    blocks: &[ContentBlock],
    height: f32,
    width: f32,
    font: f32,
    title_scale: f32,
) -> f32 {
    let heading = blocks
        .first()
        .filter(|b| matches!(b, ContentBlock::Heading { .. }))
        .map_or(font * title_scale * 1.8 + TITLE_SPACING, |block| {
            super::reader_block::estimate_block_height(
                block,
                font,
                title_scale,
                font * super::reader_state::line_spacing(),
                width,
            )
        });
    (height - FRAME_MARGIN - heading - 16.).max(32.)
}
pub(crate) fn reading_column(blocks: &[ContentBlock], width: f32, font: f32, title_scale: f32) {
    HEIGHT.set(if SCROLL.get() {
        f32::INFINITY
    } else {
        page_limit_for(blocks, VIEWPORT.get(), width, font, title_scale)
    });
}
pub(crate) fn media_for(key: BlockKey) -> Vec<SupplementalImage> {
    MEDIA.with(|media| media.borrow().get(&key).cloned().unwrap_or_default())
}

fn uncached_dimensions(data: &[u8]) -> Option<egui::Vec2> {
    let (w, h) = image::ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()?;
    (w > 0 && h > 0).then(|| egui::vec2(w as f32, h as f32))
}

pub(crate) fn dimensions(data: &[u8]) -> Option<egui::Vec2> {
    // Borrowed slices cannot provide a lifetime-safe identity; dimensions are cached
    // by the Arc-owning paint path below. Measurement uses cheap header reads only.
    uncached_dimensions(data)
}

static IMAGE_REVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub(crate) fn revision() -> u64 {
    IMAGE_REVISION.load(std::sync::atomic::Ordering::Relaxed)
}
type DecodeResult = Arc<Mutex<Option<Option<egui::ColorImage>>>>;
struct DecodeJob {
    data: Arc<Vec<u8>>,
    max_side: u32,
    result: DecodeResult,
    ctx: egui::Context,
}
fn decode_queue() -> &'static mpsc::SyncSender<DecodeJob> {
    static QUEUE: OnceLock<mpsc::SyncSender<DecodeJob>> = OnceLock::new();
    QUEUE.get_or_init(|| {
        let (sender, receiver) = mpsc::sync_channel::<DecodeJob>(8);
        let receiver = Arc::new(Mutex::new(receiver));
        for index in 0..2 {
            let receiver = receiver.clone();
            std::thread::Builder::new()
                .name(format!("epub-image-{index}"))
                .spawn(move || loop {
                    let Ok(job) = receiver.lock().unwrap().recv() else {
                        break;
                    };
                    let decoded = uncached_dimensions(&job.data)
                        .filter(|s| s.x * s.y <= 16_777_216.)
                        .and_then(|_| image::load_from_memory(&job.data).ok())
                        .map(|image| {
                            let rgba = image.thumbnail(job.max_side, job.max_side).to_rgba8();
                            egui::ColorImage::from_rgba_unmultiplied(
                                [rgba.width() as usize, rgba.height() as usize],
                                rgba.as_raw(),
                            )
                        });
                    *job.result.lock().unwrap() = Some(decoded);
                    IMAGE_REVISION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    job.ctx.request_repaint();
                })
                .expect("image decoder worker");
        }
        sender
    })
}

#[cfg(test)]
pub(crate) fn fitted_size(data: &[u8], width: f32, max_height: f32) -> Option<egui::Vec2> {
    let native = dimensions(data)?;
    let factor = (width.max(1.) / native.x)
        .min(max_height.max(1.) / native.y)
        .min(1.);
    Some(native * factor)
}

pub(crate) fn styled_size(
    data: &[u8],
    width: f32,
    max_height: f32,
    font: f32,
    style: &reader_core::epub::native::NativeStyle,
) -> Option<egui::Vec2> {
    let natural = dimensions(data)?;
    let (w, h) = style.image_size(natural.x, natural.y, font, width, max_height);
    Some(egui::vec2(w, h))
}
pub(crate) fn styled_height(
    data: &[u8],
    width: f32,
    max_height: f32,
    font: f32,
    style: &reader_core::epub::native::NativeStyle,
) -> f32 {
    styled_size(data, width, max_height, font, style).map_or(font * 1.8, |s| s.y) + font * 0.7
}
#[cfg(test)]
pub(crate) fn height(data: &[u8], width: f32, max_height: f32, font: f32) -> f32 {
    fitted_size(data, width, max_height).map_or(font * 1.8, |s| s.y) + font * 0.7
}

pub(crate) fn current_limit() -> f32 {
    HEIGHT.get()
}

pub(crate) fn media_limit(text_height: f32, count: usize, font: f32, limit: f32) -> f32 {
    ((limit - text_height - count as f32 * font * 0.7) / count.max(1) as f32).max(24.)
}

#[derive(Clone)]
struct CachedImage {
    source: Weak<Vec<u8>>,
    texture: Option<egui::TextureHandle>,
    pending: Option<DecodeResult>,
    natural: Option<egui::Vec2>,
    failed: bool,
    decode_side: u32,
    used: u64,
    pass: u64,
    visible_pass: Option<u64>,
}
#[derive(Clone, Default)]
struct ImageCache {
    entries: HashMap<usize, CachedImage>,
    clock: u64,
}

#[cfg(test)]
pub(crate) fn paint(
    ui: &mut egui::Ui,
    data: &Arc<Vec<u8>>,
    alt: Option<&str>,
    width: f32,
    max_height: f32,
    font: f32,
    i18n: &reader_core::i18n::I18n,
) {
    styled_paint(
        ui,
        data,
        alt,
        width,
        max_height,
        font,
        i18n,
        &Default::default(),
    );
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn styled_paint(
    ui: &mut egui::Ui,
    data: &Arc<Vec<u8>>,
    alt: Option<&str>,
    width: f32,
    max_height: f32,
    font: f32,
    i18n: &reader_core::i18n::I18n,
    style: &reader_core::epub::native::NativeStyle,
) {
    let key = Arc::as_ptr(data) as usize;
    let pass = ui.ctx().cumulative_pass_nr();
    let cache_id = egui::Id::new("epub_illustrations");
    // Context data locks must be released before load_texture reads Context input.
    let mut cache = ui
        .ctx()
        .data(|store| store.get_temp::<ImageCache>(cache_id))
        .unwrap_or_default();
    let texture = {
        cache.clock += 1;
        cache
            .entries
            .retain(|_, entry| entry.source.strong_count() > 0);
        if !cache
            .entries
            .get(&key)
            .is_some_and(|entry| entry.source.ptr_eq(&Arc::downgrade(data)))
        {
            // Only header inspection occurs here; full decoding/scaling runs on
            // two bounded workers, never in the rendering/gesture thread.
            let natural = uncached_dimensions(data);
            cache.entries.insert(
                key,
                CachedImage {
                    source: Arc::downgrade(data),
                    texture: None,
                    pending: None,
                    natural,
                    failed: natural.is_none(),
                    decode_side: 0,
                    used: cache.clock,
                    pass,
                    visible_pass: None,
                },
            );
        }
        let entry = cache.entries.get_mut(&key).unwrap();
        let desired_side = entry
            .natural
            .map_or(256, |n| {
                let (w, h) = style.image_size(n.x, n.y, font, width, max_height);
                ((w.max(h) * ui.ctx().pixels_per_point()).ceil() as u32).div_ceil(256) * 256
            })
            .clamp(
                256,
                ui.ctx().input(|input| input.max_texture_side).min(2048) as u32,
            );
        let near_viewport = entry.natural.is_some_and(|n| {
            let (_, h) = style.image_size(n.x, n.y, font, width, max_height);
            egui::Rect::from_min_size(ui.cursor().min, egui::vec2(width, h)).intersects(
                ui.clip_rect()
                    .expand2(egui::vec2(0., ui.clip_rect().height())),
            )
        });
        if ui.is_visible()
            && entry.natural.is_some_and(|n| {
                let (_, h) = style.image_size(n.x, n.y, font, width, max_height);
                egui::Rect::from_min_size(ui.cursor().min, egui::vec2(width, h))
                    .intersects(ui.clip_rect())
            })
        {
            entry.visible_pass = Some(pass);
        }
        if ui.is_visible()
            && near_viewport
            && !entry.failed
            && entry.pending.is_none()
            && desired_side > entry.decode_side
        {
            let result = Arc::new(Mutex::new(None));
            if decode_queue()
                .try_send(DecodeJob {
                    data: data.clone(),
                    max_side: desired_side,
                    result: result.clone(),
                    ctx: ui.ctx().clone(),
                })
                .is_ok()
            {
                entry.pending = Some(result);
                entry.decode_side = desired_side;
            } else {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(16));
            }
        }
        let ready = entry
            .pending
            .as_ref()
            .and_then(|pending| pending.lock().unwrap().take());
        if let Some(decoded) = ready {
            entry.pending = None;
            entry.failed = decoded.is_none();
            let texture = decoded.map(|image| {
                ui.ctx().load_texture(
                    format!("epub_image_{key}"),
                    image,
                    egui::TextureOptions::LINEAR,
                )
            });
            let incoming_bytes = texture
                .as_ref()
                .map_or(0, |texture| texture.size()[0] * texture.size()[1] * 4);
            while cache.entries.len() >= 24
                || cache
                    .entries
                    .values()
                    .filter_map(|e| e.texture.as_ref())
                    .map(|t| t.size()[0] * t.size()[1] * 4)
                    .sum::<usize>()
                    + incoming_bytes
                    > 96 * 1024 * 1024
            {
                if let Some(oldest) = cache
                    .entries
                    .iter()
                    .filter(|(_, entry)| entry.pass.saturating_add(1) < pass)
                    .min_by_key(|(_, e)| e.used)
                    .map(|(&id, _)| id)
                {
                    cache.entries.remove(&oldest);
                } else {
                    break;
                }
            }
            cache.entries.get_mut(&key).unwrap().texture = texture;
        }
        let entry = cache.entries.get_mut(&key).unwrap();
        entry.used = cache.clock;
        entry.pass = pass;
        entry.texture.clone()
    };
    ui.ctx()
        .data_mut(|store| store.insert_temp(cache_id, cache));
    let natural = ui
        .ctx()
        .data(|store| store.get_temp::<ImageCache>(cache_id))
        .and_then(|cache| cache.entries.get(&key).and_then(|e| e.natural));
    let size = natural.map(|n| {
        let (w, h) = style.image_size(n.x, n.y, font, width, max_height);
        egui::vec2(w, h)
    });
    ui.add_space(font * 0.35);
    if let Some(size) = size {
        // Reserve final dimensions while loading: no pagination/position jump.
        let (area, _) = ui.allocate_exact_size(egui::vec2(width, size.y), egui::Sense::hover());
        let rect = egui::Rect::from_center_size(area.center(), size);
        super::reader_pagination::record_band(rect);
        if let Some(texture) = texture {
            ui.painter().image(
                texture.id(),
                rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                egui::Color32::WHITE,
            );
        } else if ui.is_visible() {
            if ui
                .ctx()
                .data(|store| store.get_temp::<ImageCache>(cache_id))
                .is_some_and(|cache| cache.entries[&key].failed)
            {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    alt.unwrap_or(i18n.t("reader.image_load_failed")),
                    egui::FontId::proportional(font),
                    egui::Color32::GRAY,
                );
            }
            ui.painter().rect_filled(
                rect,
                0.,
                egui::Color32::from_gray(128).linear_multiply(0.08),
            );
        }
        if let Some(alt) = alt {
            ui.interact(rect, ui.id().with(key), egui::Sense::hover())
                .on_hover_text(alt);
        }
    } else {
        ui.label(
            egui::RichText::new(
                alt.filter(|s| !s.trim().is_empty())
                    .unwrap_or(i18n.t("reader.image_load_failed")),
            )
            .italics()
            .color(egui::Color32::GRAY),
        );
    }
    ui.add_space(font * 0.35);
}

/// Only images painted in this viewport block the initial reveal. Broken assets
/// have a terminal fallback; neighbouring prefetch work must never hold the book.
pub(crate) fn visible_images_ready(ctx: &egui::Context) -> bool {
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|store| store.get_temp::<ImageCache>(egui::Id::new("epub_illustrations")))
        .is_none_or(|cache| {
            cache.entries.values().all(|image| {
                image.visible_pass != Some(pass) || image.failed || image.texture.is_some()
            })
        })
}

/// Warm upcoming leaves without painting or blocking the gesture thread.
pub(crate) fn prefetch(
    ctx: &egui::Context,
    book: &reader_core::epub::EpubBook,
    chapter: usize,
    ranges: &[(usize, usize)],
    page: usize,
    width: f32,
    font: f32,
) {
    let Some(ch) = book.chapters.get(chapter) else {
        return;
    };
    let id = egui::Id::new("epub_illustrations");
    let mut cache = ctx
        .data(|store| store.get_temp::<ImageCache>(id))
        .unwrap_or_default();
    cache
        .entries
        .retain(|_, entry| entry.source.strong_count() > 0);
    for &(start, end) in ranges.iter().skip(page.saturating_sub(1)).take(4) {
        for index in start..end.min(ch.blocks.len()) {
            let block = &ch.blocks[index];
            let images = media_for(BlockKey::new(chapter, index));
            let primary = if let ContentBlock::Image { data, .. } = block {
                Some(data)
            } else {
                None
            };
            for data in primary
                .into_iter()
                .chain(images.iter().map(|image| &image.data))
            {
                let key = Arc::as_ptr(data) as usize;
                let entry = cache.entries.entry(key).or_insert_with(|| CachedImage {
                    source: Arc::downgrade(data),
                    texture: None,
                    pending: None,
                    natural: uncached_dimensions(data),
                    failed: false,
                    decode_side: 0,
                    used: 0,
                    pass: 0,
                    visible_pass: None,
                });
                let side = entry
                    .natural
                    .map_or(256, |n| {
                        let (w, h) =
                            block
                                .native_style()
                                .image_size(n.x, n.y, font, width, current_limit());
                        ((w.max(h) * ctx.pixels_per_point()).ceil() as u32).div_ceil(256) * 256
                    })
                    .clamp(
                        256,
                        ctx.input(|input| input.max_texture_side).min(2048) as u32,
                    );
                if !entry.failed && entry.pending.is_none() && side > entry.decode_side {
                    let result = Arc::new(Mutex::new(None));
                    if decode_queue()
                        .try_send(DecodeJob {
                            data: data.clone(),
                            max_side: side,
                            result: result.clone(),
                            ctx: ctx.clone(),
                        })
                        .is_ok()
                    {
                        entry.pending = Some(result);
                        entry.decode_side = side;
                    }
                }
            }
        }
    }
    ctx.data_mut(|store| store.insert_temp(id, cache));
}

/// Curl meshes refer to texture IDs after the cache's next eviction. Keep their
/// handles alive for exactly the duration of the captured sheet.
pub(crate) fn retain_textures(
    ctx: &egui::Context,
    shapes: &[egui::epaint::ClippedShape],
) -> Vec<egui::TextureHandle> {
    fn collect(shape: &egui::Shape, ids: &mut std::collections::HashSet<egui::TextureId>) {
        match shape {
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, ids);
                }
            }
            egui::Shape::Mesh(mesh) => {
                ids.insert(mesh.texture_id);
            }
            _ => {}
        }
    }
    let mut ids = std::collections::HashSet::new();
    for shape in shapes {
        collect(&shape.shape, &mut ids);
    }
    ctx.data(|store| store.get_temp::<ImageCache>(egui::Id::new("epub_illustrations")))
        .map(|cache| {
            cache
                .entries
                .values()
                .filter_map(|entry| entry.texture.as_ref())
                .filter(|texture| ids.contains(&texture.id()))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
pub(crate) fn block_media_height(
    block: &ContentBlock,
    images: &[SupplementalImage],
    text_height: f32,
    width: f32,
    limit: f32,
    font: f32,
) -> f32 {
    let base = if let ContentBlock::Image { data, .. } = block {
        styled_height(data, width, limit, font, &block.native_style())
    } else {
        text_height
    };
    let image_limit = media_limit(base, images.len(), font, limit);
    base + images
        .iter()
        .map(|image| styled_height(&image.data, width, image_limit, font, &block.native_style()))
        .sum::<f32>()
}

#[cfg(feature = "performance-checks")]
pub(crate) fn diagnostic_visible(ctx: &egui::Context) -> (usize, usize, Vec<usize>) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|store| store.get_temp::<ImageCache>(egui::Id::new("epub_illustrations")))
        .map_or((0, 0, vec![]), |cache| {
            let mut pending = 0;
            let mut failed = 0;
            let mut ready = vec![];
            for (key, image) in cache.entries {
                if image.visible_pass == Some(pass) {
                    pending += usize::from(image.pending.is_some() || image.texture.is_none());
                    failed += usize::from(image.failed);
                    if image.texture.is_some() {
                        ready.push(key);
                    }
                }
            }
            (pending, failed, ready)
        })
}

#[cfg(test)]
mod tests {
    use super::super::{
        reader::paginate_blocks,
        reader_block::{estimate_block_height, render_content_layout},
        reader_state::set_spacing,
    };
    use super::*;

    fn render_until_loaded(
        ctx: &egui::Context,
        mut render: impl FnMut() -> egui::FullOutput,
    ) -> egui::FullOutput {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let output = render();
            let pending = ctx
                .data(|store| store.get_temp::<ImageCache>(egui::Id::new("epub_illustrations")))
                .is_some_and(|cache| {
                    cache
                        .entries
                        .values()
                        .any(|entry| entry.texture.is_none() && !entry.failed)
                });
            if !pending {
                return output;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "decoder did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    fn fixture() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../core/tests/fixtures/illustrations.epub")
    }
    #[test]
    fn large_image_cold_frame_queues_decode_and_warm_frames_reuse_the_texture() {
        let mut pixels = image::RgbImage::new(4000, 3000);
        for (x, y, pixel) in pixels.enumerate_pixels_mut() {
            *pixel = image::Rgb([(x % 251) as u8, (y % 241) as u8, ((x + y) % 239) as u8]);
        }
        let mut encoded = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, 90)
            .encode_image(&pixels)
            .unwrap();
        let data = Arc::new(encoded);
        let baseline = std::time::Instant::now();
        let decoded = image::load_from_memory(&data)
            .unwrap()
            .thumbnail(2048, 2048)
            .to_rgba8();
        let blocking_ms = baseline.elapsed().as_secs_f64() * 1000.;
        drop(decoded);
        let ctx = egui::Context::default();
        let i18n = reader_core::i18n::I18n::new(reader_core::i18n::Language::En);
        let ready = std::cell::Cell::new(false);
        let draw = || {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 800.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .show(ctx, |ui| paint(ui, &data, None, 650., 650., 18., &i18n));
                    ready.set(visible_images_ready(ctx));
                },
            )
        };
        let started = std::time::Instant::now();
        let _ = draw();
        let cold_frame_ms = started.elapsed().as_secs_f64() * 1000.;
        assert!(
            !ready.get(),
            "opening paper revealed an undecoded visible image"
        );
        assert!(
            ctx.data(|store| store.get_temp::<ImageCache>(egui::Id::new("epub_illustrations")))
                .unwrap()
                .entries
                .values()
                .any(|entry| entry.pending.is_some()),
            "first frame blocked until decode completed"
        );
        let started = std::time::Instant::now();
        let _ = render_until_loaded(&ctx, draw);
        let wait_ms = started.elapsed().as_secs_f64() * 1000.;
        let textures = retain_textures(&ctx, &draw().shapes);
        assert_eq!(textures.len(), 1);
        assert!(
            ready.get(),
            "ready texture did not release the opening paper"
        );
        let started = std::time::Instant::now();
        for _ in 0..20 {
            let _ = draw();
        }
        let warm_frame_ms = started.elapsed().as_secs_f64() * 50.;
        assert_eq!(
            textures[0].id(),
            retain_textures(&ctx, &draw().shapes)[0].id()
        );
        if let Ok(path) = std::env::var("EPUB_IMAGE_PERF_REPORT") {
            std::fs::write(path,serde_json::to_string_pretty(&serde_json::json!({
                "profile":"cargo test debug; CPU egui frame, excludes GPU/present and device acceptance",
                "source_pixels":12_000_000,"blocking_decode_and_scale_ms":blocking_ms,
                "cold_frame_ms":cold_frame_ms,"async_wait_ms":wait_ms,"warm_frame_mean_ms":warm_frame_ms,
                "cached_texture_reused":true
            })).unwrap()).unwrap();
        }
    }
    #[test]
    fn illustrations_paint_in_scroll_and_page_layouts_and_fit_single_and_dual_columns() {
        let book = reader_core::epub::EpubBook::open(fixture()).unwrap();
        let ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        fonts.families.insert(
            egui::FontFamily::Name("Bold".into()),
            fonts.families[&egui::FontFamily::Proportional].clone(),
        );
        ctx.set_fonts(fonts);
        let i18n = reader_core::i18n::I18n::new(reader_core::i18n::Language::En);
        set_spacing(1.8, 0.6, 2.);
        for scroll in [true, false] {
            for width in [260., 480.] {
                configure(Some(&book), 700., scroll, 16., 1.5);
                for chapter in 0..8 {
                    let ch = &book.chapters[chapter];
                    let media = book
                        .supplemental_images
                        .get(&chapter)
                        .map(Vec::as_slice)
                        .unwrap_or(&[]);
                    let ranges = paginate_blocks(&ch.blocks, 16., 1.5, 1.8, 700., width, media);
                    if !scroll {
                        assert!(
                            ranges[0].1 > 1,
                            "chapter title orphaned from illustration: {chapter}"
                        );
                        for &(start, end) in &ranges {
                            let sum: f32 = ch.blocks[start..end]
                                .iter()
                                .enumerate()
                                .map(|(offset, block)| {
                                    let images: Vec<_> = media
                                        .iter()
                                        .filter(|image| image.block == start + offset)
                                        .cloned()
                                        .collect();
                                    block_media_height(
                                        block,
                                        &images,
                                        estimate_block_height(block, 16., 1.5, 28.8, width),
                                        width,
                                        page_limit_for(&ch.blocks, 700., width, 16., 1.5),
                                        16.,
                                    )
                                })
                                .sum();
                            assert!(
                                sum <= 700. - FRAME_MARGIN,
                                "image overflows page: {chapter}, {sum}"
                            );
                        }
                    }
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width + 20., 700.),
                        )),
                        ..Default::default()
                    };
                    let output = render_until_loaded(&ctx, || {
                        ctx.run(input.clone(), |ctx| {
                            egui::CentralPanel::default().show(ctx, |ui| {
                                let mut link = None;
                                render_content_layout(
                                    ui,
                                    0.,
                                    width,
                                    &ch.title,
                                    &ch.blocks,
                                    0,
                                    ch.blocks.len(),
                                    true,
                                    None,
                                    16.,
                                    1.5,
                                    egui::Color32::WHITE,
                                    chapter,
                                    None,
                                    "Sans",
                                    &i18n,
                                    &mut link,
                                    &HashMap::new(),
                                );
                            });
                        })
                    });
                    fn image_rects(shape: &egui::Shape, out: &mut Vec<egui::Rect>) {
                        match shape {
                            egui::Shape::Vec(shapes) => {
                                for shape in shapes {
                                    image_rects(shape, out)
                                }
                            }
                            egui::Shape::Mesh(mesh)
                                if mesh.texture_id != egui::TextureId::default() =>
                            {
                                out.push(mesh.calc_bounds())
                            }
                            _ => {}
                        }
                    }
                    let mut rects = vec![];
                    for shape in output.shapes {
                        image_rects(&shape.shape, &mut rects);
                    }
                    assert_eq!(
                        rects.len(),
                        1,
                        "illustration absent: scroll={scroll}, width={width}, chapter={chapter}"
                    );
                    assert!(rects[0].width() <= width + 0.1);
                    if !scroll {
                        assert!(rects[0].bottom() < 700., "painted image clipped: {chapter}");
                    }
                }
            }
        }
    }
    #[test]
    fn invalid_images_have_a_bounded_fallback() {
        assert!(fitted_size(b"not an image", 300., 500.).is_none());
        assert!(height(b"not an image", 300., 500., 16.) < 60.);
    }
    #[test]
    fn a_large_illustration_frame_and_curl_snapshot_survive_cache_eviction() {
        let book = reader_core::epub::EpubBook::open(fixture()).unwrap();
        let source = &book.supplemental_images[&0][0].data;
        let make_images = || {
            (0..32)
                .map(|_| Arc::new(source.as_ref().clone()))
                .collect::<Vec<_>>()
        };
        let first = make_images();
        let second = make_images();
        let ctx = egui::Context::default();
        let i18n = reader_core::i18n::I18n::new(reader_core::i18n::Language::En);
        let render = |images: &[Arc<Vec<u8>>]| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(2000., 200.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            for image in images {
                                paint(ui, image, None, 20., 20., 16., &i18n);
                            }
                        });
                    });
                },
            )
        };
        let output = render_until_loaded(&ctx, || render(&first));
        let held = retain_textures(&ctx, &output.shapes);
        assert_eq!(
            held.len(),
            32,
            "same-frame cache eviction dropped visible images"
        );
        let ids: Vec<_> = held.iter().map(egui::TextureHandle::id).collect();
        let _ = render_until_loaded(&ctx, || render(&second));
        // Evict inactive resources after the one-frame grace for visible layers.
        let _ = render(&second);
        assert!(
            ids.iter()
                .all(|id| ctx.tex_manager().read().meta(*id).is_some()),
            "turning sheet lost its illustration textures"
        );
        drop(held);
        assert!(
            ids.iter()
                .all(|id| ctx.tex_manager().read().meta(*id).is_none()),
            "settled sheet retained obsolete textures"
        );
    }
}
