//! JNI bridge and bindings for the Android platform.

use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jstring};
use jni::JNIEnv;
use reader_core::epub::EpubBook;
use reader_core::i18n::{I18n, Language};

macro_rules! dbg_log {
    ($($arg:tt)*) => {
        reader_core::sharing::share_dbg_log(&format!($($arg)*));
    };
}

/// Safely create a JNI string and return its raw pointer.
/// Returns null on failure instead of panicking.
macro_rules! jni_string_or_null {
    ($env:expr, $val:expr) => {
        match $env.new_string($val) {
            Ok(s) => s.into_raw(),
            Err(e) => {
                dbg_log!("JNI new_string failed: {}", e);
                std::ptr::null_mut()
            }
        }
    };
}

#[allow(unused_macros)]
/// Safely lock a mutex, returning null_mut() jstring on failure.
macro_rules! lock_or_null {
    ($mutex:expr) => {
        match $mutex.lock() {
            Ok(guard) => guard,
            Err(e) => {
                dbg_log!("Mutex lock poisoned: {}", e);
                return std::ptr::null_mut();
            }
        }
    };
}

#[allow(unused_macros)]
/// Safely lock a mutex, returning () on failure (for void JNI functions).
macro_rules! lock_or_return {
    ($mutex:expr) => {
        match $mutex.lock() {
            Ok(guard) => guard,
            Err(e) => {
                dbg_log!("Mutex lock poisoned: {}", e);
                return;
            }
        }
    };
}
use reader_core::library::Library;
// Propagate IO failures as Java exceptions; never report a successful mutation.
macro_rules! checked_storage {
    ($env:expr, $result:expr) => {
        match $result {
            Ok(value) => value,
            Err(error) => {
                let _ = $env.throw_new("java/io/IOException", error);
                return Default::default();
            }
        }
    };
}

mod image_transport;

use reader_core::base64_encode;
use reader_core::sharing::protocol::Message;
use reader_core::sharing::{
    auto_sync_session, connect_to_peer, handle_client, resolve_broadcast_addr, start_broadcast,
    start_listener, start_server, DiscoveredPeer, DiscoveryAnnouncement, PeerStore,
};

use lru::LruCache;
use once_cell::sync::Lazy;
use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

// Global cache for parsed EPUB books to prevent re-parsing the ZIP file and HTML
// every time Java requests a chapter. Keep only the current publication: a
// single very large novel can otherwise multiply memory use across book opens.
static BOOK_CACHE: Lazy<Mutex<LruCache<String, Arc<EpubBook>>>> =
    Lazy::new(|| Mutex::new(LruCache::new(NonZeroUsize::new(1).unwrap())));

fn get_book(path: &str) -> std::result::Result<Arc<EpubBook>, String> {
    let mut cache = BOOK_CACHE
        .lock()
        .map_err(|e| format!("BOOK_CACHE lock poisoned: {}", e))?;
    if let Some(book) = cache.get(path) {
        return Ok(book.clone());
    }

    // We need to parse it if not in cache
    match EpubBook::open_cached(
        path,
        &std::path::Path::new(path)
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(".chapter-index"),
    ) {
        Ok(book) => {
            reader_core::open_operation::check()?;
            let arc_book = Arc::new(book);
            cache.put(path.to_string(), arc_book.clone());
            Ok(arc_book)
        }
        Err(err) => Err(err),
    }
}

fn to_android_last_opened(ts: u64) -> u64 {
    // Core reading clocks use milliseconds; tolerate older second-based catalogs.
    if ts < 10_000_000_000 {
        ts.saturating_mul(1000)
    } else {
        ts
    }
}

fn book_entry_to_android_json(
    data_dir: &str,
    entry: &reader_core::library::BookEntry,
) -> serde_json::Value {
    let config_path = if entry.id.is_empty() {
        None
    } else {
        Some(
            std::path::PathBuf::from(data_dir)
                .join("books")
                .join(format!("{}.json", entry.id))
                .to_string_lossy()
                .to_string(),
        )
    };

    serde_json::json!({
        "id": entry.id.clone(),
        "title": entry.title.clone(),
        "uri": entry.path.clone(),
        "config_path": config_path,
        "lastChapter": entry.last_chapter,
        "lastBlock": entry.last_block,
        "lastCharOffset": entry.last_char_offset,
        "last_chapter_title": entry.last_chapter_title.clone(),
        "lastOpened": to_android_last_opened(entry.last_opened),
    })
}

static OPEN_OPERATIONS: Lazy<reader_core::open_operation::Registry> = Lazy::new(Default::default);

#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_beginBookOpen(
    mut env: JNIEnv,
    _class: JClass,
) -> jni::sys::jlong {
    checked_storage!(env, OPEN_OPERATIONS.begin()) as jni::sys::jlong
}
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_cancelBookOpen(
    _env: JNIEnv,
    _class: JClass,
    id: jni::sys::jlong,
) {
    OPEN_OPERATIONS.cancel(id as u64);
}
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_finishBookOpen(
    _env: JNIEnv,
    _class: JClass,
    id: jni::sys::jlong,
) {
    OPEN_OPERATIONS.finish(id as u64);
}
/// Only opening work holds this scope. Cached books never retain its token.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_openBookCancellable(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
    id: jni::sys::jlong,
    chapter: jni::sys::jint,
) -> jstring {
    let path: String = match env.get_string(&path) {
        Ok(value) => value.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let token = checked_storage!(env, OPEN_OPERATIONS.token(id as u64));
    let result = token.run(|| {
        let book = get_book(&path)?;
        if !book.chapters.is_empty() {
            let _ = book.chapter((chapter.max(0) as usize).min(book.chapters.len() - 1))?;
        }
        Ok(book_metadata(&book))
    });
    let json = checked_storage!(env, result);
    jni_string_or_null!(env, json)
}

/// Open an EPUB file and return its metadata as JSON.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_openBook(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
) -> jstring {
    let path: String = match env.get_string(&path) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };

    let book = match get_book(&path) {
        Ok(b) => b,
        Err(_) => return std::ptr::null_mut(),
    };

    jni_string_or_null!(env, book_metadata(&book))
}

fn book_metadata(book: &EpubBook) -> String {
    let chapter_reviews: Vec<serde_json::Value> = book
        .chapter_reviews
        .iter()
        .map(|(&main, &review)| serde_json::json!({ "main": main, "review": review }))
        .collect();
    let review_chapter_indices: Vec<usize> = {
        let mut v: Vec<usize> = book.review_chapter_indices.iter().copied().collect();
        v.sort_unstable();
        v
    };
    let json = serde_json::json!({
        "title": book.title,
        "chapterCount": book.chapters.len(),
        "chapterTitles": book.chapters.iter().map(|ch| &ch.title).collect::<Vec<_>>(),
        "toc": book.toc.iter().map(|t| serde_json::json!({
            "title": t.title,
            "chapterIndex": t.chapter_index,
        })).collect::<Vec<_>>(),
        "hasCover": book.cover_data.is_some(),
        "chapterReviews": chapter_reviews,
        "reviewChapterIndices": review_chapter_indices,
    });

    json.to_string()
}

/// Get chapter content as JSON array of content blocks.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_getChapter(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
    chapter_index: jni::sys::jint,
) -> jstring {
    let path: String = match env.get_string(&path) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };

    let book = match get_book(&path) {
        Ok(b) => b,
        Err(_) => return std::ptr::null_mut(),
    };

    let json = checked_storage!(
        env,
        image_transport::chapter(&book, chapter_index as usize, None)
    );
    jni_string_or_null!(env, json.to_string())
}

