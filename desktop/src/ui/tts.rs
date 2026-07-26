//! Text-to-Speech (TTS) control floating UI and settings.
use crate::app::{ReaderApp, TtsAudioResultSlot};
use eframe::egui;
use egui::{Color32, CornerRadius, Stroke, Vec2};
use reader_core::epub::ContentBlock;
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// Predefined Chinese TTS voices.
const VOICE_PRESETS: &[(&str, &str)] = &[
    ("zh-CN-XiaoxiaoNeural", "晓晓 (女)"),
    ("zh-CN-YunyangNeural", "云扬 (男)"),
    ("zh-CN-XiaoyiNeural", "晓依 (女)"),
    ("zh-CN-YunjianNeural", "云健 (男)"),
    ("zh-CN-YunxiNeural", "云希 (男)"),
    ("zh-CN-XiaochenNeural", "晓辰 (女)"),
    ("zh-CN-XiaohanNeural", "晓涵 (女)"),
    ("zh-CN-XiaomoNeural", "晓墨 (女)"),
    ("zh-CN-XiaoruiNeural", "晓睿 (女)"),
    ("zh-CN-XiaoshuangNeural", "晓双 (女)"),
    ("en-US-AriaNeural", "Aria (EN Female)"),
    ("en-US-GuyNeural", "Guy (EN Male)"),
    ("ja-JP-NanamiNeural", "Nanami (JP Female)"),
];

const RATE_OPTIONS: &[(i32, &str)] = &[
    (-50, "0.5×"),
    (-25, "0.75×"),
    (0, "1×"),
    (25, "1.25×"),
    (50, "1.5×"),
    (100, "2×"),
    (150, "2.5×"),
    (200, "3×"),
];
const VOLUME_OPTIONS: &[(i32, &str)] = &[
    (-50, "-50%"),
    (-25, "-25%"),
    (0, "正常"),
    (25, "+25%"),
    (50, "+50%"),
];

fn page_for_block(ranges: &[(usize, usize)], block: usize, dual_column: bool) -> Option<usize> {
    ranges
        .iter()
        .position(|(start, end)| *start <= block && block < *end)
        .map(|page| if dual_column { page - page % 2 } else { page })
}

fn next_readable_block(blocks: &[ContentBlock], from: usize) -> usize {
    blocks[from.min(blocks.len())..]
        .iter()
        .position(|block| {
            matches!(
                block,
                ContentBlock::Paragraph { .. } | ContentBlock::Heading { .. }
            )
        })
        .map_or(blocks.len(), |offset| from + offset)
}

fn text_from_char(text: &str, char_offset: usize) -> String {
    text.chars().skip(char_offset).collect()
}

