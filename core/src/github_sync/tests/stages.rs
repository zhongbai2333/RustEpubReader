use super::*;

pub(super) fn listed(snapshot: &DeviceSnapshot) -> Vec<Step> {
    vec![
        step(
            "GET",
            "/repos/tester/EpubReaderInfo/contents/devices",
            200,
            json!([{"name":format!("{}.json",snapshot.device_id),"type":"file"}]),
        ),
        raw(
            &format!(
                "/repos/tester/EpubReaderInfo/contents/devices/{}.json",
                snapshot.device_id
            ),
            serde_json::to_vec(snapshot).unwrap(),
        ),
    ]
}
pub(super) fn empty_devices() -> Step {
    step(
        "GET",
        "/repos/tester/EpubReaderInfo/contents/devices",
        200,
        json!([]),
    )
}
fn local(path: &std::path::Path, bytes: &[u8], title: &str) -> LocalBook {
    std::fs::write(path, bytes).unwrap();
    LocalBook {
        info: CloudBook {
            hash: digest(bytes),
            title: title.into(),
            size: bytes.len() as u64,
        },
        path: path.into(),
    }
}

#[test]
fn progress_finishes_without_accessing_any_book_file() {
    let mut own = DeviceSnapshot::new("local".into());
    let hash = "a".repeat(64);
    own.progress.insert(
        hash.clone(),
        crate::sharing::ProgressEntry {
            book_hash: hash.clone(),
            title: "Book".into(),
            chapter: 2,
            block: 0,
            char_offset: 0,
            chapter_title: None,
            timestamp: 20,
        },
    );
    let mut remote = DeviceSnapshot::new("remote".into());
    let mut p = own.progress[&hash].clone();
    p.chapter = 1;
    p.timestamp = 30;
    remote.progress.insert(hash.clone(), p);
    remote.books.insert(
        hash.clone(),
        CloudBook {
            hash: hash.clone(),
            title: "Book".into(),
            size: 1024,
        },
    );
    let path = "/repos/tester/EpubReaderInfo/contents/devices/local.json";
    let mut steps = prefix(true, "repo");
    steps.extend([
        step("GET", path, 404, json!({})),
        step("GET", path, 404, json!({})),
        step("PUT", path, 201, json!({})),
    ]);
    steps.extend(listed(&remote));
    let (api, server) = mock(steps);
    let result = staged::progress(&api, &mut own).unwrap();
    assert!(result.downloaded.is_empty());
    assert_eq!(
        crate::cloud_sync::latest_progress(&result.snapshots, &result.own)[&hash].chapter,
        1
    );
    assert!(
        result.own.books.is_empty(),
        "An untransferred book must not be advertised"
    );
    server.join().unwrap();
}