/// Get the cover image bytes (Base64 encoded) for a book.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_getCover(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
) -> jstring {
    let path: String = match env.get_string(&path) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };

    match EpubBook::read_cover(&path).ok().flatten() {
        Some(data) => {
            let encoded = base64_encode(&data);
            jni_string_or_null!(env, encoded)
        }
        None => std::ptr::null_mut(),
    }
}

/// Shelf requests never acquire BOOK_CACHE or parse chapter HTML/CSS.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_getShelfCover(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    path: JString,
) -> jstring {
    let Ok(data_dir) = env.get_string(&data_dir) else {
        return std::ptr::null_mut();
    };
    let data_dir: String = data_dir.into();
    let Ok(path) = env.get_string(&path) else {
        return std::ptr::null_mut();
    };
    let path: String = path.into();
    let cache = std::path::Path::new(&data_dir).join(".cache/shelf-covers-v1");
    match reader_core::library::covers::thumbnail(std::path::Path::new(&path), &cache) {
        Some(bytes) => jni_string_or_null!(env, base64_encode(&bytes)),
        None => std::ptr::null_mut(),
    }
}

/// Load the library index from the data directory.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_loadLibrary(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
) -> jstring {
    let data_dir: String = match env.get_string(&data_dir) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };

    let library = checked_storage!(env, Library::load_from(&data_dir));
    let entries: Vec<serde_json::Value> = library
        .books
        .iter()
        .map(|e| book_entry_to_android_json(&data_dir, e))
        .collect();

    let output =
        env.new_string(serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string()));
    match output {
        Ok(s) => s.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_renameBook(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    path: JString,
    title: JString,
) -> jstring {
    let result = (|| -> Result<serde_json::Value, String> {
        let data_dir: String = env.get_string(&data_dir).map_err(|e| e.to_string())?.into();
        let path: String = env.get_string(&path).map_err(|e| e.to_string())?.into();
        let title: String = env.get_string(&title).map_err(|e| e.to_string())?.into();
        let mut library = Library::load_from(&data_dir)?;
        let id = library
            .books
            .iter()
            .find(|b| b.path == path)
            .ok_or("Book is no longer in the library")?
            .id
            .clone();
        let renamed = library.rename_book(&data_dir, &id, &title)?;
        Ok(book_entry_to_android_json(&data_dir, &renamed))
    })();
    let json = match result {
        Ok(entry) => serde_json::json!({"ok":true, "entry":entry}),
        Err(error) => serde_json::json!({"ok":false, "error":error}),
    };
    jni_string_or_null!(env, json.to_string())
}

#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_prepareBookExports(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    paths: JString,
    directory: JString,
    mode: JString,
) -> jstring {
    let result = (|| -> Result<Vec<String>, String> {
        let data_dir: String = env.get_string(&data_dir).map_err(|e| e.to_string())?.into();
        let paths: String = env.get_string(&paths).map_err(|e| e.to_string())?.into();
        let directory: String = env
            .get_string(&directory)
            .map_err(|e| e.to_string())?
            .into();
        let paths: Vec<String> = serde_json::from_str(&paths).map_err(|e| e.to_string())?;
        if paths.is_empty() {
            return Err("No books selected".into());
        }
        let library = Library::load_from(&data_dir)?;
        let books = paths
            .iter()
            .map(|path| {
                library
                    .books
                    .iter()
                    .find(|b| &b.path == path)
                    .cloned()
                    .ok_or("Book is no longer in the library".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mode: String = env
            .get_string(&mode)
            .map_err(|_| "Invalid export mode")?
            .into();
        reader_core::export::export_selection(
            &data_dir,
            &books,
            std::path::Path::new(&directory),
            reader_core::export::ExportMode::parse(&mode)?,
        )
    })();
    let json = match result {
        Ok(paths) => serde_json::json!({"ok":true, "paths":paths}),
        Err(error) => serde_json::json!({"ok":false, "error":error}),
    };
    jni_string_or_null!(env, json.to_string())
}

/// Explicit sharing is independent of whole-library sync and persistent pairing.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_bookShare(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    request: JString,
) -> jstring {
    let result = (|| -> Result<serde_json::Value, String> {
        let root: String = env
            .get_string(&data_dir)
            .map_err(|_| "Invalid share storage")?
            .into();
        let request: String = env
            .get_string(&request)
            .map_err(|_| "Invalid share request")?
            .into();
        let request = serde_json::from_str(&request).map_err(|_| "Invalid share request")?;
        reader_core::book_share::dispatch(&root, request)
    })();
    let envelope = match result {
        Ok(value) => serde_json::json!({"ok":true,"result":value}),
        Err(error) => serde_json::json!({"ok":false,"error":error}),
    };
    jni_string_or_null!(env, envelope.to_string())
}

/// Add or update a book in the library.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_addOrUpdateBook(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    title: JString,
    path: JString,
    chapter: jni::sys::jint,
    chapter_title: JString,
) -> jstring {
    let data_dir: String = match env.get_string(&data_dir) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let title: String = match env.get_string(&title) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let path: String = match env.get_string(&path) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let chapter_title: String = match env.get_string(&chapter_title) {
        Ok(s) => s.into(),
        Err(_) => String::new(),
    };
    let chapter_title_opt = if chapter_title.trim().is_empty() {
        None
    } else {
        Some(chapter_title)
    };

    github_sync::seed_local(&data_dir, &path);
    let mut library = checked_storage!(env, Library::load_from(&data_dir));
    let entry = checked_storage!(
        env,
        library.add_or_update(&data_dir, title, path, chapter as usize, chapter_title_opt)
    );
    let json_str = book_entry_to_android_json(&data_dir, &entry).to_string();
    jni_string_or_null!(env, json_str)
}

/// Update the current chapter for a book.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_updateChapter(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    path: JString,
    chapter: jni::sys::jint,
    chapter_title: JString,
) {
    let data_dir: String = match env.get_string(&data_dir) {
        Ok(s) => s.into(),
        Err(_) => return,
    };
    let path: String = match env.get_string(&path) {
        Ok(s) => s.into(),
        Err(_) => return,
    };
    let chapter_title: String = match env.get_string(&chapter_title) {
        Ok(s) => s.into(),
        Err(_) => String::new(),
    };
    let chapter_title_opt = if chapter_title.trim().is_empty() {
        None
    } else {
        Some(chapter_title)
    };

    let store = get_peer_store(&data_dir);
    let device = store
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .device_id
        .clone();
    if let Some(record) = checked_storage!(
        env,
        github_sync::commit_local(
            &data_dir,
            &device,
            &path,
            chapter.max(0) as usize,
            chapter_title_opt,
            0,
            0
        )
    ) {
        let mut live = store.lock().unwrap_or_else(|e| e.into_inner());
        let versions =
            std::iter::once((record.progress.book_hash.clone(), record.version)).collect();
        live.merge_progress_versioned(&[record.progress], &versions);
        live.save(&data_dir);
    }
}

