//! The Desktop application state machine, defining main app logic.
mod opening;
mod settings;
mod shortcuts;
use settings::AppSettings;

use std::collections::HashMap;
use std::fmt::Write as _;
#[cfg(not(target_os = "macos"))]
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// (device_code, user_code, interval, expires_in)
pub type DeviceCodeResult = Result<(String, String, u64, u64), String>;

use eframe::egui;
use egui::Color32;

use reader_core::epub::EpubBook;
use reader_core::i18n::{I18n, Language};
use reader_core::library::Library;
use reader_core::sharing::{start_listener, DiscoveredPeer, PeerStore};

use crate::ui::reader_state::ContinuousScrollState;

type FontDiscoveryResult = Arc<
    Mutex<
        Option<(
            Vec<String>,
            HashMap<String, crate::system_fonts::SystemFont>,
        )>,
    >,
>;
pub(crate) type TtsAudioResultSlot = Arc<Mutex<Option<Result<Vec<u8>, String>>>>;

type PendingBookOpen = (
    String,
    Option<usize>,
    std::sync::mpsc::Receiver<Result<(EpubBook, Option<String>), String>>,
);

fn extend_unique_font_chain(target: &mut Vec<String>, fonts: impl IntoIterator<Item = String>) {
    for font in fonts {
        if !target.contains(&font) {
            target.push(font);
        }
    }
}

/// Push a debug log entry from anywhere (including background threads).
/// Writes to both the in-memory feedback_logs buffer and stderr (when debug logging is enabled).
pub fn dbg_log(logs: &Arc<Mutex<Vec<String>>>, msg: impl AsRef<str>) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let line = format!("[{ts}] {}", msg.as_ref());
    if reader_core::sharing::is_debug_logging_enabled() {
        eprintln!("[DBG] {line}");
    }
    if let Ok(mut v) = logs.lock() {
        v.push(line);
        if v.len() > 600 {
            let remove = v.len().saturating_sub(600);
            v.drain(0..remove);
        }
    }
}

/// Work item sent to the CSC background worker thread.
pub struct CscWork {
    pub chapter: usize,
    pub blocks: Vec<(usize, String)>,
    pub mode: reader_core::csc::CorrectionMode,
    pub threshold: reader_core::csc::CscThreshold,
}

/// Result returned from the CSC background worker thread.
pub struct CscResult {
    pub chapter: usize,
    pub corrections: Vec<(usize, Vec<reader_core::epub::CorrectionInfo>)>,
}

/// State for the CSC correction popup (accept / revert / ignore).
#[derive(Clone)]
pub struct CscPopupInfo {
    pub chapter: usize,
    pub block_idx: usize,
    pub char_offset: usize,
    pub original: String,
    pub corrected: String,
    pub confidence: f32,
    pub pos: egui::Pos2,
    /// Skip "click outside to close" on the frame the popup was just opened.
    pub just_opened: bool,
}

fn default_data_dir() -> String {
    let dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&dir);
    dir.to_string_lossy().to_string()
}

#[cfg(not(target_os = "macos"))]
fn is_font_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            matches!(
                e.to_ascii_lowercase().as_str(),
                "ttf" | "otf" | "ttc" | "otc"
            )
        })
        .unwrap_or(false)
}

#[cfg(not(target_os = "macos"))]
fn collect_font_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 5 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_font_files(&path, depth + 1, out);
        } else if is_font_file(&path) {
            out.push(path);
        }
    }
}

pub(crate) fn is_cjk_font_name(s: &str) -> bool {
    s.chars().any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c))
        || s.contains("CJK")
        || s.ends_with(" SC")
        || s.ends_with(" TC")
        || ["PingFang", "Songti", "Heiti", "Kaiti", "Hiragino Sans GB"]
            .iter()
            .any(|name| s.contains(name))
}

fn default_sans() -> String {
    if cfg!(target_os = "macos") {
        "苹方-简"
    } else {
        "Sans"
    }
    .to_string()
}