#[test]
fn failed_upload_does_not_block_success_and_only_failed_book_is_retried() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let bad = local(&dir.path().join("bad.epub"), b"bad upload", "Failed");
    let good = local(&dir.path().join("good.epub"), b"good upload", "Successful");
    let bad_path = format!(
        "/repos/tester/EpubReaderInfo/contents/books/{}.epub",
        bad.info.hash
    );
    let good_path = format!(
        "/repos/tester/EpubReaderInfo/contents/books/{}.epub",
        good.info.hash
    );
    let own_path = "/repos/tester/EpubReaderInfo/contents/devices/local.json";
    let mut steps = prefix(true, "repo");
    steps.push(empty_devices());
    steps.extend([
        step("GET", &bad_path, 404, json!({})),
        step("PUT", &bad_path, 500, json!({})),
        step("GET", &good_path, 404, json!({})),
        step("PUT", &good_path, 201, json!({})),
        step("GET", own_path, 404, json!({})),
        step("GET", own_path, 404, json!({})),
        step("PUT", own_path, 201, json!({})),
    ]);
    let (api, server) = mock(steps);
    let mut own = DeviceSnapshot::new("local".into());
    let mut queue = staged::RetryQueue::load(root, "tester", "local").unwrap();
    let result = staged::files(&api, &mut own, vec![bad, good], &mut queue, false).unwrap();
    assert_eq!(result.uploaded, 1);
    assert_eq!(result.failed.len(), 1);
    assert!(!own.books.contains_key(&result.failed[0].hash));
    assert_eq!(own.books.len(), 1);
    server.join().unwrap();
    // The persisted queue survives a restart. No book endpoints are requested during backoff.
    let mut steps = prefix(true, "repo");
    steps.extend(listed(&own));
    let (api, server) = mock(steps);
    let mut queue = staged::RetryQueue::load(root, "tester", "local").unwrap();
    let books = vec![
        local(&dir.path().join("bad.epub"), b"bad upload", "Failed"),
        local(&dir.path().join("good.epub"), b"good upload", "Successful"),
    ];
    let result = staged::files(&api, &mut own, books, &mut queue, false).unwrap();
    assert_eq!(result.uploaded, 0);
    assert_eq!(result.failed[0].attempts, 1);
    server.join().unwrap();
    // Manual retry bypasses this book's backoff, never reuploads the successful book.
    let mut steps = prefix(true, "repo");
    steps.extend(listed(&own));
    steps.extend([
        step("GET", &bad_path, 404, json!({})),
        step("PUT", &bad_path, 201, json!({})),
        step("GET", own_path, 200, json!({"sha":"old"})),
        raw(own_path, serde_json::to_vec(&own).unwrap()),
        step("PUT", own_path, 200, json!({})),
    ]);
    let (api, server) = mock(steps);
    let books = vec![
        local(&dir.path().join("bad.epub"), b"bad upload", "Failed"),
        local(&dir.path().join("good.epub"), b"good upload", "Successful"),
    ];
    let result = staged::files(&api, &mut own, books, &mut queue, true).unwrap();
    assert_eq!(result.uploaded, 1);
    assert!(result.failed.is_empty());
    assert_eq!(own.books.len(), 2);
    server.join().unwrap();
    let isolated = staged::RetryQueue::load(root, "another-account", "local").unwrap();
    assert_eq!(
        serde_json::to_value(&isolated).unwrap()["failures"],
        json!({})
    );
    // Different account/device queues never share transfer decisions.
    let (api, server) = mock(vec![]);
    api.cancel.store(true, Ordering::SeqCst);
    assert!(staged::files(&api, &mut own, vec![], &mut { isolated }, false).is_err());
    server.join().unwrap();
}

#[test]
fn progress_conflict_merges_newer_device_record_before_retry() {
    let mut own = DeviceSnapshot::new("local".into());
    let hash = "a".repeat(64);
    own.progress.insert(
        hash.clone(),
        crate::sharing::ProgressEntry {
            book_hash: hash.clone(),
            title: "Book".into(),
            chapter: 2,
            block: 0,
            char_offset: 0,
            chapter_title: None,
            timestamp: 20,
        },
    );
    let mut remote = own.clone();
    remote.progress.get_mut(&hash).unwrap().timestamp = 30;
    remote.progress.get_mut(&hash).unwrap().chapter = 1;
    let path = "/repos/tester/EpubReaderInfo/contents/devices/local.json";
    let mut steps = prefix(true, "repo");
    // First CAS races; the second read contains a newer own-device record.
    steps.extend([
        step("GET", path, 200, json!({"sha":"old"})),
        raw(
            path,
            serde_json::to_vec(&DeviceSnapshot::new("local".into())).unwrap(),
        ),
        step("PUT", path, 409, json!({})),
        step("GET", path, 200, json!({"sha":"new"})),
        raw(path, serde_json::to_vec(&remote).unwrap()),
        empty_devices(),
    ]);
    let (api, server) = mock(steps);
    let result = staged::progress(&api, &mut own).unwrap();
    assert_eq!(result.own.progress[&hash].timestamp, 30);
    assert_eq!(result.own.progress[&hash].chapter, 1);
    server.join().unwrap();
}