/// Update the stable content locator for a book.
/// Accept already-merged peer progress without generating a local progress revision.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_applyRemotePosition(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    path: JString,
    chapter: jni::sys::jint,
    chapter_title: JString,
    block: jni::sys::jint,
    char_offset: jni::sys::jint,
) {
    let Ok(data_dir) = env.get_string(&data_dir).map(String::from) else {
        return;
    };
    let Ok(path) = env.get_string(&path).map(String::from) else {
        return;
    };
    let title = env
        .get_string(&chapter_title)
        .map(String::from)
        .unwrap_or_default();
    checked_storage!(
        env,
        checked_storage!(env, Library::load_from(&data_dir)).apply_position(
            &data_dir,
            &path,
            chapter.max(0) as usize,
            (!title.is_empty()).then_some(title),
            block.max(0) as usize,
            char_offset.max(0) as usize,
        )
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_updatePosition(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    path: JString,
    chapter: jni::sys::jint,
    chapter_title: JString,
    block: jni::sys::jint,
    char_offset: jni::sys::jint,
) {
    let data_dir: String = match env.get_string(&data_dir) {
        Ok(s) => s.into(),
        Err(_) => return,
    };
    let path: String = match env.get_string(&path) {
        Ok(s) => s.into(),
        Err(_) => return,
    };
    let chapter_title: String = env
        .get_string(&chapter_title)
        .map(Into::into)
        .unwrap_or_default();
    let chapter_title = (!chapter_title.trim().is_empty()).then_some(chapter_title);
    let chapter = chapter.max(0) as usize;
    let block = block.max(0) as usize;
    let char_offset = char_offset.max(0) as usize;

    let store = get_peer_store(&data_dir);
    let device = store
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .device_id
        .clone();
    if let Some(record) = checked_storage!(
        env,
        github_sync::commit_local(
            &data_dir,
            &device,
            &path,
            chapter,
            chapter_title,
            block,
            char_offset
        )
    ) {
        let mut live = store.lock().unwrap_or_else(|e| e.into_inner());
        let versions =
            std::iter::once((record.progress.book_hash.clone(), record.version)).collect();
        live.merge_progress_versioned(&[record.progress], &versions);
        live.save(&data_dir);
    }
}

/// Remove a book from the library by index.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_removeBook(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    index: jni::sys::jint,
) {
    let data_dir: String = match env.get_string(&data_dir) {
        Ok(s) => s.into(),
        Err(_) => return,
    };

    let mut library = checked_storage!(env, Library::load_from(&data_dir));
    checked_storage!(env, library.remove(&data_dir, index as usize));
}

/// Remove a book from the library by absolute path.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_removeBookByPath(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    path: JString,
) -> jboolean {
    let data_dir: String = match env.get_string(&data_dir) {
        Ok(s) => s.into(),
        Err(_) => return 0,
    };
    let path: String = match env.get_string(&path) {
        Ok(s) => s.into(),
        Err(_) => return 0,
    };

    let mut library = checked_storage!(env, Library::load_from(&data_dir));
    let before = library.books.len();
    if reader_core::library::archive::ArchiveStore::archive(&data_dir, &mut library, &[path])
        .is_err()
    {
        return 0;
    }
    let after = library.books.len();
    if after < before {
        1
    } else {
        0
    }
}

/// Return the available language list as JSON.
/// JSON format: [{"code":"zh_cn","label":"中文"}, ...]
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_getAvailableLanguages(
    env: JNIEnv,
    _class: JClass,
) -> jstring {
    let langs: Vec<serde_json::Value> = Language::all()
        .iter()
        .map(|l| {
            serde_json::json!({
                "code": l.code(),
                "label": l.label(),
            })
        })
        .collect();
    let json = serde_json::json!(langs);
    jni_string_or_null!(env, json.to_string())
}

/// Return the full translation map for the given language code as JSON.
/// Pass "auto" to get the default (ZhCN) translations.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_getTranslations(
    mut env: JNIEnv,
    _class: JClass,
    lang_code: JString,
) -> jstring {
    let code: String = match env.get_string(&lang_code) {
        Ok(s) => s.into(),
        Err(_) => "zh_cn".to_string(),
    };
    let language = Language::from_code(&code);
    let i18n = I18n::new(language);

    let translations = i18n.get_all_translations();
    let json = serde_json::to_string(translations).unwrap_or_else(|_| "{}".to_string());
    jni_string_or_null!(env, json)
}

// ── Sharing global state ──

struct SharingServer {
    stop_flag: Arc<AtomicBool>,
    #[allow(dead_code)]
    addr: String,
}

static SHARING_SERVER: Lazy<Mutex<Option<SharingServer>>> = Lazy::new(|| Mutex::new(None));

/// Global PeerStore shared across all JNI calls to prevent race conditions
/// where multiple PeerStore::load() calls generate different device_ids.
type GlobalPeerStore = Option<(String, Arc<Mutex<PeerStore>>)>;
static GLOBAL_PEER_STORE: Lazy<Mutex<GlobalPeerStore>> = Lazy::new(|| Mutex::new(None));

/// Get or create the global PeerStore for the given data_dir.
fn get_peer_store(data_dir: &str) -> Arc<Mutex<PeerStore>> {
    let mut guard = GLOBAL_PEER_STORE.lock().unwrap_or_else(|e| {
        dbg_log!("GLOBAL_PEER_STORE lock poisoned, recovering: {}", e);
        e.into_inner()
    });
    if let Some((ref dir, ref store)) = *guard {
        if dir == data_dir {
            return store.clone();
        }
    }
    let store = Arc::new(Mutex::new(PeerStore::load(data_dir)));
    *guard = Some((data_dir.to_string(), store.clone()));
    store
}

/// Collect all existing book file paths from library.json for sharing fallback.
/// This is important when some books are not physically under books_dir yet
/// (e.g. during migration / mixed historical data).
fn collect_library_book_paths(data_dir: &str) -> Result<Vec<String>, String> {
    let library = Library::load_from(data_dir)?;
    let mut seen = HashSet::new();
    let mut paths = Vec::new();

    for entry in library.books {
        if entry.path.trim().is_empty() {
            continue;
        }
        if !std::path::Path::new(&entry.path).exists() {
            continue;
        }
        if seen.insert(entry.path.clone()) {
            paths.push(entry.path);
        }
    }

    Ok(paths)
}

// ── Discovery global state ──

struct DiscoveryListenerState {
    stop_flag: Arc<AtomicBool>,
    peers: Arc<Mutex<Vec<DiscoveredPeer>>>,
}

static DISCOVERY_LISTENER: Lazy<Mutex<Option<DiscoveryListenerState>>> =
    Lazy::new(|| Mutex::new(None));

/// Generate a random 4-digit PIN.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_generatePin(
    env: JNIEnv,
    _class: JClass,
) -> jstring {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::SystemTime;
    let mut hasher = DefaultHasher::new();
    SystemTime::now().hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    let pin = format!("{:04}", hasher.finish() % 10000);
    jni_string_or_null!(env, &pin)
}

