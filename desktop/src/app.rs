use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui;
use egui::Color32;
use serde::{Deserialize, Serialize};

use reader_core::epub::EpubBook;
use reader_core::i18n::{I18n, Language};
use reader_core::library::Library;
use reader_core::sharing::{start_listener, DiscoveredPeer, PeerStore};

type FontDiscoveryResult = Arc<Mutex<Option<(Vec<String>, HashMap<String, String>)>>>;

fn default_data_dir() -> String {
    let dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&dir);
    dir.to_string_lossy().to_string()
}

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

fn discover_system_fonts() -> (Vec<String>, HashMap<String, String>) {
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

    let mut font_map: HashMap<String, String> = HashMap::new();
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
            .or_insert_with(|| path.to_string_lossy().to_string());
    }

    // CJK 字体排前面，其余按字母升序
    let is_cjk = |s: &str| -> bool {
        s.chars().any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c))
            || s.contains("CJK")
            || s.ends_with(" SC")
            || s.ends_with(" TC")
    };
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

fn generate_pin() -> String {
    use rand::RngCore;
    let val = rand::rngs::OsRng.next_u32() % 10000;
    format!("{:04}", val)
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
struct AppSettings {
    font_size: f32,
    dark_mode: bool,
    reader_bg_color: [u8; 4],
    reader_font_color: Option<[u8; 4]>,
    reader_font_family: String,
    reader_page_animation: String,
    #[serde(default = "default_anim_speed")]
    reader_page_animation_speed: f32,
    reader_bg_image_path: Option<String>,
    reader_bg_image_alpha: f32,
    scroll_mode: bool,
    show_toc: bool,
    #[serde(default)]
    language: String,
    #[serde(default)]
    last_book_path: Option<String>,
    #[serde(default)]
    last_chapter: usize,
    #[serde(default)]
    auto_start_sharing: bool,
}

impl AppSettings {
    fn path(data_dir: &str) -> PathBuf {
        PathBuf::from(data_dir).join("settings.json")
    }

    fn load(data_dir: &str) -> Option<Self> {
        let path = Self::path(data_dir);
        let text = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn save(&self, data_dir: &str) {
        let path = Self::path(data_dir);
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, text);
        }
    }

    fn from_color(c: Color32) -> [u8; 4] {
        c.to_array()
    }

    fn to_color(c: [u8; 4]) -> Color32 {
        Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
    }

    fn from_app(app: &ReaderApp) -> Self {
        let (last_book_path, last_chapter) = if matches!(app.view, AppView::Reader) {
            (app.book_path.clone(), app.current_chapter)
        } else {
            (None, 0)
        };

        Self {
            font_size: app.font_size,
            dark_mode: app.dark_mode,
            reader_bg_color: Self::from_color(app.reader_bg_color),
            reader_font_color: app.reader_font_color.map(Self::from_color),
            reader_font_family: app.reader_font_family.clone(),
            reader_page_animation: app.reader_page_animation.clone(),
            reader_page_animation_speed: app.reader_page_animation_speed,
            reader_bg_image_path: app.reader_bg_image_path.clone(),
            reader_bg_image_alpha: app.reader_bg_image_alpha,
            scroll_mode: app.scroll_mode,
            show_toc: app.show_toc,
            language: app.i18n.language().code().to_string(),
            last_book_path,
            last_chapter,
            auto_start_sharing: app.auto_start_sharing,
        }
    }

    fn apply_to_app(&self, app: &mut ReaderApp) {
        app.font_size = self.font_size.clamp(12.0, 40.0);
        app.dark_mode = self.dark_mode;
        app.reader_bg_color = Self::to_color(self.reader_bg_color);
        app.reader_font_color = self.reader_font_color.map(Self::to_color);
        app.reader_font_family = self.reader_font_family.clone();
        app.reader_page_animation = self.reader_page_animation.clone();
        app.reader_page_animation_speed = self.reader_page_animation_speed.clamp(0.04, 0.40);
        app.reader_bg_image_path = self.reader_bg_image_path.clone();
        app.reader_bg_image_alpha = self.reader_bg_image_alpha.clamp(0.0, 1.0);
        app.scroll_mode = self.scroll_mode;
        app.show_toc = self.show_toc;
        app.auto_start_sharing = self.auto_start_sharing;
        app.i18n.set_language(Language::from_code(&self.language));
        // last_book_path/last_chapter applied in Default::default after call
    }
}

