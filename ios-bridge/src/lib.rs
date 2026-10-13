//! Owned JSON C ABI for Swift. No Rust references or tokens cross the boundary.
mod annotations;
mod nearby;
mod sync;
use reader_core::{
    cloud_sync::{valid_device_id, SyncJournal},
    epub::{ContentBlock, EpubBook},
    library::{BookEntry, Library},
    sharing::ProgressEntry,
};
use serde_json::{json, Value};
use std::{
    ffi::{CStr, CString},
    os::raw::c_char,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

pub(crate) static DATA_LOCK: Mutex<()> = Mutex::new(());
struct Pending {
    epoch: u64,
    root: String,
    device: String,
    result: sync::PreparedResult,
}
#[derive(Default)]
struct Session {
    epoch: u64,
    active: Option<Arc<AtomicBool>>,
    pending: Option<Pending>,
}
struct ActiveReset {
    cancel: Arc<AtomicBool>,
    armed: bool,
}
impl Drop for ActiveReset {
    fn drop(&mut self) {
        if self.armed {
            let mut session = SESSION.lock().unwrap_or_else(|e| e.into_inner());
            if session
                .active
                .as_ref()
                .is_some_and(|active| Arc::ptr_eq(active, &self.cancel))
            {
                session.active = None;
            }
        }
    }
}
static SESSION: Mutex<Session> = Mutex::new(Session {
    epoch: 0,
    active: None,
    pending: None,
});

#[no_mangle]
pub extern "C" fn reader_ios_stop_nearby() {
    nearby::stop();
}

#[cfg(test)]
static TEST_SESSION_LOCK: Mutex<()> = Mutex::new(());
static BOOK: Mutex<Option<(String, String, EpubBook)>> = Mutex::new(None);
fn field<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v[key].as_str().ok_or_else(|| format!("Missing {key}"))
}
fn number(v: &Value, key: &str) -> Result<usize, String> {
    v[key]
        .as_u64()
        .and_then(|x| usize::try_from(x).ok())
        .ok_or_else(|| format!("Invalid {key}"))
}
fn entry(root: &str, id: &str) -> Result<BookEntry, String> {
    Library::load_from(root)
        .books
        .into_iter()
        .find(|b| b.id == id)
        .ok_or("Book is no longer in the library".into())
}
struct ImportScratch(PathBuf);
impl Drop for ImportScratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn entry_value(root: &str, entry: &BookEntry) -> Value {
    let mut value = json!(entry);
    value["cover_path"] = json!(PathBuf::from(root)
        .join("books")
        .join(format!("{}.cover", entry.id)));
    value
}
fn book(root: &str, entry: &BookEntry) -> Result<EpubBook, String> {
    let mut cached = BOOK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((r, id, b)) = &*cached {
        if r == root && id == &entry.id {
            return Ok(b.clone());
        }
    }
    let parsed = EpubBook::open(&entry.path)?;
    if parsed.chapters.is_empty() {
        return Err("EPUB has no readable chapters".into());
    }
    *cached = Some((root.into(), entry.id.clone(), parsed.clone()));
    Ok(parsed)
}
fn validate_position(
    b: &EpubBook,
    chapter: usize,
    block: usize,
    offset: usize,
) -> Result<(), String> {
    let ch = b
        .chapters
        .get(chapter)
        .ok_or("Reading chapter is out of range")?;
    if ch.blocks.is_empty() && block == 0 && offset == 0 {
        return Ok(());
    }
    let content = ch
        .blocks
        .get(block)
        .ok_or("Reading block is out of range")?;
    let count = match content {
        ContentBlock::Heading { spans, .. } | ContentBlock::Paragraph { spans, .. } => {
            spans.iter().map(|s| s.text.chars().count()).sum()
        }
        _ => 0,
    };
    if offset > count {
        return Err("Reading character offset is out of range".into());
    }
    Ok(())
}
fn dispatch(v: Value) -> Result<Value, String> {
    let root = field(&v, "root")?;
    let device = field(&v, "device")?;
    if !PathBuf::from(root).is_absolute() || !valid_device_id(device) {
        return Err("Invalid storage or device identifier".into());
    }
    std::fs::create_dir_all(root).map_err(|_| "Cannot create library storage")?;
    match field(&v, "op")? {
        "peer_state" | "peer_start" | "peer_connect" | "peer_remove" => {
            nearby::dispatch(root, device, &v)
        }
        "sync" => {
            let epoch = v["epoch"].as_u64().ok_or("Missing session epoch")?;
            let token = field(&v, "token")?.to_owned();
            let owner = field(&v, "owner")?.to_owned();
            let cancel = Arc::new(AtomicBool::new(false));
            {
                let mut session = SESSION.lock().unwrap_or_else(|e| e.into_inner());
                if epoch != session.epoch {
                    return Err("Sync cancelled".into());
                }
                if session.active.is_some() {
                    return Err("Sync is already running".into());
                }
                session.active = Some(cancel.clone());
                session.pending = None;
            }
            let mut reset = ActiveReset {
                cancel: cancel.clone(),
                armed: true,
            };
            let result = sync::run(
                root,
                device,
                token,
                owner,
                cancel.clone(),
                v["count_contributions"].as_bool().unwrap_or(false),
                match v["phase"].as_str().unwrap_or("progress") {
                    "progress" => reader_core::github_sync::SyncPhase::Progress,
                    "books" => reader_core::github_sync::SyncPhase::Books,
                    _ => return Err("Invalid sync phase".into()),
                },
                v["retry_failed"].as_bool().unwrap_or(false),
            );
            let mut session = SESSION.lock().unwrap_or_else(|e| e.into_inner());
            reset.armed = false;
            session.active = None;
            if session.epoch != epoch || cancel.load(Ordering::SeqCst) {
                sync::discard_staging(root);
                return Err("Sync cancelled".into());
            }
            let result = match result {
                Ok(value) => value,
                Err(error) => {
                    sync::discard_staging(root);
                    return Err(error);
                }
            };
            session.pending = Some(Pending {
                epoch,
                root: root.into(),
                device: device.into(),
                result,
            });
            Ok(json!({"epoch":epoch}))
        }
        "apply_sync" => {
            let mut session = SESSION.lock().unwrap_or_else(|e| e.into_inner());
            let epoch = v["epoch"].as_u64().ok_or("Missing session epoch")?;
            if epoch != session.epoch {
                return Err("Sync cancelled".into());
            }
            let pending = session.pending.take().ok_or("No prepared sync")?;
            if pending.epoch != epoch || pending.root != root || pending.device != device {
                return Err("Stale sync result".into());
            }
            sync::apply(
                root,
                device,
                pending.result,
                v["open_path"].as_str().unwrap_or(""),
            )
        }
        op => {
            let _guard = DATA_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            match op {
                "closed" => {
                    nearby::closed();
                    Ok(json!({"ok":true}))
                }
                "annotations" => annotations::dispatch(root, &v),
                "remove" => annotations::remove(root, &v),
                "library" => {
                    let entries = Library::load_from(root).books;
                    let mut values = Vec::new();
                    for e in entries {
                        let cover = PathBuf::from(root)
                            .join("books")
                            .join(format!("{}.cover", e.id));
                        if !cover.exists() {
                            let data = EpubBook::open(&e.path)
                                .ok()
                                .and_then(|b| b.cover_data)
                                .filter(|d| d.len() <= 8 * 1024 * 1024)
                                .unwrap_or_default();
                            let _ = std::fs::write(&cover, data);
                        }
                        let mut value = json!(e);
                        value["cover_path"] = json!(cover);
                        values.push(value);
                    }
                    Ok(json!(values))
                }
                "import" => {
                    let path = field(&v, "path")?;
                    // Local books can be larger than the cloud's 20 MiB limit.
                    if std::fs::metadata(path)
                        .map_err(|_| "Cannot read selected file")?
                        .len()
                        > 100 * 1024 * 1024
                    {
                        return Err("EPUB exceeds the local 100 MiB limit".into());
                    }
                    // Do not derive a managed UUID from a user-supplied filename.
                    // Copy a bounded, validated snapshot first; an external file named
                    // after an existing book must never overwrite that book.
                    use std::io::Read;
                    let scratch = ImportScratch(PathBuf::from(root).join(".epub-import.tmp"));
                    let source =
                        std::fs::File::open(path).map_err(|_| "Cannot read selected file")?;
                    let mut target = std::fs::File::create(&scratch.0)
                        .map_err(|_| "Cannot copy EPUB into library storage")?;
                    let size = std::io::copy(&mut source.take(100 * 1024 * 1024 + 1), &mut target)
                        .map_err(|_| "Cannot copy EPUB into library storage")?;
                    if size > 100 * 1024 * 1024 {
                        return Err("EPUB exceeds the local 100 MiB limit".into());
                    }
                    target
                        .sync_all()
                        .map_err(|_| "Cannot copy EPUB into library storage")?;
                    let parsed = EpubBook::open(&scratch.0)?;
                    if parsed.chapters.is_empty() {
                        return Err("EPUB has no readable chapters".into());
                    }
                    let hash =
                        EpubBook::file_hash(scratch.0.to_str().ok_or("Invalid import path")?)?;
                    let mut lib = Library::load_from(root);
                    if let Some(existing) = lib
                        .books
                        .iter()
                        .find(|b| EpubBook::file_hash(&b.path).ok().as_ref() == Some(&hash))
                    {
                        return Ok(entry_value(root, existing));
                    }
                    let before = lib.clone();
                    let e = lib.add_or_update(
                        root,
                        parsed.title,
                        scratch.0.to_string_lossy().into_owned(),
                        0,
                        None,
                    );
                    if !PathBuf::from(&e.path).starts_with(PathBuf::from(root).join("books"))
                        || EpubBook::file_hash(&e.path).ok().as_ref() != Some(&hash)
                    {
                        before.save_to(root);
                        let _ = std::fs::remove_file(
                            PathBuf::from(root)
                                .join("books")
                                .join(format!("{}.epub", e.id)),
                        );
                        let _ = std::fs::remove_file(
                            PathBuf::from(root)
                                .join("books")
                                .join(format!("{}.json", e.id)),
                        );
                        return Err("Cannot copy EPUB into library storage".into());
                    }
                    Ok(entry_value(root, &e))
                }
                "open" => {
                    let e = entry(root, field(&v, "id")?)?;
                    let parsed = book(root, &e)?;
                    let rendered = parsed.render_blocks(&e.path)?;
                    let mut budget = 0usize;
                    let mut chapters = Vec::new();
                    for (ch, rendered) in parsed.chapters.iter().zip(rendered) {
                        let mut blocks = Vec::new();
                        for (index, offset, block) in rendered {
                            let mut value = match block {
                                ContentBlock::Image { data, alt } => {
                                    budget =
                                        budget.saturating_add(data.len().saturating_mul(4) / 3);
                                    json!({"type":"Image", "data":reader_core::base64_encode(&data), "alt":alt})
                                }
                                _ => {
                                    let value = serde_json::to_value(block)
                                        .map_err(|_| "Cannot encode chapter")?;
                                    budget = budget.saturating_add(value.to_string().len());
                                    value
                                }
                            };
                            if budget > 64 * 1024 * 1024 {
                                return Err("EPUB rendered content exceeds the 64 MiB limit".into());
                            }
                            value["core_block"] = json!(index);
                            value["core_char_offset"] = json!(offset);
                            blocks.push(value);
                        }
                        chapters.push(json!({"title":ch.title,"blocks":blocks}));
                    }
                    nearby::opened(root, &e.path);
                    Ok(
                        json!({"entry":entry_value(root,&e),"chapters":chapters,"toc":parsed.toc,"cover":parsed.cover_data.map(|d| reader_core::base64_encode(&d))}),
                    )
                }
                "save" => {
                    let e = entry(root, field(&v, "id")?)?;
                    let b = book(root, &e)?;
                    let (chapter, block, offset) = (
                        number(&v, "chapter")?,
                        number(&v, "block")?,
                        number(&v, "char_offset")?,
                    );
                    validate_position(&b, chapter, block, offset)?;
                    let title = Some(b.chapters[chapter].title.clone());
                    let mut lib = Library::load_from(root);
                    lib.update_position(root, &e.path, chapter, title.clone(), block, offset);
                    let saved = entry(root, &e.id)?;
                    let config = Library::read_book_config(root, &e.id)
                        .ok_or("Cannot save reading position")?;
                    if (saved.last_chapter, saved.last_block, saved.last_char_offset)
                        != (chapter, block, offset)
                        || (
                            config.last_chapter,
                            config.last_block,
                            config.last_char_offset,
                        ) != (chapter, block, offset)
                    {
                        return Err("Cannot save reading position".into());
                    }
                    let mut journal = SyncJournal::load(root, device);
                    if journal.record(ProgressEntry {
                        book_hash: EpubBook::file_hash(&e.path)?,
                        title: e.title.clone(),
                        chapter,
                        block,
                        char_offset: offset,
                        chapter_title: title,
                        timestamp: reader_core::now_secs(),
                    }) {
                        journal.save(root)?;
                    }
                    Ok(entry_value(root, &saved))
                }
                _ => Err("Unknown core operation".into()),
            }
        }
    }
}
/// Input is borrowed for this call. Result is owned and MUST be freed once.
/// Submit a UTF-8 JSON request; release the returned owned string with `reader_ios_free`.
///
/// # Safety
/// `input` must point to a readable, NUL-terminated UTF-8 string that remains
/// valid for the duration of this call. A null pointer is accepted as an error.
#[no_mangle]
pub unsafe extern "C" fn reader_ios_request(input: *const c_char) -> *mut c_char {
    let result = std::panic::catch_unwind(|| {
        if input.is_null() {
            return Err("Null request".into());
        }
        let input = unsafe { CStr::from_ptr(input) }
            .to_str()
            .map_err(|_| "Invalid UTF-8 request")?;
        if input.len() > 1024 * 1024 {
            return Err("Request exceeds size limit".into());
        }
        dispatch(serde_json::from_str(input).map_err(|_| "Invalid JSON request")?)
    })
    .unwrap_or_else(|_| Err("Reader core failed".into()));
    let value = match result {
        Ok(result) => json!({"ok":true,"result":result}),
        Err(error) => json!({"ok":false,"error":error}),
    };
    CString::new(value.to_string())
        .map(CString::into_raw)
        .unwrap_or(std::ptr::null_mut())
}
/// Release a response allocated by `reader_ios_request`.
///
/// # Safety
/// `value` must be null or a still-owned pointer returned by this library.
/// Free each non-null response exactly once; never free a borrowed pointer.
#[no_mangle]
pub unsafe extern "C" fn reader_ios_free(value: *mut c_char) {
    if !value.is_null() {
        drop(unsafe { CString::from_raw(value) });
    }
}
#[no_mangle]
pub extern "C" fn reader_ios_epoch() -> u64 {
    SESSION.lock().unwrap_or_else(|e| e.into_inner()).epoch
}
#[no_mangle]
pub extern "C" fn reader_ios_cancel_sync() {
    let mut session = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    session.epoch = session.epoch.wrapping_add(1);
    if let Some(pending) = session.pending.take() {
        sync::discard_staging(&pending.root);
    }
    if let Some(cancel) = &session.active {
        cancel.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::TEST_SESSION_LOCK as TEST_LOCK;
    use super::*;
    fn request(root: &str, op: &str, extra: Value) -> Value {
        let mut v = json!({"root":root,"device":"ios-test","op":op});
        if let Some(fields) = extra.as_object() {
            for (k, value) in fields {
                v[k] = value.clone();
            }
        }
        let input = CString::new(v.to_string()).unwrap();
        unsafe {
            let output = reader_ios_request(input.as_ptr());
            assert!(!output.is_null());
            let result = serde_json::from_str(CStr::from_ptr(output).to_str().unwrap()).unwrap();
            reader_ios_free(output);
            result
        }
    }
    fn fixture(root: &str) -> PathBuf {
        let text = PathBuf::from(root).join("fixture.txt");
        std::fs::write(&text,"第一章 开始\n中文 😀 e\u{301} 测试。\n第二章 回读\n继续阅读。\n第三章 结束\n最后一章。").unwrap();
        reader_core::txt::convert_txt_to_epub(&text, &PathBuf::from(root), &Default::default())
            .unwrap()
            .epub_path
    }
    #[test]
    fn annotations_roundtrip_unicode_validation_and_local_batch_delete_preserve_external_source() {
        let _test = TEST_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let source = fixture(root);
        let e = request(root, "import", json!({"path":source}))["result"].clone();
        let id = e["id"].as_str().unwrap();
        let parsed = book(root, &entry(root, id).unwrap()).unwrap();
        let (chapter, block) = parsed
            .chapters
            .iter()
            .enumerate()
            .find_map(|(c, ch)| {
                ch.blocks
                    .iter()
                    .enumerate()
                    .find_map(|(b, block)| match block {
                        ContentBlock::Paragraph { spans, .. }
                            if spans.iter().any(|s| s.text.contains('😀')) =>
                        {
                            Some((c, b))
                        }
                        _ => None,
                    })
            })
            .unwrap();
        assert_eq!(
            request(
                root,
                "save",
                json!({"id":id,"chapter":chapter,"block":block,"char_offset":4})
            )["ok"],
            true
        );
        let added = request(
            root,
            "annotations",
            json!({"id":id,"action":"highlight","chapter":chapter,"start_block":block,"start_offset":0,"end_block":block,"end_offset":4,"color":"Blue","note":"测试笔记"}),
        );
        assert_eq!(added["ok"], true, "{added}");
        let hid = added["result"]["highlights"][0]["id"].as_str().unwrap();
        assert_eq!(
            request(root, "annotations", json!({"id":id}))["result"]["notes"][0]["content"],
            "测试笔记"
        );
        assert_eq!(
            request(
                root,
                "annotations",
                json!({"id":id,"action":"highlight","chapter":chapter,"start_block":block,"start_offset":4,"end_block":block,"end_offset":0})
            )["ok"],
            false
        );
        assert_eq!(
            request(
                root,
                "annotations",
                json!({"id":id,"action":"delete","highlight_id":hid})
            )["result"]["highlights"],
            json!([])
        );
        assert_eq!(
            Library::read_book_config(root, id)
                .unwrap()
                .last_char_offset,
            4
        );
        assert_eq!(
            request(root, "remove", json!({"ids":[id,"unknown"]}))["ok"],
            false
        );
        assert!(std::path::Path::new(e["path"].as_str().unwrap()).exists());
        assert_eq!(request(root, "remove", json!({"ids":[id]}))["ok"], true);
        assert!(source.exists());
        assert!(Library::load_from(root).books.is_empty());
        assert!(PathBuf::from(root)
            .join("deleted-local-books.json")
            .exists());
    }
    #[test]
    fn ffi_rejects_null_and_invalid_utf8_and_returns_owned_errors() {
        let _test = TEST_LOCK.lock().unwrap();
        unsafe {
            for input in [std::ptr::null(), [0xffu8, 0].as_ptr().cast()] {
                let output = reader_ios_request(input);
                assert!(!output.is_null());
                let value: Value =
                    serde_json::from_str(CStr::from_ptr(output).to_str().unwrap()).unwrap();
                assert_eq!(value["ok"], false);
                reader_ios_free(output);
            }
            reader_ios_free(std::ptr::null_mut());
        }
    }
    #[test]
    fn import_owned_epub_reopen_scalar_progress_and_deduplicate_without_reset() {
        let _test = TEST_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let path = fixture(root);
        let imported = request(root, "import", json!({"path":path}));
        assert_eq!(imported["ok"], true, "{imported}");
        let id = imported["result"]["id"].as_str().unwrap();
        assert!(PathBuf::from(imported["result"]["path"].as_str().unwrap())
            .starts_with(dir.path().join("books")));
        let opened = request(root, "open", json!({"id":id}));
        assert_eq!(opened["ok"], true);
        let chapters = opened["result"]["chapters"].as_array().unwrap();
        let (chapter, block) = chapters
            .iter()
            .enumerate()
            .find_map(|(c, ch)| {
                ch["blocks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .position(|b| {
                        b["spans"].as_array().is_some_and(|spans| {
                            spans
                                .iter()
                                .any(|s| s["text"].as_str().unwrap().contains('😀'))
                        })
                    })
                    .map(|b| (c, b))
            })
            .unwrap();
        assert_eq!(
            request(
                root,
                "save",
                json!({"id":id,"chapter":chapter,"block":block,"char_offset":4})
            )["ok"],
            true
        );
        // Reopen the persistent files, not the Swift prototype's UserDefaults.
        let saved = Library::load_from(root).books[0].clone();
        assert_eq!(
            (saved.last_chapter, saved.last_block, saved.last_char_offset),
            (chapter, block, 4)
        );
        let journal = SyncJournal::load(root, "ios-test");
        let p = journal.own.progress.values().next().unwrap();
        assert_eq!(p.char_offset, 4);
        assert_eq!(
            request(
                root,
                "save",
                json!({"id":id,"chapter":chapter,"block":block,"char_offset":9999})
            )["ok"],
            false
        );
        assert_eq!(
            request(root, "import", json!({"path":path}))["result"]["id"],
            id
        );
        assert_eq!(Library::load_from(root).books.len(), 1);
        assert_eq!(Library::load_from(root).books[0].last_char_offset, 4);
        // Source may disappear: the app has its own copy.
        std::fs::remove_file(path).unwrap();
        *BOOK.lock().unwrap() = None;
        assert_eq!(request(root, "open", json!({"id":id}))["ok"], true);
        let old_hash = EpubBook::file_hash(&saved.path).unwrap();
        let other_text = dir.path().join("different.txt");
        std::fs::write(
            &other_text,
            "第一章 新书\n独立的正文内容。\n第二章 后续\n另一本书。",
        )
        .unwrap();
        let other =
            reader_core::txt::convert_txt_to_epub(&other_text, dir.path(), &Default::default())
                .unwrap();
        let incoming = dir.path().join("incoming");
        std::fs::create_dir(&incoming).unwrap();
        let collision = incoming.join(format!("{id}.epub"));
        std::fs::copy(other.epub_path, &collision).unwrap();
        let second = request(root, "import", json!({"path":collision}));
        assert_eq!(second["ok"], true);
        assert_ne!(second["result"]["id"], id);
        assert_eq!(EpubBook::file_hash(&saved.path).unwrap(), old_hash);
        let invalid = dir.path().join("bad.epub");
        std::fs::write(&invalid, "not EPUB").unwrap();
        assert_eq!(
            request(root, "import", json!({"path":invalid}))["ok"],
            false
        );
        assert_eq!(Library::load_from(root).books.len(), 2);
    }
    #[test]
    fn an_unwind_cannot_leave_sync_permanently_busy() {
        let _test = TEST_LOCK.lock().unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        SESSION.lock().unwrap().active = Some(cancel.clone());
        let result = std::panic::catch_unwind(|| {
            let _reset = ActiveReset {
                cancel,
                armed: true,
            };
            panic!("synthetic worker failure");
        });
        assert!(result.is_err());
        assert!(SESSION.lock().unwrap().active.is_none());
    }
    #[test]
    fn stale_epoch_stops_network_and_cannot_apply_after_cancel() {
        let _test = TEST_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let old = reader_ios_epoch();
        reader_ios_cancel_sync();
        let result = request(
            root,
            "sync",
            json!({"token":"test-only","owner":"test","epoch":old}),
        );
        assert_eq!(result["ok"], false);
        assert_eq!(result["error"], "Sync cancelled");
        assert_eq!(
            request(root, "apply_sync", json!({"epoch":old}))["ok"],
            false
        );
        // Missing credentials must not leave the active guard permanently busy.
        assert_eq!(
            request(root, "sync", json!({"epoch":reader_ios_epoch()}))["ok"],
            false
        );
        assert!(SESSION.lock().unwrap().active.is_none());
    }
}