/// Start the sharing server. Returns JSON: {"ok": true, "addr": "ip:port"} or {"ok": false, "error": "..."}
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_startSharingServer(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    books_dir_j: JString,
    pin_j: JString,
) -> jstring {
    let data_dir: String = match env.get_string(&data_dir) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let books_dir: String = match env.get_string(&books_dir_j) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let pin: String = match env.get_string(&pin_j) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    dbg_log!(
        "JNI startSharingServer: data_dir={} books_dir={} pin='{}'",
        data_dir,
        books_dir,
        pin
    );
    let initial_extra_paths = checked_storage!(env, collect_library_book_paths(&data_dir));
    dbg_log!(
        "JNI startSharingServer: initial library fallback paths={} ",
        initial_extra_paths.len()
    );

    // Stop any existing server
    {
        let mut guard = SHARING_SERVER.lock().unwrap_or_else(|e| {
            dbg_log!("SHARING_SERVER lock poisoned, recovering: {}", e);
            e.into_inner()
        });
        if let Some(server) = guard.take() {
            server.stop_flag.store(true, Ordering::SeqCst);
        }
    }

    let store = get_peer_store(&data_dir);
    let (device_id, device_name) = {
        let s = store.lock().unwrap_or_else(|e| {
            dbg_log!("PeerStore lock poisoned, recovering: {}", e);
            e.into_inner()
        });
        (s.device_id.clone(), s.device_name.clone())
    };

    match start_server("0.0.0.0:0", &data_dir, &books_dir, &pin, store.clone()) {
        Ok((listener, addr)) => {
            let resolved_addr = resolve_broadcast_addr(&addr);
            dbg_log!(
                "JNI startSharingServer: bound to {} resolved to {}",
                addr,
                resolved_addr
            );
            let stop_flag = Arc::new(AtomicBool::new(false));
            let sf = stop_flag.clone();

            // Broadcast our presence so other devices can discover us
            start_broadcast(
                DiscoveryAnnouncement {
                    device_id,
                    device_name,
                    addr: resolved_addr.clone(),
                },
                stop_flag.clone(),
            );

            {
                let mut guard = SHARING_SERVER.lock().unwrap_or_else(|e| {
                    dbg_log!("SHARING_SERVER lock poisoned, recovering: {}", e);
                    e.into_inner()
                });
                *guard = Some(SharingServer {
                    stop_flag: stop_flag.clone(),
                    addr: resolved_addr.clone(),
                });
            }

            listener.set_nonblocking(true).ok();

            std::thread::spawn(move || loop {
                if sf.load(Ordering::SeqCst) {
                    break;
                }
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_read_timeout(Some(std::time::Duration::from_secs(60)))
                            .ok();
                        stream.set_nonblocking(false).ok();
                        let dd = data_dir.clone();
                        let bd = books_dir.clone();
                        let p = pin.clone();
                        let s = store.clone();
                        std::thread::spawn(move || {
                            let extra_paths = match collect_library_book_paths(&dd) {
                                Ok(paths) => paths,
                                Err(error) => {
                                    dbg_log!("Cannot load sharing library: {}", error);
                                    return;
                                }
                            };
                            dbg_log!(
                                "JNI startSharingServer: handle_client fallback paths={}",
                                extra_paths.len()
                            );
                            let _ = handle_client(&mut stream, &dd, &bd, &p, s, &extra_paths);
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(200));
                    }
                    Err(_) => break,
                }
            });

            let json = serde_json::json!({"ok": true, "addr": resolved_addr});
            jni_string_or_null!(env, json.to_string())
        }
        Err(e) => {
            let json = serde_json::json!({"ok": false, "error": e});
            jni_string_or_null!(env, json.to_string())
        }
    }
}

/// Stop the sharing server.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_stopSharingServer(
    _env: JNIEnv,
    _class: JClass,
) {
    let mut guard = SHARING_SERVER.lock().unwrap_or_else(|e| {
        dbg_log!("SHARING_SERVER lock poisoned, recovering: {}", e);
        e.into_inner()
    });
    if let Some(server) = guard.take() {
        server.stop_flag.store(true, Ordering::SeqCst);
    }
}

/// Connect to a peer and sync. Returns JSON: {"ok": true, "books": [...]} or {"error": "..."}.
/// Parameters: addr, pin, device_id, data_dir, books_dir
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_connectAndListBooks(
    mut env: JNIEnv,
    _class: JClass,
    addr: JString,
    pin_str: JString,
    device_id_str: JString,
    data_dir: JString,
    books_dir: JString,
) -> jstring {
    let addr: String = match env.get_string(&addr) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let pin_str: String = match env.get_string(&pin_str) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let device_id_str: String = match env.get_string(&device_id_str) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let data_dir: String = match env.get_string(&data_dir) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let books_dir: String = match env.get_string(&books_dir) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let extra_book_paths = checked_storage!(env, collect_library_book_paths(&data_dir));

    let store_arc = get_peer_store(&data_dir);
    let mut store = store_arc
        .lock()
        .unwrap_or_else(|e| {
            dbg_log!("PeerStore lock poisoned, recovering: {}", e);
            e.into_inner()
        })
        .clone();
    let pairing_baseline = store.clone();
    let pin_opt = if pin_str.is_empty() {
        None
    } else {
        Some(pin_str.as_str())
    };
    let dev_id_opt = if device_id_str.is_empty() {
        None
    } else {
        Some(device_id_str.as_str())
    };
    dbg_log!(
        "JNI connectAndListBooks: addr={} pin='{}' device_id={:?} fallback_paths={}",
        addr,
        pin_opt.unwrap_or("(none)"),
        dev_id_opt,
        extra_book_paths.len()
    );
    match connect_to_peer(&addr, &mut store, &data_dir, dev_id_opt, pin_opt) {
        Ok((mut stream, aes_key, mut send_ctr, mut recv_ctr)) => {
            dbg_log!("JNI connectAndListBooks: connect_to_peer OK, starting sync...");
            store_arc
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .merge_session_since(&store, &pairing_baseline, &data_dir);
            match auto_sync_session(
                &mut stream,
                &aes_key,
                &mut send_ctr,
                &mut recv_ctr,
                &mut store,
                &data_dir,
                &books_dir,
                &extra_book_paths,
            ) {
                Ok((_changed_progress, fetched_books)) => {
                    dbg_log!(
                        "JNI connectAndListBooks: sync OK, {} books fetched",
                        fetched_books.len()
                    );
                    // Keep in-memory global store in sync with the latest merged progress/pairing state.
                    store_arc
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .merge_session_since(&store, &pairing_baseline, &data_dir);
                    let json =
                        serde_json::to_string(&fetched_books).unwrap_or_else(|_| "[]".into());
                    jni_string_or_null!(env, &json)
                }
                Err(e) => {
                    dbg_log!("JNI connectAndListBooks: sync error: {}", e);
                    // Even on error, store may have partial updates; keep global cache consistent.
                    store_arc
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .merge_session_since(&store, &pairing_baseline, &data_dir);
                    let json = serde_json::json!({"error": e, "phase": "sync"});
                    jni_string_or_null!(env, json.to_string())
                }
            }
        }
        Err(e) => {
            dbg_log!("JNI connectAndListBooks: connect error: {}", e);
            let json = serde_json::json!({"error": e, "phase": "connect"});
            jni_string_or_null!(env, json.to_string())
        }
    }
}

/// Request and download a book from a peer by hash.
/// Note: with the new encrypted protocol, this does a full connect+auth cycle.
/// Returns JSON: {"ok": true, "path": "..."} or {"ok": false, "error": "..."}
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_requestBookFromPeer(
    mut env: JNIEnv,
    _class: JClass,
    addr: JString,
    pin_str: JString,
    device_id_str: JString,
    data_dir: JString,
    books_dir: JString,
    hash: JString,
) -> jstring {
    let addr: String = match env.get_string(&addr) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let pin_str: String = match env.get_string(&pin_str) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let device_id_str: String = match env.get_string(&device_id_str) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let data_dir: String = match env.get_string(&data_dir) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let _books_dir: String = match env.get_string(&books_dir) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let hash: String = match env.get_string(&hash) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };

    let store_arc = get_peer_store(&data_dir);
    let mut store = store_arc
        .lock()
        .unwrap_or_else(|e| {
            dbg_log!("PeerStore lock poisoned, recovering: {}", e);
            e.into_inner()
        })
        .clone();
    let pairing_baseline = store.clone();
    let pin_opt = if pin_str.is_empty() {
        None
    } else {
        Some(pin_str.as_str())
    };
    let dev_id_opt = if device_id_str.is_empty() {
        None
    } else {
        Some(device_id_str.as_str())
    };

    match connect_to_peer(&addr, &mut store, &data_dir, dev_id_opt, pin_opt) {
        Ok((mut stream, aes_key, mut send_ctr, mut recv_ctr)) => {
            store_arc
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .merge_session_since(&store, &pairing_baseline, &data_dir);
            use reader_core::sharing::crypto;
            if crypto::write_encrypted_message(
                &mut stream,
                &Message::RequestBook { hash: hash.clone() },
                &aes_key,
                &mut send_ctr,
            )
            .is_ok()
            {
                match crypto::read_encrypted_message(&mut stream, &aes_key, &mut recv_ctr) {
                    Ok(Message::BookData { title, .. }) => {
                        if let Ok(data) =
                            crypto::read_encrypted_raw(&mut stream, &aes_key, &mut recv_ctr)
                        {
                            let mut library = checked_storage!(env, Library::load_from(&data_dir));
                            let entry = checked_storage!(
                                env,
                                library.add_or_update_from_bytes(
                                    &data_dir,
                                    title.clone(),
                                    &data,
                                    0,
                                    None,
                                )
                            );
                            let json = serde_json::json!({
                                "ok": true,
                                "path": entry.path,
                                "title": entry.title,
                                "id": entry.id,
                            });
                            return jni_string_or_null!(env, json.to_string());
                        }
                    }
                    Ok(Message::BookNotFound { .. }) => {
                        let json = serde_json::json!({"ok": false, "error": "Book not found"});
                        return jni_string_or_null!(env, json.to_string());
                    }
                    _ => {}
                }
            }
            let json = serde_json::json!({"ok": false, "error": "Download failed"});
            jni_string_or_null!(env, json.to_string())
        }
        Err(e) => {
            let json = serde_json::json!({"ok": false, "error": e});
            jni_string_or_null!(env, json.to_string())
        }
    }
}

