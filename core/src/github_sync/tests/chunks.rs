use super::super::chunks as wire;
use super::*;
const C: u64 = wire::CHUNK_BYTES;
fn payload() -> Vec<u8> {
    let mut bytes = vec![b'x'; (21 * 1024 * 1024) as usize];
    bytes[0] = b'a';
    *bytes.last_mut().unwrap() = b'z';
    bytes
}
fn info(bytes: &[u8]) -> CloudBook {
    CloudBook {
        hash: digest(bytes),
        title: "Large book".into(),
        size: bytes.len() as u64,
    }
}
fn manifest(book: &CloudBook, bytes: &[u8]) -> Value {
    json!({"version":2,"book":book,"release":7,"chunk_bytes":C,"parts":bytes.chunks(C as usize).enumerate().map(|(i,b)|json!({"index":i,"size":b.len(),"hash":digest(b),"asset":i+1})).collect::<Vec<_>>()})
}
fn manifest_step(book: &CloudBook, value: Value) -> Step {
    raw(
        &format!(
            "/repos/tester/EpubReaderInfo/contents/books-v2/{}.json",
            book.hash
        ),
        serde_json::to_vec(&value).unwrap(),
    )
}
fn absent_manifest(book: &CloudBook) -> Step {
    step(
        "GET",
        &format!(
            "/repos/tester/EpubReaderInfo/contents/books-v2/{}.json",
            book.hash
        ),
        404,
        json!({}),
    )
}
fn release_step(book: &CloudBook) -> Step {
    step(
        "GET",
        "/repos/tester/EpubReaderInfo/releases?per_page=100&page=1",
        200,
        json!([{"id":7,"tag_name":format!("reader-file-v2-{}",book.hash),"draft":true}]),
    )
}
fn asset(index: usize, bytes: &[u8]) -> Value {
    json!({"id":index+1,"name":format!("{index:04}-{}.bin",digest(bytes)),"size":bytes.len(),"state":"uploaded","digest":format!("sha256:{}",digest(bytes))})
}
fn list_assets(values: Vec<Value>) -> Step {
    step(
        "GET",
        "/repos/tester/EpubReaderInfo/releases/7/assets?per_page=100&page=1",
        200,
        json!(values),
    )
}
fn upload_step(index: usize, bytes: &[u8]) -> Step {
    let name = format!("{index:04}-{}.bin", digest(bytes));
    let mut result = step(
        "POST",
        &format!("/repos/tester/EpubReaderInfo/releases/7/assets?name={name}"),
        201,
        asset(index, bytes),
    );
    result.request_hash = Some(digest(bytes));
    result
}
fn read_chunk(index: usize, bytes: &[u8]) -> Step {
    raw(
        &format!("/repos/tester/EpubReaderInfo/releases/assets/{}", index + 1),
        bytes.to_vec(),
    )
}
#[test]
fn large_manifest_rejects_bad_indices_lengths_duplicate_assets_and_hashes() {
    let bytes = payload();
    let book = info(&bytes);
    let valid = manifest(&book, &bytes);
    let parsed: wire::Manifest = serde_json::from_value(valid.clone()).unwrap();
    parsed.validate(&book).unwrap();
    for (path, value) in [
        ("/version", json!(1)),
        ("/chunk_bytes", json!(C * 2)),
        ("/release", json!(0)),
        ("/parts/1/index", json!(0)),
        ("/parts/1/asset", json!(1)),
        ("/parts/1/size", json!(C - 1)),
        ("/parts/1/hash", json!("../secret")),
        ("/book/size", json!(MAX_DOCUMENT_BYTES + 1)),
    ] {
        let mut changed = valid.clone();
        *changed.pointer_mut(path).unwrap() = value;
        let parsed: wire::Manifest = serde_json::from_value(changed).unwrap();
        assert!(parsed.validate(&book).is_err(), "{path}");
    }
}
#[test]
fn interrupted_download_resumes_verified_chunks_and_retries_only_corrupt_part() {
    let bytes = payload();
    let book = info(&bytes);
    let parts: Vec<_> = bytes.chunks(C as usize).collect();
    let value = manifest(&book, &bytes);
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let (api, server) = mock(vec![
        manifest_step(&book, value.clone()),
        read_chunk(0, parts[0]),
    ]);
    assert!(wire::download(&api, &book, root, &mut { C })
        .unwrap()
        .is_none());
    server.join().unwrap();
    let mut corrupt = parts[2].to_vec();
    corrupt[0] ^= 1;
    let (api, server) = mock(vec![
        manifest_step(&book, value.clone()),
        read_chunk(1, parts[1]),
        read_chunk(2, &corrupt),
    ]);
    assert!(wire::download(&api, &book, root, &mut (100 * 1024 * 1024))
        .unwrap_err()
        .contains("integrity"));
    server.join().unwrap();
    let (api, server) = mock(vec![manifest_step(&book, value), read_chunk(2, parts[2])]);
    let path = wire::download(&api, &book, root, &mut (100 * 1024 * 1024))
        .unwrap()
        .unwrap();
    server.join().unwrap();
    assert_eq!(
        crate::file_hash(&path.to_string_lossy()).unwrap(),
        book.hash
    );
    assert_eq!(std::fs::metadata(path).unwrap().len(), book.size);
    let (api, server) = mock(vec![]);
    let mut cancelled = api;
    cancelled.cancel = Arc::new(AtomicBool::new(true));
    assert!(wire::download(&cancelled, &book, root, &mut { C })
        .unwrap_err()
        .contains("cancelled"));
    server.join().unwrap();
}
#[test]
fn partial_upload_reuses_remote_chunks_then_publishes_manifest_last() {
    let bytes = payload();
    let book = info(&bytes);
    let parts: Vec<_> = bytes.chunks(C as usize).collect();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large.epub");
    std::fs::write(&path, &bytes).unwrap();
    let local = LocalBook {
        info: book.clone(),
        path,
    };
    let (api, server) = mock(vec![
        absent_manifest(&book),
        step(
            "GET",
            "/repos/tester/EpubReaderInfo/releases?per_page=100&page=1",
            200,
            json!([]),
        ),
        step(
            "POST",
            "/repos/tester/EpubReaderInfo/releases",
            201,
            json!({"id":7,"tag_name":format!("reader-file-v2-{}",book.hash),"draft":true}),
        ),
        list_assets(vec![]),
        upload_step(0, parts[0]),
    ]);
    assert_eq!(wire::upload(&api, &local, &mut { C }).unwrap(), None);
    server.join().unwrap();
    let mut damaged = asset(1, parts[1]);
    damaged["digest"] = json!(format!("sha256:{}", "a".repeat(64)));
    let mut starter = asset(2, parts[2]);
    starter["state"] = json!("starter");
    let (api, server) = mock(vec![
        absent_manifest(&book),
        release_step(&book),
        list_assets(vec![asset(0, parts[0]), damaged, starter]),
        step(
            "DELETE",
            "/repos/tester/EpubReaderInfo/releases/assets/2",
            204,
            json!(null),
        ),
        upload_step(1, parts[1]),
        step(
            "DELETE",
            "/repos/tester/EpubReaderInfo/releases/assets/3",
            204,
            json!(null),
        ),
        upload_step(2, parts[2]),
        step(
            "PUT",
            &format!(
                "/repos/tester/EpubReaderInfo/contents/books-v2/{}.json",
                book.hash
            ),
            201,
            json!({}),
        ),
    ]);
    assert_eq!(
        wire::upload(&api, &local, &mut (100 * 1024 * 1024)).unwrap(),
        Some(true)
    );
    server.join().unwrap();
}
#[test]
fn bad_whole_file_hash_never_promotes_a_partial_download() {
    let bytes = payload();
    let mut book = info(&bytes);
    book.hash = "a".repeat(64);
    let value = manifest(&book, &bytes);
    let mut steps = vec![manifest_step(&book, value)];
    steps.extend(
        bytes
            .chunks(C as usize)
            .enumerate()
            .map(|(i, b)| read_chunk(i, b)),
    );
    let (api, server) = mock(steps);
    let dir = tempfile::tempdir().unwrap();
    assert!(wire::download(
        &api,
        &book,
        dir.path().to_str().unwrap(),
        &mut (100 * 1024 * 1024)
    )
    .unwrap_err()
    .contains("integrity"));
    server.join().unwrap();
    assert!(!dir
        .path()
        .join("github-sync-chunks")
        .join(digest(b"tester"))
        .join(book.hash)
        .join("complete.epub")
        .exists());
}
#[test]
fn untrusted_asset_redirect_is_rejected_without_forwarding_credentials() {
    let bytes = payload();
    let book = info(&bytes);
    let mut redirect = step(
        "GET",
        "/repos/tester/EpubReaderInfo/releases/assets/1",
        302,
        json!({}),
    );
    redirect.location = Some("https://attacker.invalid/steal".into());
    let (api, server) = mock(vec![
        manifest_step(&book, manifest(&book, &bytes)),
        redirect,
    ]);
    let dir = tempfile::tempdir().unwrap();
    assert!(
        wire::download(&api, &book, dir.path().to_str().unwrap(), &mut { C })
            .unwrap_err()
            .contains("Untrusted")
    );
    server.join().unwrap();
}
#[test]
fn staged_large_download_is_disk_backed_and_does_not_enter_legacy_book_map() {
    let bytes = payload();
    let book = info(&bytes);
    let mut remote = DeviceSnapshot::new("remote".into());
    remote.large_books.insert(book.hash.clone(), book.clone());
    let mut steps = prefix(true, "repo");
    steps.extend(super::stages::listed(&remote));
    steps.push(manifest_step(&book, manifest(&book, &bytes)));
    steps.extend(
        bytes
            .chunks(C as usize)
            .enumerate()
            .map(|(i, b)| read_chunk(i, b)),
    );
    let (api, server) = mock(steps);
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let mut retries = staged::RetryQueue::load(root, "tester", "local").unwrap();
    let result = staged::files(
        &api,
        &mut DeviceSnapshot::new("local".into()),
        vec![],
        &mut retries,
        false,
    )
    .unwrap();
    server.join().unwrap();
    assert_eq!(result.downloaded.len(), 1);
    assert!(result.downloaded[0].bytes.is_empty());
    assert!(result.downloaded[0].path.as_ref().unwrap().is_file());
    assert!(result.own.books.is_empty());
    let saved = dir.path().join("platform-stage.epub");
    result.downloaded[0].save_to(&saved).unwrap();
    assert_eq!(
        crate::file_hash(&saved.to_string_lossy()).unwrap(),
        book.hash
    );
    super::super::complete_transfer(root, "tester", "local", "download", &book.hash).unwrap();
    assert!(saved.is_file());
    assert!(!result.downloaded[0].path.as_ref().unwrap().exists());
}
#[test]
fn deleting_chunked_book_removes_owned_release_and_manifest() {
    let hash = "b".repeat(64);
    let book = CloudBook {
        hash: hash.clone(),
        title: "Large".into(),
        size: 21 * 1024 * 1024,
    };
    let (api, server) = mock(vec![
        release_step(&book),
        step(
            "DELETE",
            "/repos/tester/EpubReaderInfo/releases/7",
            204,
            json!({}),
        ),
        step(
            "GET",
            &format!("/repos/tester/EpubReaderInfo/contents/books-v2/{hash}.json"),
            200,
            json!({"sha":"manifest-revision"}),
        ),
        step(
            "DELETE",
            &format!("/repos/tester/EpubReaderInfo/contents/books-v2/{hash}.json"),
            200,
            json!({}),
        ),
    ]);
    wire::delete(&api, &hash).unwrap();
    server.join().unwrap();
}
#[test]
fn release_mismatch_cannot_delete_an_unrelated_release() {
    let hash = "b".repeat(64);
    let (api, server) = mock(vec![
        step(
            "GET",
            "/repos/tester/EpubReaderInfo/releases?per_page=100&page=1",
            200,
            json!([{"id":7,"tag_name":"unrelated","draft":true}]),
        ),
        step(
            "GET",
            &format!("/repos/tester/EpubReaderInfo/contents/books-v2/{hash}.json"),
            404,
            json!({}),
        ),
    ]);
    wire::delete(&api, &hash).unwrap();
    server.join().unwrap();
}