#[cfg(not(target_os = "macos"))]
fn discover_system_fonts() -> (
    Vec<String>,
    HashMap<String, crate::system_fonts::SystemFont>,
) {
    // 已知字体文件 stem → 人性化显示名（优先匹配，避免显示 "msyh" 之类的文件名）
    let known_names: HashMap<&str, &str> = [
        // Windows 中文字体
        ("msyh", "微软雅黑"),
        ("msyhbd", "微软雅黑 粗体"),
        ("msyhl", "微软雅黑 细体"),
        ("simsun", "宋体 (SimSun)"),
        ("simsunb", "宋体-ExtB"),
        ("simhei", "黑体 (SimHei)"),
        ("simkai", "楷体 (SimKai)"),
        ("simfang", "仿宋 (FangSong)"),
        ("simli", "隶书 (SimLi)"),
        ("simyou", "幼圆 (YouYuan)"),
        ("STXIHEI", "华文细黑"),
        ("STKAITI", "华文楷体"),
        ("STFANGSO", "华文仿宋"),
        ("STSONG", "华文宋体"),
        ("STLITI", "华文隶书"),
        ("STXINWEI", "华文新魏"),
        ("STZHONGS", "华文中宋"),
        ("STHUPO", "华文琥珀"),
        ("STCAIYUN", "华文彩云"),
        ("FZSTK", "方正舒体"),
        ("FZYTK", "方正姚体"),
        ("FZLTH", "方正兰亭黑"),
        ("FZLTZHK", "方正兰亭中黑"),
        ("mingliu", "細明體 (MingLiU)"),
        ("kaiu", "標楷體 (KaiU)"),
        // macOS 中文字体
        ("PingFang SC", "苹方-简"),
        ("PingFang TC", "苹方-繁"),
        ("PingFang", "苹方"),
        ("Songti SC", "宋体-简"),
        ("Songti TC", "宋体-繁"),
        ("Heiti SC", "黑体-简"),
        ("Heiti TC", "黑体-繁"),
        ("Kaiti SC", "楷体-简"),
        ("Kaiti TC", "楷体-繁"),
        ("STSong", "华文宋体"),
        ("STKaiti", "华文楷体"),
        ("STFangsong", "华文仿宋"),
        ("STHeiti", "华文黑体"),
        // Noto CJK（跨平台）
        ("NotoSansCJK-Regular", "Noto Sans CJK"),
        ("NotoSansCJKsc-Regular", "Noto Sans CJK SC"),
        ("NotoSansCJKtc-Regular", "Noto Sans CJK TC"),
        ("NotoSerifCJK-Regular", "Noto Serif CJK"),
        ("NotoSerifCJKsc-Regular", "Noto Serif CJK SC"),
        ("NotoSansSC-Regular", "Noto Sans SC"),
        ("NotoSerifSC-Regular", "Noto Serif SC"),
        // 思源字体
        ("SourceHanSans-Regular", "思源黑体"),
        ("SourceHanSerif-Regular", "思源宋体"),
        ("SourceHanSansCN-Regular", "思源黑体 CN"),
        ("SourceHanSerifCN-Regular", "思源宋体 CN"),
    ]
    .iter()
    .cloned()
    .collect();

    let mut roots: Vec<PathBuf> = Vec::new();
    #[cfg(target_os = "windows")]
    {
        roots.push(PathBuf::from("C:\\Windows\\Fonts"));
    }
    #[cfg(target_os = "linux")]
    {
        roots.push(PathBuf::from("/usr/share/fonts"));
        roots.push(PathBuf::from("/usr/local/share/fonts"));
        if let Ok(home) = std::env::var("HOME") {
            roots.push(PathBuf::from(home).join(".fonts"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        roots.push(PathBuf::from("/System/Library/Fonts"));
        roots.push(PathBuf::from("/Library/Fonts"));
        if let Ok(home) = std::env::var("HOME") {
            roots.push(PathBuf::from(home).join("Library/Fonts"));
        }
    }

    let mut files = Vec::new();
    for root in roots {
        if root.exists() {
            collect_font_files(&root, 0, &mut files);
        }
    }

    let mut font_map = HashMap::new();
    for path in files {
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        // 优先用已知友好名，否则对文件名做简单清理
        let name = if let Some(&friendly) = known_names.get(stem) {
            friendly.to_string()
        } else {
            let cleaned = stem.replace(['_', '-'], " ").trim().to_string();
            if cleaned.is_empty() {
                continue;
            }
            cleaned
        };
        font_map
            .entry(name)
            .or_insert_with(|| crate::system_fonts::SystemFont {
                path: path.to_string_lossy().to_string(),
                index: 0,
            });
    }

    // CJK 字体排前面，其余按字母升序
    let is_cjk = is_cjk_font_name;
    let mut names: Vec<String> = font_map.keys().cloned().collect();
    names.sort_by(|a, b| {
        let a_cjk = is_cjk(a);
        let b_cjk = is_cjk(b);
        match (a_cjk, b_cjk) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()),
        }
    });
    (names, font_map)
}

fn default_anim_speed() -> f32 {
    0.14
}

fn default_line_spacing() -> f32 {
    1.8
}

fn default_para_spacing() -> f32 {
    0.6
}

fn default_text_indent() -> u8 {
    2
}

#[cfg(target_os = "macos")]
fn discover_system_fonts() -> (
    Vec<String>,
    HashMap<String, crate::system_fonts::SystemFont>,
) {
    let fonts = crate::system_fonts::discover();
    let mut names: Vec<_> = fonts.keys().cloned().collect();
    names.sort_by_key(|name| (!is_cjk_font_name(name), name.to_lowercase()));
    (names, fonts)
}

fn default_title_font_scale() -> f32 {
    1.5
}

fn generate_pin() -> String {
    use rand::RngCore;
    let val = rand::rngs::OsRng.next_u32() % 10000;
    format!("{:04}", val)
}

#[derive(PartialEq)]
pub enum AppView {
    Library,
    Reader,
    Sync,
}

/// Custom text selection state (replaces egui's native selectable label).
#[derive(Clone, Debug)]
pub struct TextSelection {
    /// Chapter-level block index where the drag started.
    pub start_block: usize,
    /// Char offset within start_block.
    pub start_char: usize,
    /// Chapter-level block index where the drag currently ends.
    pub end_block: usize,
    /// Char offset within end_block.
    pub end_char: usize,
    /// True while the user is still dragging.
    pub is_dragging: bool,
}

impl TextSelection {
    /// Returns (first_block, first_char) regardless of drag direction.
    pub fn normalized(&self) -> (usize, usize) {
        if self.start_block < self.end_block
            || (self.start_block == self.end_block && self.start_char <= self.end_char)
        {
            (self.start_block, self.start_char)
        } else {
            (self.end_block, self.end_char)
        }
    }

    /// Returns (start_block, start_char, end_block, end_char) in order: start <= end.
    pub fn normalized_range(&self) -> (usize, usize, usize, usize) {
        if self.start_block < self.end_block
            || (self.start_block == self.end_block && self.start_char <= self.end_char)
        {
            (
                self.start_block,
                self.start_char,
                self.end_block,
                self.end_char,
            )
        } else {
            (
                self.end_block,
                self.end_char,
                self.start_block,
                self.start_char,
            )
        }
    }
}

pub struct ReaderApp {
    pub reading_service: Option<crate::ui::reading_services::ServicePanel>,
    pub pending_batch_import: Option<std::sync::mpsc::Receiver<Vec<String>>>,
    pub(crate) scripts_enabled: bool,
    pub(crate) native_events:
        std::collections::HashMap<usize, Vec<reader_core::epub::native::NativeEvent>>,
    pub(crate) native_applied:
        std::collections::HashMap<usize, (reader_core::epub::native::CssPolicy, bool, usize)>,
    pub(crate) native_book_path: Option<String>,
    pub(crate) publication_note: Option<reader_core::epub::native::NativeNote>,
    pub(crate) css_policy: reader_core::epub::native::CssPolicy,
    pub data_dir: String,
    pub book: Option<EpubBook>,
    pub book_path: Option<String>,
    pub current_book_hash: Option<String>,
    pub last_synced_chapter: Option<usize>,
    pub last_synced_block: Option<usize>,
    pub(crate) last_synced_char_offset: Option<usize>,
    pub current_chapter: usize,
    pub current_block: usize,
    pub(crate) current_char_offset: usize,
    pub pending_restore_block: Option<usize>,
    pub position_save_due: Option<std::time::Instant>,
    pub position_save_modified_ms: u64,
    pub font_size: f32,
    pub dark_mode: bool,
    pub appearance_linkage: reader_core::appearance::AppearanceLinkage,
    pub appearance_paints: crate::ui::appearance_linkage::ReadingPaintPair,
    pub reading_themes: crate::ui::reading_themes::ReadingThemes,
    pub reading_theme_editor: Option<crate::ui::reading_themes::ReadingThemeDraft>,
    pub appearance_current_dark: bool,
    appearance_transition: crate::ui::appearance_transition::AppearanceTransition,
    pub system_appearance_dark: bool,
    pub show_appearance_linkage: bool,
    pub appearance_selected_node: usize,
    pub reader_bg_color: Color32,
    pub reader_day_palette: Option<(Color32, Option<Color32>)>,
    pub reader_font_color: Option<Color32>,
    pub reader_font_family: String,
    pub reader_cjk_font_family: String,
    pub reader_page_animation: String,
    pub reader_page_animation_speed: f32,
    pub reader_bg_image_path: Option<String>,
    pub reader_bg_image_alpha: f32,
    pub reader_bg_texture: Option<egui::TextureHandle>,
    pub show_settings: bool,
    pub(crate) reader_introduction_seen: bool,
    pub(crate) show_reader_introduction: bool,
    pub confirm_display_reset: bool,
    pub show_csc_settings: bool,
    pub show_toc: bool,
    pub show_app_navigation: bool,
    pub scroll_to_top: bool,
    pub error_msg: Option<String>,
    pub view: AppView,
    pub pending_book_open: Option<PendingBookOpen>,
    pub(crate) pending_open_cancel: Option<reader_core::open_operation::Cancellation>,
    pub opening_started: Option<std::time::Instant>,
    pub(crate) book_presentation: crate::ui::book_presentation::BookPresentation,
    pub library_manage: bool,
    pub library_selected: std::collections::HashSet<String>,
    pub library_confirm_delete: bool,
    pub library_show_archive: bool,
    pub library_archive_delete: Option<reader_core::cloud_sync::CloudBook>,
    pub library_query: String,
    pub library_sort_title: bool,
    pub library_list_mode: bool,
    pub sync_tab: usize,
    pub library: Library,
    pub scroll_mode: bool,
    pub(crate) continuous_scroll: ContinuousScrollState,
    pub current_page: usize,
    pub total_pages: usize,
    pub(crate) reader_render_blocks: Option<(usize, Arc<Vec<reader_core::epub::ContentBlock>>)>,
    pub page_block_ranges: crate::ui::reader_pagination::PageRanges,
    pub pages_dirty: bool,
    pub cover_textures: HashMap<String, Option<egui::TextureHandle>>,
    pub(crate) shelf_covers: crate::ui::shelf_covers::ShelfCovers,
    pub last_avail_width: f32,
    pub last_avail_height: f32,
    pub page_anim_from: usize,
    pub page_anim_to: usize,
    pub page_anim_progress: f32,
    pub page_anim_direction: f32,
    pub page_anim_cross_chapter: bool,
    pub page_anim_cross_chapter_snapshot: Option<CrossChapterSnapshot>,
    pub(crate) reader_layout_anchor: Option<(usize, usize)>,
    pub(crate) panel_reflow_anchor: Option<(usize, usize)>,
    pub(crate) page_curl: crate::ui::curl_reader::PageCurlState,
    pub(crate) reader_chrome: crate::ui::reader_chrome::ReaderChromeState,
    pub(crate) reader_navigation: crate::ui::reader_navigation::ReaderNavigation,
    pub is_dual_column: bool,
    pub force_single_column: bool,
    pub paging_page_rect: Option<egui::Rect>,
    pub embedded_font_names: Vec<String>,
    pub embedded_fonts_registered: bool,
    pub defer_custom_font_for_frame: bool,
    /// Font family the user originally chose; kept so we can retry after async
    /// font discovery completes (the fallback may have temporarily reset
    /// `reader_font_family` to "Sans").
    pending_font_family: Option<String>,
    pending_cjk_font_family: Option<String>,
    pub system_font_names: Vec<String>,
    pub(crate) system_font_paths: HashMap<String, crate::system_fonts::SystemFont>,
    font_discovery_result: FontDiscoveryResult,
    last_saved_settings: Option<AppSettings>,
    pub font_search: String,
    pub cjk_font_search: String,
    pub i18n: I18n,
    language_checked: std::time::Instant,
    pub scroll_toc_to_current: bool,
    // ── Sharing ──
    pub auto_start_sharing: bool,
    pub peer_store: Arc<Mutex<PeerStore>>,
    pub show_sharing_panel: bool,
    pub sharing_server_running: bool,
    pub sharing_server_addr: String,
    pub sharing_pin: String,
    pub connect_addr_input: String,
    pub connect_pin_input: String,
    /// When Some, a pairing dialog is shown for this peer
    pub pairing_dialog_peer: Option<DiscoveredPeer>,
    pub pairing_dialog_pin: String,
    pub sharing_status: Arc<Mutex<String>>,
    pub server_stop_flag: Arc<AtomicBool>,
    /// Held to keep the discovery listener thread alive.
    #[allow(dead_code)]
    pub discovery_stop_flag: Arc<AtomicBool>,
    pub discovered_peers: Arc<Mutex<Vec<DiscoveredPeer>>>,
    pub pending_sync_updates: Arc<Mutex<Vec<reader_core::sharing::CommittedProgress>>>,
    pub pending_library_reload: Arc<AtomicBool>,
    pub shared_book_paths: Arc<Mutex<Vec<String>>>,
    pub feedback_logs: Arc<Mutex<Vec<String>>>,
    pub show_feedback_github_prompt: bool,
    pub last_exported_feedback_log: Option<String>,
    pub show_about: bool,
    pub show_language: bool,
    pub about_icon_texture: Option<egui::TextureHandle>,
    // ── Self-update ──
    pub update_state: UpdateState,
    pub show_update_dialog: bool,
    pub update_latest_tag: Option<String>,
    pub _update_check_slot: Option<Arc<Mutex<Option<UpdateState>>>>,
    pub _update_download_slot: Option<Arc<Mutex<Option<UpdateState>>>>,
    pub _update_progress: Option<Arc<Mutex<f32>>>,
    // ── TXT Import ──
    pub txt_import: Option<TxtImportState>,
    // ── Typography ──
    pub line_spacing: f32,
    pub para_spacing: f32,
    pub text_indent: u8,
    pub title_font_scale: f32,
    // ── Search ──
    pub show_search: bool,
    pub search_query: String,
    #[cfg(feature = "performance-checks")]
    pub(crate) reader_performance: Option<crate::performance::ReaderPerformance>,
    #[cfg(feature = "performance-checks")]
    pub(crate) native_corpus: Option<crate::native_corpus::NativeCorpus>,
    #[cfg(feature = "performance-checks")]
    pub(crate) shelf_performance: Option<crate::shelf_performance::ShelfPerformance>,
    pub(crate) search_task: Option<crate::ui::search::SearchTask>,
    pub search_results: Vec<reader_core::search::SearchResult>,
    pub search_selected: Option<usize>,
    // ── Annotations ──
    pub show_annotations: bool,
    pub(crate) annotations_panel: crate::ui::annotations::PanelState,
    pub(crate) annotation_export: Option<crate::ui::annotations::AnnotationExportTask>,
    pub(crate) backup: crate::ui::backup::BackupState,
    pub book_config: Option<reader_core::library::BookConfig>,
    // ── Export ──
    pub show_export_dialog: bool,
    pub book_share: crate::ui::book_share::ShareUI,
    pub share_paths: Vec<String>,
    pub share_mode: reader_core::export::ExportMode,
    pub rename_book: Option<(String, String)>,
    #[allow(dead_code)]
    pub export_book_id: Option<String>,
    // ── Stats ──
    pub show_stats: bool,
    pub activity: crate::ui::stats::DesktopActivity,
    // ── Auto-scroll ──
    #[allow(dead_code)]
    pub auto_scroll: bool,
    pub auto_scroll_speed: f32,
    // ── Library export ──
    pub export_library_path: Option<String>,
    // ── Custom text selection ──
    pub text_selection: Option<TextSelection>,
    pub sel_toolbar_pos: egui::Pos2,
    /// Pending drag origin: (press_pos, block_idx, char_idx). Created on press, promoted to
    /// TextSelection only when pointer moves > threshold. Cleared on release if no drag occurred.
    pub sel_press_origin: Option<(egui::Pos2, usize, usize)>,
    /// When set, the user clicked on a highlighted region → show note popup for this highlight.
    pub clicked_highlight_id: Option<String>,
    pub hl_note_toolbar_pos: egui::Pos2,
    pub hl_note_just_opened: bool,
    /// Note editing in annotations panel: highlight id being edited
    pub editing_note_id: Option<String>,
    pub editing_note_buf: String,
    pub editing_note_original: Option<String>,
    // ── TTS ──
    pub tts_playing: bool,
    pub tts_paused: bool,
    pub tts_voice_name: String,
    pub tts_rate: i32,   // e.g. 0, -20, +50 (percent)
    pub tts_volume: i32, // e.g. 0, -50, +50 (percent)
    pub tts_current_chapter: usize,
    pub tts_current_block: usize,
    pub tts_current_char: usize,
    pub tts_stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub tts_generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub tts_audio_sink: Option<std::sync::Arc<rodio::Sink>>,
    pub tts_output_stream: Option<rodio::OutputStream>,
    pub tts_output_handle: Option<rodio::OutputStreamHandle>,
    pub tts_status: std::sync::Arc<std::sync::Mutex<String>>,
    pub show_tts_panel: bool,
    pub tts_pending_audio: Option<TtsAudioResultSlot>,
    /// Prefetched audio for the next block (ready to play immediately when current finishes).
    pub tts_prefetch_audio: Option<TtsAudioResultSlot>,
    /// Chapter and block that the prefetch corresponds to.
    pub tts_prefetch_chapter: usize,
    pub tts_prefetch_block: usize,
    /// Keep the reader viewport aligned with the current TTS block.
    pub tts_follow_view: bool,
    /// Prevent TTS-driven page/chapter navigation from detaching the viewport.
    pub tts_syncing_navigation: bool,
    pub last_egui_ctx: Option<egui::Context>,
    // ── API Settings ──
    pub translate_api_url: String,
    pub translate_api_model: String,
    pub translate_api_provider: String,
    pub translate_api_key: String,
    pub dictionary_api_url: String,
    pub dictionary_api_model: String,
    pub dictionary_api_provider: String,
    pub dictionary_api_key: String,

    // ── CSC (Chinese Spelling Correction) ──
    pub csc_mode: reader_core::csc::CorrectionMode,
    pub csc_threshold: reader_core::csc::CscThreshold,
    pub csc_model_status: reader_core::csc::ModelStatus,
    pub csc_download_progress: std::sync::Arc<std::sync::Mutex<f32>>,
    pub csc_engine: std::sync::Arc<std::sync::Mutex<Option<reader_core::csc::CscEngine>>>,
    /// Cached correction results: (chapter, block_idx) → Vec<CorrectionInfo>
    pub csc_cache:
        std::collections::HashMap<(usize, usize), Vec<reader_core::epub::CorrectionInfo>>,
    /// Channel to send work to the CSC background worker thread.
    pub csc_work_tx: Option<std::sync::mpsc::Sender<CscWork>>,
    /// Channel to receive results from the CSC background worker thread.
    pub csc_result_rx: Option<std::sync::mpsc::Receiver<CscResult>>,
    /// Active CSC correction popup (for accept / revert / ignore).
    pub csc_popup: Option<CscPopupInfo>,
    /// Buffer for custom CSC replacement text input.
    pub csc_custom_replace_buf: String,
    /// Whether the custom replace popup is shown (selection-based).
    pub csc_custom_replace_active: bool,
    // ── GitHub OAuth ──
    pub github_sync_enabled: bool,
    pub github_sync_contributions: bool,
    pub cloud_sync: crate::ui::github_sync::CloudSyncState,
    pub github_oauth_scope: String,
    pub github_repo_authorized: bool,
    pub github_enable_sync_after_login: bool,
    pub github_token: Option<String>,
    pub github_username: Option<String>,
    pub github_device_code: Option<String>,
    pub github_user_code: Option<String>,
    pub github_oauth_polling: bool,
    pub github_oauth_interval: u64,
    pub github_oauth_expires_at: Option<std::time::Instant>,
    pub show_github_login: bool,
    pub github_oauth_status: String,
    // GitHub OAuth Device Flow async channels
    pub github_pending_device_code: Option<std::sync::mpsc::Receiver<DeviceCodeResult>>,
    pub github_pending_token_poll:
        Option<std::sync::mpsc::Receiver<Result<crate::ui::github_oauth::PollResult, String>>>,
    pub github_last_poll: Option<std::time::Instant>,
    // ── CSC Contribution ──
    pub show_csc_contribute_dialog: bool,
    pub csc_contribute_prompted: bool,
    pub csc_contribute_dismissed: bool,
    pub csc_contribute_in_progress: bool,
    pub csc_contribution_samples: Option<Vec<reader_core::csc::contribution::Sample>>,
    pub csc_contribute_status: String,
    pub csc_contribute_pr_url: Option<String>,
    pub csc_contribute_rx:
        Option<std::sync::mpsc::Receiver<crate::ui::csc_contribute::ContributeResult>>,
    // ── Review Panel (段评覆盖层) ──
    pub show_review_panel: bool,
    pub review_panel_chapter: Option<usize>,
    pub review_panel_anchor: Option<String>,
    pub review_panel_just_opened: bool,
    /// Computed scroll offset for the current anchor. Applied once when opening.
    pub review_panel_scroll_offset: Option<f32>,
    /// When true, show all reviews; when false, filter to the anchored paragraph only.
    pub review_panel_show_all: bool,
}

#[derive(Debug, Clone, Default)]
pub enum UpdateState {
    #[default]
    Idle,
    Checking,
    Available(String),
    Downloading,
    UpToDate,
    Failed(String),
    Restarting,
    DownloadPageOpened,
    CheckFailed(String),
}

impl UpdateState {
    pub(crate) fn from_update_check(result: anyhow::Result<Option<(String, String)>>) -> Self {
        match result {
            Ok(Some((tag, _))) => Self::Available(tag),
            Ok(None) => Self::UpToDate,
            Err(error) => Self::CheckFailed(format!("{error:#}")),
        }
    }
}

#[derive(Clone)]
pub struct CrossChapterSnapshot {
    pub chapter: usize,
    pub blocks: Arc<Vec<reader_core::epub::ContentBlock>>,
    pub block_ranges: crate::ui::reader_pagination::PageRanges,
    pub total_pages: usize,
    pub from_page: usize,
    pub title: String,
}

pub type TxtConvertSlot = Arc<Mutex<Option<Result<reader_core::txt::ConvertResult, String>>>>;

#[derive(Clone, Default)]
pub enum TxtImportPage {
    #[default]
    Review,
    Rules,
    Source,
    Excerpt(reader_core::txt::ChapterPreview),
}

/// TXT 导入对话框状态。
pub struct TxtImportState {
    pub page: TxtImportPage,
    pub txt_path: PathBuf,
    pub title: String,
    pub author: String,
    pub custom_regex: String,
    pub use_heuristic: bool,
    pub previews: Vec<reader_core::txt::ChapterPreview>,
    pub excluded: std::collections::BTreeSet<usize>,
    pub included: std::collections::BTreeSet<usize>,
    pub heading_count: usize,
    pub chapter_count: usize,
    pub source_query: String,
    pub source_hits: Vec<reader_core::txt::ChapterPreview>,
    pub pending: Option<(
        String,
        std::sync::mpsc::Receiver<Result<serde_json::Value, String>>,
    )>,
    pub analyzed: bool,
    pub converting: bool,
    pub error: Option<String>,
    /// 后台转换结果回传。
    pub result_slot: Option<TxtConvertSlot>,
}

impl TxtImportState {
    pub fn new(txt_path: PathBuf) -> Self {
        let title = txt_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        let (previews, error) = (Vec::new(), None);

        Self {
            page: TxtImportPage::Review,
            txt_path,
            title,
            author: String::new(),
            custom_regex: String::new(),
            use_heuristic: true,
            previews,
            excluded: Default::default(),
            included: Default::default(),
            heading_count: 0,
            chapter_count: 0,
            source_query: String::new(),
            source_hits: Vec::new(),
            pending: None,
            analyzed: false,
            converting: false,
            error,
            result_slot: None,
        }
    }
}

impl Default for ReaderApp {
    fn default() -> Self {
        Self::from_data_dir(default_data_dir())
    }
}

impl ReaderApp {
    pub(crate) fn reload_library(&mut self) {
        match Library::load_from(&self.data_dir) {
            Ok(library) => self.library = library,
            Err(error) => self.error_msg = Some(error),
        }
    }

    pub(crate) fn from_data_dir(data_dir: String) -> Self {
        let (library, library_error) = match Library::load_from(&data_dir) {
            Ok(library) => (library, None),
            Err(error) => (Library::default(), Some(error)),
        };
        let font_discovery_result: FontDiscoveryResult = Arc::new(Mutex::new(None));
        {
            let result_slot = font_discovery_result.clone();
            let font_data_dir = data_dir.clone();
            std::thread::spawn(move || {
                let (mut names, mut paths) = discover_system_fonts();
                crate::ui::font_library::add_installed(&font_data_dir, &mut names, &mut paths);
                let fonts = (names, paths);
                if let Ok(mut slot) = result_slot.lock() {
                    *slot = Some(fonts);
                }
            });
        }
        let peer_store = PeerStore::load(&data_dir);
        let pin = generate_pin();
        let own_device_id = peer_store.device_id.clone();
        let discovery_stop_flag = Arc::new(AtomicBool::new(false));
        let discovered_peers = start_listener(&own_device_id, discovery_stop_flag.clone());
        let mut app = Self {
            reading_service: None,
            pending_batch_import: None,
            data_dir,
            book: None,
            scripts_enabled: true,
            native_events: Default::default(),
            native_applied: Default::default(),
            native_book_path: None,
            publication_note: None,
            css_policy: Default::default(),
            book_path: None,
            current_book_hash: None,
            last_synced_chapter: None,
            last_synced_block: None,
            last_synced_char_offset: None,
            current_chapter: 0,
            current_block: 0,
            current_char_offset: 0,
            pending_restore_block: None,
            position_save_due: None,
            position_save_modified_ms: 0,
            font_size: 16.0,
            dark_mode: true,
            appearance_linkage: Default::default(),
            appearance_paints: Default::default(),
            reading_themes: Default::default(),
            reading_theme_editor: None,
            appearance_current_dark: false,
            appearance_transition: Default::default(),
            system_appearance_dark: false,
            show_appearance_linkage: false,
            appearance_selected_node: 0,
            reader_bg_color: Color32::from_rgb(250, 246, 238),
            reader_day_palette: None,
            reader_font_color: None,
            reader_font_family: "Sans".to_string(),
            reader_cjk_font_family: default_sans(),
            reader_page_animation: "Realistic".to_string(),
            reader_page_animation_speed: 0.14,
            reader_bg_image_path: None,
            reader_bg_image_alpha: 0.22,
            reader_bg_texture: None,
            show_settings: false,
            reader_introduction_seen: false,
            show_reader_introduction: false,
            confirm_display_reset: false,
            show_csc_settings: false,
            show_toc: true,
            show_app_navigation: true,
            scroll_to_top: false,
            error_msg: library_error,
            view: AppView::Library,
            pending_book_open: None,
            pending_open_cancel: None,
            opening_started: None,
            book_presentation: Default::default(),
            library_manage: false,
            library_selected: Default::default(),
            library_confirm_delete: false,
            library_show_archive: false,
            library_archive_delete: None,
            library_query: String::new(),
            library_sort_title: false,
            library_list_mode: false,
            sync_tab: 0,
            library,
            scroll_mode: false,
            continuous_scroll: ContinuousScrollState::default(),
            current_page: 0,
            total_pages: 0,
            reader_render_blocks: None,
            page_block_ranges: Default::default(),
            pages_dirty: true,
            cover_textures: HashMap::new(),
            shelf_covers: Default::default(),
            last_avail_width: 0.0,
            last_avail_height: 0.0,
            page_anim_from: 0,
            page_anim_to: 0,
            page_anim_progress: 1.0,
            page_anim_direction: 1.0,
            page_anim_cross_chapter: false,
            page_anim_cross_chapter_snapshot: None,
            reader_layout_anchor: None,
            panel_reflow_anchor: None,
            page_curl: Default::default(),
            reader_chrome: Default::default(),
            reader_navigation: Default::default(),
            is_dual_column: false,
            force_single_column: false,
            paging_page_rect: None,
            embedded_font_names: Vec::new(),
            embedded_fonts_registered: true,
            defer_custom_font_for_frame: false,
            pending_font_family: None,
            pending_cjk_font_family: None,
            system_font_names: Vec::new(),
            system_font_paths: HashMap::new(),
            font_discovery_result,
            last_saved_settings: None,
            font_search: String::new(),
            cjk_font_search: String::new(),
            i18n: I18n::default(),
            language_checked: std::time::Instant::now(),
            scroll_toc_to_current: false,
            // Sharing
            auto_start_sharing: false,
            peer_store: Arc::new(Mutex::new(peer_store)),
            show_sharing_panel: false,
            sharing_server_running: false,
            sharing_server_addr: String::new(),
            sharing_pin: pin,
            connect_addr_input: String::new(),
            connect_pin_input: String::new(),
            pairing_dialog_peer: None,
            pairing_dialog_pin: String::new(),
            sharing_status: Arc::new(Mutex::new(String::new())),
            server_stop_flag: Arc::new(AtomicBool::new(false)),
            discovery_stop_flag,
            discovered_peers,
            pending_sync_updates: Arc::new(Mutex::new(Vec::new())),
            pending_library_reload: Arc::new(AtomicBool::new(false)),
            shared_book_paths: Arc::new(Mutex::new(Vec::new())),
            feedback_logs: Arc::new(Mutex::new(Vec::new())),
            show_feedback_github_prompt: false,
            last_exported_feedback_log: None,
            show_about: false,
            show_language: false,
            about_icon_texture: None,
            update_state: UpdateState::Idle,
            show_update_dialog: false,
            update_latest_tag: None,
            _update_check_slot: None,
            _update_download_slot: None,
            _update_progress: None,
            txt_import: None,
            // Typography
            line_spacing: default_line_spacing(),
            para_spacing: default_para_spacing(),
            text_indent: default_text_indent(),
            title_font_scale: default_title_font_scale(),
            // Search
            show_search: false,
            search_query: String::new(),
            #[cfg(feature = "performance-checks")]
            reader_performance: None,
            #[cfg(feature = "performance-checks")]
            native_corpus: None,
            #[cfg(feature = "performance-checks")]
            shelf_performance: None,
            search_task: None,
            search_results: Vec::new(),
            search_selected: None,
            // Annotations
            show_annotations: false,
            annotations_panel: Default::default(),
            annotation_export: None,
            backup: Default::default(),
            book_config: None,
            // Export
            show_export_dialog: false,
            book_share: Default::default(),
            share_paths: Vec::new(),
            share_mode: reader_core::export::ExportMode::Original,
            rename_book: None,
            export_book_id: None,
            // Stats
            show_stats: false,
            activity: Default::default(),
            // Auto-scroll
            auto_scroll: false,
            auto_scroll_speed: 30.0,
            export_library_path: None,
            text_selection: None,
            sel_toolbar_pos: egui::Pos2::ZERO,
            sel_press_origin: None,
            clicked_highlight_id: None,
            hl_note_toolbar_pos: egui::Pos2::ZERO,
            hl_note_just_opened: false,
            editing_note_id: None,
            editing_note_buf: String::new(),
            editing_note_original: None,
            // TTS
            tts_playing: false,
            tts_paused: false,
            tts_voice_name: "zh-CN-XiaoxiaoNeural".to_string(),
            tts_rate: 0,
            tts_volume: 0,
            tts_current_chapter: 0,
            tts_current_block: 0,
            tts_current_char: 0,
            tts_stop_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            tts_generation: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            tts_audio_sink: None,
            tts_output_stream: None,
            tts_output_handle: None,
            tts_status: std::sync::Arc::new(std::sync::Mutex::new(String::new())),
            show_tts_panel: false,
            tts_pending_audio: None,
            tts_prefetch_audio: None,
            tts_prefetch_chapter: 0,
            tts_prefetch_block: 0,
            tts_follow_view: false,
            tts_syncing_navigation: false,
            last_egui_ctx: None,
            // API settings
            translate_api_url: String::new(),
            translate_api_model: "gpt-4o-mini".into(),
            translate_api_provider: "openai".into(),
            translate_api_key: String::new(),
            dictionary_api_url: String::new(),
            dictionary_api_model: "gpt-4o-mini".into(),
            dictionary_api_provider: "openai".into(),
            dictionary_api_key: String::new(),
            // CSC
            csc_mode: reader_core::csc::CorrectionMode::None,
            csc_threshold: reader_core::csc::CscThreshold::Standard,
            csc_model_status: reader_core::csc::ModelStatus::NotDownloaded,
            csc_download_progress: std::sync::Arc::new(std::sync::Mutex::new(0.0)),
            csc_engine: std::sync::Arc::new(std::sync::Mutex::new(None)),
            csc_cache: std::collections::HashMap::new(),
            csc_work_tx: None,
            csc_result_rx: None,
            csc_popup: None,
            csc_custom_replace_buf: String::new(),
            csc_custom_replace_active: false,
            // GitHub OAuth
            github_sync_enabled: false,
            github_sync_contributions: false,
            cloud_sync: Default::default(),
            github_oauth_scope: "public_repo".into(),
            github_repo_authorized: false,
            github_enable_sync_after_login: false,
            github_token: None,
            github_username: None,
            github_device_code: None,
            github_user_code: None,
            github_oauth_polling: false,
            github_oauth_interval: 5,
            github_oauth_expires_at: None,
            show_github_login: false,
            github_oauth_status: String::new(),
            github_pending_device_code: None,
            github_pending_token_poll: None,
            github_last_poll: None,
            // CSC Contribution
            show_csc_contribute_dialog: false,
            csc_contribute_prompted: false,
            csc_contribute_dismissed: false,
            csc_contribute_in_progress: false,
            csc_contribution_samples: None,
            csc_contribute_status: String::new(),
            csc_contribute_pr_url: None,
            csc_contribute_rx: None,
            // Review Panel
            show_review_panel: false,
            review_panel_chapter: None,
            review_panel_anchor: None,
            review_panel_just_opened: false,
            review_panel_scroll_offset: None,
            review_panel_show_all: true,
        };

        if let Some(settings) = AppSettings::load(&app.data_dir) {
            app.scripts_enabled = settings.scripts_enabled;
            app.css_policy = settings.css_policy;
            app.push_feedback_log(format!("[Init] settings loaded: lang={}, font={}, font_size={}, scroll_mode={}, last_book={:?}, ch={}",
                settings.language, settings.reader_font_family, settings.font_size, settings.scroll_mode,
                settings.last_book_path.as_deref().unwrap_or("none"), settings.last_chapter));
            settings.apply_to_app(&mut app);
            app.embedded_fonts_registered = false;
            app.pages_dirty = true;
            // Restore github token
            if app.github_username.is_some() {
                let has_token = app.github_token.is_some();
                app.push_feedback_log(format!(
                    "[Init] github_username={:?}, token_restored={}",
                    app.github_username, has_token
                ));
            }
            // 自动恢复上次阅读的书籍
            if let Some(ref path) = settings.last_book_path.clone() {
                if std::path::Path::new(path).exists() {
                    app.push_feedback_log(format!("[Init] restoring last book: {}", path));
                    app.open_book_from_path(path, Some(settings.last_chapter));
                } else {
                    app.push_feedback_log(format!("[Init] last book not found: {}", path));
                }
            }
        } else {
            app.push_feedback_log("[Init] no saved settings found, using defaults");
        }
        // 检查CSC模型状态
        app.csc_check_model_status();
        // Auto-load CSC model if downloaded and correction mode enabled
        #[cfg(feature = "csc")]
        if app.csc_model_status == reader_core::csc::ModelStatus::Downloaded
            && app.csc_mode != reader_core::csc::CorrectionMode::None
        {
            app.push_feedback_log("[Init] auto-loading CSC model");
            app.csc_load_model();
        }
        if app.auto_start_sharing {
            app.push_feedback_log("[Init] auto-starting sharing server");
            app.start_sharing_server();
        }
        app.push_feedback_log(format!(
            "[Init] app initialized (data_dir={})",
            app.data_dir
        ));
        app.last_saved_settings = Some(AppSettings::from_app(&app));

        // ── 启动时检查更新 ──
        #[cfg(not(test))]
        {
            let slot: Arc<Mutex<Option<UpdateState>>> = Arc::new(Mutex::new(None));
            let slot_clone = slot.clone();
            std::thread::spawn(move || {
                let result =
                    UpdateState::from_update_check(crate::self_update::check_latest_version());
                if let Ok(mut s) = slot_clone.lock() {
                    *s = Some(result);
                }
            });
            app._update_check_slot = Some(slot);
            app.update_state = UpdateState::Checking;
        }
        app
    }
}

impl ReaderApp {
    pub fn trigger_page_animation_to(&mut self, target_page: usize, direction: f32) {
        if self.reader_page_animation == "Realistic" && !self.scroll_mode {
            self.request_curl_turn(direction >= 0., Some(target_page));
            return;
        }
        let page_changed = target_page != self.current_page;
        if self.reader_page_animation == "None"
            || self.scroll_mode
            || target_page == self.current_page
        {
            self.current_page = target_page;
            self.page_anim_progress = 1.0;
            self.page_anim_from = target_page;
            self.page_anim_to = target_page;
            if page_changed {
                self.tts_detach_view();
            }
            return;
        }

        self.page_anim_from = self.current_page;
        self.page_anim_to = target_page;
        self.page_anim_direction = if direction == 0.0 {
            1.0
        } else {
            direction.signum()
        };
        self.page_anim_progress = 0.0;
        self.page_anim_cross_chapter = false;
        self.current_page = target_page;
        self.tts_detach_view();
    }

    pub fn ensure_reader_bg_texture(&mut self, ctx: &egui::Context) {
        if self.reader_bg_texture.is_some() {
            return;
        }
        let Some(path) = &self.reader_bg_image_path else {
            return;
        };

        if let Ok(img) = image::open(path) {
            let rgba = img.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
            let tex = ctx.load_texture("reader_bg", color_image, egui::TextureOptions::LINEAR);
            self.reader_bg_texture = Some(tex);
        }
    }

    pub fn open_file_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("EPUB / TXT", &["epub", "txt"])
            .add_filter("EPUB", &["epub"])
            .add_filter("TXT", &["txt"])
            .pick_files()
        {
            if path.len() > 1 {
                self.import_epub_batch(path);
                return;
            }
            let Some(path) = path.into_iter().next() else {
                return;
            };
            let is_txt = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("txt"))
                .unwrap_or(false);

            if is_txt {
                self.txt_import = Some(TxtImportState::new(path));
            } else {
                let path_str = path.to_string_lossy().to_string();
                self.open_book_from_path(&path_str, None);
            }
        }
    }

    pub fn total_chapters(&self) -> usize {
        self.book.as_ref().map(|b| b.chapters.len()).unwrap_or(0)
    }

    pub fn next_chapter(&mut self) {
        let total = self.total_chapters();
        if total > 0 && self.current_chapter < total - 1 {
            self.current_chapter += 1;
            self.scroll_to_top = true;
            self.pages_dirty = true;
            self.current_page = 0;
            self.current_block = 0;
            self.current_char_offset = 0;
            self.pending_restore_block = None;
            self.position_save_modified_ms = reader_core::now_millis();
            self.position_save_due = Some(std::time::Instant::now());
            if let Err(error) = self.flush_reading_position_for_backup() {
                self.error_msg = Some(error);
            }
            // Trigger CSC for this chapter + prefetch prev & next
            self.csc_trigger_chapter(self.current_chapter);
            if self.current_chapter > 0 {
                self.csc_trigger_chapter(self.current_chapter - 1);
            }
            if self.current_chapter + 1 < total {
                self.csc_trigger_chapter(self.current_chapter + 1);
            }
            // Check if user should be prompted to contribute
            self.csc_check_contribution_prompt();
            self.tts_detach_view();
        }
    }

    pub fn prev_chapter(&mut self) {
        if self.current_chapter > 0 {
            self.current_chapter -= 1;
            self.scroll_to_top = true;
            self.pages_dirty = true;
            self.current_page = 0;
            self.current_block = 0;
            self.current_char_offset = 0;
            self.pending_restore_block = None;
            self.position_save_modified_ms = reader_core::now_millis();
            self.position_save_due = Some(std::time::Instant::now());
            if let Err(error) = self.flush_reading_position_for_backup() {
                self.error_msg = Some(error);
            }
            // Trigger CSC for this chapter + prefetch prev & next
            let total = self.total_chapters();
            self.csc_trigger_chapter(self.current_chapter);
            if self.current_chapter > 0 {
                self.csc_trigger_chapter(self.current_chapter - 1);
            }
            if self.current_chapter + 1 < total {
                self.csc_trigger_chapter(self.current_chapter + 1);
            }
            // Check if user should be prompted to contribute
            self.csc_check_contribution_prompt();
            self.tts_detach_view();
        }
    }

    // ── CSC background processing ──

    /// Spawn a background worker thread for CSC inference.
    /// The worker owns the engine lock and processes chapters sent via channel.
    pub fn csc_spawn_worker(&mut self) {
        let (work_tx, work_rx) = std::sync::mpsc::channel::<CscWork>();
        let (result_tx, result_rx) = std::sync::mpsc::channel::<CscResult>();
        let engine = self.csc_engine.clone();
        let logs = self.feedback_logs.clone();

        std::thread::spawn(move || {
            while let Ok(work) = work_rx.recv() {
                let start = std::time::Instant::now();
                let mut corrections = Vec::new();
                if let Ok(mut guard) = engine.lock() {
                    if let Some(ref mut eng) = *guard {
                        eng.mode = work.mode;
                        eng.threshold = work.threshold;
                        for (block_idx, text) in &work.blocks {
                            let corrs = eng.check(text);
                            if !corrs.is_empty() {
                                corrections.push((*block_idx, corrs));
                            }
                        }
                    }
                }
                let elapsed = start.elapsed().as_secs_f32();
                let total: usize = corrections.iter().map(|(_, c)| c.len()).sum();
                dbg_log(
                    &logs,
                    format!(
                        "[CSC] chapter {} done: {:.1}s, {} corrections in {} blocks",
                        work.chapter,
                        elapsed,
                        total,
                        corrections.len(),
                    ),
                );
                let _ = result_tx.send(CscResult {
                    chapter: work.chapter,
                    corrections,
                });
            }
        });

        self.csc_work_tx = Some(work_tx);
        self.csc_result_rx = Some(result_rx);
    }

    /// Queue a chapter for background CSC processing.
    /// Skips if mode is None, engine not ready, or chapter already cached.
    pub fn csc_trigger_chapter(&mut self, chapter_idx: usize) {
        use reader_core::csc::CorrectionMode;
        use reader_core::epub::ContentBlock;

        if self.csc_mode == CorrectionMode::None {
            return;
        }
        // Check engine is loaded
        {
            let guard = self.csc_engine.lock().unwrap();
            if guard.is_none() || !guard.as_ref().unwrap().is_ready() {
                return;
            }
        }
        // Skip if already cached
        if self.csc_cache.keys().any(|(ch, _)| *ch == chapter_idx) {
            return;
        }

        // Extract block texts
        let block_texts: Vec<(usize, String)> = if let Some(book) = &self.book {
            if let Ok(chapter) = book.chapter(chapter_idx) {
                chapter
                    .blocks
                    .iter()
                    .enumerate()
                    .filter_map(|(i, block)| {
                        let spans = match block {
                            ContentBlock::Paragraph { spans, .. } => spans,
                            ContentBlock::Heading { spans, .. } => spans,
                            _ => return None,
                        };
                        let text: String = spans.iter().map(|s| s.text.as_str()).collect();
                        if text.trim().is_empty() {
                            None
                        } else {
                            Some((i, text))
                        }
                    })
                    .collect()
            } else {
                return;
            }
        } else {
            return;
        };

        if block_texts.is_empty() {
            return;
        }

        self.push_feedback_log(format!(
            "[CSC] trigger ch {} ({} text blocks)",
            chapter_idx,
            block_texts.len(),
        ));

        if let Some(tx) = &self.csc_work_tx {
            let _ = tx.send(CscWork {
                chapter: chapter_idx,
                blocks: block_texts,
                mode: self.csc_mode.clone(),
                threshold: self.csc_threshold.clone(),
            });
        }
    }

    /// Poll CSC worker results and merge into cache.
    pub fn csc_poll_results(&mut self) {
        if let Some(rx) = &self.csc_result_rx {
            while let Ok(result) = rx.try_recv() {
                for (block_idx, corrections) in result.corrections {
                    self.csc_cache
                        .insert((result.chapter, block_idx), corrections);
                }
            }
        }
    }

    /// Save current book_config to disk.
    #[allow(dead_code)]
    pub fn save_book_config(&mut self) {
        if let Some(cfg) = &mut self.book_config {
            if let Err(error) = cfg.save(&self.data_dir) {
                self.error_msg = Some(error.to_string());
            };
        }
    }

    /// Capture a snapshot of the current chapter state for cross-chapter animation.
    pub fn capture_cross_chapter_snapshot(&mut self) {
        if self.scroll_mode || self.reader_page_animation == "None" {
            return;
        }
        if let Some(book) = &self.book {
            if let Some(ch) = book.chapters.get(self.current_chapter) {
                self.page_anim_cross_chapter_snapshot = Some(CrossChapterSnapshot {
                    chapter: self.current_chapter,
                    blocks: Arc::new(ch.blocks.clone()),
                    block_ranges: self.page_block_ranges.clone(),
                    total_pages: self.total_pages,
                    from_page: self.current_page,
                    title: ch.title.clone(),
                });
            }
        }
    }

    /// Begin a cross-chapter animation after chapter change.
    pub fn start_cross_chapter_animation(&mut self, direction: f32) {
        if self.scroll_mode || self.reader_page_animation == "None" {
            return;
        }
        self.page_anim_cross_chapter = true;
        self.page_anim_from = 0;
        self.page_anim_to = if direction < 0.0 { usize::MAX } else { 0 };
        self.page_anim_progress = 0.0;
        self.page_anim_direction = direction;
    }

    pub fn next_page(&mut self) {
        if self.reader_page_animation == "Realistic" && !self.scroll_mode {
            self.request_curl_turn(true, None);
            return;
        }
        if self.current_page + 1 < self.total_pages {
            self.trigger_page_animation_to(self.current_page + 1, 1.0);
        } else {
            self.capture_cross_chapter_snapshot();
            self.next_chapter();
            self.start_cross_chapter_animation(1.0);
        }
    }

    pub(crate) fn schedule_position_save_at(
        &mut self,
        chapter: usize,
        block: usize,
        char_offset: usize,
    ) {
        if self.current_chapter != chapter
            || self.current_block != block
            || self.current_char_offset != char_offset
        {
            self.current_chapter = chapter;
            self.current_block = block;
            self.current_char_offset = char_offset;
            self.position_save_modified_ms = reader_core::now_millis();
            self.position_save_due =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(400));
        }
    }

    pub(crate) fn flush_reading_position_for_backup(&mut self) -> Result<(), String> {
        if let Some(due) = self.position_save_due.take() {
            if let Err(error) = self.persist_reading_position() {
                self.position_save_due = Some(due);
                return Err(error);
            }
        }
        Ok(())
    }
    fn flush_reading_position_if_due(&mut self) {
        let Some(due) = self.position_save_due else {
            return;
        };
        if std::time::Instant::now() < due {
            return;
        }
        self.position_save_due = None;
        if let Err(error) = self.persist_reading_position() {
            self.error_msg = Some(error);
        }
    }
    fn persist_reading_position(&mut self) -> Result<(), String> {
        self.record_outgoing_progress();
        let Some(path) = self.book_path.clone() else {
            return Ok(());
        };
        if let Some(record) = self
            .current_book_hash
            .clone()
            .and_then(|hash| self.cloud_stored_progress(&hash))
        {
            // A delayed viewport flush must publish the same selected revision, not invent another.
            if (
                record.progress.chapter,
                record.progress.block,
                record.progress.char_offset,
            ) == (
                self.current_chapter,
                self.current_block,
                self.current_char_offset,
            ) {
                self.library
                    .apply_versioned_position(&self.data_dir, &path, &record)?;
            }
        } else if self.current_book_hash.is_none() {
            let title = self
                .book
                .as_ref()
                .and_then(|b| b.chapters.get(self.current_chapter))
                .map(|c| c.title.clone());
            self.library.apply_position_at(
                &self.data_dir,
                &path,
                self.current_chapter,
                title,
                self.current_block,
                self.current_char_offset,
                self.position_save_modified_ms,
            )?;
        }
        Ok(())
    }

    pub fn prev_page(&mut self) {
        if self.reader_page_animation == "Realistic" && !self.scroll_mode {
            self.request_curl_turn(false, None);
            return;
        }
        if self.current_page > 0 {
            self.trigger_page_animation_to(self.current_page - 1, -1.0);
        } else if self.current_chapter > 0 {
            self.capture_cross_chapter_snapshot();
            self.prev_chapter();
            self.current_page = usize::MAX;
            self.start_cross_chapter_animation(-1.0);
        }
    }

    pub fn push_feedback_log(&self, msg: impl AsRef<str>) {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let line = format!("[{ts}] {}", msg.as_ref());
        if reader_core::sharing::is_debug_logging_enabled() {
            eprintln!("[FEEDBACK-LOG] {line}");
        }

        let mut logs = self.feedback_logs.lock().unwrap_or_else(|e| e.into_inner());
        logs.push(line);
        if logs.len() > 600 {
            let remove = logs.len().saturating_sub(600);
            logs.drain(0..remove);
        }
    }

    pub fn export_feedback_log(&self) -> Result<String, String> {
        let log_dir = PathBuf::from(&self.data_dir).join("logs");
        std::fs::create_dir_all(&log_dir).map_err(|e| format!("create log dir: {e}"))?;

        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let file_path = log_dir.join(format!("feedback_log_{ts}.txt"));

        let mut output = String::new();
        let _ = writeln!(output, "RustEpubReader Desktop Feedback Log");
        let _ = writeln!(output, "generated_unix_ts={ts}");
        let _ = writeln!(output, "app_version={}", env!("CARGO_PKG_VERSION"));
        let _ = writeln!(output, "data_dir={}", self.data_dir);
        let _ = writeln!(output, "books_count={}", self.library.books.len());
        let _ = writeln!(
            output,
            "sharing_server_running={}",
            self.sharing_server_running
        );
        let _ = writeln!(output, "sharing_server_addr={}", self.sharing_server_addr);
        let _ = writeln!(output, "language={}", self.i18n.language().code());
        let _ = writeln!(output, "---");
        let _ = writeln!(output, "Recent Books:");
        for b in self.library.books.iter().take(30) {
            let _ = writeln!(
                output,
                "- {} | {} | chapter={} | last_opened={}",
                b.title, b.path, b.last_chapter, b.last_opened
            );
        }
        let _ = writeln!(output, "---");
        let _ = writeln!(output, "Event Logs:");

        let logs = self
            .feedback_logs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        for line in logs {
            let _ = writeln!(output, "{line}");
        }

        std::fs::write(&file_path, output).map_err(|e| format!("write log file: {e}"))?;
        Ok(file_path.to_string_lossy().to_string())
    }

    fn render_update_dialog(&mut self, ctx: &egui::Context) {
        let tag = self.update_latest_tag.clone().unwrap_or_default();
        let mut open = self.show_update_dialog;
        let mut close_requested = false;
        egui::Window::new(self.i18n.t("update.check"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(self.i18n.tf1("update.new_version", &tag))
                            .size(16.0)
                            .strong(),
                    );
                    ui.add_space(12.0);

                    match &self.update_state {
                        UpdateState::Available(_) => {
                            let button_size = egui::vec2(140.0, 32.0);
                            ui.horizontal_centered(|ui| {
                                if ui
                                    .add_sized(
                                        button_size,
                                        egui::Button::new(
                                            egui::RichText::new(
                                                self.i18n.t("update.download_update"),
                                            )
                                            .size(14.0),
                                        ),
                                    )
                                    .clicked()
                                {
                                    self.update_state = UpdateState::Downloading;
                                    let ctx = ui.ctx().clone();
                                    let done_slot: Arc<Mutex<Option<UpdateState>>> =
                                        Arc::new(Mutex::new(None));
                                    let slot = done_slot.clone();
                                    let progress_for_ui: Arc<Mutex<f32>> =
                                        Arc::new(Mutex::new(0.0));
                                    let progress_writer = progress_for_ui.clone();
                                    std::thread::spawn(move || {
                                        let cb_ctx = ctx.clone();
                                        let pw = progress_writer;
                                        let result = crate::self_update::perform_update(Some(
                                            Box::new(move |downloaded, total| {
                                                if total > 0 {
                                                    let pct = downloaded as f32 / total as f32;
                                                    if let Ok(mut p) = pw.lock() {
                                                        *p = pct;
                                                    }
                                                    cb_ctx.request_repaint();
                                                }
                                            }),
                                        ));
                                        let state = match result {
                                            Ok(
                                                crate::self_update::UpdateOutcome::UpdateLaunched,
                                            ) => UpdateState::Restarting,
                                            Ok(crate::self_update::UpdateOutcome::DownloadPageOpened) => UpdateState::DownloadPageOpened,
                                            Ok(_) => UpdateState::UpToDate,
                                            Err(e) => UpdateState::Failed(e.to_string()),
                                        };
                                        if let Ok(mut s) = slot.lock() {
                                            *s = Some(state);
                                        }
                                        ctx.request_repaint();
                                    });
                                    self._update_download_slot = Some(done_slot);
                                    self._update_progress = Some(progress_for_ui);
                                }
                                if ui
                                    .add_sized(
                                        button_size,
                                        egui::Button::new(
                                            egui::RichText::new(self.i18n.t("feedback.not_now"))
                                                .size(14.0),
                                        ),
                                    )
                                    .clicked()
                                {
                                    close_requested = true;
                                }
                            });
                        }
                        UpdateState::Downloading => {
                            let pct = self
                                ._update_progress
                                .as_ref()
                                .and_then(|p| p.lock().ok().map(|v| *v))
                                .unwrap_or(0.0);
                            ui.label(
                                egui::RichText::new(self.i18n.t("update.downloading")).size(13.0),
                            );
                            ui.add(egui::ProgressBar::new(pct).show_percentage());
                            ui.ctx().request_repaint();
                        }
                        UpdateState::Failed(msg) => {
                            ui.label(
                                egui::RichText::new(self.i18n.tf1("update.failed", msg))
                                    .size(13.0)
                                    .color(egui::Color32::from_rgb(255, 100, 100)),
                            );
                            ui.add_space(4.0);
                            if ui.button(self.i18n.t("update.check")).clicked() {
                                close_requested = true;
                                self.update_state = UpdateState::Idle;
                            }
                        }
                        UpdateState::Restarting => {
                            ui.label(
                                egui::RichText::new(self.i18n.t("update.restarting"))
                                    .size(14.0)
                                    .color(egui::Color32::from_rgb(80, 200, 120)),
                            );
                        }
                        UpdateState::DownloadPageOpened => {
                            ui.label(self.i18n.t("update.download_page_opened"));
                        }
                        _ => {}
                    }
                    ui.add_space(8.0);
                });
            });
        if close_requested {
            open = false;
        }
        self.show_update_dialog = open;
    }
}