/// Sync reading progress with a peer. Returns JSON: {"ok": true} or {"ok": false, "error": "..."}
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_syncProgressWithPeer(
    mut env: JNIEnv,
    _class: JClass,
    addr: JString,
    pin_str: JString,
    device_id_str: JString,
    data_dir: JString,
    books_dir: JString,
) -> jstring {
    let addr: String = match env.get_string(&addr) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let pin_str: String = match env.get_string(&pin_str) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let device_id_str: String = match env.get_string(&device_id_str) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let data_dir: String = match env.get_string(&data_dir) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let books_dir: String = match env.get_string(&books_dir) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let extra_book_paths = checked_storage!(env, collect_library_book_paths(&data_dir));

    let store_arc = get_peer_store(&data_dir);
    let mut store = store_arc
        .lock()
        .unwrap_or_else(|e| {
            dbg_log!("PeerStore lock poisoned, recovering: {}", e);
            e.into_inner()
        })
        .clone();
    let pairing_baseline = store.clone();
    let pin_opt = if pin_str.is_empty() {
        None
    } else {
        Some(pin_str.as_str())
    };
    let dev_id_opt = if device_id_str.is_empty() {
        None
    } else {
        Some(device_id_str.as_str())
    };

    match connect_to_peer(&addr, &mut store, &data_dir, dev_id_opt, pin_opt) {
        Ok((mut stream, aes_key, mut send_ctr, mut recv_ctr)) => {
            store_arc
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .merge_session_since(&store, &pairing_baseline, &data_dir);
            match auto_sync_session(
                &mut stream,
                &aes_key,
                &mut send_ctr,
                &mut recv_ctr,
                &mut store,
                &data_dir,
                &books_dir,
                &extra_book_paths,
            ) {
                Ok(_) => {
                    // Keep in-memory global store in sync with latest merged progress.
                    store_arc
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .merge_session_since(&store, &pairing_baseline, &data_dir);
                    let json = serde_json::json!({"ok": true});
                    jni_string_or_null!(env, json.to_string())
                }
                Err(e) => {
                    // Even on error, store may have partial updates; keep global cache consistent.
                    store_arc
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .merge_session_since(&store, &pairing_baseline, &data_dir);
                    let json = serde_json::json!({"ok": false, "error": e, "phase": "sync"});
                    jni_string_or_null!(env, json.to_string())
                }
            }
        }
        Err(e) => {
            store_arc
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .merge_session_since(&store, &pairing_baseline, &data_dir);
            let json = serde_json::json!({"ok": false, "error": e, "phase": "connect"});
            jni_string_or_null!(env, json.to_string())
        }
    }
}

/// Get list of paired devices as JSON array.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_getPairedDevices(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
) -> jstring {
    let data_dir: String = match env.get_string(&data_dir) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };

    let store = get_peer_store(&data_dir);
    let s = store.lock().unwrap_or_else(|e| {
        dbg_log!("PeerStore lock poisoned, recovering: {}", e);
        e.into_inner()
    });
    let json = serde_json::to_string(&s.paired).unwrap_or_else(|_| "[]".into());
    jni_string_or_null!(env, &json)
}

/// Start the UDP discovery listener so this device can find active sharing servers.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_removePairedDevice(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    device_id: JString,
) -> jboolean {
    let data_dir: String = match env.get_string(&data_dir) {
        Ok(s) => s.into(),
        Err(_) => return 0,
    };
    let device_id: String = match env.get_string(&device_id) {
        Ok(s) => s.into(),
        Err(_) => return 0,
    };
    let store = get_peer_store(&data_dir);
    let mut s = store.lock().unwrap_or_else(|e| {
        dbg_log!("PeerStore lock poisoned, recovering: {}", e);
        e.into_inner()
    });
    let removed = s.remove_paired(&device_id);
    if removed {
        s.save(&data_dir);
    }
    if removed {
        1
    } else {
        0
    }
}

/// Start the UDP discovery listener so this device can find active sharing servers.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_startDiscoveryListener(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
) {
    let data_dir: String = match env.get_string(&data_dir) {
        Ok(s) => s.into(),
        Err(_) => return,
    };
    let store = get_peer_store(&data_dir);
    let own_id = store
        .lock()
        .unwrap_or_else(|e| {
            dbg_log!("PeerStore lock poisoned, recovering: {}", e);
            e.into_inner()
        })
        .device_id
        .clone();
    dbg_log!("JNI startDiscoveryListener: own_id={}", own_id);

    // Stop any existing listener first
    {
        let mut guard = DISCOVERY_LISTENER.lock().unwrap_or_else(|e| {
            dbg_log!("DISCOVERY_LISTENER lock poisoned, recovering: {}", e);
            e.into_inner()
        });
        if let Some(prev) = guard.take() {
            prev.stop_flag.store(true, Ordering::SeqCst);
        }
    }

    let stop_flag = Arc::new(AtomicBool::new(false));
    let peers = start_listener(&own_id, stop_flag.clone());

    let mut guard = DISCOVERY_LISTENER.lock().unwrap_or_else(|e| {
        dbg_log!("DISCOVERY_LISTENER lock poisoned, recovering: {}", e);
        e.into_inner()
    });
    *guard = Some(DiscoveryListenerState { stop_flag, peers });
}

/// Stop the UDP discovery listener.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_stopDiscoveryListener(
    _env: JNIEnv,
    _class: JClass,
) {
    let mut guard = DISCOVERY_LISTENER.lock().unwrap_or_else(|e| {
        dbg_log!("DISCOVERY_LISTENER lock poisoned, recovering: {}", e);
        e.into_inner()
    });
    if let Some(state) = guard.take() {
        state.stop_flag.store(true, Ordering::SeqCst);
    }
}

/// Return the current list of discovered peers as a JSON array.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_getDiscoveredPeers(
    env: JNIEnv,
    _class: JClass,
) -> jstring {
    let guard = DISCOVERY_LISTENER.lock().unwrap_or_else(|e| {
        dbg_log!("DISCOVERY_LISTENER lock poisoned, recovering: {}", e);
        e.into_inner()
    });
    let json = if let Some(state) = &*guard {
        let peers = match state.peers.lock() {
            Ok(p) => p,
            Err(e) => {
                dbg_log!("peers lock poisoned: {}", e);
                return jni_string_or_null!(env, "[]");
            }
        };
        dbg_log!("JNI getDiscoveredPeers: {} peers found", peers.len());
        for p in peers.iter() {
            dbg_log!(
                "  peer: id={} name='{}' addr={} last_seen={}",
                p.device_id,
                p.device_name,
                p.addr,
                p.last_seen
            );
        }
        serde_json::to_string(&*peers).unwrap_or_else(|_| "[]".into())
    } else {
        "[]".into()
    };
    jni_string_or_null!(env, &json)
}