#[test]
fn file_larger_than_round_budget_advances_instead_of_being_permanently_skipped() {
    // An opaque 129 MiB file, generated a block at a time rather than kept whole.
    let block = vec![0u8; C as usize];
    let tail = vec![0u8; 1024 * 1024];
    let mut hash = Sha256::new();
    for _ in 0..16 {
        hash.update(&block);
    }
    hash.update(&tail);
    let book = CloudBook {
        hash: format!("{:x}", hash.finalize()),
        size: 129 * 1024 * 1024,
        title: "Opaque large document".into(),
    };
    let parts: Vec<_> = (0..17)
        .map(|i| {
            let data = if i < 16 { &block } else { &tail };
            json!({"index":i,"size":data.len(),"hash":digest(data),"asset":i+1})
        })
        .collect();
    let value = json!({"version":2,"book":book,"release":7,"chunk_bytes":C,"parts":parts});
    let mut remote = DeviceSnapshot::new("remote".into());
    remote.large_books.insert(book.hash.clone(), book.clone());
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let mut retries = staged::RetryQueue::load(root, "tester", "local").unwrap();
    for round in 0..2 {
        let mut steps = prefix(true, "repo");
        steps.extend(super::stages::listed(&remote));
        steps.push(manifest_step(&book, value.clone()));
        for i in if round == 0 { 0..12 } else { 12..17 } {
            steps.push(read_chunk(i, if i < 16 { &block } else { &tail }));
        }
        // The second round rehashes 96 MiB of cached chunks before requesting
        // the next part. Allow CPU/disk work in debug CI builds between requests;
        // this is a mock-server idle limit, not a production network timeout.
        let (api, server) = mock_with_request_wait(steps, false, Duration::from_secs(120));
        let result = staged::files(
            &api,
            &mut DeviceSnapshot::new("local".into()),
            vec![],
            &mut retries,
            false,
        )
        .unwrap();
        server.join().unwrap();
        assert!(result.failed.is_empty());
        if round == 0 {
            assert!(result.downloaded.is_empty());
            assert_eq!(result.next_retry_after, Some(15));
        } else {
            assert_eq!(result.downloaded.len(), 1);
            assert_eq!(
                crate::file_hash(
                    &result.downloaded[0]
                        .path
                        .as_ref()
                        .unwrap()
                        .to_string_lossy()
                )
                .unwrap(),
                book.hash
            );
        }
    }
}