impl Drop for ReaderApp {
    fn drop(&mut self) {
        if let Some(cancel) = self.pending_open_cancel.take() {
            cancel.cancel();
        }
        if !self.backup.busy()
            && !self
                .reader_navigation
                .pending_remote_from(self.reader_location())
            && self.book_path.is_some()
            && self.position_save_due.is_some()
        {
            if let Err(error) = self.flush_reading_position_for_backup() {
                eprintln!("Cannot flush reading position: {error}");
            }
        }
        if let Err(error) = AppSettings::from_app(self).save(&self.data_dir) {
            eprintln!("{error}");
        }
        if !self.backup.busy() {
            self.flush_reading_stats();
        }
    }
}

impl eframe::App for ReaderApp {
    #[cfg(any(debug_assertions, feature = "performance-checks"))]
    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        #[cfg(feature = "performance-checks")]
        if let Some(probe) = self.native_corpus.as_mut() {
            probe.raw_input(input)
        }
        #[cfg(debug_assertions)]
        if std::env::args().any(|arg| arg == "--verify-navigation")
            && _ctx
                .data(|data| data.get_temp::<(u32, f64)>(egui::Id::new("reader_navigation_check")))
                .is_some_and(|(step, _)| step == 3)
        {
            if let Some(paper) = self.paging_page_rect {
                input.focused = true;
                input.events.push(egui::Event::PointerMoved(egui::pos2(
                    paper.left() + 38.,
                    paper.bottom() - 38.,
                )));
            }
        }
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(feature = "performance-checks")]
        let performance_frame = std::time::Instant::now();
        #[cfg(feature = "performance-checks")]
        if let Some(probe) = self.native_corpus.as_mut() {
            probe.before_input(ctx);
        }
        self.sync_appearance(ctx);
        self.last_egui_ctx = Some(ctx.clone());
        self.tick_book_share(ctx);
        self.advance_book_presentation();
        self.poll_book_open();
        self.poll_reader_navigation();
        #[cfg(debug_assertions)]
        self.check_reader_navigation(ctx);
        // --- Poll TTS audio ---
        if self.i18n.preference() == &Language::Auto {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
            if self.language_checked.elapsed() >= std::time::Duration::from_secs(1) {
                self.language_checked = std::time::Instant::now();
                if self.i18n.refresh_system_language() {
                    ctx.request_repaint();
                }
            }
        }
        self.tts_poll_audio();
        self.poll_reading_activity(ctx);
        self.flush_reading_position_if_due();
        // --- Poll CSC model download ---
        self.csc_poll_download();
        // --- Poll GitHub OAuth ---
        self.github_poll_device_code();
        self.github_poll_token();
        self.github_poll_token_result();
        self.record_outgoing_progress();
        self.cloud_poll(ctx);
        self.poll_annotation_export(ctx);
        self.poll_backup(ctx);
        // --- Poll CSC Contribution ---
        self.csc_poll_contribution();
        // --- Check async font discovery result ---
        if self.system_font_names.is_empty() {
            if let Ok(mut slot) = self.font_discovery_result.lock() {
                if let Some((names, paths)) = slot.take() {
                    self.system_font_names = names;
                    self.system_font_paths = paths;
                    // Restore the user's preferred fonts now that system fonts
                    // are available, and trigger re-registration so the fonts
                    // actually get loaded into egui.
                    if let Some(preferred) = self.pending_font_family.take() {
                        if !self.reader_font_family.is_empty()
                            && self.system_font_paths.contains_key(&preferred)
                        {
                            self.reader_font_family = preferred;
                        }
                    }
                    if let Some(preferred) = self.pending_cjk_font_family.take() {
                        if self.system_font_paths.contains_key(&preferred) {
                            self.reader_cjk_font_family = preferred;
                        }
                    }
                    self.embedded_fonts_registered = false;
                }
            }
        }

        // --- Poll CSC background results ---
        self.csc_poll_results();

        // --- Poll startup update check result ---
        if matches!(self.update_state, UpdateState::Checking) {
            // Startup checks have no GUI context in their worker. Keep polling even
            // when the system-language heartbeat is disabled by an explicit locale.
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            if let Some(ref slot) = self._update_check_slot {
                if let Ok(s) = slot.lock() {
                    if let Some(ref state) = *s {
                        match state {
                            UpdateState::Available(tag) => {
                                self.push_feedback_log(format!(
                                    "[Update] new version available: {}",
                                    tag
                                ));
                                self.update_latest_tag = Some(tag.clone());
                                self.show_update_dialog = true;
                                self.update_state = UpdateState::Available(tag.clone());
                            }
                            UpdateState::CheckFailed(message) => {
                                self.push_feedback_log(format!("[Update] check failed: {message}"));
                                self.update_state = state.clone();
                            }
                            _ => {
                                self.push_feedback_log("[Update] app is up to date");
                                self.update_state = state.clone();
                            }
                        }
                        // drop will happen, but clear slot next frame
                    }
                }
                if !matches!(self.update_state, UpdateState::Checking) {
                    self._update_check_slot = None;
                }
            }
        }

        // --- Poll download result (if downloading in background) ---
        if matches!(self.update_state, UpdateState::Downloading) {
            if let Some(ref slot) = self._update_download_slot {
                if let Ok(s) = slot.lock() {
                    if let Some(ref state) = *s {
                        self.update_state = state.clone();
                    }
                }
                if !matches!(self.update_state, UpdateState::Downloading) {
                    self._update_download_slot = None;
                    self._update_progress = None;
                }
            }
        }

        // --- Startup update available dialog ---
        if self.show_update_dialog {
            self.render_update_dialog(ctx);
        }

        self.poll_received_progress();

        // --- Reload library from disk if sync completed ---
        if !self.backup.busy() && self.pending_library_reload.swap(false, Ordering::SeqCst) {
            self.reload_library();
        }

        // --- Keep shared_book_paths in sync with library ---
        {
            let paths: Vec<String> = self.library.books.iter().map(|b| b.path.clone()).collect();
            *self
                .shared_book_paths
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = paths;
        }

        self.defer_custom_font_for_frame = false;

        self.appearance_transition
            .begin(ctx, (self.dark_mode, self.appearance_current_dark));
        crate::ui::design::apply(ctx, self.dark_mode);
        if self.show_github_login {
            self.show_github_login = false;
            self.open_sync(0);
        }
        if self.show_sharing_panel {
            self.show_sharing_panel = false;
            self.open_sync(1);
        }
        if self.view != AppView::Reader
            || self.scroll_mode
            || self.reader_page_animation != "Realistic"
        {
            self.page_curl.clear();
        }
        if self.view != AppView::Reader {
            self.render_app_navigation(ctx);
        }

        // Register embedded fonts from the current book
        if !self.embedded_fonts_registered {
            self.embedded_fonts_registered = true;
            let mut fonts = egui::FontDefinitions::default();

            // Save default proportional chain (egui built-in Latin fonts) before CJK additions
            let default_proportional = fonts
                .families
                .get(&egui::FontFamily::Proportional)
                .cloned()
                .unwrap_or_default();
            let default_monospace = fonts
                .families
                .get(&egui::FontFamily::Monospace)
                .cloned()
                .unwrap_or_default();

            let mut selected_latin_bound = matches!(
                self.reader_font_family.as_str(),
                "" | "Sans" | "Serif" | "Monospace"
            );
            let mut selected_cjk_bound = matches!(self.reader_cjk_font_family.as_str(), "Sans");

            // Re-register system fonts (same as setup_fonts in main.rs)
            let cjk_paths: &[&str] = &[
                "C:\\Windows\\Fonts\\msyh.ttc",
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                "/System/Library/Fonts/PingFang.ttc",
            ];
            for path in cjk_paths {
                if let Some(data) = crate::system_fonts::load_path(path) {
                    fonts.font_data.insert("cjk_font".to_owned(), data);
                    fonts
                        .families
                        .entry(egui::FontFamily::Proportional)
                        .or_default()
                        .push("cjk_font".to_owned());
                    fonts
                        .families
                        .entry(egui::FontFamily::Monospace)
                        .or_default()
                        .push("cjk_font".to_owned());
                    break;
                }
            }
            let bold_paths: &[&str] = &[
                "C:\\Windows\\Fonts\\msyhbd.ttc",
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc",
                "/System/Library/Fonts/PingFang.ttc",
            ];
            for path in bold_paths {
                if let Some(data) = crate::system_fonts::load_path(path) {
                    fonts.font_data.insert("cjk_bold".to_owned(), data);
                    fonts.families.insert(
                        egui::FontFamily::Name("Bold".into()),
                        vec!["cjk_bold".to_owned()],
                    );
                    break;
                }
            }
            let serif_paths: &[&str] = &[
                "C:\\Windows\\Fonts\\simsun.ttc",
                "/usr/share/fonts/truetype/noto/NotoSerif-Regular.ttf",
                "/usr/share/fonts/opentype/noto/NotoSerifCJK-Regular.ttc",
                "/System/Library/Fonts/Times.ttc",
            ];
            for path in serif_paths {
                if let Some(data) = crate::system_fonts::load_path(path) {
                    fonts.font_data.insert("serif_font".to_owned(), data);
                    fonts.families.insert(
                        egui::FontFamily::Name("Serif".into()),
                        vec!["serif_font".to_owned(), "cjk_font".to_owned()],
                    );
                    break;
                }
            }

            #[cfg(target_os = "macos")]
            crate::system_fonts::register_defaults(&mut fonts);
            // Rebuilding for a book must retain the startup families even when
            // an optional system face is missing on Linux or Windows.
            crate::register_reader_font_families(&mut fonts);

            // Register embedded EPUB fonts
            if let Some(book) = &self.book {
                for (name, data) in &book.fonts {
                    fonts.font_data.insert(
                        name.clone(),
                        egui::FontData::from_owned(data.clone()).into(),
                    );
                    let mut fallback = vec![name.clone()];
                    if fonts.font_data.contains_key("cjk_font") {
                        fallback.push("cjk_font".to_owned());
                    }
                    fonts
                        .families
                        .insert(egui::FontFamily::Name(name.clone().into()), fallback);
                    if *name == self.reader_font_family {
                        selected_latin_bound = true;
                    }
                    if *name == self.reader_cjk_font_family {
                        selected_cjk_bound = true;
                    }
                }
            }

            // Register custom Latin system font
            if !matches!(
                self.reader_font_family.as_str(),
                "" | "Sans" | "Serif" | "Monospace"
            ) {
                if let Some(path) = self.system_font_paths.get(&self.reader_font_family) {
                    if let Some(data) = path.load() {
                        fonts
                            .font_data
                            .insert(self.reader_font_family.clone(), data);
                        let mut fallback = vec![self.reader_font_family.clone()];
                        if fonts.font_data.contains_key("cjk_font") {
                            fallback.push("cjk_font".to_owned());
                        }
                        fonts.families.insert(
                            egui::FontFamily::Name(self.reader_font_family.clone().into()),
                            fallback,
                        );
                        selected_latin_bound = true;
                    }
                }
            }

            // Register custom CJK system font
            if !matches!(self.reader_cjk_font_family.as_str(), "Sans")
                && !fonts.font_data.contains_key(&self.reader_cjk_font_family)
            {
                if let Some(path) = self.system_font_paths.get(&self.reader_cjk_font_family) {
                    if let Some(data) = path.load() {
                        fonts
                            .font_data
                            .insert(self.reader_cjk_font_family.clone(), data);
                        selected_cjk_bound = true;
                    }
                }
            }

            // Latin font fallback to Sans if not bound
            if !selected_latin_bound {
                if !self.reader_font_family.is_empty()
                    && self.reader_font_family != "Sans"
                    && self.system_font_names.is_empty()
                {
                    self.pending_font_family = Some(self.reader_font_family.clone());
                }
                self.reader_font_family = "Sans".to_string();
            }

            // CJK font fallback to Sans if not bound
            if !selected_cjk_bound {
                if !self.reader_cjk_font_family.is_empty()
                    && self.reader_cjk_font_family != "Sans"
                    && self.system_font_names.is_empty()
                {
                    self.pending_cjk_font_family = Some(self.reader_cjk_font_family.clone());
                }
                self.reader_cjk_font_family = "Sans".to_string();
            }

            // Emoji / symbol fallback font
            let emoji_paths: &[&str] = &[
                "C:\\Windows\\Fonts\\seguisym.ttf",
                "C:\\Windows\\Fonts\\seguiemj.ttf",
                "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
                "/System/Library/Fonts/Apple Color Emoji.ttc",
            ];
            for path in emoji_paths {
                if let Some(data) = crate::system_fonts::load_path(path) {
                    fonts.font_data.insert("emoji_font".to_owned(), data);
                    fonts
                        .families
                        .entry(egui::FontFamily::Proportional)
                        .or_default()
                        .push("emoji_font".to_owned());
                    fonts
                        .families
                        .entry(egui::FontFamily::Monospace)
                        .or_default()
                        .push("emoji_font".to_owned());
                    break;
                }
            }

            // Build separate Latin-first and CJK-first reader families. egui picks the first
            // font containing each glyph, so one shared chain lets Latin fonts consume CJK
            // punctuation before the user's selected CJK font gets a chance to render it.
            let mut reader_latin_fonts = Vec::new();
            match self.reader_font_family.as_str() {
                "Sans" => {
                    extend_unique_font_chain(&mut reader_latin_fonts, default_proportional.clone())
                }
                "Serif" => {
                    if fonts.font_data.contains_key("serif_font") {
                        reader_latin_fonts.push("serif_font".to_owned());
                    }
                    extend_unique_font_chain(&mut reader_latin_fonts, default_proportional.clone());
                }
                "Monospace" => {
                    extend_unique_font_chain(&mut reader_latin_fonts, default_monospace.clone());
                }
                other => {
                    if fonts.font_data.contains_key(other) {
                        reader_latin_fonts.push(other.to_owned());
                    }
                    extend_unique_font_chain(&mut reader_latin_fonts, default_proportional.clone());
                }
            }

            let mut reader_cjk_fonts = Vec::new();
            match self.reader_cjk_font_family.as_str() {
                "Sans" => {
                    if fonts.font_data.contains_key("cjk_font") {
                        reader_cjk_fonts.push("cjk_font".to_owned());
                    }
                }
                other => {
                    if fonts.font_data.contains_key(other) {
                        reader_cjk_fonts.push(other.to_owned());
                    }
                    if fonts.font_data.contains_key("cjk_font") {
                        reader_cjk_fonts.push("cjk_font".to_owned());
                    }
                }
            }

            let mut emoji_fallback = Vec::new();
            if fonts.font_data.contains_key("emoji_font") {
                emoji_fallback.push("emoji_font".to_owned());
            }

            // An empty Latin selection uses the CJK face's own Latin glyphs.
            // Keep the normal Latin chain afterward for missing characters.
            let mut reader_font_chain = if self.reader_font_family.is_empty() {
                let mut chain = reader_cjk_fonts.clone();
                extend_unique_font_chain(&mut chain, reader_latin_fonts.clone());
                chain
            } else {
                let mut chain = reader_latin_fonts.clone();
                extend_unique_font_chain(&mut chain, reader_cjk_fonts.clone());
                chain
            };
            extend_unique_font_chain(&mut reader_font_chain, emoji_fallback.clone());

            let mut reader_cjk_chain = reader_cjk_fonts.clone();
            extend_unique_font_chain(&mut reader_cjk_chain, reader_latin_fonts.clone());
            extend_unique_font_chain(&mut reader_cjk_chain, emoji_fallback.clone());

            fonts.families.insert(
                egui::FontFamily::Name("ReaderFont".into()),
                reader_font_chain,
            );
            fonts
                .families
                .insert(egui::FontFamily::Name("ReaderCjk".into()), reader_cjk_chain);

            crate::system_fonts::register_fallbacks(&mut fonts);
            ctx.set_fonts(fonts);
            self.pages_dirty = true;
            self.defer_custom_font_for_frame = true;
            ctx.request_repaint();
        }

        self.handle_reader_shortcuts(ctx);

        if let Some(err) = self.error_msg.clone() {
            let error_title = self.i18n.t("error.title").to_string();
            let error_ok = self.i18n.t("error.ok").to_string();
            egui::Window::new(error_title)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(&err);
                    if ui.button(&error_ok).clicked() {
                        self.error_msg = None;
                    }
                });
        }

        ctx.data_mut(|d| d.insert_temp(egui::Id::new("panels_animating"), false));
        if self.view == AppView::Reader {
            self.ensure_reader_bg_texture(ctx);

            // TTS bar (between toolbar and content, Edge-style)
            self.render_tts_bar(ctx);

            let collapse_label = self.i18n.t("common.collapse").to_owned();
            if crate::ui::design::animated_side_panel(
                ctx,
                "toc_panel",
                true,
                280.,
                self.show_toc && self.book.is_some(),
                &collapse_label,
                |ui| self.render_toc(ui),
            ) {
                self.show_toc = false;
            }
        }

        let reader_fill = if self.view == AppView::Reader {
            self.reader_bg_color
        } else {
            crate::ui::design::Palette::new(self.dark_mode).canvas
        };

        // ── Side-panels (rendered before CentralPanel) ──
        self.render_search_panel(ctx);
        self.render_annotations_panel(ctx);
        self.render_settings_panel(ctx);

        // ── Review Panel ──
        self.render_review_panel(ctx);

        let panels_moving = crate::ui::design::panels_animating(ctx);
        if panels_moving && self.panel_reflow_anchor.is_none() {
            self.panel_reflow_anchor = Some((self.current_chapter, self.current_block));
            self.page_curl.clear();
        }
        egui::CentralPanel::default()
            .frame(if self.view == AppView::Reader {
                egui::Frame::NONE.fill(reader_fill)
            } else {
                egui::Frame::default().fill(reader_fill)
            })
            .show(ctx, |ui| match self.view {
                AppView::Library => self.render_library(ui),
                AppView::Reader => self.render_reader(ui),
                AppView::Sync => self.render_sync_page(ui),
            });

        // Chrome floats over the paper; it never reserves space in pagination.
        self.render_reader_chrome(ctx);
        self.render_reader_correction_panel(ctx);

        if !panels_moving {
            self.panel_reflow_anchor = None;
        }

        // ── Floating windows ──
        self.render_export_dialog(ctx);
        self.render_reading_service(ctx);
        self.render_receive_books(ctx);
        self.render_rename_dialog(ctx);
        self.render_stats_window(ctx);
        self.render_appearance_window(ctx);

        self.render_sharing_dialogs(ctx);

        // ── About Window ──
        if self.show_about {
            self.render_about(ctx);
        }

        // ── CSC Contribute Dialog ──
        self.render_display_reset_confirmation(ctx);
        self.render_language_picker(ctx);
        self.render_csc_contribute_dialog(ctx);

        // ── TXT Import ──
        if self.txt_import.is_some() {
            self.render_txt_import(ctx);
        }

        self.render_book_opening(ctx);
        self.render_reader_introduction(ctx);
        self.render_backup(ctx);
        #[cfg(debug_assertions)]
        self.capture_appearance_preview(ctx);
        #[cfg(feature = "performance-checks")]
        if let Some(mut probe) = self.reader_performance.take() {
            if !probe.tick(self, ctx, performance_frame.elapsed().as_secs_f64() * 1000.) {
                self.reader_performance = Some(probe);
            }
        }
        #[cfg(feature = "performance-checks")]
        if let Some(mut probe) = self.shelf_performance.take() {
            if !probe.tick(self, ctx, performance_frame.elapsed().as_secs_f64() * 1000.) {
                self.shelf_performance = Some(probe);
            }
        }
        #[cfg(feature = "performance-checks")]
        if let Some(mut probe) = self.native_corpus.take() {
            if !probe.tick(self, ctx) {
                self.native_corpus = Some(probe);
            }
        }
        self.appearance_transition
            .finish(ctx, self.reader_bg_texture.clone());
        if self.view == AppView::Reader {
            let rect = self.paging_page_rect.unwrap_or_else(|| ctx.screen_rect());
            self.reader_navigation
                .finish_frame(ctx, rect, self.reader_bg_texture.clone());
        }
        let settings = AppSettings::from_app(self);
        if self.last_saved_settings.as_ref() != Some(&settings) {
            if let Err(error) = settings.save(&self.data_dir) {
                self.error_msg = Some(error);
            }
            self.last_saved_settings = Some(settings);
        }
    }
}