/// Return the synced progress entries from PeerStore as JSON array.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_getSyncedProgress(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
) -> jstring {
    let data_dir: String = match env.get_string(&data_dir) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let store = get_peer_store(&data_dir);
    let snapshot = store.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let committed = checked_storage!(
        env,
        reader_core::sharing::apply_received_progress(&data_dir, &snapshot)
    );
    checked_storage!(
        env,
        github_sync::accept_committed(&data_dir, &snapshot.device_id, &committed)
    );
    let progress: Vec<_> = committed
        .into_iter()
        .map(|notice| {
            let mut value = serde_json::to_value(notice.record.progress).unwrap();
            value["path"] = serde_json::json!(notice.path);
            value
        })
        .collect();
    let json = serde_json::to_string(&progress).unwrap_or_else(|_| "[]".into());
    jni_string_or_null!(env, &json)
}

/// Compute SHA-256 hash of a file (same as EpubBook::file_hash).
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_fileHash(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
) -> jstring {
    let path: String = match env.get_string(&path) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    match reader_core::epub::EpubBook::file_hash(&path) {
        Ok(hash) => {
            jni_string_or_null!(env, &hash)
        }
        Err(_) => std::ptr::null_mut(),
    }
}

/// Read epub metadata as JSON without full parsing.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_readEpubMetadata(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
) -> jstring {
    let path: String = match env.get_string(&path) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    match reader_core::epub::EpubBook::read_metadata(&path) {
        Some(meta) => match serde_json::to_string(&meta) {
            Ok(json) => {
                jni_string_or_null!(env, &json)
            }
            Err(_) => std::ptr::null_mut(),
        },
        None => std::ptr::null_mut(),
    }
}

// ── TXT → EPUB ──────────────────────────────────────────────────

/// Preview TXT chapters. Returns JSON array of {title, lineStart, charCount}.
/// Parameters: txtPath, useHeuristic (bool), customRegex (nullable string).
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_previewTxtChapters(
    mut env: JNIEnv,
    _class: JClass,
    txt_path: JString,
    use_heuristic: jboolean,
    custom_regex: JString,
) -> jstring {
    let txt_path: String = match env.get_string(&txt_path) {
        Ok(s) => s.into(),
        Err(_) => return jni_string_or_null!(env, "[]"),
    };

    let regex_str: Option<String> = if custom_regex.is_null() {
        None
    } else {
        match env.get_string(&custom_regex) {
            Ok(s) => {
                let s: String = s.into();
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            }
            Err(_) => None,
        }
    };

    let config = reader_core::txt::SplitConfig {
        min_chapter_chars: 100,
        use_heuristic: use_heuristic != 0,
        custom_regex: regex_str,
    };

    let previews =
        match reader_core::txt::preview_chapters(std::path::Path::new(&txt_path), &config) {
            Ok(previews) => previews,
            Err(error) => {
                return jni_string_or_null!(
                    env,
                    serde_json::json!({"error":error.to_string(),"errorKey":reader_core::txt::error_key(&error.to_string())}).to_string()
                )
            }
        };

    let json: Vec<serde_json::Value> = previews
        .iter()
        .map(|p| {
            serde_json::json!({
                "title": p.title,
                "lineStart": p.line_start,
                "charCount": p.char_count,
            })
        })
        .collect();

    jni_string_or_null!(env, serde_json::json!(json).to_string())
}

/// Convert TXT to EPUB. Returns JSON {epubPath, title, chapterCount} or {error}.
/// Parameters: txtPath, outputDir, title (nullable), author (nullable),
///             useHeuristic (bool), customRegex (nullable).
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_convertTxtToEpub(
    mut env: JNIEnv,
    _class: JClass,
    txt_path: JString,
    output_dir: JString,
    title: JString,
    author: JString,
    use_heuristic: jboolean,
    custom_regex: JString,
) -> jstring {
    let txt_path: String = match env.get_string(&txt_path) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let output_dir: String = match env.get_string(&output_dir) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };

    let get_opt_string = |env: &mut JNIEnv, js: &JString| -> Option<String> {
        if js.is_null() {
            return None;
        }
        match env.get_string(js) {
            Ok(s) => {
                let s: String = s.into();
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            }
            Err(_) => None,
        }
    };

    let title_opt = get_opt_string(&mut env, &title);
    let author_opt = get_opt_string(&mut env, &author);
    let regex_opt = get_opt_string(&mut env, &custom_regex);

    let options = reader_core::txt::ConvertOptions {
        title: title_opt,
        author: author_opt,
        custom_regex: regex_opt,
        use_heuristic: use_heuristic != 0,
        ..Default::default()
    };

    match reader_core::txt::convert_txt_to_epub(
        std::path::Path::new(&txt_path),
        std::path::Path::new(&output_dir),
        &options,
    ) {
        Ok(result) => {
            let json = serde_json::json!({
                "epubPath": result.epub_path.to_string_lossy(),
                "title": result.title,
                "chapterCount": result.chapter_count,
            });
            jni_string_or_null!(env, json.to_string())
        }
        Err(e) => {
            let json = serde_json::json!({ "error": e.to_string() });
            jni_string_or_null!(env, json.to_string())
        }
    }
}

/// Search book content for a query string.
/// Returns JSON array of search results.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_searchBook(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
    query: JString,
) -> jstring {
    let path: String = match env.get_string(&path) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let query: String = match env.get_string(&query) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };

    let book = match get_book(&path) {
        Ok(b) => b,
        Err(_) => return std::ptr::null_mut(),
    };

    let results = match reader_core::search::search_book_checked(&book, &query, false, || false) {
        Ok(results) => results,
        Err(_) => return std::ptr::null_mut(),
    };
    let json: Vec<_> = results
        .iter()
        .map(|r| {
            serde_json::json!({
                "chapterIndex": r.chapter_index,
                "chapterTitle": r.chapter_title,
                "blockIndex": r.block_index,
                "context": r.context,
                "matchStart": r.match_start,
                "matchLen": r.match_len,
            })
        })
        .collect();

    jni_string_or_null!(env, serde_json::to_string(&json).unwrap_or_default())
}

// ── Bookmark / Highlight / Note management ──

#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_readingActivity(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    request: JString,
) -> jstring {
    let Ok(root) = env.get_string(&data_dir).map(String::from) else {
        return std::ptr::null_mut();
    };
    let Ok(request) = env.get_string(&request).map(String::from) else {
        return std::ptr::null_mut();
    };
    let device = get_peer_store(&root)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .device_id
        .clone();
    let result = serde_json::from_str(&request)
        .map_err(|_| "Invalid activity request".into())
        .and_then(|v| reader_core::reading_activity::dispatch(&root, &device, &v));
    let envelope = match result {
        Ok(result) => serde_json::json!({"ok":true,"result":result}),
        Err(error) => serde_json::json!({"ok":false,"error":error}),
    };
    jni_string_or_null!(env, envelope.to_string())
}