#[test]
fn failed_validation_keeps_retry_attempts_until_library_save_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let info = CloudBook {
        hash: "a".repeat(64),
        title: "Broken EPUB".into(),
        size: 20,
    };
    let first = record_transfer_failure(
        root,
        "tester",
        "local",
        "download",
        &info,
        "Invalid EPUB".into(),
    )
    .unwrap();
    let second = record_transfer_failure(
        root,
        "tester",
        "local",
        "download",
        &info,
        "Cannot save EPUB".into(),
    )
    .unwrap();
    assert_eq!(first[0].attempts, 1);
    assert_eq!(second[0].attempts, 2);
    assert!(second[0].retry_at >= first[0].retry_at + 29);
    assert!(
        complete_transfer(root, "tester", "local", "download", &info.hash)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        serde_json::to_value(staged::RetryQueue::load(root, "tester", "local").unwrap()).unwrap()
            ["failures"],
        json!({})
    );
}

#[test]
fn even_failed_downloads_are_bounded_to_twenty_attempts_per_batch() {
    let dir = tempfile::tempdir().unwrap();
    let mut own = DeviceSnapshot::new("local".into());
    let mut remote = DeviceSnapshot::new("remote".into());
    for n in 0..21 {
        let hash = format!("{n:064x}");
        remote.books.insert(
            hash.clone(),
            CloudBook {
                hash,
                title: "Book".into(),
                size: 4,
            },
        );
    }
    let mut steps = prefix(true, "repo");
    steps.extend(listed(&remote));
    for n in 0..20 {
        steps.push(raw(
            &format!("/repos/tester/EpubReaderInfo/contents/books/{n:064x}.epub"),
            b"evil".to_vec(),
        ));
    }
    let (api, server) = mock(steps);
    let mut queue =
        staged::RetryQueue::load(dir.path().to_str().unwrap(), "tester", "local").unwrap();
    let result = staged::files(&api, &mut own, vec![], &mut queue, false).unwrap();
    assert_eq!(result.failed.len(), 20);
    assert_eq!(result.next_retry_after, Some(15));
    server.join().unwrap();
}

#[test]
fn corrupt_download_is_queued_while_other_books_finish() {
    let dir = tempfile::tempdir().unwrap();
    let mut own = DeviceSnapshot::new("local".into());
    let mut remote = DeviceSnapshot::new("remote".into());
    let bad = "0".repeat(64);
    let good = digest(b"verified");
    remote.books.insert(
        bad.clone(),
        CloudBook {
            hash: bad.clone(),
            title: "Corrupt".into(),
            size: 4,
        },
    );
    remote.books.insert(
        good.clone(),
        CloudBook {
            hash: good.clone(),
            title: "Good".into(),
            size: 8,
        },
    );
    let mut steps = prefix(true, "repo");
    steps.extend(listed(&remote));
    steps.extend([
        raw(
            &format!("/repos/tester/EpubReaderInfo/contents/books/{bad}.epub"),
            b"evil".to_vec(),
        ),
        raw(
            &format!("/repos/tester/EpubReaderInfo/contents/books/{good}.epub"),
            b"verified".to_vec(),
        ),
    ]);
    let (api, server) = mock(steps);
    let mut queue =
        staged::RetryQueue::load(dir.path().to_str().unwrap(), "tester", "local").unwrap();
    let result = staged::files(&api, &mut own, vec![], &mut queue, false).unwrap();
    assert_eq!(result.downloaded.len(), 1);
    assert_eq!(result.downloaded[0].info.hash, good);
    assert_eq!(result.failed[0].hash, bad);
    assert_eq!(result.failed[0].direction, "download");
    server.join().unwrap();
}