fn default_scripts_enabled() -> bool {
    true
}

#[cfg(test)]
mod first_reader_preferences_tests {
    use super::*;
    #[test]
    fn closing_a_pending_open_stops_its_worker_and_rejects_its_late_result() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let dir = tempfile::tempdir().unwrap();
        let mut app = ReaderApp::from_data_dir(dir.path().to_string_lossy().into());
        let token = reader_core::open_operation::Cancellation::default();
        let (tx, rx) = std::sync::mpsc::channel();
        app.pending_book_open = Some(("cancelled.epub".into(), None, rx));
        app.pending_open_cancel = Some(token.clone());
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = token.run(|| {
                ready_tx.send(()).unwrap();
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
                loop {
                    reader_core::open_operation::check()?;
                    if std::time::Instant::now() > deadline {
                        return Err("Cancellation was not observed".into());
                    }
                    std::thread::yield_now();
                }
            });
            assert_eq!(
                result.as_ref().err().map(String::as_str),
                Some(reader_core::open_operation::CANCELLED)
            );
            let _ = tx.send(result);
        });
        ready_rx.recv().unwrap();
        app.close_book_presentation();
        worker.join().unwrap();
        app.poll_book_open();
        assert!(app.pending_book_open.is_none());
        assert!(app.pending_open_cancel.is_none());
        assert!(app.book.is_none());
        assert!(app.view == AppView::Library);
    }
    #[test]
    fn corrupt_catalog_is_reported_and_a_failed_reload_keeps_the_visible_shelf() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_string_lossy().into_owned();
        std::fs::write(dir.path().join("library.json"), b"damaged").unwrap();
        let mut app = ReaderApp::from_data_dir(root);
        assert!(app
            .error_msg
            .as_ref()
            .unwrap()
            .contains("Invalid stored data"));
        app.library.books.push(reader_core::library::BookEntry {
            id: "00000000-0000-4000-8000-000000000001".into(),
            title: "Keep visible".into(),
            path: "book.epub".into(),
            last_chapter: 0,
            last_block: 0,
            last_char_offset: 0,
            last_chapter_title: None,
            last_opened: 0,
        });
        app.reload_library();
        assert_eq!(app.library.books.len(), 1);
        assert_eq!(
            std::fs::read(dir.path().join("library.json")).unwrap(),
            b"damaged"
        );
    }

    #[test]
    fn single_column_preference_persists_and_legacy_settings_default_to_auto() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().into_owned();
        let mut app = ReaderApp::from_data_dir(path.clone());
        assert!(!app.force_single_column);
        app.force_single_column = true;
        app.font_size = 23.;
        let settings = AppSettings::from_app(&app);
        settings.save(&path).unwrap();
        let restored = ReaderApp::from_data_dir(path);
        assert!(restored.force_single_column);
        assert_eq!(restored.font_size, 23.);
        let mut legacy = serde_json::to_value(settings).unwrap();
        legacy
            .as_object_mut()
            .unwrap()
            .remove("force_single_column");
        let legacy: AppSettings = serde_json::from_value(legacy).unwrap();
        assert!(!legacy.force_single_column);
        assert_eq!(legacy.font_size, 23.);
    }
    #[test]
    fn first_run_defaults_and_previous_reader_migration() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().to_string_lossy().into_owned();
        let fresh = ReaderApp::from_data_dir(path.clone());
        assert!(!fresh.scroll_mode);
        assert_eq!(fresh.reader_page_animation, "Realistic");
        assert!(!fresh.reader_introduction_seen);
        let mut saved = AppSettings::from_app(&fresh);
        saved.scroll_mode = true;
        saved.reader_page_animation = "Slide".into();
        saved.reader_introduction_seen = Some(true);
        saved.save(&path).unwrap();
        let restored = ReaderApp::from_data_dir(path.clone());
        assert!(restored.scroll_mode && restored.reader_introduction_seen);
        assert_eq!(restored.reader_page_animation, "Slide");
        let mut legacy = serde_json::to_value(saved).unwrap();
        legacy
            .as_object_mut()
            .unwrap()
            .remove("reader_introduction_seen");
        legacy["last_book_path"] = serde_json::json!("/missing/previous-reading.epub");
        std::fs::write(
            AppSettings::path(&path),
            serde_json::to_vec(&legacy).unwrap(),
        )
        .unwrap();
        let upgraded = ReaderApp::from_data_dir(path);
        assert!(upgraded.reader_introduction_seen && upgraded.scroll_mode);
        assert_eq!(upgraded.reader_page_animation, "Slide");
        legacy["reader_introduction_seen"] = serde_json::json!("malformed");
        let decoded: AppSettings = serde_json::from_value(legacy).unwrap();
        assert_eq!(decoded.reader_introduction_seen, None);
        assert!(decoded.scroll_mode);
        assert_eq!(decoded.reader_page_animation, "Slide");
    }
}