/// Annotation tools, shared with the iOS and desktop adapters.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_annotationTools(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    request: JString,
) -> jstring {
    let root: String = match env.get_string(&data_dir) {
        Ok(value) => value.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let request: String = match env.get_string(&request) {
        Ok(value) => value.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let result = serde_json::from_str(&request)
        .map_err(|_| "Invalid annotation tools request".into())
        .and_then(|request| reader_core::annotations::command(&root, &request));
    let envelope = match result {
        Ok(value) => serde_json::json!({"ok":true,"result":value}),
        Err(error) => serde_json::json!({"ok":false,"error":error}),
    };
    jni_string_or_null!(env, envelope.to_string())
}

/// Versioned backup commands; status polling never waits for a storage lease.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_backupTools(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    request: JString,
) -> jstring {
    let result = (|| -> Result<serde_json::Value, String> {
        let root: String = env
            .get_string(&data_dir)
            .map_err(|_| "Invalid backup storage")?
            .into();
        let request: String = env
            .get_string(&request)
            .map_err(|_| "Invalid backup request")?
            .into();
        let request = serde_json::from_str(&request).map_err(|_| "Invalid backup request")?;
        reader_core::backup::command(&root, &request)
    })();
    let envelope = match result {
        Ok(value) => serde_json::json!({"ok":true,"result":value}),
        Err(error) => serde_json::json!({"ok":false,"error":error}),
    };
    jni_string_or_null!(env, envelope.to_string())
}

/// Return full BookConfig JSON for a given book ID.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_getBookConfig(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    book_id: JString,
) -> jstring {
    let data_dir: String = match env.get_string(&data_dir) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let book_id: String = match env.get_string(&book_id) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let cfg = match checked_storage!(env, Library::load_book_config(&data_dir, &book_id)) {
        Some(c) => c,
        None => return std::ptr::null_mut(),
    };
    let mut payload = serde_json::to_value(&cfg).expect("BookConfig serialization is infallible");
    payload["concurrent_notes"] =
        serde_json::json!(reader_core::annotations::concurrent_note_ids(&cfg));
    jni_string_or_null!(env, payload.to_string())
}

/// Toggle bookmark for a chapter: add if not present, remove if present.
/// Returns "added" or "removed".
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_toggleBookmark(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    book_id: JString,
    chapter: jni::sys::jint,
) -> jstring {
    let data_dir: String = match env.get_string(&data_dir) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let book_id: String = match env.get_string(&book_id) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let chapter = chapter as usize;

    let mut cfg = match checked_storage!(env, Library::load_book_config(&data_dir, &book_id)) {
        Some(c) => c,
        None => return std::ptr::null_mut(),
    };

    let had = cfg.bookmarks.iter().any(|b| b.chapter == chapter);
    if had {
        cfg.bookmarks.retain(|b| b.chapter != chapter);
    } else {
        cfg.bookmarks.push(reader_core::library::Bookmark {
            chapter,
            block: 0,
            created_at: reader_core::now_secs(),
        });
    }
    checked_storage!(env, cfg.save(&data_dir));

    jni_string_or_null!(env, if had { "removed" } else { "added" })
}

/// Add a highlight and return its ID, or null on failure.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_addHighlight(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    book_id: JString,
    json_payload: JString,
) -> jstring {
    let data_dir: String = match env.get_string(&data_dir) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let book_id: String = match env.get_string(&book_id) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let payload: String = match env.get_string(&json_payload) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };

    let mut cfg = match checked_storage!(env, Library::load_book_config(&data_dir, &book_id)) {
        Some(c) => c,
        None => return std::ptr::null_mut(),
    };

    #[derive(serde::Deserialize)]
    struct HlInput {
        chapter: usize,
        start_block: usize,
        start_offset: usize,
        end_block: usize,
        end_offset: usize,
        color: String,
    }

    let input: HlInput = match serde_json::from_str(&payload) {
        Ok(v) => v,
        Err(_) => return std::ptr::null_mut(),
    };

    let color = match input.color.as_str() {
        "Yellow" => reader_core::library::HighlightColor::Yellow,
        "Green" => reader_core::library::HighlightColor::Green,
        "Blue" => reader_core::library::HighlightColor::Blue,
        "Pink" | "Purple" => reader_core::library::HighlightColor::Pink,
        _ => reader_core::library::HighlightColor::Yellow,
    };

    let id = reader_core::annotations::new_highlight_id();
    cfg.highlights.push(reader_core::library::Highlight {
        id: id.clone(),
        chapter: input.chapter,
        start_block: input.start_block,
        start_offset: input.start_offset,
        end_block: input.end_block,
        end_offset: input.end_offset,
        color,
        created_at: reader_core::now_secs(),
    });
    checked_storage!(env, cfg.save(&data_dir));

    jni_string_or_null!(env, id)
}

/// Remove a highlight by ID.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_removeHighlight(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    book_id: JString,
    highlight_id: JString,
) {
    let data_dir: String = match env.get_string(&data_dir) {
        Ok(s) => s.into(),
        Err(_) => return,
    };
    let book_id: String = match env.get_string(&book_id) {
        Ok(s) => s.into(),
        Err(_) => return,
    };
    let hl_id: String = match env.get_string(&highlight_id) {
        Ok(s) => s.into(),
        Err(_) => return,
    };

    let mut cfg = match checked_storage!(env, Library::load_book_config(&data_dir, &book_id)) {
        Some(c) => c,
        None => return,
    };
    cfg.highlights.retain(|h| h.id != hl_id);
    // Also remove notes attached to this highlight
    cfg.notes.retain(|n| n.highlight_id != hl_id);
    checked_storage!(env, cfg.save(&data_dir));
}

/// Save or update a note for a highlight.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_saveNote(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    book_id: JString,
    highlight_id: JString,
    content: JString,
) {
    let data_dir: String = match env.get_string(&data_dir) {
        Ok(s) => s.into(),
        Err(_) => return,
    };
    let book_id: String = match env.get_string(&book_id) {
        Ok(s) => s.into(),
        Err(_) => return,
    };
    let hl_id: String = match env.get_string(&highlight_id) {
        Ok(s) => s.into(),
        Err(_) => return,
    };
    let content: String = match env.get_string(&content) {
        Ok(s) => s.into(),
        Err(_) => return,
    };

    let mut cfg = match checked_storage!(env, Library::load_book_config(&data_dir, &book_id)) {
        Some(c) => c,
        None => return,
    };

    let now = reader_core::now_secs();
    if let Some(note) = cfg.notes.iter_mut().find(|n| n.highlight_id == hl_id) {
        note.content = content;
        note.updated_at = now;
    } else {
        cfg.notes.push(reader_core::library::Note {
            highlight_id: hl_id,
            content,
            created_at: now,
            updated_at: now,
        });
    }
    checked_storage!(env, cfg.save(&data_dir));
}

// ── CSC Contribution ──

/// Get the count of resolved (accepted/rejected) corrections for the current book.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_getCscCorrectionCount(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    book_id: JString,
) -> jni::sys::jint {
    let data_dir: String = match env.get_string(&data_dir) {
        Ok(s) => s.into(),
        Err(_) => return 0,
    };
    let book_id: String = match env.get_string(&book_id) {
        Ok(s) => s.into(),
        Err(_) => return 0,
    };

    let cfg = match checked_storage!(env, Library::load_book_config(&data_dir, &book_id)) {
        Some(c) => c,
        None => return 0,
    };

    cfg.corrections
        .iter()
        .filter(|r| r.status == "accepted" || r.status == "rejected")
        .count() as jni::sys::jint
}

