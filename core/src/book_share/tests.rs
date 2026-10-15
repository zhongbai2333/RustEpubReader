use super::package::*;
use super::*;
use rand::{Rng, SeedableRng};
use std::{
    fs,
    net::{TcpListener, TcpStream},
    thread,
};
fn fixture(root: &str, title: &str, large: bool) -> String {
    let text = Path::new(root).join(format!("{}.txt", uuid::Uuid::new_v4()));
    let body = if large {
        let mut rng = rand::rngs::StdRng::seed_from_u64(17);
        (0..220000)
            .map(|_| char::from(rng.gen_range(b'a'..=b'z')))
            .collect::<String>()
    } else {
        format!("第一章 开始\n{title} 中文书籍测试。\n第二章 继续\n测试内容。")
    };
    fs::write(&text, body).unwrap();
    let epub = crate::txt::convert_txt_to_epub(&text, Path::new(root), &Default::default())
        .unwrap()
        .epub_path;
    let entry = crate::library::Library::load_from(root).add_or_update(
        root,
        title.into(),
        epub.to_string_lossy().into_owned(),
        0,
        None,
    );
    entry.id
}
fn ticket(key: [u8; 32], address: String) -> Ticket {
    Ticket {
        location: Location::Lan {
            address,
            session: uuid::Uuid::new_v4().to_string(),
            pin: "2718".into(),
            expires: crate::now_secs() + 600,
        },
        key: URL_SAFE_NO_PAD.encode(key),
    }
}
#[test]
fn qr_roundtrip_and_ticket_rejects_untrusted_addresses_and_expiry() {
    let dir = tempfile::tempdir().unwrap();
    let t = ticket([7; 32], "127.0.0.1:12345".into());
    let link = t.link().unwrap();
    let qr = ticket::qr_png(&link).unwrap();
    let path = dir.path().join("qr.png");
    fs::write(&path, qr).unwrap();
    assert_eq!(ticket::decode_image(&path).unwrap(), link);
    let mut bad = t.clone();
    if let Location::Lan { address, .. } = &mut bad.location {
        *address = "8.8.8.8:80".into();
    }
    assert!(Ticket::parse(&bad.link().unwrap()).is_err());
    let mut expired = t.clone();
    if let Location::Lan { expires, .. } = &mut expired.location {
        *expires = crate::now_secs() - 1;
    }
    assert!(Ticket::parse(&expired.link().unwrap()).is_err());
    assert!(Ticket::parse("https://evil.example/book?token=secret").is_err());
    let remote = Ticket {
        location: Location::Github {
            owner: "user/evil".into(),
            repository: "repo".into(),
            asset: 1,
            share: None,
        },
        key: t.key.clone(),
    };
    assert!(Ticket::parse(&remote.link().unwrap()).is_err());
}
#[test]
fn streamed_package_verifies_all_frames_and_keeps_titles_inside_ciphertext() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let root = a.path().to_str().unwrap();
    let id = fixture(root, "私密 中文 Display Name", true);
    let work = Work::new();
    let package = prepare(root, &[id], &work).unwrap();
    assert!(package.manifest.books[0].size > 65536);
    let bytes = fs::read(&package.path).unwrap();
    assert!(!bytes
        .windows("私密 中文 Display Name".len())
        .any(|w| w == "私密 中文 Display Name".as_bytes()));
    let received = decode(
        b.path().to_str().unwrap(),
        &package.path,
        &ticket(package.key, "127.0.0.1:1".into()),
        &Work::new(),
    )
    .unwrap();
    assert_eq!(received.manifest.books[0].title, "私密 中文 Display Name");
    assert_eq!(
        crate::file_hash(received.paths[0].to_str().unwrap()).unwrap(),
        package.manifest.books[0].hash
    );
    assert!(!b.path().join("library.json").exists());
}
#[test]
fn corruption_wrong_key_trailing_bytes_and_cancel_never_import() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let root = a.path().to_str().unwrap();
    let id = fixture(root, "测试", false);
    let package = prepare(root, &[id], &Work::new()).unwrap();
    let t = ticket(package.key, "127.0.0.1:1".into());
    assert!(decode(
        b.path().to_str().unwrap(),
        &package.path,
        &ticket([99; 32], "127.0.0.1:1".into()),
        &Work::new()
    )
    .is_err());
    let original = fs::read(&package.path).unwrap();
    let mut corrupt = original.clone();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    fs::write(&package.path, corrupt).unwrap();
    assert!(decode(b.path().to_str().unwrap(), &package.path, &t, &Work::new()).is_err());
    let mut trailing = original.clone();
    trailing.push(0);
    fs::write(&package.path, trailing).unwrap();
    assert!(decode(b.path().to_str().unwrap(), &package.path, &t, &Work::new()).is_err());
    fs::write(&package.path, original).unwrap();
    let stopped = Work::new();
    stopped.stop();
    assert!(decode(b.path().to_str().unwrap(), &package.path, &t, &stopped).is_err());
    assert!(!b.path().join("library.json").exists());
}
#[test]
fn importing_duplicates_preserves_progress_annotations_identity_and_original() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let root = a.path().to_str().unwrap();
    let id = fixture(root, "分享名称", false);
    let package = prepare(root, std::slice::from_ref(&id), &Work::new()).unwrap();
    let receiver = b.path().to_str().unwrap();
    let received = decode(
        receiver,
        &package.path,
        &ticket(package.key, "127.0.0.1:1".into()),
        &Work::new(),
    )
    .unwrap();
    let list = received
        .manifest
        .books
        .iter()
        .zip(&received.paths)
        .map(|(x, p)| (x.title.clone(), x.hash.clone(), p.clone()))
        .collect::<Vec<_>>();
    let mut library = crate::library::Library::load_from(receiver);
    let (first, skipped) = library.import_shared_books(receiver, &list).unwrap();
    assert_eq!((first.len(), skipped), (1, 0));
    assert_ne!(first[0].id, id);
    library.update_position(receiver, &first[0].path, 1, Some("继续".into()), 3, 2);
    let cfgpath = b.path().join("books").join(format!("{}.json", first[0].id));
    let before = fs::read(&cfgpath).unwrap();
    let books_before = serde_json::to_value(&library).unwrap();
    let bytes = fs::read(&first[0].path).unwrap();
    let (second, skipped) = library.import_shared_books(receiver, &list).unwrap();
    assert_eq!((second.len(), skipped), (0, 1));
    assert_eq!(fs::read(cfgpath).unwrap(), before);
    assert_eq!(serde_json::to_value(&library).unwrap(), books_before);
    assert_eq!(fs::read(&first[0].path).unwrap(), bytes);
}
#[test]
fn lan_only_selected_book_no_permanent_pairing_retry_after_bad_pin_then_close() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let root = a.path().to_str().unwrap();
    let first = fixture(root, "选择的书", false);
    fixture(root, "未选择的书", false);
    let package = prepare(root, &[first], &Work::new()).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let t = ticket(package.key, address.to_string());
    let server_work = Work::new();
    server_work.phase("waiting", 0);
    let thread_work = server_work.clone();
    let server_ticket = t.clone();
    let handle = thread::spawn(move || lan::listen(package, server_ticket, listener, thread_work));
    let mut bad = t.clone();
    if let Location::Lan { pin, .. } = &mut bad.location {
        *pin = "0000".into();
    }
    assert!(lan::receive(b.path().to_str().unwrap(), &bad, &Work::new()).is_err());
    let received = lan::receive(b.path().to_str().unwrap(), &t, &Work::new()).unwrap();
    assert_eq!(received.manifest.books.len(), 1);
    assert_eq!(received.manifest.books[0].title, "选择的书");
    handle.join().unwrap();
    assert_eq!(server_work.progress.lock().unwrap().phase, "completed");
    assert!(TcpStream::connect(address).is_err());
    assert!(!a.path().join("peers.json").exists());
    assert!(!b.path().join("peers.json").exists());
}
#[test]
fn cancelled_listener_closes_and_removes_its_snapshot() {
    let a = tempfile::tempdir().unwrap();
    let root = a.path().to_str().unwrap();
    let id = fixture(root, "取消", false);
    let work = Work::new();
    let package = prepare(root, &[id], &work).unwrap();
    let path = package.path.clone();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let t = ticket(package.key, address.to_string());
    let worker = work.clone();
    let handle = thread::spawn(move || lan::listen(package, t, listener, worker));
    work.stop();
    handle.join().unwrap();
    assert!(!path.exists());
    assert!(TcpStream::connect(address).is_err());
}
#[test]
fn oversized_manifest_and_frame_are_bounded_before_allocation() {
    let m = Manifest {
        version: 1,
        books: vec![BookInfo {
            title: "x".into(),
            hash: "a".repeat(64),
            size: MAX_BOOK + 1,
        }],
    };
    assert!(m.validate().is_err());
    let m = Manifest {
        version: 1,
        books: vec![
            BookInfo {
                title: "x".into(),
                hash: "a".repeat(64),
                size: MAX_BOOK
            };
            5
        ],
    };
    assert!(m.validate().is_err());
    let a = tempfile::tempdir().unwrap();
    let root = a.path().to_str().unwrap();
    let path = a.path().join("oversized.rershare");
    let mut bytes = b"RERSHARE1".to_vec();
    bytes.extend(u32::MAX.to_be_bytes());
    fs::write(&path, bytes).unwrap();
    assert!(decode(
        root,
        &path,
        &ticket([1; 32], "127.0.0.1:1".into()),
        &Work::new()
    )
    .is_err());
}