#[cfg(test)]
mod reading_theme_settings_tests {
    use super::*;
    use crate::ui::reading_themes::{template, ReadingThemes};

    #[test]
    fn themes_persist_active_edits_without_touching_other_slots_or_reading_position() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().into_owned();
        let mut app = ReaderApp::from_data_dir(path.clone());
        let mut custom = template("custom").unwrap();
        custom.dark.image = Some("/external/custom.png".into());
        app.reading_themes.save("custom", custom.clone());
        let mut warm = template("warm").unwrap();
        warm.dark.ink = Some([210, 211, 212, 255]);
        app.reading_themes.save("warm", warm.clone());
        app.font_size = 23.;
        app.line_spacing = 1.9;
        app.current_chapter = 7;
        app.current_block = 3;
        app.current_char_offset = 5;
        app.apply_selected_reading_theme();
        assert_eq!(
            (
                app.current_chapter,
                app.current_block,
                app.current_char_offset
            ),
            (7, 3, 5)
        );
        assert_eq!((app.font_size, app.line_spacing), (23., 1.9));
        app.reader_bg_color = Color32::from_rgb(11, 22, 33);
        let settings = AppSettings::from_app(&app);
        let themes = settings.reading_themes.as_ref().unwrap();
        assert_eq!(themes.selected, "warm");
        assert_eq!(themes.custom.as_ref(), Some(&custom));
        assert_eq!(themes.pair("warm").dark, warm.dark);
        assert_eq!(themes.pair("warm").light.paper, [11, 22, 33, 255]);
        settings.save(&path).unwrap();
        let restored = ReaderApp::from_data_dir(path);
        assert_eq!(&restored.reading_themes, themes);
        assert_eq!(restored.reader_bg_color, Color32::from_rgb(11, 22, 33));
        assert_eq!((restored.font_size, restored.line_spacing), (23., 1.9));
    }

    #[test]
    fn missing_or_malformed_new_field_preserves_legacy_pair_and_unrelated_settings() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().into_owned();
        let app = ReaderApp::from_data_dir(path.clone());
        let mut settings = AppSettings::from_app(&app);
        let mut pair = template("gray").unwrap();
        pair.light.image = Some("/external/old-light.png".into());
        pair.dark.image = Some("/external/old-dark.png".into());
        pair.dark.strength = 0.61;
        pair.light.ink = Some([41, 42, 43, 255]);
        settings.appearance_paints = Some(pair.clone());
        settings.reading_themes = None;
        settings.appearance_current_dark = Some(false);
        settings.font_size = 27.;
        settings.scroll_mode = true;
        for malformed in [false, true] {
            let mut raw = serde_json::to_value(&settings).unwrap();
            if malformed {
                raw["reading_themes"] =
                    serde_json::json!({"custom":"invalid", "selected":"custom"});
            } else {
                raw.as_object_mut().unwrap().remove("reading_themes");
            }
            std::fs::write(AppSettings::path(&path), serde_json::to_vec(&raw).unwrap()).unwrap();
            let restored = ReaderApp::from_data_dir(path.clone());
            assert_eq!(
                restored.reading_themes,
                ReadingThemes::migrate(pair.clone())
            );
            assert_eq!(restored.appearance_paints, pair);
            assert_eq!(
                restored.reader_bg_image_path.as_deref(),
                Some("/external/old-light.png")
            );
            assert_eq!(restored.font_size, 27.);
            assert!(restored.scroll_mode);
        }
    }
}

