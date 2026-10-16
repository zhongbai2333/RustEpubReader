//! iOS sync adapter. Network work stages validated EPUBs; apply merges against the latest local journal.
use reader_core::{
    cloud_sync::{valid_hash, CloudBook, DeviceSnapshot, SyncJournal, MAX_BOOK_BYTES},
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
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

#[derive(Serialize, Deserialize)]
struct StagedBook {
    info: CloudBook,
}
#[derive(Serialize, Deserialize)]
pub(crate) struct PreparedResult {
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
}

fn stage_path(data_dir: &str, hash: &str) -> Result<PathBuf, String> {
    if !valid_hash(hash) {
        return Err("Invalid staged book hash".into());
    }
    Ok(PathBuf::from(data_dir)
        .join("github-sync-staging")
        .join(format!("{hash}.epub")))
}
pub(crate) fn discard_staging(data_dir: &str) {
    // Only our own hash-named temporary EPUBs; no library files or user imports.
    let Ok(entries) = std::fs::read_dir(PathBuf::from(data_dir).join("github-sync-staging")) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.path().extension().is_some_and(|s| s == "epub")
            && entry
                .path()
                .file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(valid_hash)
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}
// Mirrors the explicit phase/session parameters passed across the JSON bridge.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    data_dir: &str,
    device_id: &str,
    token: String,
    owner: String,
    cancel: Arc<AtomicBool>,
    count_contributions: bool,
    phase: SyncPhase,
    retry_failed: bool,
) -> Result<PreparedResult, String> {
    discard_staging(data_dir);
    let (library, journal) = {
        let _guard = super::DATA_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        (
            Library::load_from(data_dir),
            SyncJournal::load(data_dir, device_id),
        )
    };
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
        if metadata.len() > MAX_BOOK_BYTES {
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
            std::fs::write(&path, &book.bytes).map_err(|_| "Cannot stage downloaded book")?;
            EpubBook::open(&path).map_err(|_| "Downloaded file is not a valid EPUB")?;
            Ok::<_, String>(())
        })();
        if let Err(error) = staged {
            let _ = std::fs::remove_file(&path);
            result.failed = record_transfer_failure(
                data_dir, &owner, device_id, "download", &book.info, error,
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
    })
}
pub(crate) fn apply(
    data_dir: &str,
    device_id: &str,
    mut prepared: PreparedResult,
    open_path: &str,
) -> Result<serde_json::Value, String> {
    let _guard = super::DATA_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut journal = SyncJournal::load(data_dir, device_id);
    journal.own.merge_own(&prepared.own)?;
    reader_core::reading_activity::import(data_dir, device_id, &prepared.snapshots)?;
    journal.seen.extend(prepared.own.progress.keys().cloned());
    let latest = reader_core::cloud_progress::latest_records(&prepared.snapshots, &journal.own);
    let mut library = Library::load_from(data_dir);
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
                device_id,
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
                EpubBook::open(&staged)?;
                let previous = library.clone();
                let entry = if let Some(restored) = reader_core::library::archive::restore_download(
                    data_dir,
                    &mut library,
                    &book.info.hash,
                    &std::fs::read(&staged).map_err(|_| "Cannot read staged book")?,
                )? {
                    restored
                } else {
                    library.add_or_update(
                        data_dir,
                        book.info.title.clone(),
                        staged.to_string_lossy().into_owned(),
                        0,
                        None,
                    )
                };
                if !PathBuf::from(&entry.path).starts_with(PathBuf::from(data_dir).join("books"))
                    || EpubBook::file_hash(&entry.path).ok().as_ref() != Some(&book.info.hash)
                {
                    library = previous;
                    library.save_to(data_dir);
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
                        device_id,
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
                device_id,
                "download",
                &book.info.hash,
            )?;
        }
        let _ = std::fs::remove_file(staged);
    }
    let mut skipped = prepared.skipped;
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
            && !EpubBook::open(path)
                .is_ok_and(|book| reader_core::cloud_progress::validate_position(&book, p))
        {
            skipped += 1;
            continue;
        }
        if path == open_path && changed {
            offer = Some(p.clone());
        }
        journal.accept(&record);
        if changed {
            library.update_position(
                data_dir,
                path,
                p.chapter,
                p.chapter_title.clone(),
                p.block,
                p.char_offset,
            );
        }
        journal.applied.insert(hash, p.timestamp);
        if changed {
            updated.push(serde_json::json!({"path":path,"chapter":p.chapter}));
        }
    }
    journal.save(data_dir)?;
    Ok(
        serde_json::json!({"uploaded":prepared.uploaded,"downloaded":imported,"skipped":skipped,"offer":offer,"updated":updated,"failed":prepared.failed,"next_retry_after":prepared.next_retry_after,"terminal_error":prepared.terminal_error}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    static TEST_LOCK: Mutex<()> = Mutex::new(());
    fn fixture() -> (tempfile::TempDir, String, String) {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().to_str().unwrap().to_owned();
        let txt = dir.path().join("source.txt");
        std::fs::write(
            &txt,
            "第一章 开始\n中文阅读。\n第二章 回读\n继续阅读。\n第三章 结束\n最后一章。",
        )
        .unwrap();
        let result =
            reader_core::txt::convert_txt_to_epub(&txt, dir.path(), &Default::default()).unwrap();
        let mut library = Library::load_from(&data_dir);
        let entry = library.add_or_update(
            &data_dir,
            "Book".into(),
            result.epub_path.to_string_lossy().into_owned(),
            1,
            None,
        );
        let hash = EpubBook::file_hash(&entry.path).unwrap();
        library.books[0].last_opened = 10;
        library.save_to(&data_dir);
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
        let mut own = DeviceSnapshot::new("ios-test".into());
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
        }
    }
    #[test]
    fn latest_progress_updates_open_ios_catalog_and_emits_ui_locator() {
        let _test = TEST_LOCK.lock().unwrap();
        let (dir, path, hash) = fixture();
        let data_dir = dir.path().to_str().unwrap();
        let result = apply(data_dir, "ios-test", prepared(&hash), &path).unwrap();
        assert_eq!(result["offer"]["chapter"], 0);
        assert_eq!(Library::load_from(data_dir).books[0].last_chapter, 0);
        assert!(SyncJournal::load(data_dir, "ios-test")
            .own
            .validate()
            .is_ok());
    }
    #[test]
    fn newer_ios_local_progress_wins_over_inflight_result() {
        let _test = TEST_LOCK.lock().unwrap();
        let (dir, _, hash) = fixture();
        let data_dir = dir.path().to_str().unwrap();
        let mut journal = SyncJournal::load(data_dir, "ios-test");
        journal.record(progress(&hash, 2, 40));
        journal.save(data_dir).unwrap();
        Library::load_from(data_dir).update_position(
            data_dir,
            &Library::load_from(data_dir).books[0].path,
            2,
            None,
            0,
            0,
        );
        let result = apply(data_dir, "ios-test", prepared(&hash), "").unwrap();
        assert!(result["offer"].is_null());
        assert_eq!(
            SyncJournal::load(data_dir, "ios-test").own.progress[&hash].chapter,
            2
        );
        assert_eq!(result["updated"].as_array().unwrap().len(), 0);
    }
    #[test]
    fn remote_reread_can_restore_an_earlier_chapter_for_closed_book() {
        let _test = TEST_LOCK.lock().unwrap();
        let (dir, _, hash) = fixture();
        let data_dir = dir.path().to_str().unwrap();
        apply(data_dir, "ios-test", prepared(&hash), "").unwrap();
        assert_eq!(Library::load_from(data_dir).books[0].last_chapter, 0);
        assert_eq!(SyncJournal::load(data_dir, "ios-test").applied[&hash], 30);
    }
    #[test]
    fn invalid_remote_coordinate_does_not_corrupt_persisted_position() {
        let _test = TEST_LOCK.lock().unwrap();
        let (dir, _, hash) = fixture();
        let mut result = prepared(&hash);
        result.snapshots[0].progress.get_mut(&hash).unwrap().chapter = 99999;
        let applied = apply(dir.path().to_str().unwrap(), "ios-test", result, "").unwrap();
        assert_eq!(applied["skipped"], 1);
        assert_eq!(
            Library::load_from(dir.path().to_str().unwrap()).books[0].last_chapter,
            1
        );
    }
    #[test]
    fn prepared_result_is_discarded_on_cancel_before_apply() {
        let _session = super::super::TEST_SESSION_LOCK.lock().unwrap();
        let _test = TEST_LOCK.lock().unwrap();
        let (dir, _, hash) = fixture();
        let root = dir.path().to_str().unwrap();
        let epoch = super::super::reader_ios_epoch();
        super::super::SESSION.lock().unwrap().pending = Some(super::super::Pending {
            epoch,
            root: root.into(),
            device: "ios-test".into(),
            result: prepared(&hash),
        });
        super::super::reader_ios_cancel_sync();
        let result = super::super::dispatch(
            serde_json::json!({"root":root,"device":"ios-test","op":"apply_sync","epoch":epoch}),
        );
        assert_eq!(result.unwrap_err(), "Sync cancelled");
        assert_eq!(Library::load_from(root).books[0].last_chapter, 1);
        assert!(super::super::SESSION.lock().unwrap().pending.is_none());
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
        apply(root, "ios-test", prepared(&hash), "").unwrap();
        let mut files = prepared(&hash);
        files.downloaded.push(StagedBook {
            info: CloudBook {
                hash: "b".repeat(64),
                title: "Missing staged file".into(),
                size: 10,
            },
        });
        let result = apply(root, "ios-test", files, "").unwrap();
        assert_eq!(result["failed"].as_array().unwrap().len(), 1);
        assert_eq!(result["downloaded"], 0);
        assert_eq!(Library::load_from(root).books[0].last_chapter, 0);
        assert_eq!(SyncJournal::load(root, "ios-test").applied[&hash], 30);
    }
}