#[derive(PartialEq)]
pub enum AppView {
    Library,
    Reader,
}

pub struct ReaderApp {
    pub data_dir: String,
    pub book: Option<EpubBook>,
    pub book_path: Option<String>,
    pub current_book_hash: Option<String>,
    pub last_synced_chapter: Option<usize>,
    pub current_chapter: usize,
    pub font_size: f32,
    pub dark_mode: bool,
    pub reader_bg_color: Color32,
    pub reader_font_color: Option<Color32>,
    pub reader_font_family: String,
    pub reader_page_animation: String,
    pub reader_page_animation_speed: f32,
    pub reader_bg_image_path: Option<String>,
    pub reader_bg_image_alpha: f32,
    pub reader_bg_texture: Option<egui::TextureHandle>,
    pub show_reader_settings: bool,
    pub show_toc: bool,
    pub scroll_to_top: bool,
    pub error_msg: Option<String>,
    pub view: AppView,
    pub library: Library,
    pub scroll_mode: bool,
    pub current_page: usize,
    pub total_pages: usize,
    pub page_block_ranges: Vec<(usize, usize)>,
    pub pages_dirty: bool,
    pub cover_textures: HashMap<String, Option<egui::TextureHandle>>,
    pub last_avail_width: f32,
    pub last_avail_height: f32,
    pub page_anim_from: usize,
    pub page_anim_to: usize,
    pub page_anim_progress: f32,
    pub page_anim_direction: f32,
    pub page_anim_cross_chapter: bool,
    pub page_anim_cross_chapter_snapshot: Option<CrossChapterSnapshot>,
    pub is_dual_column: bool,
    pub embedded_font_names: Vec<String>,
    pub embedded_fonts_registered: bool,
    pub defer_custom_font_for_frame: bool,
    pub system_font_names: Vec<String>,
    system_font_paths: HashMap<String, String>,
    font_discovery_result: FontDiscoveryResult,
    last_saved_settings: Option<AppSettings>,
    pub font_search: String,
    pub i18n: I18n,
    pub previous_chapter: Option<usize>,
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
    pub pending_sync_updates: Arc<Mutex<Vec<reader_core::sharing::ProgressEntry>>>,
    pub pending_library_reload: Arc<AtomicBool>,
    pub shared_book_paths: Arc<Mutex<Vec<String>>>,
    pub feedback_logs: Arc<Mutex<Vec<String>>>,
    pub show_feedback_github_prompt: bool,
    pub last_exported_feedback_log: Option<String>,
    pub show_about: bool,
    pub about_icon_texture: Option<egui::TextureHandle>,
}

#[derive(Clone)]
pub struct CrossChapterSnapshot {
    pub blocks: Arc<Vec<reader_core::epub::ContentBlock>>,
    pub block_ranges: Vec<(usize, usize)>,
    pub total_pages: usize,
    pub from_page: usize,
    pub title: String,
}