impl ReaderApp {
    /// Render TTS as a horizontal bar between toolbar and content (Edge-style).
    pub fn render_tts_bar(&mut self, ctx: &egui::Context) {
        let accent = Color32::from_rgb(56, 132, 255);
        let dark = self.dark_mode;
        let bar_bg = if dark {
            Color32::from_rgb(38, 38, 42)
        } else {
            Color32::from_rgb(245, 245, 250)
        };
        let subtle_color = if dark {
            Color32::from_gray(130)
        } else {
            Color32::from_gray(100)
        };

        egui::TopBottomPanel::top("tts_bar")
            .frame(
                egui::Frame::default()
                    .fill(bar_bg)
                    .inner_margin(egui::Margin {
                        left: 16,
                        right: 16,
                        top: 6,
                        bottom: 6,
                    })
                    .stroke(Stroke::new(
                        0.5_f32,
                        if dark {
                            Color32::from_gray(55)
                        } else {
                            Color32::from_gray(210)
                        },
                    )),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;

                    // ── Playback controls ──
                    if !self.tts_playing {
                        let play_btn = egui::Button::new(
                            egui::RichText::new("▶").size(14.0).color(Color32::WHITE),
                        )
                        .fill(accent)
                        .corner_radius(CornerRadius::same(4))
                        .min_size(Vec2::new(32.0, 26.0));
                        if ui
                            .add(play_btn)
                            .on_hover_text(self.i18n.t("tts.play"))
                            .clicked()
                        {
                            self.tts_start_playback();
                        }
                    } else {
                        // Pause / Resume
                        let pause_icon = if self.tts_paused { "▶" } else { "⏸" };
                        let pause_tip = if self.tts_paused {
                            self.i18n.t("tts.resume")
                        } else {
                            self.i18n.t("tts.pause")
                        };
                        let pause_btn = egui::Button::new(
                            egui::RichText::new(pause_icon)
                                .size(14.0)
                                .color(Color32::WHITE),
                        )
                        .fill(accent)
                        .corner_radius(CornerRadius::same(4))
                        .min_size(Vec2::new(32.0, 26.0));
                        if ui.add(pause_btn).on_hover_text(pause_tip).clicked() {
                            if self.tts_paused {
                                if let Some(sink) = &self.tts_audio_sink {
                                    sink.play();
                                }
                                self.tts_paused = false;
                            } else {
                                if let Some(sink) = &self.tts_audio_sink {
                                    sink.pause();
                                }
                                self.tts_paused = true;
                            }
                        }

                        let stop_btn = egui::Button::new(egui::RichText::new("⏹").size(14.0))
                            .corner_radius(CornerRadius::same(4))
                            .min_size(Vec2::new(32.0, 26.0));
                        if ui
                            .add(stop_btn)
                            .on_hover_text(self.i18n.t("tts.stop"))
                            .clicked()
                        {
                            self.tts_stop_playback();
                        }
                    }

                    ui.separator();

                    // ── Voice selector ──
                    ui.label(
                        egui::RichText::new(self.i18n.t("tts.voice"))
                            .size(12.0)
                            .color(subtle_color),
                    );
                    egui::ComboBox::from_id_salt("tts_voice")
                        .width(120.0)
                        .selected_text(
                            VOICE_PRESETS
                                .iter()
                                .find(|(name, _)| *name == self.tts_voice_name)
                                .map(|(_, label)| *label)
                                .unwrap_or(&self.tts_voice_name),
                        )
                        .show_ui(ui, |ui| {
                            for (name, label) in VOICE_PRESETS {
                                ui.selectable_value(
                                    &mut self.tts_voice_name,
                                    name.to_string(),
                                    *label,
                                );
                            }
                        });

                    // ── Rate selector ──
                    ui.label(
                        egui::RichText::new(self.i18n.t("tts.rate"))
                            .size(12.0)
                            .color(subtle_color),
                    );
                    egui::ComboBox::from_id_salt("tts_rate")
                        .width(64.0)
                        .selected_text(
                            RATE_OPTIONS
                                .iter()
                                .find(|(v, _)| *v == self.tts_rate)
                                .map(|(_, l)| *l)
                                .unwrap_or("正常"),
                        )
                        .show_ui(ui, |ui| {
                            for (val, label) in RATE_OPTIONS {
                                ui.selectable_value(&mut self.tts_rate, *val, *label);
                            }
                        });

                    // ── Volume selector ──
                    ui.label(
                        egui::RichText::new(self.i18n.t("tts.volume"))
                            .size(12.0)
                            .color(subtle_color),
                    );
                    egui::ComboBox::from_id_salt("tts_volume")
                        .width(64.0)
                        .selected_text(
                            VOLUME_OPTIONS
                                .iter()
                                .find(|(v, _)| *v == self.tts_volume)
                                .map(|(_, l)| *l)
                                .unwrap_or("正常"),
                        )
                        .show_ui(ui, |ui| {
                            for (val, label) in VOLUME_OPTIONS {
                                ui.selectable_value(&mut self.tts_volume, *val, *label);
                            }
                        });

                    // ── Status text ──
                    let status = self.tts_status.lock().unwrap().clone();
                    if !status.is_empty() {
                        ui.label(egui::RichText::new(&status).size(12.0).color(subtle_color));
                    }

                    // ── Close button (right-aligned) ──
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✕").clicked() {
                            self.show_tts_panel = false;
                        }
                    });
                });

                if self.tts_playing {
                    ui.ctx().request_repaint();
                }
            });
    }

    pub fn tts_start_playback(&mut self) {
        self.push_feedback_log(format!(
            "[TTS] start_playback: voice={}, rate={}, volume={}",
            self.tts_voice_name, self.tts_rate, self.tts_volume
        ));
        let start_block = if self.scroll_mode {
            self.current_block
        } else {
            self.page_block_ranges
                .get(self.current_page)
                .map(|(start, _)| *start)
                .unwrap_or(self.current_block)
        };
        self.tts_start_from_position(self.current_chapter, start_block, 0);
    }

    pub(crate) fn tts_start_from_position(
        &mut self,
        chapter: usize,
        block: usize,
        char_offset: usize,
    ) {
        self.tts_cancel_audio();
        self.tts_stop_flag.store(false, Ordering::Relaxed);
        self.tts_playing = true;
        self.tts_paused = false;
        self.tts_follow_view = true;
        self.tts_current_chapter = chapter;
        self.tts_current_block = self.tts_next_readable_block(chapter, block);
        self.tts_current_char = if self.tts_current_block == block {
            char_offset
        } else {
            0
        };
        if let Some(total) = self.tts_block_count(chapter) {
            self.push_feedback_log(format!(
                "[TTS] chapter {} has {} blocks, first readable={}",
                chapter, total, self.tts_current_block
            ));
            if self.tts_current_block >= total {
                self.push_feedback_log("[TTS] no readable blocks in chapter");
                self.tts_stop_playback();
                return;
            }
        }
        self.tts_follow_current_position();
        self.tts_synthesize_current_block();
    }

    pub fn tts_stop_playback(&mut self) {
        self.push_feedback_log("[TTS] stop_playback");
        self.tts_cancel_audio();
        self.tts_playing = false;
        self.tts_paused = false;
        self.tts_follow_view = false;
        *self.tts_status.lock().unwrap() = String::new();
    }

    fn tts_cancel_audio(&mut self) {
        self.tts_stop_flag.store(true, Ordering::Relaxed);
        self.tts_generation.fetch_add(1, Ordering::Relaxed);
        if let Some(sink) = self.tts_audio_sink.take() {
            sink.stop();
        }
        self.tts_pending_audio = None;
        self.tts_prefetch_audio = None;
    }

    pub(crate) fn tts_detach_view(&mut self) {
        if self.tts_playing && !self.tts_syncing_navigation {
            self.tts_follow_view = false;
            self.pending_restore_block = None;
        }
    }

    pub(crate) fn tts_follow_playback(&mut self) {
        if self.tts_playing {
            self.tts_follow_view = true;
            self.tts_follow_current_position();
        }
    }

    /// Return the total number of blocks in a chapter.
    fn tts_block_count(&self, chapter: usize) -> Option<usize> {
        self.book
            .as_ref()
            .and_then(|b| b.chapters.get(chapter).map(|ch| ch.blocks.len()))
    }

    /// Starting from `from`, find the next block index that is a Paragraph or Heading.
    fn tts_next_readable_block(&self, chapter: usize, from: usize) -> usize {
        self.book
            .as_ref()
            .and_then(|book| book.chapters.get(chapter))
            .map_or(0, |chapter| next_readable_block(&chapter.blocks, from))
    }

    fn tts_next_readable_position(
        &self,
        mut chapter: usize,
        mut from: usize,
    ) -> Option<(usize, usize)> {
        while chapter < self.total_chapters() {
            let block = self.tts_next_readable_block(chapter, from);
            if block < self.tts_block_count(chapter).unwrap_or(0) {
                return Some((chapter, block));
            }
            chapter += 1;
            from = 0;
        }
        None
    }

    /// Get the text content of a block by index (empty string for non-text blocks).
    fn tts_block_text(&self, chapter: usize, block_idx: usize) -> String {
        self.book
            .as_ref()
            .and_then(|b| {
                b.chapters.get(chapter).and_then(|ch| {
                    ch.blocks.get(block_idx).map(|block| match block {
                        reader_core::epub::ContentBlock::Paragraph { spans, .. } => {
                            spans.iter().map(|s| s.text.as_str()).collect::<String>()
                        }
                        reader_core::epub::ContentBlock::Heading { spans, .. } => {
                            spans.iter().map(|s| s.text.as_str()).collect::<String>()
                        }
                        _ => String::new(),
                    })
                })
            })
            .unwrap_or_default()
    }

    fn tts_navigate_to_chapter(&mut self, chapter: usize) {
        self.tts_syncing_navigation = true;
        while self.current_chapter < chapter {
            self.next_chapter();
        }
        while self.current_chapter > chapter {
            self.prev_chapter();
        }
        self.tts_syncing_navigation = false;
    }

    fn tts_follow_current_position(&mut self) {
        if !self.tts_follow_view {
            return;
        }
        if self.current_chapter != self.tts_current_chapter {
            self.tts_navigate_to_chapter(self.tts_current_chapter);
        }
        if self.scroll_mode || self.pages_dirty {
            self.pending_restore_block = Some(self.tts_current_block);
            return;
        }
        let Some(page) = page_for_block(
            &self.page_block_ranges,
            self.tts_current_block,
            self.is_dual_column,
        ) else {
            return;
        };
        if page != self.current_page {
            let direction = if page > self.current_page { 1.0 } else { -1.0 };
            self.tts_syncing_navigation = true;
            self.trigger_page_animation_to(page, direction);
            self.tts_syncing_navigation = false;
        }
    }

    fn tts_advance_to_next_block(&mut self) {
        let Some((chapter, block)) =
            self.tts_next_readable_position(self.tts_current_chapter, self.tts_current_block + 1)
        else {
            self.push_feedback_log("[TTS] book finished");
            self.tts_stop_playback();
            *self.tts_status.lock().unwrap() = self.i18n.t("tts.book_done").to_string();
            return;
        };
        self.tts_current_chapter = chapter;
        self.tts_current_block = block;
        self.tts_current_char = 0;
        self.tts_follow_current_position();

        // Check if we have prefetched audio for this block
        if let Some(prefetch) = self.tts_prefetch_audio.take() {
            if self.tts_prefetch_chapter == self.tts_current_chapter
                && self.tts_prefetch_block == self.tts_current_block
            {
                let data = prefetch.lock().unwrap().take();
                match data {
                    Some(Ok(bytes)) => {
                        if let Some(sink) = self.tts_audio_sink.take() {
                            sink.stop();
                        }
                        if let Err(e) = self.tts_play_bytes(&bytes) {
                            *self.tts_status.lock().unwrap() = format!("Play error: {}", e);
                            self.tts_playing = false;
                        }
                        self.tts_start_prefetch();
                        return;
                    }
                    Some(Err(error)) => {
                        *self.tts_status.lock().unwrap() = format!("TTS Error: {error}");
                        self.tts_playing = false;
                        return;
                    }
                    None => {
                        // Promote the in-flight prefetch instead of synthesizing the same block twice.
                        self.tts_pending_audio = Some(prefetch);
                        self.tts_start_prefetch();
                        return;
                    }
                }
            }
        }
        // No prefetch available — synthesize the current block
        self.tts_synthesize_current_block();
    }

    fn tts_synthesize_current_block(&mut self) {
        // Clear old sink so the "empty" check doesn't fire while synthesizing next block
        if let Some(sink) = self.tts_audio_sink.take() {
            sink.stop();
        }

        let text = text_from_char(
            &self.tts_block_text(self.tts_current_chapter, self.tts_current_block),
            self.tts_current_char,
        );
        if text.trim().is_empty() {
            self.tts_advance_to_next_block();
            return;
        }

        let pending = self.tts_spawn_synthesis(text);
        self.tts_pending_audio = Some(pending);

        // Also start prefetching the next block
        self.tts_start_prefetch();
    }

    /// Start prefetching audio for the next readable block after current.
    fn tts_start_prefetch(&mut self) {
        let Some((chapter, block)) =
            self.tts_next_readable_position(self.tts_current_chapter, self.tts_current_block + 1)
        else {
            self.tts_prefetch_audio = None;
            return;
        };
        let text = self.tts_block_text(chapter, block);
        if text.trim().is_empty() {
            self.tts_prefetch_audio = None;
            return;
        }
        self.tts_prefetch_chapter = chapter;
        self.tts_prefetch_block = block;
        let prefetch = self.tts_spawn_synthesis(text);
        self.tts_prefetch_audio = Some(prefetch);

        // For short text (< 20 chars), also check if we should prefetch one more ahead
        // (the prefetch-of-prefetch will be handled when this block becomes current)
    }

    /// Spawn a background thread to synthesize `text` and return a handle to poll.
    fn tts_spawn_synthesis(&self, text: String) -> TtsAudioResultSlot {
        let voice_name = self.tts_voice_name.clone();
        let rate = self.tts_rate;
        let volume = self.tts_volume;
        let stop_flag = self.tts_stop_flag.clone();
        let generation = self.tts_generation.load(Ordering::Relaxed);
        let current_generation = self.tts_generation.clone();
        let status = self.tts_status.clone();
        let ctx = self.last_egui_ctx.clone();
        let logs = self.feedback_logs.clone();

        let audio_ready: TtsAudioResultSlot = Arc::new(std::sync::Mutex::new(None));
        let audio_ready2 = audio_ready.clone();

        let text_preview: String = text.chars().take(30).collect();
        crate::app::dbg_log(
            &logs,
            format!(
                "[TTS] synthesize: voice={}, text={}...",
                voice_name, text_preview
            ),
        );

        std::thread::spawn(move || {
            if stop_flag.load(Ordering::Relaxed)
                || current_generation.load(Ordering::Relaxed) != generation
            {
                return;
            }
            let t0 = std::time::Instant::now();
            let result = (|| -> Result<Vec<u8>, Box<dyn std::error::Error>> {
                let voices = msedge_tts::voice::get_voices_list()?;
                let voice = voices.iter().find(|v| {
                    v.short_name.as_deref() == Some(voice_name.as_str())
                        || v.name.contains(&voice_name)
                });
                let voice = match voice {
                    Some(v) => v,
                    None => {
                        crate::app::dbg_log(
                            &logs,
                            format!(
                                "[TTS] ERROR: voice '{}' not found in {} available voices",
                                voice_name,
                                voices.len()
                            ),
                        );
                        return Err("Voice not found".into());
                    }
                };
                let mut config = msedge_tts::tts::SpeechConfig::from(voice);
                config.rate = rate;
                config.volume = volume;
                let mut tts = msedge_tts::tts::client::connect()?;
                let audio = tts.synthesize(&text, &config)?;
                Ok(audio.audio_bytes)
            })();

            if stop_flag.load(Ordering::Relaxed)
                || current_generation.load(Ordering::Relaxed) != generation
            {
                return;
            }

            match result {
                Ok(bytes) => {
                    let elapsed = t0.elapsed();
                    crate::app::dbg_log(
                        &logs,
                        format!(
                            "[TTS] synthesized {} bytes in {:.1}s",
                            bytes.len(),
                            elapsed.as_secs_f64()
                        ),
                    );
                    *audio_ready2.lock().unwrap() = Some(Ok(bytes));
                }
                Err(e) => {
                    crate::app::dbg_log(&logs, format!("[TTS] ERROR synthesis: {}", e));
                    *status.lock().unwrap() = format!("TTS Error: {}", e);
                    *audio_ready2.lock().unwrap() = Some(Err(e.to_string()));
                }
            }
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        });

        audio_ready
    }

    /// Called each frame to check if pending TTS audio is ready.
    pub fn tts_poll_audio(&mut self) {
        if let Some(pending) = &self.tts_pending_audio {
            let data = pending.lock().unwrap().take();
            if let Some(result) = data {
                self.tts_pending_audio = None;
                match result {
                    Ok(bytes) => {
                        if let Err(e) = self.tts_play_bytes(&bytes) {
                            *self.tts_status.lock().unwrap() = format!("Play error: {}", e);
                            self.tts_playing = false;
                        }
                    }
                    Err(error) => {
                        *self.tts_status.lock().unwrap() = format!("TTS Error: {error}");
                        self.tts_playing = false;
                    }
                }
            }
        }
        let finished = self.tts_playing
            && !self.tts_paused
            && self
                .tts_audio_sink
                .as_ref()
                .is_some_and(|sink| sink.empty());
        if finished {
            if let Some(sink) = self.tts_audio_sink.take() {
                sink.stop();
            }
            self.tts_advance_to_next_block();
        }
    }

    fn tts_play_bytes(&mut self, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        self.push_feedback_log(format!("[TTS] play_bytes: {} bytes", bytes.len()));
        if self.tts_output_handle.is_none() {
            let (stream, handle) = rodio::OutputStream::try_default()?;
            self.tts_output_stream = Some(stream);
            self.tts_output_handle = Some(handle);
        }
        let stream_handle = self
            .tts_output_handle
            .as_ref()
            .ok_or("Audio output is unavailable")?;
        let sink = rodio::Sink::try_new(stream_handle)?;
        let cursor = std::io::Cursor::new(bytes.to_vec());
        let source = rodio::Decoder::new(cursor)?;
        sink.append(source);
        if self.tts_paused {
            sink.pause();
        }
        let sink = Arc::new(sink);
        self.tts_audio_sink = Some(sink);
        *self.tts_status.lock().unwrap() = self.i18n.t("tts.playing").to_string();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{next_readable_block, page_for_block, text_from_char};
    use reader_core::epub::ContentBlock;

    #[test]
    fn maps_blocks_to_single_and_dual_pages() {
        let ranges = [(0, 3), (3, 6), (6, 9), (9, 12)];
        assert_eq!(page_for_block(&ranges, 7, false), Some(2));
        assert_eq!(page_for_block(&ranges, 10, true), Some(2));
        assert_eq!(page_for_block(&ranges, 12, false), None);
    }

    #[test]
    fn skips_non_text_blocks() {
        let blocks = [
            ContentBlock::BlankLine,
            ContentBlock::Separator,
            ContentBlock::Paragraph {
                spans: Vec::new(),
                anchor_id: None,
            },
        ];
        assert_eq!(next_readable_block(&blocks, 0), 2);
        assert_eq!(next_readable_block(&blocks, 3), 3);
    }

    #[test]
    fn starts_text_at_unicode_character_offset() {
        assert_eq!(text_from_char("中文，English!", 3), "English!");
    }
}