#[cfg(test)]
mod reading_navigation_tests {
    use super::*;
    use crate::ui::reader_navigation::{NavigationKind, ReaderLocation};

    fn queued_fixture() -> (
        tempfile::TempDir,
        ReaderApp,
        reader_core::sharing::CommittedProgress,
    ) {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let dir = tempfile::tempdir().unwrap();
        let mut app = ReaderApp::from_data_dir(dir.path().to_string_lossy().into_owned());
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../ios/Tests/fixtures/native-reading-modes.epub");
        app.finish_book_open(
            source.to_str().unwrap(),
            Some(19),
            Ok((EpubBook::open(&source).unwrap(), None)),
        );
        let notice = reader_core::sharing::CommittedProgress {
            path: app.book_path.clone().unwrap(),
            record: reader_core::cloud_progress::VersionedProgress {
                progress: reader_core::sharing::ProgressEntry {
                    book_hash: app.current_book_hash.clone().unwrap(),
                    title: "Book".into(),
                    chapter: 21,
                    block: 3,
                    char_offset: 3,
                    chapter_title: None,
                    timestamp: 1700000000,
                },
                version: reader_core::cloud_progress::ProgressVersion {
                    modified_ms: 1700000000123,
                    device: "remote".into(),
                    legacy: false,
                },
            },
        };
        app.library
            .apply_versioned_position(&app.data_dir, &notice.path, &notice.record)
            .unwrap();
        (dir, app, notice)
    }
    #[test]
    fn reopening_normalizes_an_out_of_range_legacy_chapter_without_resetting_valid_locations() {
        let (_dir, mut app, notice) = queued_fixture();
        app.library
            .update_position(&app.data_dir, &notice.path, 9999, None, 99, 99)
            .unwrap();
        let book = EpubBook::open(&notice.path).unwrap();
        let last = book.chapters.len() - 1;
        app.finish_book_open(&notice.path, Some(0), Ok((book, None)));
        assert!(app.error_msg.is_none(), "{:?}", app.error_msg);
        assert_eq!(
            app.reader_location(),
            ReaderLocation {
                chapter: last,
                block: 0,
                char_offset: 0
            }
        );
        let stored = reader_core::library::Library::load_from(&app.data_dir).unwrap();
        assert_eq!(
            (stored.books[0].last_chapter, stored.books[0].last_block),
            (last, 0)
        );
        app.library
            .update_position(&app.data_dir, &notice.path, 19, None, 3, 37)
            .unwrap();
        app.finish_book_open(
            &notice.path,
            Some(0),
            Ok((EpubBook::open(&notice.path).unwrap(), None)),
        );
        assert_eq!(
            app.reader_location(),
            ReaderLocation {
                chapter: 19,
                block: 3,
                char_offset: 37
            }
        );
    }

