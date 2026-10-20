//! JNI adapter: network work only stages files; application happens on the Android UI thread.
use jni::{
    objects::{JClass, JString},
    sys::jstring,
    JNIEnv,
};
use reader_core::{
    cloud_sync::{valid_hash, CloudBook, DeviceSnapshot, SyncJournal, MAX_DOCUMENT_BYTES},
    epub::EpubBook,
    github_sync::{
        complete_transfer, record_transfer_failure, sync_phase, LocalBook, SyncPhase,
        TransferFailure,
    },
    library::Library,
    sharing::ProgressEntry,
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

static JOURNAL_LOCK: Mutex<()> = Mutex::new(());
static EPOCH: AtomicU64 = AtomicU64::new(0);
static ACTIVE: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

#[derive(Serialize, Deserialize)]
struct StagedBook {
    info: CloudBook,
}
#[derive(Serialize, Deserialize)]
struct PreparedResult {
    #[serde(default)]
    owner: String,
    own: DeviceSnapshot,
    snapshots: Vec<DeviceSnapshot>,
    downloaded: Vec<StagedBook>,
    uploaded: usize,
    skipped: usize,
    #[serde(default)]
    confirmed_archives: std::collections::BTreeSet<String>,
    #[serde(default)]
    failed: Vec<TransferFailure>,
    #[serde(default)]
    next_retry_after: Option<u64>,
    #[serde(default)]
    terminal_error: Option<String>,
    #[serde(default)]
    annotation_error: Option<String>,
}

fn journal(data_dir: &str) -> SyncJournal {
    let device_id = super::get_peer_store(data_dir)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .device_id
        .clone();
    SyncJournal::load(data_dir, &device_id)
}
pub(super) fn seed_local(data_dir: &str, path: &str) {
    let _guard = JOURNAL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut journal = journal(data_dir);
    if let Ok(library) = Library::read_snapshot(data_dir) {
        if let Some(entry) = library.books.iter().find(|e| e.path == path) {
            journal.seed_entry(data_dir, entry);
        }
    }
    let _ = journal.save(data_dir);
}
#[allow(clippy::too_many_arguments)]
pub(super) fn commit_local(
    data_dir: &str,
    device: &str,
    path: &str,
    chapter: usize,
    chapter_title: Option<String>,
    block: usize,
    char_offset: usize,
) -> Result<Option<reader_core::cloud_progress::VersionedProgress>, String> {
    let event_ms = reader_core::now_millis();
    let _guard = JOURNAL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut library = Library::load_from(data_dir)?;
    let Some(entry) = library.books.iter().find(|e| e.path == path).cloned() else {
        return Ok(None);
    };
    let mut journal = SyncJournal::load(data_dir, device);
    journal.seed_entry(data_dir, &entry);
    if (entry.last_chapter, entry.last_block, entry.last_char_offset)
        == (chapter, block, char_offset)
    {
        library.apply_position(data_dir, path, chapter, chapter_title, block, char_offset)?;
        return Ok(None);
    }
    let hash = Library::load_book_config(data_dir, &entry.id)?
        .and_then(|c| c.file_hash)
        .map(Ok)
        .unwrap_or_else(|| EpubBook::file_hash(path))?;
    let progress = ProgressEntry {
        book_hash: hash.clone(),
        title: entry.title,
        chapter,
        block,
        char_offset,
        chapter_title,
        timestamp: event_ms / 1000,
    };
    if !journal.record_at(progress, event_ms) {
        return Ok(None);
    }
    let record = reader_core::cloud_progress::VersionedProgress {
        progress: journal.own.progress[&hash].clone(),
        version: journal.own.progress_version(&hash),
    };
    if !library.apply_versioned_position(data_dir, path, &record)? {
        return Ok(None);
    }
    journal.save(data_dir)?;
    Ok(Some(record))
}
pub(super) fn accept_committed(
    data_dir: &str,
    device: &str,
    notices: &[reader_core::sharing::CommittedProgress],
) -> Result<(), String> {
    if notices.is_empty() {
        return Ok(());
    }
    let _guard = JOURNAL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut journal = SyncJournal::load(data_dir, device);
    for notice in notices {
        journal.accept(&notice.record);
    }
    journal.save(data_dir)
}
fn stage_path(data_dir: &str, hash: &str) -> Result<PathBuf, String> {
    if !valid_hash(hash) {
        return Err("Invalid staged book hash".into());
    }
    Ok(PathBuf::from(data_dir)
        .join("github-sync-staging")
        .join(format!("{hash}.epub")))
}
fn run(
    data_dir: &str,
    token: String,
    owner: String,
    cancel: Arc<AtomicBool>,
    count_contributions: bool,
    phase: SyncPhase,
    retry_failed: bool,
) -> Result<PreparedResult, String> {
    // Read the index without migrations/writes while UI may be saving its current position.
    let bytes = std::fs::read(PathBuf::from(data_dir).join("library.json"))
        .unwrap_or_else(|_| b"{\"books\":[]}".to_vec());
    let library: Library =
        serde_json::from_slice(&bytes).map_err(|_| "Cannot read local library")?;
    let journal = {
        let _guard = JOURNAL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        journal(data_dir)
    };
    let mut journal = journal;
    for entry in &library.books {
        journal.seed_entry(data_dir, entry);
    }
    let mut own = journal.own.clone();
    let mut books = Vec::new();
    let mut skipped = 0;
    for entry in library.books {
        if cancel.load(Ordering::SeqCst) {
            return Err("Sync cancelled".into());
        }
        if phase == SyncPhase::Progress {
            if let Some(hash) =
                Library::read_book_config(data_dir, &entry.id).and_then(|c| c.file_hash)
            {
                if !journal.seen.contains(&hash) {
                    own.progress
                        .entry(hash.clone())
                        .or_insert_with(|| ProgressEntry {
                            book_hash: hash,
                            title: entry.title,
                            chapter: entry.last_chapter,
                            block: entry.last_block,
                            char_offset: entry.last_char_offset,
                            chapter_title: entry.last_chapter_title,
                            timestamp: if (
                                entry.last_chapter,
                                entry.last_block,
                                entry.last_char_offset,
                            ) == (0, 0, 0)
                            {
                                0
                            } else {
                                entry.last_opened
                            },
                        });
                }
            }
            continue;
        }
        let Ok(metadata) = std::fs::metadata(&entry.path) else {
            skipped += 1;
            continue;
        };
        if metadata.len() > MAX_DOCUMENT_BYTES {
            skipped += 1;
            continue;
        }
        let Some(hash) = Library::read_book_config(data_dir, &entry.id)
            .and_then(|c| c.file_hash)
            .or_else(|| EpubBook::file_hash(&entry.path).ok())
        else {
            skipped += 1;
            continue;
        };
        if !journal.seen.contains(&hash) {
            own.progress
                .entry(hash.clone())
                .or_insert_with(|| ProgressEntry {
                    book_hash: hash.clone(),
                    title: entry.title.clone(),
                    chapter: entry.last_chapter,
                    block: entry.last_block,
                    char_offset: entry.last_char_offset,
                    chapter_title: entry.last_chapter_title.clone(),
                    timestamp: if (entry.last_chapter, entry.last_block, entry.last_char_offset)
                        == (0, 0, 0)
                    {
                        0
                    } else {
                        entry.last_opened
                    },
                });
        }
        books.push(LocalBook {
            info: CloudBook {
                hash,
                title: entry.title,
                size: metadata.len(),
            },
            path: PathBuf::from(entry.path),
        });
    }
    let mut result = sync_phase(
        token,
        owner.clone(),
        own,
        books,
        cancel.clone(),
        count_contributions,
        phase,
        data_dir,
        retry_failed,
    )?;
    let mut downloaded = Vec::new();
    for book in result.downloaded {
        if cancel.load(Ordering::SeqCst) {
            return Err("Sync cancelled".into());
        }
        let path = stage_path(data_dir, &book.info.hash)?;
        let staged = (|| {
            std::fs::create_dir_all(path.parent().unwrap())
                .map_err(|_| "Cannot create sync staging directory")?;
            book.save_to(&path)?;
            EpubBook::open_cached(&path, &PathBuf::from(data_dir).join(".cache/chapter-index"))
                .map_err(|_| "Downloaded file is not a valid EPUB")?;
            Ok::<_, String>(())
        })();
        if let Err(error) = staged {
            let _ = std::fs::remove_file(&path);
            result.failed = record_transfer_failure(
                data_dir,
                &owner,
                &result.own.device_id,
                "download",
                &book.info,
                error,
            )?;
            result.next_retry_after = Some(30);
            continue;
        }
        downloaded.push(StagedBook { info: book.info });
    }
    Ok(PreparedResult {
        owner,
        own: result.own,
        snapshots: result.snapshots,
        downloaded,
        uploaded: result.uploaded,
        skipped,
        confirmed_archives: result.confirmed_archives,
        failed: result.failed,
        next_retry_after: result.next_retry_after,
        terminal_error: result.terminal_error,
        annotation_error: result.annotation_error,
    })
}
fn apply(
    data_dir: &str,
    mut prepared: PreparedResult,
    open_path: &str,
) -> Result<serde_json::Value, String> {
    let _guard = JOURNAL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut journal = journal(data_dir);
    journal.own.merge_own(&prepared.own)?;
    reader_core::reading_activity::import(data_dir, &journal.own.device_id, &prepared.snapshots)?;
    journal.seen.extend(prepared.own.progress.keys().cloned());
    let latest = reader_core::cloud_progress::latest_records(&prepared.snapshots, &journal.own);
    let mut library = Library::load_from(data_dir)?;
    reader_core::library::archive::ArchiveStore::apply_verified(
        data_dir,
        &mut library,
        &prepared.snapshots,
        &journal.own,
        &prepared.confirmed_archives,
    )?;
    let mut paths = std::collections::BTreeMap::new();
    for entry in &library.books {
        if let Some(hash) = Library::read_book_config(data_dir, &entry.id)
            .and_then(|config| config.file_hash)
            .or_else(|| EpubBook::file_hash(&entry.path).ok())
        {
            paths.insert(hash, entry.path.clone());
        }
    }
    let mut imported = 0;
    for book in prepared.downloaded {
        let staged = stage_path(data_dir, &book.info.hash)?;
        if reader_core::library::archive::ArchiveStore::load(data_dir)?.hidden(&book.info.hash) {
            let _ = std::fs::remove_file(staged);
            prepared.failed = complete_transfer(
                data_dir,
                &prepared.owner,
                &journal.own.device_id,
                "download",
                &book.info.hash,
            )?;
            continue;
        }
        if !paths.contains_key(&book.info.hash) {
            let import = (|| {
                if EpubBook::file_hash(staged.to_str().ok_or("Invalid staging path")?)?
                    != book.info.hash
                {
                    return Err("Staged book integrity check failed".into());
                }
                EpubBook::open_cached(
                    &staged,
                    &PathBuf::from(data_dir).join(".cache/chapter-index"),
                )?;
                let previous = library.clone();
                let entry = if let Some(restored) =
                    reader_core::library::archive::restore_download_file(
                        data_dir,
                        &mut library,
                        &book.info.hash,
                        &staged,
                    )? {
                    restored
                } else {
                    library.add_or_update(
                        data_dir,
                        book.info.title.clone(),
                        staged.to_string_lossy().into_owned(),
                        0,
                        None,
                    )?
                };
                if !PathBuf::from(&entry.path).starts_with(PathBuf::from(data_dir).join("books"))
                    || EpubBook::file_hash(&entry.path).ok().as_ref() != Some(&book.info.hash)
                {
                    library = previous;
                    library.save_to(data_dir)?;
                    return Err("Cannot save downloaded book".into());
                }
                Ok::<_, String>(entry)
            })();
            match import {
                Ok(entry) => {
                    paths.insert(book.info.hash.clone(), entry.path);
                    journal.seen.insert(book.info.hash.clone());
                    imported += 1;
                    prepared.failed = complete_transfer(
                        data_dir,
                        &prepared.owner,
                        &journal.own.device_id,
                        "download",
                        &book.info.hash,
                    )?;
                }
                Err(error) => {
                    prepared.failed = record_transfer_failure(
                        data_dir,
                        &prepared.owner,
                        &journal.own.device_id,
                        "download",
                        &book.info,
                        error,
                    )?;
                    prepared.next_retry_after = Some(30);
                }
            }
        } else {
            prepared.failed = complete_transfer(
                data_dir,
                &prepared.owner,
                &journal.own.device_id,
                "download",
                &book.info.hash,
            )?;
        }
        let _ = std::fs::remove_file(staged);
    }
    let mut offer = None;
    let mut updated = Vec::new();
    for (hash, record) in latest {
        let p = &record.progress;
        let Some(path) = paths.get(&hash) else {
            continue;
        };
        let changed = library
            .books
            .iter()
            .find(|e| &e.path == path)
            .is_some_and(|e| {
                (e.last_chapter, e.last_block, e.last_char_offset)
                    != (p.chapter, p.block, p.char_offset)
            });
        if changed
            && !EpubBook::open_cached(path, &PathBuf::from(data_dir).join(".cache/chapter-index"))
                .is_ok_and(|book| reader_core::cloud_progress::validate_position(&book, p))
        {
            continue;
        }
        if !library.apply_versioned_position(data_dir, path, &record)? {
            continue;
        }
        if path == open_path && changed {
            offer = Some(p.clone());
        }
        journal.accept(&record);
        journal.applied.insert(hash, p.timestamp);
        if changed {
            updated.push(serde_json::json!({"path":path,"chapter":p.chapter}));
        }
    }
    if let Err(error) =
        reader_core::github_sync::apply_cached_annotations(data_dir, &prepared.owner)
    {
        prepared.annotation_error.get_or_insert(error);
    }
    journal.apply_recent(data_dir, &mut library, &prepared.snapshots)?;
    journal.save(data_dir)?;
    Ok(
        serde_json::json!({"uploaded":prepared.uploaded,"downloaded":imported,"skipped":prepared.skipped,"offer":offer,"updated":updated,"failed":prepared.failed,"next_retry_after":prepared.next_retry_after,"terminal_error":prepared.terminal_error,"annotation_error":prepared.annotation_error}),
    )
}
fn output(env: &mut JNIEnv, result: Result<serde_json::Value, String>) -> jstring {
    let value = match result {
        Ok(value) => serde_json::json!({"ok":true,"result":value}),
        Err(error) => serde_json::json!({"ok":false,"error":error}),
    };
    env.new_string(value.to_string())
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_runGithubSync(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    token: JString,
    owner: JString,
    epoch: jni::sys::jlong,
    count_contributions: jni::sys::jboolean,
    progress_only: jni::sys::jboolean,
    retry_failed: jni::sys::jboolean,
) -> jstring {
    let result = (|| {
        let data_dir: String = env
            .get_string(&data_dir)
            .map_err(|_| "Invalid data directory")?
            .into();
        let token: String = env.get_string(&token).map_err(|_| "Invalid token")?.into();
        let owner: String = env
            .get_string(&owner)
            .map_err(|_| "Invalid account")?
            .into();
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
            if epoch as u64 != EPOCH.load(Ordering::SeqCst) {
                return Err("Sync cancelled".into());
            }
            if active.is_some() {
                return Err("A sync request is already running".into());
            }
            *active = Some(cancel.clone());
        }
        let result = run(
            &data_dir,
            token,
            owner,
            cancel,
            count_contributions != 0,
            if progress_only != 0 {
                SyncPhase::Progress
            } else {
                SyncPhase::Books
            },
            retry_failed != 0,
        );
        *ACTIVE.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let prepared = result?;
        serde_json::to_value(prepared).map_err(|_| "Cannot encode sync result".into())
    })();
    output(&mut env, result)
}
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_applyGithubSync(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    payload: JString,
    open_path: JString,
) -> jstring {
    let result = (|| {
        let data_dir: String = env
            .get_string(&data_dir)
            .map_err(|_| "Invalid data directory")?
            .into();
        let payload: String = env
            .get_string(&payload)
            .map_err(|_| "Invalid sync result")?
            .into();
        let open_path: String = env
            .get_string(&open_path)
            .map_err(|_| "Invalid open book path")?
            .into();
        if payload.len() > 4 * 1024 * 1024 {
            return Err("Sync result exceeds size limit".into());
        }
        let prepared: PreparedResult =
            serde_json::from_str(&payload).map_err(|_| "Invalid sync result")?;
        prepared.own.validate()?;
        for snapshot in &prepared.snapshots {
            snapshot.validate()?
        }
        apply(&data_dir, prepared, &open_path)
    })();
    output(&mut env, result)
}
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_cancelGithubSync(
    _env: JNIEnv,
    _class: JClass,
) {
    let active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
    EPOCH.fetch_add(1, Ordering::SeqCst);
    if let Some(cancel) = active.as_ref() {
        cancel.store(true, Ordering::SeqCst);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_githubSyncEpoch(
    _env: JNIEnv,
    _class: JClass,
) -> jni::sys::jlong {
    EPOCH.load(Ordering::SeqCst) as jni::sys::jlong
}

pub(super) fn opened(data_dir: &str, path: &str) -> Result<(), String> {
    let _guard = JOURNAL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut library = Library::load_from(data_dir)?;
    library.set_last_opened(data_dir, path, reader_core::now_millis())?;
    let journal = journal(data_dir);
    journal.save(data_dir)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    static TEST_LOCK: Mutex<()> = Mutex::new(());
    fn fixture() -> (tempfile::TempDir, String, String) {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().to_str().unwrap().to_owned();
        let peer = reader_core::sharing::PeerStore {
            device_id: "android-test".into(),
            ..Default::default()
        };
        *super::super::GLOBAL_PEER_STORE.lock().unwrap() =
            Some((data_dir.clone(), Arc::new(Mutex::new(peer))));
        let txt = dir.path().join("source.txt");
        std::fs::write(
            &txt,
            "第一章 开始\n中文阅读。\n第二章 回读\n继续阅读。\n第三章 结束\n最后一章。",
        )
        .unwrap();
        let result =
            reader_core::txt::convert_txt_to_epub(&txt, dir.path(), &Default::default()).unwrap();
        let mut library = Library::load_from(&data_dir).unwrap();
        let entry = library
            .add_or_update(
                &data_dir,
                "Book".into(),
                result.epub_path.to_string_lossy().into_owned(),
                1,
                None,
            )
            .unwrap();
        let hash = EpubBook::file_hash(&entry.path).unwrap();
        library.books[0].last_opened = 10;
        library.save_to(&data_dir).unwrap();
        (dir, entry.path, hash)
    }
    fn progress(hash: &str, chapter: usize, timestamp: u64) -> ProgressEntry {
        ProgressEntry {
            book_hash: hash.into(),
            title: "Book".into(),
            chapter,
            block: 0,
            char_offset: 0,
            chapter_title: None,
            timestamp,
        }
    }
    fn prepared(hash: &str) -> PreparedResult {
        let mut own = DeviceSnapshot::new("android-test".into());
        own.progress.insert(hash.into(), progress(hash, 1, 20));
        let mut remote = DeviceSnapshot::new("desktop-test".into());
        remote.progress.insert(hash.into(), progress(hash, 0, 30));
        PreparedResult {
            owner: "tester".into(),
            own,
            snapshots: vec![remote],
            downloaded: vec![],
            uploaded: 0,
            skipped: 0,
            confirmed_archives: Default::default(),
            failed: vec![],
            next_retry_after: None,
            terminal_error: None,
            annotation_error: None,
        }
    }
    #[test]
    fn latest_progress_updates_open_android_catalog_and_emits_ui_locator() {
        let _test = TEST_LOCK.lock().unwrap();
        let (dir, path, hash) = fixture();
        let data_dir = dir.path().to_str().unwrap();
        let result = apply(data_dir, prepared(&hash), &path).unwrap();
        assert_eq!(result["offer"]["chapter"], 0);
        assert_eq!(
            Library::load_from(data_dir).unwrap().books[0].last_chapter,
            0
        );
        assert!(SyncJournal::load(data_dir, "android-test")
            .own
            .validate()
            .is_ok());
    }
    #[test]
    fn newer_android_local_progress_wins_over_inflight_desktop_result() {
        let _test = TEST_LOCK.lock().unwrap();
        let (dir, _, hash) = fixture();
        let data_dir = dir.path().to_str().unwrap();
        let path = Library::load_from(data_dir).unwrap().books[0].path.clone();
        commit_local(data_dir, "android-test", &path, 2, None, 0, 0).unwrap();
        Library::load_from(data_dir)
            .unwrap()
            .update_position(
                data_dir,
                &Library::load_from(data_dir).unwrap().books[0].path,
                2,
                None,
                0,
                0,
            )
            .unwrap();
        let result = apply(data_dir, prepared(&hash), "").unwrap();
        assert!(result["offer"].is_null());
        assert_eq!(
            SyncJournal::load(data_dir, "android-test").own.progress[&hash].chapter,
            2
        );
        assert_eq!(result["updated"].as_array().unwrap().len(), 0);
    }
    #[test]
    fn newer_remote_progress_can_restore_an_earlier_chapter_for_closed_book() {
        let _test = TEST_LOCK.lock().unwrap();
        let (dir, _, hash) = fixture();
        let data_dir = dir.path().to_str().unwrap();
        apply(data_dir, prepared(&hash), "").unwrap();
        assert_eq!(
            Library::load_from(data_dir).unwrap().books[0].last_chapter,
            0
        );
        assert_eq!(
            SyncJournal::load(data_dir, "android-test").applied[&hash],
            30
        );
    }
    #[test]
    fn recent_only_update_keeps_source_time_and_never_becomes_a_local_open() {
        let _test = TEST_LOCK.lock().unwrap();
        let (dir, _, hash) = fixture();
        let root = dir.path().to_str().unwrap();
        let mut incoming = prepared(&hash);
        incoming.snapshots[0]
            .progress
            .get_mut(&hash)
            .unwrap()
            .chapter = 1;
        let event = reader_core::cloud_recent::ReadingEvent {
            opened_ms: 1_800_000_000_321,
            device: "desktop-test".into(),
        };
        incoming.snapshots[0]
            .recent_reading
            .insert(hash.clone(), event.clone());
        apply(root, incoming, "").unwrap();
        assert_eq!(
            Library::load_from(root).unwrap().books[0].last_opened,
            event.opened_ms
        );
        let journal = SyncJournal::load(root, "android-test");
        assert_eq!(journal.own.recent_reading[&hash], event);
        apply(root, prepared(&hash), "").unwrap();
        assert_eq!(
            SyncJournal::load(root, "android-test").own.recent_reading[&hash],
            event
        );
    }
    #[test]
    fn staging_rejects_path_traversal() {
        assert!(stage_path("/tmp", "../../secret").is_err());
    }
    #[test]
    fn failed_book_import_does_not_undo_already_applied_progress() {
        let _test = TEST_LOCK.lock().unwrap();
        let (dir, _, hash) = fixture();
        let root = dir.path().to_str().unwrap();
        apply(root, prepared(&hash), "").unwrap();
        let mut files = prepared(&hash);
        files.downloaded.push(StagedBook {
            info: CloudBook {
                hash: "b".repeat(64),
                title: "Missing staged file".into(),
                size: 10,
            },
        });
        let result = apply(root, files, "").unwrap();
        assert_eq!(result["failed"].as_array().unwrap().len(), 1);
        assert_eq!(result["downloaded"], 0);
        assert_eq!(Library::load_from(root).unwrap().books[0].last_chapter, 0);
        assert_eq!(SyncJournal::load(root, "android-test").applied[&hash], 30);
    }
}
