//! The Desktop application state machine, defining main app logic.
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// (device_code, user_code, interval, expires_in)
pub type DeviceCodeResult = Result<(String, String, u64, u64), String>;

use eframe::egui;
use egui::Color32;
use serde::{Deserialize, Serialize};

use reader_core::epub::EpubBook;
use reader_core::i18n::{I18n, Language};
use reader_core::library::Library;
use reader_core::sharing::{start_listener, DiscoveredPeer, PeerStore};

type FontDiscoveryResult = Arc<Mutex<Option<(Vec<String>, HashMap<String, String>)>>>;

#[derive(Clone, Debug)]
struct BossHotkeySpec {
    normalized: String,
    ctrl: bool,
    alt: bool,
    shift: bool,
    win: bool,
    key_token: String,
}

#[cfg(target_os = "windows")]
struct BossHotkeyRuntime {
    thread_id: u32,
    worker: Option<std::thread::JoinHandle<()>>,
}

fn default_boss_key() -> String {
    "F1".to_string()
}

fn normalize_boss_hotkey(ctrl: bool, alt: bool, shift: bool, win: bool, key_token: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    if ctrl {
        parts.push("Ctrl".to_string());
    }
    if alt {
        parts.push("Alt".to_string());
    }
    if shift {
        parts.push("Shift".to_string());
    }
    if win {
        parts.push("Win".to_string());
    }
    parts.push(key_token.to_string());
    parts.join("+")
}

fn normalize_boss_key_token(token: &str) -> Option<String> {
    let up = token.trim().to_ascii_uppercase();
    match up.as_str() {
        "" => None,
        "ESC" | "ESCAPE" => Some("Esc".to_string()),
        "SPACE" => Some("Space".to_string()),
        "ENTER" | "RETURN" => Some("Enter".to_string()),
        "TAB" => Some("Tab".to_string()),
        "BACKSPACE" => Some("Backspace".to_string()),
        "INSERT" | "INS" => Some("Insert".to_string()),
        "DELETE" | "DEL" => Some("Delete".to_string()),
        "HOME" => Some("Home".to_string()),
        "END" => Some("End".to_string()),
        "PAGEUP" | "PGUP" => Some("PageUp".to_string()),
        "PAGEDOWN" | "PGDN" => Some("PageDown".to_string()),
        "LEFT" => Some("Left".to_string()),
        "RIGHT" => Some("Right".to_string()),
        "UP" => Some("Up".to_string()),
        "DOWN" => Some("Down".to_string()),
        _ => {
            if up.len() == 1 && up.chars().next().is_some_and(|c| c.is_ascii_alphanumeric()) {
                return Some(up);
            }
            if up
                .strip_prefix('F')
                .and_then(|s| s.parse::<u8>().ok())
                .is_some_and(|n| (1..=12).contains(&n))
            {
                return Some(up);
            }
            None
        }
    }
}

fn is_low_conflict_single_boss_key(token: &str) -> bool {
    token
        .strip_prefix('F')
        .and_then(|s| s.parse::<u8>().ok())
        .is_some_and(|n| (1..=12).contains(&n))
}

fn egui_key_to_boss_token(key: egui::Key) -> Option<&'static str> {
    use egui::Key;
    match key {
        Key::ArrowDown => Some("Down"),
        Key::ArrowLeft => Some("Left"),
        Key::ArrowRight => Some("Right"),
        Key::ArrowUp => Some("Up"),
        Key::Escape => Some("Esc"),
        Key::Tab => Some("Tab"),
        Key::Backspace => Some("Backspace"),
        Key::Enter => Some("Enter"),
        Key::Space => Some("Space"),
        Key::Insert => Some("Insert"),
        Key::Delete => Some("Delete"),
        Key::Home => Some("Home"),
        Key::End => Some("End"),
        Key::PageUp => Some("PageUp"),
        Key::PageDown => Some("PageDown"),
        Key::Num0 => Some("0"),
        Key::Num1 => Some("1"),
        Key::Num2 => Some("2"),
        Key::Num3 => Some("3"),
        Key::Num4 => Some("4"),
        Key::Num5 => Some("5"),
        Key::Num6 => Some("6"),
        Key::Num7 => Some("7"),
        Key::Num8 => Some("8"),
        Key::Num9 => Some("9"),
        Key::A => Some("A"),
        Key::B => Some("B"),
        Key::C => Some("C"),
        Key::D => Some("D"),
        Key::E => Some("E"),
        Key::F => Some("F"),
        Key::G => Some("G"),
        Key::H => Some("H"),
        Key::I => Some("I"),
        Key::J => Some("J"),
        Key::K => Some("K"),
        Key::L => Some("L"),
        Key::M => Some("M"),
        Key::N => Some("N"),
        Key::O => Some("O"),
        Key::P => Some("P"),
        Key::Q => Some("Q"),
        Key::R => Some("R"),
        Key::S => Some("S"),
        Key::T => Some("T"),
        Key::U => Some("U"),
        Key::V => Some("V"),
        Key::W => Some("W"),
        Key::X => Some("X"),
        Key::Y => Some("Y"),
        Key::Z => Some("Z"),
        Key::F1 => Some("F1"),
        Key::F2 => Some("F2"),
        Key::F3 => Some("F3"),
        Key::F4 => Some("F4"),
        Key::F5 => Some("F5"),
        Key::F6 => Some("F6"),
        Key::F7 => Some("F7"),
        Key::F8 => Some("F8"),
        Key::F9 => Some("F9"),
        Key::F10 => Some("F10"),
        Key::F11 => Some("F11"),
        Key::F12 => Some("F12"),
        _ => None,
    }
}