    #[test]
    fn an_old_committed_notice_cannot_rewind_a_new_local_viewport_edit() {
        let (_dir, mut app, notice) = queued_fixture();
        app.schedule_position_save_at(19, 3, 37);
        app.record_outgoing_progress();
        app.pending_sync_updates.lock().unwrap().push(notice);
        app.poll_received_progress();
        assert_eq!(
            app.reader_location(),
            ReaderLocation {
                chapter: 19,
                block: 3,
                char_offset: 37
            }
        );
        assert!(app.reader_navigation.history.is_empty());
        app.flush_reading_position_for_backup().unwrap();
        assert_eq!(app.library.books[0].last_char_offset, 37);
    }
    #[test]
    fn closing_an_unchanged_viewport_does_not_overwrite_an_inflight_committed_remote_edit() {
        let (dir, app, notice) = queued_fixture();
        let root = dir.path().to_str().unwrap();
        assert!(app.position_save_due.is_none());
        drop(app);
        let library = reader_core::library::Library::load_from(root).unwrap();
        assert_eq!(
            (
                library.books[0].last_chapter,
                library.books[0].last_char_offset
            ),
            (21, 3)
        );
        let config =
            reader_core::library::Library::read_book_config(root, &library.books[0].id).unwrap();
        assert_eq!(config.reading_version(), Some(notice.record.version));
    }