impl Default for ReaderApp {
    fn default() -> Self {
        let data_dir = default_data_dir();
        let library = Library::load_from(&data_dir);
        let font_discovery_result: FontDiscoveryResult = Arc::new(Mutex::new(None));
        {
            let result_slot = font_discovery_result.clone();
            std::thread::spawn(move || {
                let fonts = discover_system_fonts();
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
            data_dir,
            book: None,
            book_path: None,
            current_book_hash: None,
            last_synced_chapter: None,
            current_chapter: 0,
            font_size: 16.0,
            dark_mode: true,
            reader_bg_color: Color32::from_rgb(250, 246, 238),
            reader_font_color: None,
            reader_font_family: "Sans".to_string(),
            reader_page_animation: "Slide".to_string(),
            reader_page_animation_speed: 0.14,
            reader_bg_image_path: None,
            reader_bg_image_alpha: 0.22,
            reader_bg_texture: None,
            show_reader_settings: false,
            show_toc: true,
            scroll_to_top: false,
            error_msg: None,
            view: AppView::Library,
            library,
            scroll_mode: false,
            current_page: 0,
            total_pages: 0,
            page_block_ranges: Vec::new(),
            pages_dirty: true,
            cover_textures: HashMap::new(),
            last_avail_width: 0.0,
            last_avail_height: 0.0,
            page_anim_from: 0,
            page_anim_to: 0,
            page_anim_progress: 1.0,
            page_anim_direction: 1.0,
            page_anim_cross_chapter: false,
            page_anim_cross_chapter_snapshot: None,
            is_dual_column: false,
            embedded_font_names: Vec::new(),
            embedded_fonts_registered: true,
            defer_custom_font_for_frame: false,
            system_font_names: Vec::new(),
            system_font_paths: HashMap::new(),
            font_discovery_result,
            last_saved_settings: None,
            font_search: String::new(),
            i18n: I18n::default(),
            previous_chapter: None,
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
            about_icon_texture: None,
        };

        if let Some(settings) = AppSettings::load(&app.data_dir) {
            settings.apply_to_app(&mut app);
            app.embedded_fonts_registered = false;
            app.pages_dirty = true;
            // 自动恢复上次阅读的书籍
            if let Some(ref path) = settings.last_book_path.clone() {
                if std::path::Path::new(path).exists() {
                    app.open_book_from_path(path, Some(settings.last_chapter));
                }
            }
        }
        if app.auto_start_sharing {
            app.start_sharing_server();
        }
        app.push_feedback_log("app initialized");
        app.last_saved_settings = Some(AppSettings::from_app(&app));
        app
    }
}

impl ReaderApp {
    pub fn trigger_page_animation_to(&mut self, target_page: usize, direction: f32) {
        if self.reader_page_animation == "None"
            || self.scroll_mode
            || target_page == self.current_page
        {
            self.current_page = target_page;
            self.page_anim_progress = 1.0;
            self.page_anim_from = target_page;
            self.page_anim_to = target_page;
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
    }

    pub fn pick_reader_background_image(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Image", &["png", "jpg", "jpeg", "webp", "bmp"])
            .pick_file()
        {
            self.reader_bg_image_path = Some(path.to_string_lossy().to_string());
            self.reader_bg_texture = None;
        }
    }

    pub fn clear_reader_background_image(&mut self) {
        self.reader_bg_image_path = None;
        self.reader_bg_texture = None;
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
            .add_filter("EPUB", &["epub"])
            .pick_file()
        {
            let path_str = path.to_string_lossy().to_string();
            self.open_book_from_path(&path_str, None);
        }
    }

    pub fn open_book_from_path(&mut self, path: &str, chapter: Option<usize>) {
        match EpubBook::open(path) {
            Ok(mut book) => {
                let mut ch = chapter.unwrap_or(0);
                if !book.chapters.is_empty() {
                    ch = ch.min(book.chapters.len() - 1);
                }

                let initial_title = book.title.clone();
                let initial_chapter_title = book.chapters.get(ch).map(|c| c.title.clone());
                let mut entry = self.library.add_or_update(
                    &self.data_dir,
                    initial_title,
                    path.to_string(),
                    ch,
                    initial_chapter_title,
                );

                if entry.path != path {
                    if let Ok(managed_book) = EpubBook::open(&entry.path) {
                        book = managed_book;
                        if !book.chapters.is_empty() {
                            ch = ch.min(book.chapters.len() - 1);
                        }
                        let chapter_title = book.chapters.get(ch).map(|c| c.title.clone());
                        entry = self.library.add_or_update(
                            &self.data_dir,
                            book.title.clone(),
                            entry.path.clone(),
                            ch,
                            chapter_title,
                        );
                    }
                }

                let font_names: Vec<String> =
                    book.fonts.iter().map(|(name, _)| name.clone()).collect();
                self.embedded_font_names = font_names;
                self.embedded_fonts_registered = false;
                self.book = Some(book);
                self.current_book_hash = EpubBook::file_hash(&entry.path).ok();
                self.book_path = Some(entry.path.clone());
                self.current_chapter = ch;
                self.last_synced_chapter = None; // Reset so progress is pushed immediately on first update
                self.scroll_to_top = true;
                self.pages_dirty = true;
                self.current_page = 0;
                self.view = AppView::Reader;
                self.error_msg = None;
            }
            Err(e) => {
                self.error_msg = Some(e);
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
            if let Some(p) = &self.book_path {
                let chap_title = self
                    .book
                    .as_ref()
                    .and_then(|b| b.chapters.get(self.current_chapter))
                    .map(|c| c.title.clone());
                self.library
                    .update_chapter(&self.data_dir, p, self.current_chapter, chap_title);
            }
        }
    }

    pub fn prev_chapter(&mut self) {
        if self.current_chapter > 0 {
            self.current_chapter -= 1;
            self.scroll_to_top = true;
            self.pages_dirty = true;
            self.current_page = 0;
            if let Some(p) = &self.book_path {
                let chap_title = self
                    .book
                    .as_ref()
                    .and_then(|b| b.chapters.get(self.current_chapter))
                    .map(|c| c.title.clone());
                self.library
                    .update_chapter(&self.data_dir, p, self.current_chapter, chap_title);
            }
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
        if self.current_page + 1 < self.total_pages {
            self.trigger_page_animation_to(self.current_page + 1, 1.0);
        } else {
            self.capture_cross_chapter_snapshot();
            self.next_chapter();
            self.start_cross_chapter_animation(1.0);
        }
    }

    pub fn prev_page(&mut self) {
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
}

impl eframe::App for ReaderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // --- Check async font discovery result ---
        if self.system_font_names.is_empty() {
            if let Ok(mut slot) = self.font_discovery_result.lock() {
                if let Some((names, paths)) = slot.take() {
                    self.system_font_names = names;
                    self.system_font_paths = paths;
                }
            }
        }

        // --- Handle incoming sync updates ---
        {
            let mut sync_updates = self
                .pending_sync_updates
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            for update in sync_updates.drain(..) {
                // If the update applies to the currently opened book
                if let Some(hash) = &self.current_book_hash {
                    if &update.book_hash == hash {
                        if update.chapter != self.current_chapter {
                            self.previous_chapter = Some(self.current_chapter);
                            self.current_chapter = update.chapter;
                            self.last_synced_chapter = Some(update.chapter); // Don't bounce it back
                            self.pages_dirty = true;
                            self.current_page = 0;
                        }

                        if let Some(p) = &self.book_path {
                            let chap_title = update.chapter_title.clone().or_else(|| {
                                self.book
                                    .as_ref()
                                    .and_then(|b| b.chapters.get(self.current_chapter))
                                    .map(|c| c.title.clone())
                            });
                            self.library.update_chapter(
                                &self.data_dir,
                                p,
                                self.current_chapter,
                                chap_title,
                            );
                        }
                        continue;
                    }
                }
                // For non-open books: match by hash against library entries
                let mut matched_paths = Vec::new();
                for entry in &self.library.books {
                    if let Ok(h) = reader_core::epub::EpubBook::file_hash(&entry.path) {
                        if h == update.book_hash
                            && (entry.last_chapter != update.chapter
                                || (update.chapter_title.is_some()
                                    && entry.last_chapter_title != update.chapter_title))
                        {
                            matched_paths.push(entry.path.clone());
                        }
                    }
                }
                for p in matched_paths {
                    self.library.update_chapter(
                        &self.data_dir,
                        &p,
                        update.chapter,
                        update.chapter_title.clone(),
                    );
                }
            }
        }

        // --- Reload library from disk if sync completed ---
        if self.pending_library_reload.swap(false, Ordering::SeqCst) {
            let reloaded = Library::load_from(&self.data_dir);
            // Preserve current book's chapter if open (avoid overwrite by stale sync data)
            if let Some(ref book_path) = self.book_path {
                let current_ch = self.current_chapter;
                self.library = reloaded;
                if let Some(entry) = self.library.books.iter().find(|b| b.path == *book_path) {
                    if entry.last_chapter != current_ch {
                        let chap_title = self
                            .book
                            .as_ref()
                            .and_then(|b| b.chapters.get(current_ch))
                            .map(|c| c.title.clone());
                        self.library.update_chapter(
                            &self.data_dir,
                            book_path,
                            current_ch,
                            chap_title,
                        );
                    }
                }
            } else {
                self.library = reloaded;
            }
        }

        // --- Keep shared_book_paths in sync with library ---
        {
            let paths: Vec<String> = self.library.books.iter().map(|b| b.path.clone()).collect();
            *self
                .shared_book_paths
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = paths;
        }

        // --- Push outgoing local progress to PeerStore ---
        if Some(self.current_chapter) != self.last_synced_chapter {
            if let Some(hash) = &self.current_book_hash {
                let mut store = self.peer_store.lock().unwrap_or_else(|e| e.into_inner());
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let title = self
                    .book
                    .as_ref()
                    .map(|b| b.title.clone())
                    .unwrap_or_default();
                let chapter_title = self
                    .book
                    .as_ref()
                    .and_then(|b| b.chapters.get(self.current_chapter))
                    .map(|c| c.title.clone());

                if let Some(local) = store.progress.iter_mut().find(|p| p.book_hash == *hash) {
                    local.chapter = self.current_chapter;
                    local.chapter_title = chapter_title.clone();
                    local.title = title.clone();
                    local.timestamp = now;
                } else {
                    store.progress.push(reader_core::sharing::ProgressEntry {
                        book_hash: hash.clone(),
                        title,
                        chapter: self.current_chapter,
                        chapter_title,
                        timestamp: now,
                    });
                }
                store.save(&self.data_dir);
                self.last_synced_chapter = Some(self.current_chapter);
            }
        }

        self.defer_custom_font_for_frame = false;

        if self.dark_mode {
            ctx.set_visuals(egui::Visuals::dark());
        } else {
            ctx.set_visuals(egui::Visuals::light());
        }

        // Register embedded fonts from the current book
        if !self.embedded_fonts_registered {
            self.embedded_fonts_registered = true;
            let mut fonts = egui::FontDefinitions::default();
            let mut selected_family_bound = matches!(
                self.reader_font_family.as_str(),
                "Sans" | "Serif" | "Monospace"
            );

            // Re-register system fonts (same as setup_fonts in main.rs)
            let cjk_paths: &[&str] = &[
                "C:\\Windows\\Fonts\\msyh.ttc",
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                "/System/Library/Fonts/PingFang.ttc",
            ];
            for path in cjk_paths {
                if let Ok(data) = std::fs::read(path) {
                    fonts.font_data.insert(
                        "cjk_font".to_owned(),
                        egui::FontData::from_owned(data).into(),
                    );
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
                if let Ok(data) = std::fs::read(path) {
                    fonts.font_data.insert(
                        "cjk_bold".to_owned(),
                        egui::FontData::from_owned(data).into(),
                    );
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
                "/System/Library/Fonts/Times.ttc",
            ];
            for path in serif_paths {
                if let Ok(data) = std::fs::read(path) {
                    fonts.font_data.insert(
                        "serif_font".to_owned(),
                        egui::FontData::from_owned(data).into(),
                    );
                    fonts.families.insert(
                        egui::FontFamily::Name("Serif".into()),
                        vec!["serif_font".to_owned(), "cjk_font".to_owned()],
                    );
                    break;
                }
            }

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
                        selected_family_bound = true;
                    }
                }
            }

            if !matches!(
                self.reader_font_family.as_str(),
                "Sans" | "Serif" | "Monospace"
            ) {
                if let Some(path) = self.system_font_paths.get(&self.reader_font_family) {
                    if let Ok(data) = std::fs::read(path) {
                        fonts.font_data.insert(
                            self.reader_font_family.clone(),
                            egui::FontData::from_owned(data).into(),
                        );
                        let mut fallback = vec![self.reader_font_family.clone()];
                        if fonts.font_data.contains_key("cjk_font") {
                            fallback.push("cjk_font".to_owned());
                        }
                        fonts.families.insert(
                            egui::FontFamily::Name(self.reader_font_family.clone().into()),
                            fallback,
                        );
                        selected_family_bound = true;
                    }
                }
            }

            if !selected_family_bound {
                self.reader_font_family = "Sans".to_string();
            }

            ctx.set_fonts(fonts);
            self.pages_dirty = true;
            // Font definitions take effect next frame. Keep rendering, but temporarily
            // fallback custom font family to Sans in this frame to avoid panic and flash.
            if !matches!(
                self.reader_font_family.as_str(),
                "Sans" | "Serif" | "Monospace"
            ) {
                self.defer_custom_font_for_frame = true;
            }
            ctx.request_repaint();
        }

        if self.view == AppView::Reader && !self.show_reader_settings && !self.show_sharing_panel {
            ctx.input(|i| {
                if i.key_pressed(egui::Key::ArrowLeft) {
                    if self.scroll_mode {
                        self.prev_chapter();
                    } else if self.is_dual_column {
                        if self.current_page >= 2 {
                            self.trigger_page_animation_to(self.current_page - 2, -1.0);
                        } else if self.current_chapter > 0 {
                            self.capture_cross_chapter_snapshot();
                            self.prev_chapter();
                            self.current_page = usize::MAX;
                            self.start_cross_chapter_animation(-1.0);
                        }
                    } else {
                        self.prev_page();
                    }
                }
                if i.key_pressed(egui::Key::ArrowRight) {
                    if self.scroll_mode {
                        self.next_chapter();
                    } else if self.is_dual_column {
                        if self.current_page + 2 < self.total_pages {
                            self.trigger_page_animation_to(self.current_page + 2, 1.0);
                        } else {
                            self.capture_cross_chapter_snapshot();
                            self.next_chapter();
                            self.start_cross_chapter_animation(1.0);
                        }
                    } else {
                        self.next_page();
                    }
                }
            });
        }

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

        let show_reader_settings_at_frame_start = self.show_reader_settings;
        if self.view == AppView::Reader {
            self.ensure_reader_bg_texture(ctx);
            egui::TopBottomPanel::top("toolbar")
                .frame(
                    egui::Frame::default()
                        .inner_margin(egui::Margin {
                            left: 16,
                            right: 16,
                            top: 8,
                            bottom: 8,
                        })
                        .fill(ctx.style().visuals.panel_fill),
                )
                .show(ctx, |ui| {
                    self.render_toolbar(ui);
                });

            if self.show_toc && self.book.is_some() {
                egui::SidePanel::left("toc_panel")
                    .resizable(true)
                    .default_width(280.0)
                    .frame(
                        egui::Frame::default()
                            .inner_margin(egui::Margin {
                                left: 12,
                                right: 12,
                                top: 12,
                                bottom: 12,
                            })
                            .fill(ctx.style().visuals.window_fill()),
                    )
                    .show(ctx, |ui| {
                        self.render_toc(ui);
                    });
            }
        }

        let reader_fill = if self.view == AppView::Reader {
            self.reader_bg_color
        } else if self.dark_mode {
            egui::Color32::from_rgb(26, 26, 28)
        } else {
            egui::Color32::from_rgb(250, 246, 238)
        };
        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(reader_fill))
            .show(ctx, |ui| match self.view {
                AppView::Library => self.render_library(ui),
                AppView::Reader => self.render_reader(ui),
            });

        if self.show_reader_settings {
            let mut should_close_settings = false;
            let mut settings_rect: Option<egui::Rect> = None;

            let window_resp = egui::Window::new(self.i18n.t("settings.title"))
                .collapsible(false)
                .resizable(true)
                .default_width(460.0)
                .frame(
                    egui::Frame::window(&ctx.style())
                        .corner_radius(12.0)
                        .inner_margin(egui::Margin::same(14)),
                )
                .show(ctx, |ui| {
                    ui.heading(self.i18n.t("settings.title"));
                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        ui.label(self.i18n.t("settings.language"));
                        let current_label = self.i18n.language().label().to_string();
                        egui::ComboBox::from_id_salt("language_combo")
                            .selected_text(&current_label)
                            .show_ui(ui, |ui| {
                                for lang in Language::all() {
                                    if ui
                                        .selectable_label(
                                            self.i18n.language() == lang,
                                            lang.label(),
                                        )
                                        .clicked()
                                    {
                                        self.i18n.set_language(lang.clone());
                                    }
                                }
                            });
                    });

                    ui.add_space(8.0);

                    ui.group(|ui| {
                        ui.label(egui::RichText::new(self.i18n.t("settings.typography")).strong());
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                self.i18n
                                    .tf1("settings.font_size", &format!("{:.0}", self.font_size)),
                            );
                            if ui
                                .add_sized(
                                    [240.0, 18.0],
                                    egui::Slider::new(&mut self.font_size, 12.0..=40.0),
                                )
                                .changed()
                            {
                                self.pages_dirty = true;
                            }
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.label(self.i18n.t("settings.reading_mode"));
                            if ui
                                .selectable_label(self.scroll_mode, self.i18n.t("settings.scroll"))
                                .clicked()
                            {
                                self.scroll_mode = true;
                                self.pages_dirty = true;
                            }
                            if ui
                                .selectable_label(!self.scroll_mode, self.i18n.t("settings.paging"))
                                .clicked()
                            {
                                self.scroll_mode = false;
                                self.pages_dirty = true;
                            }
                        });
                    });

                    ui.add_space(8.0);
                    ui.group(|ui| {
                        ui.label(egui::RichText::new(self.i18n.t("settings.visual")).strong());
                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(self.i18n.t("settings.bg_color"));
                            let presets = [
                                Color32::from_rgb(250, 246, 238),
                                Color32::from_rgb(241, 243, 245),
                                Color32::from_rgb(232, 240, 232),
                                Color32::from_rgb(26, 26, 28),
                                Color32::from_rgb(36, 38, 43),
                            ];
                            for p in presets {
                                let mut btn = egui::Button::new(" ")
                                    .fill(p)
                                    .min_size(egui::vec2(22.0, 22.0));
                                if self.reader_bg_color == p {
                                    btn = btn.stroke(egui::Stroke::new(2.0, Color32::LIGHT_BLUE));
                                }
                                if ui.add(btn).clicked() {
                                    self.reader_bg_color = p;
                                }
                            }
                            egui::color_picker::color_edit_button_srgba(
                                ui,
                                &mut self.reader_bg_color,
                                egui::color_picker::Alpha::Opaque,
                            );
                        });

                        ui.horizontal_wrapped(|ui| {
                            ui.label(self.i18n.t("settings.font_color"));
                            if ui
                                .selectable_label(
                                    self.reader_font_color.is_none(),
                                    self.i18n.t("settings.auto"),
                                )
                                .clicked()
                            {
                                self.reader_font_color = None;
                            }
                            if ui
                                .selectable_label(
                                    self.reader_font_color.is_some(),
                                    self.i18n.t("settings.custom"),
                                )
                                .clicked()
                                && self.reader_font_color.is_none()
                            {
                                self.reader_font_color = Some(Color32::from_gray(30));
                            }
                            if let Some(ref mut color) = self.reader_font_color {
                                egui::color_picker::color_edit_button_srgba(
                                    ui,
                                    color,
                                    egui::color_picker::Alpha::Opaque,
                                );
                            }
                        });

                        ui.horizontal_wrapped(|ui| {
                            ui.label(self.i18n.t("settings.font"));
                            let font_popup_id = ui.make_persistent_id("font_family_popup");
                            let btn = ui.button(&self.reader_font_family);
                            if btn.clicked() {
                                ui.memory_mut(|m| m.toggle_popup(font_popup_id));
                            }
                            egui::popup_below_widget(
                                ui,
                                font_popup_id,
                                &btn,
                                egui::PopupCloseBehavior::CloseOnClickOutside,
                                |ui| {
                                    ui.set_min_width(220.0);
                                    let te = ui.text_edit_singleline(&mut self.font_search);
                                    if btn.clicked() {
                                        te.request_focus();
                                    }
                                    let query = self.font_search.to_lowercase();
                                    let mut close_popup = false;
                                    egui::ScrollArea::vertical()
                                        .max_height(300.0)
                                        .show(ui, |ui| {
                                            for fam in ["Sans", "Serif", "Monospace"] {
                                                if (query.is_empty()
                                                    || fam.to_lowercase().contains(&query))
                                                    && ui
                                                        .selectable_label(
                                                            self.reader_font_family == fam,
                                                            fam,
                                                        )
                                                        .clicked()
                                                {
                                                    self.reader_font_family = fam.to_string();
                                                    self.pages_dirty = true;
                                                    self.embedded_fonts_registered = false;
                                                    close_popup = true;
                                                }
                                            }
                                            let sys_filtered: Vec<String> = self
                                                .system_font_names
                                                .iter()
                                                .filter(|n| {
                                                    query.is_empty()
                                                        || n.to_lowercase().contains(&query)
                                                })
                                                .cloned()
                                                .collect();
                                            if !sys_filtered.is_empty() {
                                                ui.separator();
                                                for name in sys_filtered {
                                                    if ui
                                                        .selectable_label(
                                                            self.reader_font_family == name,
                                                            &name,
                                                        )
                                                        .clicked()
                                                    {
                                                        self.reader_font_family = name;
                                                        self.pages_dirty = true;
                                                        self.embedded_fonts_registered = false;
                                                        close_popup = true;
                                                    }
                                                }
                                            }
                                            let emb_filtered: Vec<String> = self
                                                .embedded_font_names
                                                .iter()
                                                .filter(|n| {
                                                    query.is_empty()
                                                        || n.to_lowercase().contains(&query)
                                                })
                                                .cloned()
                                                .collect();
                                            if !emb_filtered.is_empty() {
                                                ui.separator();
                                                for name in emb_filtered {
                                                    if ui
                                                        .selectable_label(
                                                            self.reader_font_family == name,
                                                            &name,
                                                        )
                                                        .clicked()
                                                    {
                                                        self.reader_font_family = name;
                                                        self.pages_dirty = true;
                                                        self.embedded_fonts_registered = false;
                                                        close_popup = true;
                                                    }
                                                }
                                            }
                                        });
                                    if close_popup {
                                        ui.memory_mut(|m| m.close_popup());
                                    }
                                },
                            );
                        });

                        ui.horizontal_wrapped(|ui| {
                            ui.label(self.i18n.t("settings.page_animation"));
                            for mode in ["Slide", "Cover", "None"] {
                                let label = match mode {
                                    "Slide" => self.i18n.t("settings.slide"),
                                    "Cover" => self.i18n.t("settings.cover"),
                                    _ => self.i18n.t("settings.none"),
                                };
                                if ui
                                    .selectable_label(self.reader_page_animation == mode, label)
                                    .clicked()
                                {
                                    self.reader_page_animation = mode.to_string();
                                }
                            }
                        });
                        if self.reader_page_animation != "None" {
                            ui.add_space(4.0);
                            ui.label(self.i18n.t("settings.animation_speed"));
                            ui.add_sized(
                                [280.0, 18.0],
                                egui::Slider::new(
                                    &mut self.reader_page_animation_speed,
                                    0.04..=0.40,
                                )
                                .step_by(0.02),
                            );
                        }
                    });

                    ui.add_space(8.0);
                    ui.group(|ui| {
                        ui.label(egui::RichText::new(self.i18n.t("settings.bg_image")).strong());
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui.button(self.i18n.t("settings.pick_bg_image")).clicked() {
                                self.pick_reader_background_image();
                            }
                            if ui.button(self.i18n.t("settings.clear_bg_image")).clicked() {
                                self.clear_reader_background_image();
                            }
                        });
                        ui.label(self.i18n.tf1(
                            "settings.opacity",
                            &format!("{}", (self.reader_bg_image_alpha * 100.0) as i32),
                        ));
                        ui.add_sized(
                            [280.0, 18.0],
                            egui::Slider::new(&mut self.reader_bg_image_alpha, 0.0..=1.0),
                        );
                    });

                    ui.add_space(10.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(self.i18n.t("settings.close")).clicked() {
                            should_close_settings = true;
                        }
                    });
                });

            if let Some(resp) = window_resp {
                settings_rect = Some(resp.response.rect);
            }

            // 点击弹窗外区域自动关闭。避免"刚打开就关闭"：只在该帧开始时已打开时启用。
            if show_reader_settings_at_frame_start && ctx.input(|i| i.pointer.primary_clicked()) {
                if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                    if let Some(rect) = settings_rect {
                        if !rect.contains(pos) {
                            should_close_settings = true;
                        }
                    }
                }
            }

            if should_close_settings {
                self.show_reader_settings = false;
            }
        }

        // ── Sharing Panel ──
        if self.show_sharing_panel {
            self.render_sharing(ctx);
        }

        // ── About Window ──
        if self.show_about {
            self.render_about(ctx);
        }

        let settings = AppSettings::from_app(self);
        if self.last_saved_settings.as_ref() != Some(&settings) {
            settings.save(&self.data_dir);
            self.last_saved_settings = Some(settings);
        }
    }
}
