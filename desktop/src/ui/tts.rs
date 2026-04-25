//! Text-to-Speech (TTS) control floating UI and settings.
use crate::app::ReaderApp;
use eframe::egui;
use egui::{Color32, CornerRadius, Stroke, Vec2};
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
    (-50, "-50%"),
    (-25, "-25%"),
    (0, "正常"),
    (25, "+25%"),
    (50, "+50%"),
    (100, "+100%"),
];
const VOLUME_OPTIONS: &[(i32, &str)] = &[
    (-50, "-50%"),
    (-25, "-25%"),
    (0, "正常"),
    (25, "+25%"),
    (50, "+50%"),
];

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
                        0.5,
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

                // Poll playback completion
                if self.tts_playing {
                    if let Some(sink) = &self.tts_audio_sink {
                        if sink.empty() && !self.tts_paused {
                            self.tts_advance_to_next_block();
                        }
                    }
                    ui.ctx().request_repaint();
                }
            });
    }

    pub fn tts_start_playback(&mut self) {
        self.push_feedback_log(format!(
            "[TTS] start_playback: voice={}, rate={}, volume={}",
            self.tts_voice_name, self.tts_rate, self.tts_volume
        ));
        self.tts_stop_flag.store(false, Ordering::Relaxed);
        self.tts_playing = true;
        self.tts_paused = false;
        self.tts_current_block = 0;
        self.tts_prefetch_audio = None;
        self.tts_prefetch_block = 0;
        // Find first readable block
        self.tts_current_block = self.tts_next_readable_block(0);
        if let Some(total) = self.tts_block_count() {
            self.push_feedback_log(format!(
                "[TTS] chapter has {} blocks, first readable={}",
                total, self.tts_current_block
            ));
            if self.tts_current_block >= total {
                self.push_feedback_log("[TTS] no readable blocks in chapter");
                self.tts_stop_playback();
                return;
            }
        }
        self.tts_synthesize_current_block();
    }

    pub fn tts_stop_playback(&mut self) {
        self.push_feedback_log("[TTS] stop_playback");
        self.tts_stop_flag.store(true, Ordering::Relaxed);
        if let Some(sink) = self.tts_audio_sink.take() {
            sink.stop();
        }
        self.tts_playing = false;
        self.tts_paused = false;
        self.tts_pending_audio = None;
        self.tts_prefetch_audio = None;
        *self.tts_status.lock().unwrap() = String::new();
    }

    /// Return the total number of blocks in the current chapter.
    fn tts_block_count(&self) -> Option<usize> {
        self.book.as_ref().and_then(|b| {
            b.chapters
                .get(self.current_chapter)
                .map(|ch| ch.blocks.len())
        })
    }

    /// Starting from `from`, find the next block index that is a Paragraph or Heading.
    fn tts_next_readable_block(&self, from: usize) -> usize {
        let mut idx = from;
        let total = self.tts_block_count().unwrap_or(0);
        while idx < total {
            if let Some(block) = self.book.as_ref().and_then(|b| {
                b.chapters
                    .get(self.current_chapter)
                    .and_then(|ch| ch.blocks.get(idx))
            }) {
                if matches!(
                    block,
                    reader_core::epub::ContentBlock::Paragraph { .. }
                        | reader_core::epub::ContentBlock::Heading { .. }
                ) {
                    return idx;
                }
            }
            idx += 1;
        }
        idx // past end
    }

    /// Get the text content of a block by index (empty string for non-text blocks).
    fn tts_block_text(&self, block_idx: usize) -> String {
        self.book
            .as_ref()
            .and_then(|b| {
                b.chapters.get(self.current_chapter).and_then(|ch| {
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

    fn tts_advance_to_next_block(&mut self) {
        let total = self.tts_block_count().unwrap_or(0);
        let next = self.tts_next_readable_block(self.tts_current_block + 1);
        if next >= total {
            // Chapter finished
            self.push_feedback_log("[TTS] chapter finished");
            self.tts_stop_playback();
            *self.tts_status.lock().unwrap() = self.i18n.t("tts.chapter_done").to_string();
            return;
        }
        self.tts_current_block = next;

        // Check if we have prefetched audio for this block
        if let Some(prefetch) = self.tts_prefetch_audio.take() {
            if self.tts_prefetch_block == self.tts_current_block {
                let data = prefetch.lock().unwrap().take();
                if let Some(bytes) = data {
                    // Prefetch ready — play immediately with no gap!
                    if let Some(sink) = self.tts_audio_sink.take() {
                        sink.stop();
                    }
                    if let Err(e) = self.tts_play_bytes(&bytes) {
                        *self.tts_status.lock().unwrap() = format!("Play error: {}", e);
                        self.tts_playing = false;
                    }
                    // Start prefetching the NEXT block
                    self.tts_start_prefetch();
                    return;
                }
                // Prefetch not ready yet — fall through to synthesize normally
                // (the prefetch thread is still running, but we'll ignore it)
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

        let text = self.tts_block_text(self.tts_current_block);
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
        let total = self.tts_block_count().unwrap_or(0);
        let next = self.tts_next_readable_block(self.tts_current_block + 1);
        if next >= total {
            self.tts_prefetch_audio = None;
            return;
        }
        let text = self.tts_block_text(next);
        if text.trim().is_empty() {
            self.tts_prefetch_audio = None;
            return;
        }
        self.tts_prefetch_block = next;
        let prefetch = self.tts_spawn_synthesis(text);
        self.tts_prefetch_audio = Some(prefetch);

        // For short text (< 20 chars), also check if we should prefetch one more ahead
        // (the prefetch-of-prefetch will be handled when this block becomes current)
    }

    /// Spawn a background thread to synthesize `text` and return a handle to poll.
    fn tts_spawn_synthesis(&self, text: String) -> Arc<std::sync::Mutex<Option<Vec<u8>>>> {
        let voice_name = self.tts_voice_name.clone();
        let rate = self.tts_rate;
        let volume = self.tts_volume;
        let stop_flag = self.tts_stop_flag.clone();
        let status = self.tts_status.clone();
        let ctx = self.last_egui_ctx.clone();
        let logs = self.feedback_logs.clone();

        let audio_ready: Arc<std::sync::Mutex<Option<Vec<u8>>>> =
            Arc::new(std::sync::Mutex::new(None));
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
            if stop_flag.load(Ordering::Relaxed) {
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
                    *audio_ready2.lock().unwrap() = Some(bytes);
                }
                Err(e) => {
                    crate::app::dbg_log(&logs, format!("[TTS] ERROR synthesis: {}", e));
                    *status.lock().unwrap() = format!("TTS Error: {}", e);
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
            if let Some(bytes) = data {
                self.tts_pending_audio = None;
                // Play the audio
                if let Err(e) = self.tts_play_bytes(&bytes) {
                    *self.tts_status.lock().unwrap() = format!("Play error: {}", e);
                    self.tts_playing = false;
                }
            }
        }
    }

    fn tts_play_bytes(&mut self, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        self.push_feedback_log(format!("[TTS] play_bytes: {} bytes", bytes.len()));
        let (_stream, stream_handle) = rodio::OutputStream::try_default()?;
        let sink = rodio::Sink::try_new(&stream_handle)?;
        let cursor = std::io::Cursor::new(bytes.to_vec());
        let source = rodio::Decoder::new(cursor)?;
        sink.append(source);
        // Keep stream alive by leaking it (rodio requires OutputStream to stay alive)
        std::mem::forget(_stream);
        let sink = Arc::new(sink);
        self.tts_audio_sink = Some(sink);
        *self.tts_status.lock().unwrap() = self.i18n.t("tts.playing").to_string();
        Ok(())
    }
}