#[test]
fn failed_large_upload_does_not_block_another_book_or_publish_partial_metadata() {
    let bytes = payload();
    let book = info(&bytes);
    let parts: Vec<_> = bytes.chunks(C as usize).collect();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let large = dir.path().join("large.epub");
    std::fs::write(&large, &bytes).unwrap();
    let small_bytes = b"another document";
    let small = info(small_bytes);
    let small_path = dir.path().join("small.epub");
    std::fs::write(&small_path, small_bytes).unwrap();
    let mut failed = upload_step(1, parts[1]);
    failed.status = 503;
    let mut steps = prefix(true, "repo");
    steps.push(super::stages::empty_devices());
    steps.extend([
        absent_manifest(&book),
        release_step(&book),
        list_assets(vec![]),
        upload_step(0, parts[0]),
        failed,
        step(
            "GET",
            &format!(
                "/repos/tester/EpubReaderInfo/contents/books/{}.epub",
                small.hash
            ),
            404,
            json!({}),
        ),
        step(
            "PUT",
            &format!(
                "/repos/tester/EpubReaderInfo/contents/books/{}.epub",
                small.hash
            ),
            201,
            json!({}),
        ),
        step(
            "GET",
            "/repos/tester/EpubReaderInfo/contents/devices/local.json",
            404,
            json!({}),
        ),
        step(
            "GET",
            "/repos/tester/EpubReaderInfo/contents/devices/local.json",
            404,
            json!({}),
        ),
        step(
            "PUT",
            "/repos/tester/EpubReaderInfo/contents/devices/local.json",
            201,
            json!({}),
        ),
    ]);
    let (api, server) = mock(steps);
    let mut retries = staged::RetryQueue::load(root, "tester", "local").unwrap();
    let result = staged::files(
        &api,
        &mut DeviceSnapshot::new("local".into()),
        vec![
            LocalBook {
                info: book.clone(),
                path: large,
            },
            LocalBook {
                info: small.clone(),
                path: small_path,
            },
        ],
        &mut retries,
        false,
    )
    .unwrap();
    server.join().unwrap();
    assert!(result.own.large_books.is_empty());
    assert!(result.own.books.contains_key(&small.hash));
    assert_eq!(result.failed.len(), 1);
    assert_eq!(result.failed[0].hash, book.hash);
}