#[test]
fn cancelled_queued_job_cannot_start_or_cancel_a_new_job() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let first = dispatch(root, json!({"op":"arm"})).unwrap();
    dispatch(root, json!({"op":"cancel","job_id":first["job_id"]})).unwrap();
    let delayed = dispatch(
        root,
        json!({"op":"start_lan","ids":["missing"],"job_id":first["job_id"]}),
    );
    assert_eq!(delayed.unwrap_err(), "Share cancelled");
    let second = dispatch(root, json!({"op":"arm"})).unwrap();
    dispatch(root, json!({"op":"cancel","job_id":first["job_id"]})).unwrap();
    let current = state(root);
    assert_eq!(current["job_id"], second["job_id"]);
    assert_eq!(current["progress"]["phase"], "preparing");
    stop(root);
}
#[test]
fn abandoned_cleanup_preserves_live_and_unmarked_directories() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let live = Scratch::new(root).unwrap();
    let unmarked = Path::new(root)
        .join(".book-share-tmp")
        .join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&unmarked).unwrap();
    fs::write(unmarked.join("evidence"), b"keep").unwrap();
    package::cleanup_abandoned(root);
    assert!(live.0.exists());
    assert!(unmarked.join("evidence").exists());
}

#[test]
fn reading_a_migrated_catalog_does_not_rewrite_config_or_future_annotations() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let id = fixture(root, "只读书库", false);
    let config = dir.path().join("books").join(format!("{id}.json"));
    let mut value: Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    value["updated_at"] = json!(7);
    value["future_annotations"] = json!({"preserve":"extension"});
    let before = serde_json::to_vec_pretty(&value).unwrap();
    fs::write(&config, &before).unwrap();
    assert_eq!(
        crate::library::Library::read_snapshot(root)
            .unwrap()
            .books
            .len(),
        1
    );
    assert_eq!(crate::library::Library::load_from(root).books.len(), 1);
    assert_eq!(fs::read(&config).unwrap(), before);
    let catalog = dir.path().join("library.json");
    fs::write(&catalog, b"corrupt catalog").unwrap();
    assert!(crate::library::Library::read_snapshot(root).is_err());
    assert_eq!(fs::read(catalog).unwrap(), b"corrupt catalog");
}
#[test]
fn cancelling_import_mid_copy_rolls_back_new_files_and_preserves_catalog() {
    let sender = tempfile::tempdir().unwrap();
    let receiver = tempfile::tempdir().unwrap();
    let root = sender.path().to_str().unwrap();
    let id = fixture(root, "取消导入", true);
    let package = prepare(root, &[id], &Work::new()).unwrap();
    let received = decode(
        receiver.path().to_str().unwrap(),
        &package.path,
        &ticket(package.key, "127.0.0.1:1".into()),
        &Work::new(),
    )
    .unwrap();
    let list = received
        .manifest
        .books
        .iter()
        .zip(&received.paths)
        .map(|(b, p)| (b.title.clone(), b.hash.clone(), p.clone()))
        .collect::<Vec<_>>();
    let cancelled = AtomicBool::new(false);
    let mut library = crate::library::Library::default();
    let result = library.import_shared_books_checked(
        receiver.path().to_str().unwrap(),
        &list,
        || {
            if cancelled.load(Ordering::SeqCst) {
                Err("Share cancelled".into())
            } else {
                Ok(())
            }
        },
        |_| cancelled.store(true, Ordering::SeqCst),
    );
    assert!(result.is_err());
    assert!(library.books.is_empty());
    assert!(!receiver.path().join("library.json").exists());
    assert_eq!(
        fs::read_dir(receiver.path().join("books")).unwrap().count(),
        0
    );
}
#[test]
fn authenticated_prefix_bounds_download_and_rejects_wrong_key_early() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let id = fixture(root, "清单边界", false);
    let package = prepare(root, &[id], &Work::new()).unwrap();
    let data = fs::read(&package.path).unwrap();
    let t = ticket(package.key, "127.0.0.1:1".into());
    assert_eq!(
        package::inspect_prefix(&data, &t).unwrap(),
        Some(data.len() as u64)
    );
    assert!(package::inspect_prefix(&data, &ticket([66; 32], "127.0.0.1:1".into())).is_err());
    assert_eq!(package::inspect_prefix(&data[..12], &t).unwrap(), None);
}