#[test]
fn rate_limit_stops_other_file_requests_and_preserves_progress() {
    let dir = tempfile::tempdir().unwrap();
    let mut own = DeviceSnapshot::new("local".into());
    let mut remote = DeviceSnapshot::new("remote".into());
    for ch in ['0', '1'] {
        let hash = ch.to_string().repeat(64);
        remote.books.insert(
            hash.clone(),
            CloudBook {
                hash,
                title: "Book".into(),
                size: 4,
            },
        );
    }
    let mut steps = prefix(true, "repo");
    steps.extend(listed(&remote));
    steps.push(step(
        "GET",
        &format!(
            "/repos/tester/EpubReaderInfo/contents/books/{}.epub",
            "0".repeat(64)
        ),
        429,
        json!({}),
    ));
    let (api, server) = mock(steps);
    let mut queue =
        staged::RetryQueue::load(dir.path().to_str().unwrap(), "tester", "local").unwrap();
    let result = staged::files(&api, &mut own, vec![], &mut queue, false).unwrap();
    assert!(result.terminal_error.unwrap().starts_with("RETRY_AFTER:"));
    assert_eq!(result.failed.len(), 1);
    server.join().unwrap();
}

#[test]
fn previously_downloaded_but_missing_local_file_is_downloaded_again() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let bytes = b"previously downloaded book";
    let hash = digest(bytes);
    let mut own = DeviceSnapshot::new("local".into());
    let book = CloudBook {
        hash: hash.clone(),
        title: "Missing local copy".into(),
        size: bytes.len() as u64,
    };
    own.books.insert(hash.clone(), book);
    let mut steps = prefix(true, "repo");
    steps.extend(listed(&own));
    steps.push(raw(
        &format!("/repos/tester/EpubReaderInfo/contents/books/{hash}.epub"),
        bytes.to_vec(),
    ));
    let (api, server) = mock(steps);
    let mut retries = staged::RetryQueue::load(root, "tester", "local").unwrap();
    let result = staged::files(&api, &mut own, vec![], &mut retries, false).unwrap();
    assert_eq!(
        result.downloaded.len(),
        1,
        "own snapshot is not proof that the file still exists locally"
    );
    server.join().unwrap();
}
#[test]
fn archived_remote_book_is_neither_downloaded_nor_removed_from_cloud() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let hash = "a".repeat(64);
    let book = CloudBook {
        hash: hash.clone(),
        title: "Archived".into(),
        size: 12,
    };
    let mut remote = DeviceSnapshot::new("remote".into());
    remote.books.insert(hash.clone(), book.clone());
    remote.lifecycle.insert(
        hash.clone(),
        crate::library::archive::BookLifecycle {
            book,
            state: crate::library::archive::BookState::Archived,
            revision: 20,
            event: uuid::Uuid::new_v4().to_string(),
        },
    );
    let mut own = DeviceSnapshot::new("local".into());
    let mut expected = own.clone();
    expected.lifecycle = remote.lifecycle.clone();
    let path = "/repos/tester/EpubReaderInfo/contents/devices/local.json";
    let mut steps = prefix(true, "repo");
    steps.extend(listed(&remote));
    steps.extend([
        step("GET", path, 404, json!({})),
        step("GET", path, 404, json!({})),
        step("PUT", path, 201, json!({})),
    ]);
    let (api, server) = mock(steps);
    let mut retries = staged::RetryQueue::load(root, "tester", "local").unwrap();
    let result = staged::files(&api, &mut own, vec![], &mut retries, false).unwrap();
    assert!(result.downloaded.is_empty());
    assert!(result.failed.is_empty());
    assert_eq!(result.own.lifecycle, expected.lifecycle);
    server.join().unwrap();
}
#[test]
fn permanent_delete_publishes_tombstone_then_deletes_only_hash_named_book() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let hash = "b".repeat(64);
    let book = CloudBook {
        hash: hash.clone(),
        title: "Deleted".into(),
        size: 12,
    };
    let mut remote = DeviceSnapshot::new("remote".into());
    remote.books.insert(hash.clone(), book.clone());
    remote.lifecycle.insert(
        hash.clone(),
        crate::library::archive::BookLifecycle {
            book,
            state: crate::library::archive::BookState::Deleted,
            revision: 30,
            event: uuid::Uuid::new_v4().to_string(),
        },
    );
    let mut own = DeviceSnapshot::new("local".into());
    let path = "/repos/tester/EpubReaderInfo/contents/devices/local.json";
    let epub = format!("/repos/tester/EpubReaderInfo/contents/books/{hash}.epub");
    let mut published = own.clone();
    published.lifecycle = remote.lifecycle.clone();
    let mut steps = prefix(true, "repo");
    steps.extend(listed(&remote));
    steps.extend([
        step("GET", path, 404, json!({})),
        step("GET", path, 404, json!({})),
        step("PUT", path, 201, json!({})),
        step("GET", &epub, 200, json!({"sha":"book-blob"})),
        step("DELETE", &epub, 200, json!({})),
        step("GET", path, 200, json!({"sha":"state"})),
        raw(path, serde_json::to_vec_pretty(&published).unwrap()),
    ]);
    let (api, server) = mock(steps);
    let mut retries = staged::RetryQueue::load(root, "tester", "local").unwrap();
    let result = staged::files(&api, &mut own, vec![], &mut retries, false).unwrap();
    assert!(result.downloaded.is_empty());
    assert!(result.failed.is_empty());
    assert_eq!(
        serde_json::to_value(&retries).unwrap()["deleted"][&hash],
        30
    );
    server.join().unwrap();
}