    #[test]
    fn remote_jump_return_and_restart_preserve_exact_coordinates_without_echo() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let directory = tempfile::tempdir().unwrap();
        let data = directory.path().to_string_lossy().into_owned();
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../ios/Tests/fixtures/native-reading-modes.epub");
        let path = source.to_str().unwrap();
        let mut app = ReaderApp::from_data_dir(data.clone());
        app.finish_book_open(path, Some(19), Ok((EpubBook::open(path).unwrap(), None)));
        let origin = ReaderLocation {
            chapter: 19,
            block: 3,
            char_offset: 37,
        };
        app.current_chapter = origin.chapter;
        app.current_block = origin.block;
        app.current_char_offset = origin.char_offset;
        app.current_page = 2;
        app.total_pages = 9;
        let target = ReaderLocation {
            chapter: 21,
            block: 3,
            char_offset: 3,
        };
        app.navigate_reader(target, NavigationKind::Remote);
        // Loading must not mutate the visible position or create a local revision.
        assert_eq!(app.reader_location(), origin);
        for _ in 0..500 {
            app.poll_reader_navigation();
            if app.reader_location() == target {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(app.reader_location(), target);
        assert_eq!(app.last_synced_char_offset, Some(3));
        assert_eq!(app.reader_navigation.history.len(), 1);
        assert_eq!(app.reader_navigation.history[0].location, origin);
        assert!(app.reader_navigation.history[0].page.contains('3'));
        app.load_reader_navigation();
        assert_eq!(app.reader_navigation.history[0].location, origin);
        app.navigate_reader(origin, NavigationKind::Return);
        for _ in 0..500 {
            app.poll_reader_navigation();
            if app.reader_location() == origin {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(app.reader_location(), origin);
        assert!(app.reader_navigation.history.is_empty());
        app.navigate_reader(target, NavigationKind::Remote);
        app.schedule_position_save_at(19, 3, 45);
        app.poll_reader_navigation();
        assert_eq!(
            app.current_char_offset, 45,
            "A pending remote load replaced newer local reading"
        );
        assert!(!app
            .reader_navigation
            .pending_remote_from(app.reader_location()));
        assert!(app.reader_navigation.history.is_empty());
        app.schedule_position_save_at(19, 3, 37);
        drop(app);
        let mut restored = ReaderApp::from_data_dir(data.clone());
        for _ in 0..500 {
            restored.poll_book_open();
            if restored.book.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(restored.reader_location(), origin);
        assert!(restored.book.is_some());
        restored.close_book_presentation();
        assert!(AppSettings::from_app(&restored).last_book_path.is_none());
    }
    #[test]
    fn batch_import_keeps_valid_books_and_reports_bad_inputs_without_opening_them() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let dir = tempfile::tempdir().unwrap();
        let input = tempfile::tempdir().unwrap();
        let mut paths = Vec::new();
        for name in ["One", "Two"] {
            let txt = input.path().join(format!("{name}.txt"));
            std::fs::write(&txt, format!("第一章 {name}\n{name} body text")).unwrap();
            paths.push(
                reader_core::txt::convert_txt_to_epub(&txt, input.path(), &Default::default())
                    .unwrap()
                    .epub_path,
            );
        }
        paths.push(paths[0].clone());
        let invalid = input.path().join("bad.epub");
        std::fs::write(&invalid, b"not an epub").unwrap();
        paths.push(invalid);
        let txt = input.path().join("manual.txt");
        std::fs::write(&txt, b"body").unwrap();
        paths.push(txt);
        let mut app = ReaderApp::from_data_dir(dir.path().to_string_lossy().into_owned());
        app.import_epub_batch(paths);
        for _ in 0..500 {
            app.poll_book_open();
            if app.pending_batch_import.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(app.pending_batch_import.is_none());
        assert_eq!(app.library.books.len(), 2);
        assert!(app.book.is_none());
        assert!(app
            .error_msg
            .as_ref()
            .is_some_and(|e| e.contains("bad.epub") && e.contains("manual.txt")));
        assert!(app.library.books.iter().all(|b| b.last_opened == 0));
    }
}