fn boss_hotkey_spec_from_key(modifiers: egui::Modifiers, key: egui::Key) -> Option<BossHotkeySpec> {
    let key_token = egui_key_to_boss_token(key)?.to_string();
    Some(BossHotkeySpec {
        normalized: normalize_boss_hotkey(
            modifiers.ctrl,
            modifiers.alt,
            modifiers.shift,
            modifiers.mac_cmd || modifiers.command,
            &key_token,
        ),
        ctrl: modifiers.ctrl,
        alt: modifiers.alt,
        shift: modifiers.shift,
        win: modifiers.mac_cmd || modifiers.command,
        key_token,
    })
}

fn parse_boss_hotkey(input: &str) -> Option<BossHotkeySpec> {
    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut win = false;
    let mut key_token: Option<String> = None;

    for raw in input.split('+') {
        let token = raw.trim();
        if token.is_empty() {
            continue;
        }
        let up = token.to_ascii_uppercase();
        match up.as_str() {
            "CTRL" | "CONTROL" => ctrl = true,
            "ALT" => alt = true,
            "SHIFT" => shift = true,
            "WIN" | "WINDOWS" | "META" => win = true,
            _ => {
                if key_token.is_some() {
                    return None;
                }
                key_token = normalize_boss_key_token(token);
            }
        }
    }

    let key_token = key_token?;
    Some(BossHotkeySpec {
        normalized: normalize_boss_hotkey(ctrl, alt, shift, win, &key_token),
        ctrl,
        alt,
        shift,
        win,
        key_token,
    })
}

#[cfg(target_os = "windows")]
fn boss_key_token_to_vk(token: &str) -> Option<u32> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        VK_BACK, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_HOME, VK_INSERT, VK_LEFT,
        VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SPACE, VK_TAB, VK_UP,
    };
    match token {
        "Esc" => Some(VK_ESCAPE.into()),
        "Space" => Some(VK_SPACE.into()),
        "Enter" => Some(VK_RETURN.into()),
        "Tab" => Some(VK_TAB.into()),
        "Backspace" => Some(VK_BACK.into()),
        "Insert" => Some(VK_INSERT.into()),
        "Delete" => Some(VK_DELETE.into()),
        "Home" => Some(VK_HOME.into()),
        "End" => Some(VK_END.into()),
        "PageUp" => Some(VK_PRIOR.into()),
        "PageDown" => Some(VK_NEXT.into()),
        "Left" => Some(VK_LEFT.into()),
        "Right" => Some(VK_RIGHT.into()),
        "Up" => Some(VK_UP.into()),
        "Down" => Some(VK_DOWN.into()),
        _ => {
            if token.len() == 1 {
                return token.chars().next().map(|c| c as u32);
            }
            let n = token.strip_prefix('F')?.parse::<u32>().ok()?;
            if !(1..=12).contains(&n) {
                return None;
            }
            Some(u32::from(VK_F1) + (n - 1))
        }
    }
}

#[cfg(target_os = "windows")]
fn boss_key_modifiers(spec: &BossHotkeySpec) -> u32 {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
    };

    let mut modifiers = MOD_NOREPEAT;
    if spec.ctrl {
        modifiers |= MOD_CONTROL;
    }
    if spec.alt {
        modifiers |= MOD_ALT;
    }
    if spec.shift {
        modifiers |= MOD_SHIFT;
    }
    if spec.win {
        modifiers |= MOD_WIN;
    }
    modifiers
}

#[cfg(target_os = "windows")]
fn start_boss_hotkey_runtime(spec: &BossHotkeySpec) -> Result<BossHotkeyRuntime, ()> {
    use std::sync::mpsc;
    use std::time::Duration;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetMessageW, PeekMessageW, MSG, PM_NOREMOVE, WM_HOTKEY,
    };

    const BOSS_HOTKEY_ID: i32 = 0x5245;
    let Some(vk) = boss_key_token_to_vk(&spec.key_token) else {
        return Err(());
    };
    let modifiers = boss_key_modifiers(spec);
    let (tx, rx) = mpsc::channel();
    let worker = std::thread::spawn(move || unsafe {
        use windows_sys::Win32::System::Threading::GetCurrentThreadId;

        let thread_id = GetCurrentThreadId();
        let mut msg = std::mem::zeroed::<MSG>();
        PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE);
        if RegisterHotKey(std::ptr::null_mut(), BOSS_HOTKEY_ID, modifiers, vk) == 0 {
            let _ = tx.send(Err(()));
            return;
        }
        let _ = tx.send(Ok(thread_id));

        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            if msg.message == WM_HOTKEY {
                toggle_main_window_visibility();
            }
        }

        UnregisterHotKey(std::ptr::null_mut(), BOSS_HOTKEY_ID);
    });

    match rx.recv_timeout(Duration::from_secs(2)) {
        Ok(Ok(thread_id)) => Ok(BossHotkeyRuntime {
            thread_id,
            worker: Some(worker),
        }),
        _ => {
            let _ = worker.join();
            Err(())
        }
    }
}