/// Collect CSC training samples from corrections and return JSONL string.
/// Returns null if no accepted corrections or book not found.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_collectCscSamples(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    book_path: JString,
    book_id: JString,
) -> jstring {
    let data_dir: String = match env.get_string(&data_dir) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let book_path: String = match env.get_string(&book_path) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let book_id: String = match env.get_string(&book_id) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };

    let cfg = match checked_storage!(env, Library::load_book_config(&data_dir, &book_id)) {
        Some(c) => c,
        None => return std::ptr::null_mut(),
    };

    let samples = match get_book(&book_path)
        .and_then(|book| reader_core::csc::contribution::samples(&book, &cfg))
    {
        Ok(samples) => samples,
        Err(_) => return std::ptr::null_mut(),
    };
    match reader_core::csc::contribution::jsonl(&samples) {
        Ok(lines) => jni_string_or_null!(env, lines),
        Err(_) => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_upsertCorrection(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    book_id: JString,
    json_payload: JString,
) -> jstring {
    let data_dir: String = match env.get_string(&data_dir) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let book_id: String = match env.get_string(&book_id) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let payload: String = match env.get_string(&json_payload) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };

    let mut cfg = match checked_storage!(env, Library::load_book_config(&data_dir, &book_id)) {
        Some(c) => c,
        None => return std::ptr::null_mut(),
    };

    #[derive(serde::Deserialize)]
    struct CorrInput {
        chapter: usize,
        block_idx: usize,
        char_offset: usize,
        original: String,
        corrected: String,
        status: String,
    }

    let input: CorrInput = match serde_json::from_str(&payload) {
        Ok(v) => v,
        Err(_) => return std::ptr::null_mut(),
    };

    if let Some(existing) = cfg.corrections.iter_mut().find(|r| {
        r.chapter == input.chapter
            && r.block_idx == input.block_idx
            && r.char_offset == input.char_offset
    }) {
        existing.status = input.status.clone();
    } else {
        cfg.corrections
            .push(reader_core::library::CorrectionRecord {
                chapter: input.chapter,
                block_idx: input.block_idx,
                char_offset: input.char_offset,
                original: input.original,
                corrected: input.corrected,
                status: input.status.clone(),
            });
    }

    checked_storage!(env, cfg.save(&data_dir));

    jni_string_or_null!(env, input.status)
}

mod github_sync;

#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_nativePresentation(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
    chapter: jni::sys::jint,
    policy: JString,
    enabled: jboolean,
    events: JString,
) -> jstring {
    let result = (|| -> Result<String, String> {
        let path: String = env.get_string(&path).map_err(|e| e.to_string())?.into();
        let policy: String = env.get_string(&policy).map_err(|e| e.to_string())?.into();
        let events: String = env.get_string(&events).map_err(|e| e.to_string())?.into();
        if events.len() > 32768 {
            return Err("Interaction request exceeds limit".into());
        }
        let events: Vec<reader_core::epub::native::NativeEvent> =
            serde_json::from_str(&events).map_err(|e| e.to_string())?;
        let policy =
            serde_json::from_value(serde_json::json!(policy)).map_err(|e| e.to_string())?;
        let book = get_book(&path)?;
        let chapter = book.chapter(chapter as usize)?;
        serde_json::to_string(
            &chapter
                .native_document
                .present(policy, enabled != 0, &events),
        )
        .map_err(|e| e.to_string())
    })();
    match result {
        Ok(v) => jni_string_or_null!(env, v),
        Err(_) => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_nativeNote(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
    chapter: jni::sys::jint,
    href: JString,
) -> jstring {
    let result = (|| -> Option<String> {
        let path: String = env.get_string(&path).ok()?.into();
        let href: String = env.get_string(&href).ok()?.into();
        if href.len() > 8192 {
            return None;
        }
        let book = get_book(&path).ok()?;
        serde_json::to_string(&book.native_note(chapter as usize, &href)?).ok()
    })();
    match result {
        Some(value) => jni_string_or_null!(env, value),
        None => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_bookArchive(
    mut env: JNIEnv,
    _class: JClass,
    root: JString,
    request: JString,
) -> jstring {
    let result = (|| {
        let root: String = env.get_string(&root).map_err(|_| "Invalid storage")?.into();
        let input: String = env
            .get_string(&request)
            .map_err(|_| "Invalid archive request")?
            .into();
        let value = serde_json::from_str(&input).map_err(|_| "Invalid archive request")?;
        reader_core::library::archive::dispatch(&root, &value)
    })();
    let value = match result {
        Ok(v) => serde_json::json!({"ok":true,"result":v}),
        Err(e) => serde_json::json!({"ok":false,"error":e}),
    };
    env.new_string(value.to_string())
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_bookCategories(
    mut env: JNIEnv,
    _class: JClass,
    root: JString,
    request: JString,
) -> jstring {
    let result = (|| {
        let root: String = env.get_string(&root).map_err(|_| "Invalid storage")?.into();
        let input: String = env
            .get_string(&request)
            .map_err(|_| "Invalid archive request")?
            .into();
        let value = serde_json::from_str(&input).map_err(|_| "Invalid archive request")?;
        reader_core::library::categories::command(&root, &value)
    })();
    let value = match result {
        Ok(v) => serde_json::json!({"ok":true,"result":v}),
        Err(e) => serde_json::json!({"ok":false,"error":e}),
    };
    env.new_string(value.to_string())
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_githubSyncStatus(
    mut env: JNIEnv,
    _class: JClass,
    root: JString,
) -> jstring {
    let root: String = match env.get_string(&root) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    env.new_string(serde_json::to_string(&reader_core::github_sync::sync_meter(&root)).unwrap())
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// Bounded cache counters for renderer/performance verification.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_chapterCacheStats(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
) -> jstring {
    let result = (|| -> std::result::Result<String, String> {
        let path: String = env.get_string(&path).map_err(|e| e.to_string())?.into();
        let stats = get_book(&path)?.chapter_cache_stats();
        Ok(
            serde_json::json!({"resident":stats.0,"body_loads":stats.1,"index_probes":stats.2})
                .to_string(),
        )
    })();
    match result {
        Ok(value) => jni_string_or_null!(env, value),
        Err(_) => std::ptr::null_mut(),
    }
}

/// A successful reader open is independent of importing metadata or changing page.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_recordBookOpened(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    path: JString,
) {
    let Ok(root) = env.get_string(&data_dir) else {
        return;
    };
    let root: String = root.into();
    let Ok(path) = env.get_string(&path) else {
        return;
    };
    let path: String = path.into();
    checked_storage!(env, github_sync::opened(&root, &path));
}

/// Native reading tools share one validated request/response contract.
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_readingTools(
    mut env: JNIEnv,
    _class: JClass,
    request: JString,
) -> jstring {
    let result = (|| -> Result<serde_json::Value, String> {
        let request: String = env
            .get_string(&request)
            .map_err(|_| "Invalid tool request")?
            .into();
        let v: serde_json::Value =
            serde_json::from_str(&request).map_err(|_| "Invalid tool request")?;
        match v["op"].as_str() {
            Some("peer_identity") => {
                let store = get_peer_store(v["root"].as_str().ok_or("Missing storage")?);
                let store = store.lock().unwrap_or_else(|e| e.into_inner());
                Ok(serde_json::json!({"id":store.device_id,"name":store.device_name}))
            }
            Some("txt") => reader_core::txt::command(&v),
            Some("csc_submit") => {
                let samples: Vec<reader_core::csc::contribution::Sample> =
                    serde_json::from_value(v["samples"].clone()).map_err(|_| "Invalid samples")?;
                reader_core::csc::contribution::submit(
                    v["token"].as_str().ok_or("Missing token")?,
                    &samples,
                )
                .map(serde_json::Value::String)
            }
            Some("service") => {
                let request = serde_json::from_value(v).map_err(|_| "Invalid service request")?;
                reader_core::reading_services::query(&request).map(serde_json::Value::String)
            }
            _ => Err("Invalid reading tool".into()),
        }
    })();
    let envelope = match result {
        Ok(value) => serde_json::json!({"ok":true,"result":value}),
        Err(error) => serde_json::json!({"ok":false,"error":error}),
    };
    jni_string_or_null!(env, envelope.to_string())
}