#[test]
fn archived_copy_requires_actual_cloud_bytes_before_local_release() {
    for corrupt in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let bytes = b"archive safety fixture";
        let book = local(&dir.path().join("local.epub"), bytes, "Archive");
        let path_on_disk = book.path.clone();
        let hash = book.info.hash.clone();
        let mut own = DeviceSnapshot::new("local".into());
        own.books.insert(hash.clone(), book.info.clone());
        own.lifecycle.insert(
            hash.clone(),
            crate::library::archive::BookLifecycle {
                book: book.info.clone(),
                state: crate::library::archive::BookState::Archived,
                revision: 20,
                event: uuid::Uuid::new_v4().to_string(),
            },
        );
        let state = "/repos/tester/EpubReaderInfo/contents/devices/local.json";
        let epub = format!("/repos/tester/EpubReaderInfo/contents/books/{hash}.epub");
        let mut steps = prefix(true, "repo");
        steps.extend(listed(&own));
        steps.extend([
            step("GET", &epub, 200, json!({"size":bytes.len(),"sha":"book"})),
            step("GET", state, 200, json!({"sha":"state"})),
            raw(state, serde_json::to_vec_pretty(&own).unwrap()),
            raw(
                &epub,
                if corrupt {
                    b"corrupted cloud bytes".to_vec()
                } else {
                    bytes.to_vec()
                },
            ),
        ]);
        let (api, server) = mock(steps);
        let mut retries = staged::RetryQueue::load(root, "tester", "local").unwrap();
        let result = staged::files(&api, &mut own, vec![book], &mut retries, false).unwrap();
        assert_eq!(result.confirmed_archives.contains(&hash), !corrupt);
        assert_eq!(result.failed.len(), usize::from(corrupt));
        assert!(
            path_on_disk.is_file(),
            "transport must never unlink the local file"
        );
        server.join().unwrap();
    }
}

#[test]
fn bulk_failure_delivers_staged_metadata_without_claiming_transfer_success_or_losing_progress() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let queue = staged::RetryQueue::load(root, "tester", "local").unwrap();
    let mut own = DeviceSnapshot::new("local".into());
    let hash = "a".repeat(64);
    own.progress.insert(
        hash.clone(),
        crate::sharing::ProgressEntry {
            book_hash: hash.clone(),
            title: "Book".into(),
            chapter: 9,
            block: 3,
            char_offset: 2,
            chapter_title: None,
            timestamp: 50,
        },
    );
    let result =
        staged::finish_file_phase(&own, &queue, None, Err("Cannot reach GitHub".into())).unwrap();
    assert_eq!(result.own.progress[&hash].chapter, 9);
    assert!(result.downloaded.is_empty());
    assert_eq!(result.uploaded, 0);
    assert_eq!(
        result.terminal_error.as_deref(),
        Some("Cannot reach GitHub")
    );
    assert_eq!(result.next_retry_after, Some(30));
    assert!(staged::finish_file_phase(&own, &queue, None, Err("Sync cancelled".into())).is_err());
}