#[cfg(target_os = "windows")]
fn current_process_main_window() -> Option<windows_sys::Win32::Foundation::HWND> {
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowThreadProcessId, GW_OWNER,
    };

    struct Search {
        pid: u32,
        hwnd: HWND,
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = &mut *(lparam as *mut Search);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == search.pid && GetWindow(hwnd, GW_OWNER) == std::ptr::null_mut() {
            search.hwnd = hwnd;
            return 0;
        }
        1
    }

    let mut search = Search {
        pid: unsafe { GetCurrentProcessId() },
        hwnd: std::ptr::null_mut(),
    };
    unsafe {
        EnumWindows(Some(enum_proc), &mut search as *mut Search as isize);
    }
    (search.hwnd != std::ptr::null_mut()).then_some(search.hwnd)
}

#[cfg(target_os = "windows")]
fn toggle_main_window_visibility() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IsWindowVisible, SetForegroundWindow, ShowWindow, SW_HIDE, SW_RESTORE, SW_SHOW,
    };

    if let Some(hwnd) = current_process_main_window() {
        unsafe {
            if IsWindowVisible(hwnd) != 0 {
                ShowWindow(hwnd, SW_HIDE);
            } else {
                ShowWindow(hwnd, SW_SHOW);
                ShowWindow(hwnd, SW_RESTORE);
                SetForegroundWindow(hwnd);
            }
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

fn default_line_spacing() -> f32 {
    1.8
}

fn default_para_spacing() -> f32 {
    0.6
}

fn default_text_indent() -> u8 {
    2
}

fn default_bg_opacity() -> f32 {
    1.0
}

fn default_reader_toolbar_visible() -> bool {
    false
}

fn generate_pin() -> String {
    use rand::RngCore;
    let val = rand::rngs::OsRng.next_u32() % 10000;
    format!("{:04}", val)
}

fn resize_direction_from_pointer(
    rect: egui::Rect,
    pointer_pos: egui::Pos2,
    border: f32,
) -> Option<egui::ResizeDirection> {
    let left = pointer_pos.x <= rect.left() + border;
    let right = pointer_pos.x >= rect.right() - border;
    let top = pointer_pos.y <= rect.top() + border;
    let bottom = pointer_pos.y >= rect.bottom() - border;

    match (left, right, top, bottom) {
        (true, false, true, false) => Some(egui::ResizeDirection::NorthWest),
        (false, true, true, false) => Some(egui::ResizeDirection::NorthEast),
        (true, false, false, true) => Some(egui::ResizeDirection::SouthWest),
        (false, true, false, true) => Some(egui::ResizeDirection::SouthEast),
        (true, false, false, false) => Some(egui::ResizeDirection::West),
        (false, true, false, false) => Some(egui::ResizeDirection::East),
        (false, false, true, false) => Some(egui::ResizeDirection::North),
        (false, false, false, true) => Some(egui::ResizeDirection::South),
        _ => None,
    }
}

fn cursor_icon_for_resize(direction: egui::ResizeDirection) -> egui::CursorIcon {
    match direction {
        egui::ResizeDirection::North | egui::ResizeDirection::South => {
            egui::CursorIcon::ResizeVertical
        }
        egui::ResizeDirection::East | egui::ResizeDirection::West => {
            egui::CursorIcon::ResizeHorizontal
        }
        egui::ResizeDirection::NorthEast | egui::ResizeDirection::SouthWest => {
            egui::CursorIcon::ResizeNeSw
        }
        egui::ResizeDirection::NorthWest | egui::ResizeDirection::SouthEast => {
            egui::CursorIcon::ResizeNwSe
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
struct AppSettings {
    font_size: f32,
    dark_mode: bool,
    reader_bg_color: [u8; 4],
    #[serde(default = "default_bg_opacity")]
    reader_bg_opacity: f32,
    reader_font_color: Option<[u8; 4]>,
    reader_font_family: String,
    reader_page_animation: String,
    #[serde(default = "default_anim_speed")]
    reader_page_animation_speed: f32,
    reader_bg_image_path: Option<String>,
    reader_bg_image_alpha: f32,
    scroll_mode: bool,
    show_toc: bool,
    #[serde(default = "default_reader_toolbar_visible")]
    reader_toolbar_visible: bool,
    #[serde(default)]
    language: String,
    #[serde(default)]
    last_book_path: Option<String>,
    #[serde(default)]
    last_chapter: usize,
    #[serde(default)]
    auto_start_sharing: bool,
    #[serde(default = "default_line_spacing")]
    line_spacing: f32,
    #[serde(default = "default_para_spacing")]
    para_spacing: f32,
    #[serde(default = "default_text_indent")]
    text_indent: u8,
    #[serde(default)]
    auto_scroll_speed: f32,
    #[serde(default)]
    tts_voice_name: String,
    #[serde(default)]
    tts_rate: i32,
    #[serde(default)]
    tts_volume: i32,
    #[serde(default)]
    translate_api_url: String,
    #[serde(default)]
    translate_api_key: String,
    #[serde(default)]
    dictionary_api_url: String,
    #[serde(default)]
    dictionary_api_key: String,
    #[serde(default = "default_boss_key")]
    boss_key_shortcut: String,
    #[serde(default)]
    csc_mode: reader_core::csc::CorrectionMode,
    #[serde(default)]
    csc_threshold: reader_core::csc::CscThreshold,
    #[serde(default)]
    github_username: Option<String>,
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
            reader_bg_opacity: app.reader_bg_opacity,
            reader_font_color: app.reader_font_color.map(Self::from_color),
            reader_font_family: app.reader_font_family.clone(),
            reader_page_animation: app.reader_page_animation.clone(),
            reader_page_animation_speed: app.reader_page_animation_speed,
            reader_bg_image_path: app.reader_bg_image_path.clone(),
            reader_bg_image_alpha: app.reader_bg_image_alpha,
            scroll_mode: app.scroll_mode,
            show_toc: app.show_toc,
            reader_toolbar_visible: app.reader_toolbar_visible,
            language: app.i18n.language().code().to_string(),
            last_book_path,
            last_chapter,
            auto_start_sharing: app.auto_start_sharing,
            line_spacing: app.line_spacing,
            para_spacing: app.para_spacing,
            text_indent: app.text_indent,
            auto_scroll_speed: app.auto_scroll_speed,
            tts_voice_name: app.tts_voice_name.clone(),
            tts_rate: app.tts_rate,
            tts_volume: app.tts_volume,
            translate_api_url: app.translate_api_url.clone(),
            translate_api_key: app.translate_api_key.clone(),
            dictionary_api_url: app.dictionary_api_url.clone(),
            dictionary_api_key: app.dictionary_api_key.clone(),
            boss_key_shortcut: app.boss_key_shortcut.clone(),
            csc_mode: app.csc_mode.clone(),
            csc_threshold: app.csc_threshold.clone(),
            github_username: app.github_username.clone(),
        }
    }

    fn apply_to_app(&self, app: &mut ReaderApp) {
        app.font_size = self.font_size.clamp(12.0, 40.0);
        app.dark_mode = self.dark_mode;
        app.reader_bg_color = Self::to_color(self.reader_bg_color);
        app.reader_bg_opacity = self.reader_bg_opacity.clamp(0.0, 1.0);
        app.reader_font_color = self.reader_font_color.map(Self::to_color);
        app.reader_font_family = self.reader_font_family.clone();
        app.reader_page_animation = self.reader_page_animation.clone();
        app.reader_page_animation_speed = self.reader_page_animation_speed.clamp(0.04, 0.40);
        app.reader_bg_image_path = self.reader_bg_image_path.clone();
        app.reader_bg_image_alpha = self.reader_bg_image_alpha.clamp(0.0, 1.0);
        app.scroll_mode = self.scroll_mode;
        app.show_toc = self.show_toc;
        app.reader_toolbar_visible = self.reader_toolbar_visible;
        app.auto_start_sharing = self.auto_start_sharing;
        app.line_spacing = self.line_spacing.clamp(0.8, 2.5);
        app.para_spacing = self.para_spacing.clamp(0.0, 2.0);
        app.text_indent = self.text_indent.min(4);
        app.auto_scroll_speed = self.auto_scroll_speed.clamp(0.0, 200.0);
        app.i18n.set_language(Language::from_code(&self.language));
        if !self.tts_voice_name.is_empty() {
            app.tts_voice_name = self.tts_voice_name.clone();
        }
        app.tts_rate = self.tts_rate;
        app.tts_volume = self.tts_volume;
        app.translate_api_url = self.translate_api_url.clone();
        app.translate_api_key = self.translate_api_key.clone();
        app.dictionary_api_url = self.dictionary_api_url.clone();
        app.dictionary_api_key = self.dictionary_api_key.clone();
        app.boss_key_shortcut = self.boss_key_shortcut.clone();
        app.boss_key_input = app.boss_key_shortcut.clone();
        app.csc_mode = self.csc_mode.clone();
        app.csc_threshold = self.csc_threshold.clone();
        app.github_username = self.github_username.clone();
        // Restore GitHub token from OS credential store
        if app.github_username.is_some() {
            app.github_token = reader_core::sharing::keystore::load_github_token();
        }
        // last_book_path/last_chapter applied in Default::default after call
    }
}

#[derive(PartialEq)]
pub enum AppView {
    Library,
    Reader,
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
    pub data_dir: String,
    pub book: Option<EpubBook>,
    pub book_path: Option<String>,
    pub current_book_hash: Option<String>,
    pub last_synced_chapter: Option<usize>,
    pub current_chapter: usize,
    pub font_size: f32,
    pub dark_mode: bool,
    pub reader_bg_color: Color32,
    pub reader_bg_opacity: f32,
    pub reader_font_color: Option<Color32>,
    pub reader_font_family: String,
    pub reader_page_animation: String,
    pub reader_page_animation_speed: f32,
    pub reader_bg_image_path: Option<String>,
    pub reader_bg_image_alpha: f32,
    pub reader_bg_texture: Option<egui::TextureHandle>,
    pub show_settings: bool,
    pub show_toc: bool,
    pub reader_toolbar_visible: bool,
    pub reader_window_level: egui::WindowLevel,
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
    pub paging_page_rect: Option<egui::Rect>,
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
    // ── Search ──
    pub show_search: bool,
    pub search_query: String,
    pub search_results: Vec<reader_core::search::SearchResult>,
    pub search_selected: Option<usize>,
    // ── Annotations ──
    pub show_annotations: bool,
    pub book_config: Option<reader_core::library::BookConfig>,
    // ── Export ──
    pub show_export_dialog: bool,
    #[allow(dead_code)]
    pub export_book_id: Option<String>,
    // ── Stats ──
    pub show_stats: bool,
    pub reading_session_start: Option<u64>,
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
    // ── TTS ──
    pub tts_playing: bool,
    pub tts_paused: bool,
    pub tts_voice_name: String,
    pub tts_rate: i32,   // e.g. 0, -20, +50 (percent)
    pub tts_volume: i32, // e.g. 0, -50, +50 (percent)
    pub tts_current_block: usize,
    pub tts_stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub tts_audio_sink: Option<std::sync::Arc<rodio::Sink>>,
    pub tts_status: std::sync::Arc<std::sync::Mutex<String>>,
    pub show_tts_panel: bool,
    pub tts_pending_audio: Option<std::sync::Arc<std::sync::Mutex<Option<Vec<u8>>>>>,
    /// Prefetched audio for the next block (ready to play immediately when current finishes).
    pub tts_prefetch_audio: Option<std::sync::Arc<std::sync::Mutex<Option<Vec<u8>>>>>,
    /// Block index that the prefetch corresponds to.
    pub tts_prefetch_block: usize,
    pub last_egui_ctx: Option<egui::Context>,
    // ── API Settings ──
    pub translate_api_url: String,
    pub translate_api_key: String,
    pub dictionary_api_url: String,
    pub dictionary_api_key: String,
    pub boss_key_shortcut: String,
    pub boss_key_input: String,
    pub boss_key_status: String,
    pub boss_key_capturing: bool,
    #[cfg(target_os = "windows")]
    boss_hotkey_runtime: Option<BossHotkeyRuntime>,

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
    pub csc_contribute_status: String,
    pub csc_contribute_pr_url: Option<String>,
    pub csc_contribute_rx:
        Option<std::sync::mpsc::Receiver<crate::ui::csc_contribute::ContributeResult>>,
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
}

#[derive(Clone)]
pub struct CrossChapterSnapshot {
    pub blocks: Arc<Vec<reader_core::epub::ContentBlock>>,
    pub block_ranges: Vec<(usize, usize)>,
    pub total_pages: usize,
    pub from_page: usize,
    pub title: String,
}

pub type TxtConvertSlot = Arc<Mutex<Option<Result<reader_core::txt::ConvertResult, String>>>>;

/// TXT 导入对话框状态。
pub struct TxtImportState {
    pub txt_path: PathBuf,
    pub title: String,
    pub author: String,
    pub custom_regex: String,
    pub use_heuristic: bool,
    pub previews: Vec<reader_core::txt::ChapterPreview>,
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

        // 立即做一次预览
        let config = reader_core::txt::SplitConfig::default();
        let previews = reader_core::txt::preview_chapters(&txt_path, &config).unwrap_or_default();

        Self {
            txt_path,
            title,
            author: String::new(),
            custom_regex: String::new(),
            use_heuristic: false,
            previews,
            converting: false,
            error: None,
            result_slot: None,
        }
    }
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
            reader_bg_opacity: default_bg_opacity(),
            reader_font_color: None,
            reader_font_family: "Sans".to_string(),
            reader_page_animation: "Slide".to_string(),
            reader_page_animation_speed: 0.14,
            reader_bg_image_path: None,
            reader_bg_image_alpha: 0.22,
            reader_bg_texture: None,
            show_settings: false,
            show_toc: true,
            reader_toolbar_visible: default_reader_toolbar_visible(),
            reader_window_level: egui::WindowLevel::Normal,
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
            paging_page_rect: None,
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
            // Search
            show_search: false,
            search_query: String::new(),
            search_results: Vec::new(),
            search_selected: None,
            // Annotations
            show_annotations: false,
            book_config: None,
            // Export
            show_export_dialog: false,
            export_book_id: None,
            // Stats
            show_stats: false,
            reading_session_start: None,
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
            // TTS
            tts_playing: false,
            tts_paused: false,
            tts_voice_name: "zh-CN-XiaoxiaoNeural".to_string(),
            tts_rate: 0,
            tts_volume: 0,
            tts_current_block: 0,
            tts_stop_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            tts_audio_sink: None,
            tts_status: std::sync::Arc::new(std::sync::Mutex::new(String::new())),
            show_tts_panel: false,
            tts_pending_audio: None,
            tts_prefetch_audio: None,
            tts_prefetch_block: 0,
            last_egui_ctx: None,
            // API settings
            translate_api_url: String::new(),
            translate_api_key: String::new(),
            dictionary_api_url: String::new(),
            dictionary_api_key: String::new(),
            boss_key_shortcut: default_boss_key(),
            boss_key_input: default_boss_key(),
            boss_key_status: String::new(),
            boss_key_capturing: false,
            #[cfg(target_os = "windows")]
            boss_hotkey_runtime: None,
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
            csc_contribute_status: String::new(),
            csc_contribute_pr_url: None,
            csc_contribute_rx: None,
        };

        if let Some(settings) = AppSettings::load(&app.data_dir) {
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
        let initial_boss_key = app.boss_key_shortcut.clone();
        app.rebind_boss_hotkey(initial_boss_key);
        app.push_feedback_log(format!(
            "[Init] app initialized (data_dir={})",
            app.data_dir
        ));
        app.last_saved_settings = Some(AppSettings::from_app(&app));

        // ── 启动时检查更新 ──
        {
            let slot: Arc<Mutex<Option<UpdateState>>> = Arc::new(Mutex::new(None));
            let slot_clone = slot.clone();
            std::thread::spawn(move || {
                let result = match crate::self_update::check_latest_version() {
                    Some((tag, _name)) => UpdateState::Available(tag),
                    None => UpdateState::UpToDate,
                };
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
            .add_filter("EPUB / TXT", &["epub", "txt"])
            .add_filter("EPUB", &["epub"])
            .add_filter("TXT", &["txt"])
            .pick_file()
        {
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

    pub fn open_book_from_path(&mut self, path: &str, chapter: Option<usize>) {
        self.push_feedback_log(format!(
            "[Book] open_book_from_path: path={}, chapter={:?}",
            path, chapter
        ));
        match EpubBook::open(path) {
            Ok(mut book) => {
                self.push_feedback_log(format!(
                    "[Book] opened: title={}, chapters={}, fonts={}",
                    book.title,
                    book.chapters.len(),
                    book.fonts.len()
                ));
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
                // Load annotation config and start reading timer
                self.book_config =
                    reader_core::library::Library::read_book_config(&self.data_dir, &entry.id);
                self.reading_session_start = Some(reader_core::now_secs());
                self.push_feedback_log(format!(
                    "[Book] ready: chapter={}/{}, embedded_fonts={}, path={}",
                    ch,
                    self.total_chapters(),
                    self.embedded_font_names.len(),
                    entry.path
                ));
                // Trigger CSC correction for current + adjacent chapters
                self.csc_cache.clear();
                self.csc_trigger_chapter(ch);
                let total = self.total_chapters();
                if ch > 0 {
                    self.csc_trigger_chapter(ch - 1);
                }
                if ch + 1 < total {
                    self.csc_trigger_chapter(ch + 1);
                }
            }
            Err(e) => {
                self.push_feedback_log(format!("[Book] ERROR opening {}: {}", path, e));
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
            if let Some(chapter) = book.chapters.get(chapter_idx) {
                chapter
                    .blocks
                    .iter()
                    .enumerate()
                    .filter_map(|(i, block)| {
                        let spans = match block {
                            ContentBlock::Paragraph { spans } => spans,
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

    /// Record elapsed reading time into the book config.
    pub fn flush_reading_stats(&mut self) {
        let Some(start) = self.reading_session_start.take() else {
            return;
        };
        let elapsed = reader_core::now_secs().saturating_sub(start);
        if elapsed < 5 {
            return;
        }
        if let Some(cfg) = &mut self.book_config {
            let stats = cfg.reading_stats.get_or_insert_with(Default::default);
            stats.total_seconds += elapsed;
            let today = {
                let secs = reader_core::now_secs();
                let days = secs / 86400;
                format!("{}", days) // simple day-key
            };
            if let Some(session) = stats.sessions.iter_mut().find(|s| s.date == today) {
                session.seconds += elapsed;
            } else {
                stats.sessions.push(reader_core::library::ReadingSession {
                    date: today,
                    seconds: elapsed,
                });
            }
            cfg.save(&self.data_dir);
        }
    }

    /// Save current book_config to disk.
    #[allow(dead_code)]
    pub fn save_book_config(&self) {
        if let Some(cfg) = &self.book_config {
            cfg.save(&self.data_dir);
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

    fn render_update_dialog(&mut self, ctx: &egui::Context) {
        let tag = self.update_latest_tag.clone().unwrap_or_default();
        let mut open = self.show_update_dialog;
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
                            ui.horizontal(|ui| {
                                if ui
                                    .button(
                                        egui::RichText::new(self.i18n.t("update.download_update"))
                                            .size(14.0),
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
                                    .button(
                                        egui::RichText::new(self.i18n.t("feedback.not_now"))
                                            .size(14.0),
                                    )
                                    .clicked()
                                {
                                    self.show_update_dialog = false;
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
                                self.show_update_dialog = false;
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
                        _ => {}
                    }
                    ui.add_space(8.0);
                });
            });
        self.show_update_dialog = open;
    }
}

impl ReaderApp {
    pub(crate) fn default_custom_font_color(&self) -> Color32 {
        if self.dark_mode {
            Color32::from_gray(220)
        } else {
            Color32::from_gray(30)
        }
    }

    pub(crate) fn set_reader_chrome_visible(&mut self, visible: bool) {
        self.reader_toolbar_visible = visible;
        self.show_toc = visible;
        if self.show_toc {
            self.scroll_toc_to_current = true;
        }
    }

    fn handle_reader_shortcuts(&mut self, ctx: &egui::Context) {
        if self.view != AppView::Reader || self.boss_key_capturing {
            return;
        }

        if ctx.input(|i| i.key_pressed(egui::Key::F2)) {
            self.set_reader_chrome_visible(!self.reader_toolbar_visible);
        }
    }

    fn sync_reader_window_level(&mut self, ctx: &egui::Context) {
        let desired = if self.view == AppView::Reader {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        };

        if self.reader_window_level != desired {
            self.reader_window_level = desired;
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(desired));
        }
    }

    fn handle_root_viewport_resize(&self, ctx: &egui::Context) {
        const BORDER: f32 = 6.0;

        let Some(pointer_pos) = ctx.input(|i| i.pointer.hover_pos()) else {
            return;
        };

        let rect = ctx.screen_rect();
        let Some(direction) = resize_direction_from_pointer(rect, pointer_pos, BORDER) else {
            return;
        };

        ctx.set_cursor_icon(cursor_icon_for_resize(direction));
        if ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
        }
    }

    pub(crate) fn handle_window_drag_zone(ui: &mut egui::Ui, id_suffix: &'static str) {
        let drag_response = ui.interact(
            ui.max_rect(),
            ui.id().with(id_suffix),
            egui::Sense::click_and_drag(),
        );
        if drag_response.drag_started_by(egui::PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
    }

    pub fn reader_bg_fill_color(&self) -> Color32 {
        let [r, g, b, _] = self.reader_bg_color.to_array();
        let alpha = (self.reader_bg_opacity * 255.0).round() as u8;
        Color32::from_rgba_unmultiplied(r, g, b, alpha)
    }

    pub fn begin_boss_key_capture(&mut self) {
        self.boss_key_capturing = true;
        self.boss_key_status = self.i18n.t("settings.boss_key_capturing").to_string();
    }

    pub fn cancel_boss_key_capture(&mut self) {
        self.boss_key_capturing = false;
        self.refresh_boss_key_status();
    }

    pub fn clear_boss_key(&mut self) {
        self.stop_boss_hotkey_runtime();
        self.boss_key_shortcut.clear();
        self.boss_key_input.clear();
        self.boss_key_capturing = false;
        self.boss_key_status = self.i18n.t("settings.boss_key_deleted").to_string();
    }

    pub fn poll_boss_key_capture(&mut self, ctx: &egui::Context) {
        if !self.boss_key_capturing {
            return;
        }

        let events = ctx.input(|i| i.events.clone());
        for event in events {
            let egui::Event::Key {
                key,
                pressed,
                repeat,
                modifiers,
                ..
            } = event
            else {
                continue;
            };
            if !pressed || repeat {
                continue;
            }

            let Some(spec) = boss_hotkey_spec_from_key(modifiers, key) else {
                continue;
            };
            self.boss_key_capturing = false;
            self.apply_boss_hotkey_spec(spec);
            break;
        }
    }

    fn apply_boss_hotkey_spec(&mut self, spec: BossHotkeySpec) {
        let previous_shortcut = self.boss_key_shortcut.clone();
        if !spec.ctrl
            && !spec.alt
            && !spec.shift
            && !spec.win
            && !is_low_conflict_single_boss_key(&spec.key_token)
        {
            self.boss_key_status = self
                .i18n
                .t("settings.boss_key_single_key_limited")
                .to_string();
            return;
        }

        self.boss_key_shortcut = spec.normalized.clone();
        self.boss_key_input = self.boss_key_shortcut.clone();
        self.rebind_boss_hotkey(previous_shortcut);
    }

    pub fn apply_boss_key_from_input(&mut self) {
        let Some(spec) = parse_boss_hotkey(&self.boss_key_input) else {
            self.boss_key_status = self.i18n.t("settings.boss_key_invalid").to_string();
            return;
        };
        self.apply_boss_hotkey_spec(spec);
    }

    fn refresh_boss_key_status(&mut self) {
        if self.boss_key_shortcut.is_empty() {
            self.boss_key_status = self.i18n.t("settings.boss_key_disabled").to_string();
        } else {
            self.boss_key_status = self
                .i18n
                .tf1("settings.boss_key_active", &self.boss_key_shortcut);
        }
    }

    fn rebind_boss_hotkey(&mut self, previous_shortcut: String) {
        let previous_spec = parse_boss_hotkey(&previous_shortcut);

        let new_shortcut = self.boss_key_shortcut.trim().to_string();
        if new_shortcut.is_empty() {
            self.stop_boss_hotkey_runtime();
            self.boss_key_shortcut.clear();
            self.boss_key_input.clear();
            self.boss_key_status = self.i18n.t("settings.boss_key_disabled").to_string();
            return;
        }

        let Some(spec) = parse_boss_hotkey(&new_shortcut) else {
            self.boss_key_shortcut = previous_shortcut;
            self.boss_key_input = self.boss_key_shortcut.clone();
            self.boss_key_status = self.i18n.t("settings.boss_key_invalid").to_string();
            return;
        };

        #[cfg(target_os = "windows")]
        {
            self.stop_boss_hotkey_runtime();

            if !spec.ctrl
                && !spec.alt
                && !spec.shift
                && !spec.win
                && !is_low_conflict_single_boss_key(&spec.key_token)
            {
                self.boss_key_shortcut = previous_shortcut;
                self.boss_key_input = self.boss_key_shortcut.clone();
                if let Some(old_spec) = previous_spec.clone() {
                    self.boss_hotkey_runtime = start_boss_hotkey_runtime(&old_spec).ok();
                }
                self.boss_key_status = self
                    .i18n
                    .t("settings.boss_key_single_key_limited")
                    .to_string();
                return;
            }

            let Some(_) = boss_key_token_to_vk(&spec.key_token) else {
                self.boss_key_shortcut = previous_shortcut;
                self.boss_key_input = self.boss_key_shortcut.clone();
                if let Some(old_spec) = previous_spec.clone() {
                    self.boss_hotkey_runtime = start_boss_hotkey_runtime(&old_spec).ok();
                }
                self.boss_key_status = self.i18n.t("settings.boss_key_invalid").to_string();
                return;
            };

            match start_boss_hotkey_runtime(&spec) {
                Ok(runtime) => {
                    self.boss_hotkey_runtime = Some(runtime);
                    self.boss_key_shortcut = spec.normalized.clone();
                    self.boss_key_input = self.boss_key_shortcut.clone();
                    self.boss_key_status =
                        self.i18n.tf1("settings.boss_key_active", &spec.normalized);
                }
                Err(_) => {
                    self.boss_key_shortcut = previous_shortcut;
                    self.boss_key_input = self.boss_key_shortcut.clone();
                    if let Some(old_spec) = previous_spec {
                        self.boss_hotkey_runtime = start_boss_hotkey_runtime(&old_spec).ok();
                    }
                    self.boss_key_status = self.i18n.t("settings.boss_key_conflict").to_string();
                }
            }
            return;
        }

        #[cfg(not(target_os = "windows"))]
        {
            self.boss_key_status = self.i18n.t("settings.boss_key_unsupported").to_string();
        }
    }

    fn stop_boss_hotkey_runtime(&mut self) {
        #[cfg(target_os = "windows")]
        if let Some(mut runtime) = self.boss_hotkey_runtime.take() {
            use windows_sys::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};

            unsafe {
                PostThreadMessageW(runtime.thread_id, WM_QUIT, 0, 0);
            }
            if let Some(worker) = runtime.worker.take() {
                let _ = worker.join();
            }
        }
    }
}

impl Drop for ReaderApp {
    fn drop(&mut self) {
        self.stop_boss_hotkey_runtime();
    }
}

impl eframe::App for ReaderApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Color32::TRANSPARENT.to_normalized_gamma_f32()
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.last_egui_ctx = Some(ctx.clone());
        self.sync_reader_window_level(ctx);
        self.handle_root_viewport_resize(ctx);
        self.handle_reader_shortcuts(ctx);
        self.poll_boss_key_capture(ctx);
        if !self.show_settings && self.boss_key_capturing {
            self.cancel_boss_key_capture();
        }
        // --- Poll TTS audio ---
        self.tts_poll_audio();
        // --- Poll CSC model download ---
        self.csc_poll_download();
        // --- Poll GitHub OAuth ---
        self.github_poll_device_code();
        self.github_poll_token();
        self.github_poll_token_result();
        // --- Poll CSC Contribution ---
        self.csc_poll_contribution();
        // --- Check async font discovery result ---
        if self.system_font_names.is_empty() {
            if let Ok(mut slot) = self.font_discovery_result.lock() {
                if let Some((names, paths)) = slot.take() {
                    self.system_font_names = names;
                    self.system_font_paths = paths;
                }
            }
        }

        // --- Poll CSC background results ---
        self.csc_poll_results();

        // --- Poll startup update check result ---
        if matches!(self.update_state, UpdateState::Checking) {
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

            // Emoji / symbol fallback font
            let emoji_paths: &[&str] = &[
                "C:\\Windows\\Fonts\\seguisym.ttf",
                "C:\\Windows\\Fonts\\seguiemj.ttf",
                "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
                "/System/Library/Fonts/Apple Color Emoji.ttc",
            ];
            for path in emoji_paths {
                if let Ok(data) = std::fs::read(path) {
                    fonts.font_data.insert(
                        "emoji_font".to_owned(),
                        egui::FontData::from_owned(data).into(),
                    );
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

        if self.view == AppView::Reader && !self.show_sharing_panel && !ctx.wants_keyboard_input() {
            ctx.input(|i| {
                if i.key_pressed(egui::Key::A) {
                    self.prev_chapter();
                }
                if i.key_pressed(egui::Key::D) {
                    self.next_chapter();
                }
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

        if self.view == AppView::Reader {
            self.ensure_reader_bg_texture(ctx);
            if self.reader_toolbar_visible {
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
                        Self::handle_window_drag_zone(ui, "toolbar_drag_zone");
                        self.render_toolbar(ui);
                    });
            }

            // TTS bar (between toolbar and content, Edge-style)
            if self.show_tts_panel {
                self.render_tts_bar(ctx);
            }

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
                        Self::handle_window_drag_zone(ui, "toc_drag_zone");
                        self.render_toc(ui);
                    });
            }
        }

        let reader_fill = if self.view == AppView::Reader {
            self.reader_bg_fill_color()
        } else if self.dark_mode {
            egui::Color32::from_rgb(26, 26, 28)
        } else {
            egui::Color32::from_rgb(250, 246, 238)
        };

        // ── Side-panels (rendered before CentralPanel) ──
        self.render_search_panel(ctx);
        self.render_annotations_panel(ctx);
        if self.show_settings {
            self.render_settings_panel(ctx);
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(reader_fill))
            .show(ctx, |ui| match self.view {
                AppView::Library => {
                    Self::handle_window_drag_zone(ui, "library_drag_zone");
                    self.render_library(ui);
                }
                AppView::Reader => {
                    Self::handle_window_drag_zone(ui, "reader_drag_zone");
                    self.render_reader(ui);
                }
            });

        // ── Floating windows ──
        self.render_export_dialog(ctx);
        self.render_stats_window(ctx);

        // ── Sharing Panel ──
        if self.show_sharing_panel {
            self.render_sharing(ctx);
        }

        // ── About Window ──
        if self.show_about {
            self.render_about(ctx);
        }

        // ── CSC Contribute Dialog ──
        if self.show_csc_contribute_dialog {
            self.render_csc_contribute_dialog(ctx);
        }

        // ── TXT Import ──
        if self.txt_import.is_some() {
            self.render_txt_import(ctx);
        }

        let settings = AppSettings::from_app(self);
        if self.last_saved_settings.as_ref() != Some(&settings) {
            settings.save(&self.data_dir);
            self.last_saved_settings = Some(settings);
        }
    }
}
