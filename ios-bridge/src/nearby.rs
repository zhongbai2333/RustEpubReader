//! Foreground PIN adapter. Network sessions work in isolated snapshots; only a
//! live generation may apply validated books/progress to the actual library.
use super::{book, validate_position, DATA_LOCK};
use reader_core::{cloud_sync::SyncJournal, epub::EpubBook, library::Library, sharing::*};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    net::{Shutdown, SocketAddr, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
static OPEN: Mutex<Option<(String, String)>> = Mutex::new(None);
pub fn opened(root: &str, path: &str) {
    *OPEN.lock().unwrap_or_else(|e| e.into_inner()) = Some((root.into(), path.into()));
}
pub fn closed() {
    *OPEN.lock().unwrap_or_else(|e| e.into_inner()) = None;
}
#[derive(Default)]
struct Session {
    epoch: u64,
    stop: Option<Arc<AtomicBool>>,
    streams: Vec<TcpStream>,
    address: String,
    status: String,
    result: Option<Value>,
}
static SESSION: Mutex<Session> = Mutex::new(Session {
    epoch: 0,
    stop: None,
    streams: Vec::new(),
    address: String::new(),
    status: String::new(),
    result: None,
});
pub fn stop() {
    let mut s = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    s.epoch = s.epoch.wrapping_add(1);
    if let Some(flag) = s.stop.take() {
        flag.store(true, Ordering::SeqCst);
    }
    for stream in s.streams.drain(..) {
        let _ = stream.shutdown(Shutdown::Both);
    }
    s.address.clear();
    s.result = None;
    s.status = "stopped".into();
}
fn current(epoch: u64) -> bool {
    SESSION.lock().unwrap_or_else(|e| e.into_inner()).epoch == epoch
}
fn identity(root: &str) -> Result<PeerStore, String> {
    let store = PeerStore::load(root);
    if store.private_key_pem.is_empty()
        || store.public_key_pem.is_empty()
        || keystore::load_private_key(&store.device_id).as_deref() != Some(&store.private_key_pem)
    {
        return Err("Cannot save nearby identity".into());
    }
    Ok(store)
}
struct Snapshot {
    root: PathBuf,
    known: HashSet<String>,
    store: PeerStore,
}
impl Drop for Snapshot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn stage_activity(root: &str, device: &str, stage: &str) -> Result<(), String> {
    let activity = reader_core::reading_activity::snapshots(root, device)?;
    // The staged store has no local writer: retain every logical activity source.
    reader_core::reading_activity::import(stage, "", &activity)
}
fn merge_activity(
    root: &str,
    device: &str,
    stage: &str,
    peer_identity: &str,
) -> Result<(), String> {
    let activity = reader_core::reading_activity::snapshots(stage, peer_identity)?;
    // Logical activity identity is independent of the pairing/keychain identity.
    reader_core::reading_activity::import(root, device, &activity)
}
fn snapshot(root: &str, device: &str) -> Result<Snapshot, String> {
    let _guard = DATA_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = identity(root)?;
    let library = Library::load_from(root);
    let stage = PathBuf::from(root)
        .join("pin-staging")
        .join(uuid::Uuid::new_v4().to_string());
    let mut result = Snapshot {
        root: stage.clone(),
        known: HashSet::new(),
        store: store.clone(),
    };
    std::fs::create_dir_all(stage.join("books")).map_err(|_| "Cannot prepare nearby sync")?;
    stage_activity(
        root,
        device,
        stage.to_str().ok_or("Invalid nearby storage")?,
    )?;
    for entry in &library.books {
        let hash = EpubBook::file_hash(&entry.path)?;
        result.known.insert(hash.clone());
        let target = stage.join("books").join(format!("{}.epub", entry.id));
        // Copy files; snapshots cannot mutate the live reader's files/configuration.
        std::fs::copy(&entry.path, target).map_err(|_| "Cannot prepare nearby sync")?;
        let progress = ProgressEntry {
            book_hash: hash,
            title: entry.title.clone(),
            chapter: entry.last_chapter,
            block: entry.last_block,
            char_offset: entry.last_char_offset,
            chapter_title: entry.last_chapter_title.clone(),
            timestamp: entry.last_opened,
        };
        store.merge_progress(&[progress]);
    }
    let mut staged = library.clone();
    for entry in &mut staged.books {
        entry.path = stage
            .join("books")
            .join(format!("{}.epub", entry.id))
            .to_string_lossy()
            .into_owned();
    }
    staged.save_to(stage.to_str().ok_or("Invalid nearby storage")?);
    store.save(stage.to_str().unwrap());
    result.store = store;
    Ok(result)
}
fn apply(
    root: &str,
    device: &str,
    epoch: u64,
    snapshot: &Snapshot,
    store: &PeerStore,
    open_path: &str,
) -> Result<Value, String> {
    let current_open = OPEN.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let open_path = current_open
        .as_ref()
        .filter(|(r, _)| r == root)
        .map(|(_, p)| p.as_str())
        .unwrap_or(open_path);
    let session = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    if session.epoch != epoch {
        return Err("Nearby sync cancelled".into());
    }
    let _guard = DATA_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut library = Library::load_from(root);
    let mut downloaded = 0;
    let staged = Library::load_from(snapshot.root.to_str().unwrap());
    for entry in staged.books {
        let hash = EpubBook::file_hash(&entry.path)?;
        // Deleted during a transfer must remain deleted.
        if snapshot.known.contains(&hash)
            || library
                .books
                .iter()
                .any(|e| EpubBook::file_hash(&e.path).ok().as_ref() == Some(&hash))
        {
            continue;
        }
        let parsed = EpubBook::open(&entry.path)?;
        if parsed.chapters.is_empty() {
            return Err("EPUB has no readable chapters".into());
        }
        let added = library.add_or_update(root, parsed.title, entry.path.clone(), 0, None);
        if EpubBook::file_hash(&added.path).ok().as_ref() != Some(&hash) {
            return Err("Cannot save downloaded book".into());
        }
        downloaded += 1;
    }
    let mut live = identity(root)?;
    merge_activity(
        root,
        device,
        snapshot.root.to_str().unwrap(),
        &store.device_id,
    )?;
    // Pairing updates are written only by a current generation.
    for paired in &store.paired {
        live.add_paired(
            paired.device_id.clone(),
            paired.device_name.clone(),
            paired.pairing_uuid.clone(),
            paired.remote_public_key_pem.clone(),
        );
    }
    let mut journal = SyncJournal::load(root, device);
    let mut offer = None;
    for progress in &store.progress {
        let matched = library
            .books
            .iter()
            .find(|e| EpubBook::file_hash(&e.path).ok().as_ref() == Some(&progress.book_hash))
            .cloned();
        let Some(entry) = matched else { continue };
        if progress.timestamp <= entry.last_opened {
            continue;
        }
        let parsed = book(root, &entry)?;
        if validate_position(
            &parsed,
            progress.chapter,
            progress.block,
            progress.char_offset,
        )
        .is_err()
        {
            continue;
        }
        if entry.path == open_path {
            offer = Some(progress.clone());
            continue;
        }
        library.update_position(
            root,
            &entry.path,
            progress.chapter,
            progress.chapter_title.clone(),
            progress.block,
            progress.char_offset,
        );
        journal.record(progress.clone());
    }
    live.merge_progress(&store.progress);
    live.save(root);
    journal.save(root)?;
    Ok(json!({"uploaded":0,"downloaded":downloaded,"skipped":0,"offer":offer}))
}
fn register(epoch: u64, stream: &TcpStream) -> Result<(), String> {
    let mut s = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    if s.epoch != epoch {
        return Err("Nearby sync cancelled".into());
    }
    s.streams.push(
        stream
            .try_clone()
            .map_err(|_| "Cannot connect nearby device")?,
    );
    Ok(())
}
fn validate_pin(pin: &str) -> Result<(), String> {
    if pin.len() != 4 || !pin.bytes().all(|b| b.is_ascii_digit()) {
        Err("Invalid nearby PIN".into())
    } else {
        Ok(())
    }
}
pub fn dispatch(root: &str, device: &str, v: &Value) -> Result<Value, String> {
    match v["op"].as_str().unwrap_or("") {
        "peer_state" => {
            // Reading status must not generate a keypair or prompt Keychain.
            let store: PeerStore = std::fs::read(PathBuf::from(root).join("peers.json"))
                .ok()
                .and_then(|d| serde_json::from_slice(&d).ok())
                .unwrap_or_default();
            let mut s = SESSION.lock().unwrap_or_else(|e| e.into_inner());
            let result = s.result.take();
            Ok(
                json!({"epoch":s.epoch,"address":s.address,"status":s.status,"paired":store.paired,"result":result}),
            )
        }
        "peer_remove" => {
            let _guard = DATA_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let mut store = identity(root)?;
            store.remove_paired(super::field(v, "peer_id")?);
            store.save(root);
            Ok(json!({"ok":true}))
        }
        "peer_start" => {
            let pin = super::field(v, "pin")?.to_owned();
            validate_pin(&pin)?;
            stop();
            let epoch = SESSION.lock().unwrap_or_else(|e| e.into_inner()).epoch;
            let snapshot = snapshot(root, device)?;
            let (listener, addr) = start_server(
                "0.0.0.0:0",
                root,
                "",
                &pin,
                Arc::new(Mutex::new(snapshot.store.clone())),
            )?;
            listener
                .set_nonblocking(true)
                .map_err(|_| "Cannot listen nearby devices")?;
            let mut s = SESSION.lock().unwrap_or_else(|e| e.into_inner());
            if s.epoch != epoch {
                return Err("Nearby sync cancelled".into());
            }
            let cancel = Arc::new(AtomicBool::new(false));
            s.stop = Some(cancel.clone());
            let port = addr.rsplit(':').next().unwrap_or("");
            s.address = format!(
                "{}:{port}",
                get_local_ip().unwrap_or_else(|| "127.0.0.1".into())
            );
            s.status = "listening".into();
            drop(s);
            let root = root.to_owned();
            let device = device.to_owned();
            std::thread::spawn(move || {
                let store = Arc::new(Mutex::new(snapshot.store.clone()));
                while !cancel.load(Ordering::SeqCst) && current(epoch) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            if stream.set_nonblocking(false).is_err() {
                                continue;
                            }
                            if register(epoch, &stream).is_err() {
                                break;
                            }
                            let stage = snapshot.root.to_string_lossy().into_owned();
                            let books = snapshot.root.join("books").to_string_lossy().into_owned();
                            let outcome = handle_client(
                                &mut stream,
                                &stage,
                                &books,
                                &pin,
                                store.clone(),
                                &[],
                            );
                            let result = if outcome.is_ok() {
                                apply(
                                    &root,
                                    &device,
                                    epoch,
                                    &snapshot,
                                    &store.lock().unwrap_or_else(|e| e.into_inner()),
                                    "",
                                )
                            } else {
                                Err("Nearby connection failed".into())
                            };
                            let mut s = SESSION.lock().unwrap_or_else(|e| e.into_inner());
                            if s.epoch == epoch {
                                s.status = if result.is_ok() {
                                    "synced".into()
                                } else {
                                    "failed".into()
                                };
                                s.result = result.ok();
                                s.streams.clear();
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(100))
                        }
                        Err(_) => break,
                    }
                }
            });
            Ok(json!({"ok":true}))
        }
        "peer_connect" => {
            let addr: SocketAddr = super::field(v, "address")?
                .parse()
                .map_err(|_| "Invalid nearby address")?;
            let local = match addr.ip() {
                std::net::IpAddr::V4(ip) => {
                    ip.is_private() || ip.is_loopback() || ip.is_link_local()
                }
                std::net::IpAddr::V6(ip) => {
                    ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local()
                }
            };
            if !local || addr.port() == 0 {
                return Err("Invalid nearby address".into());
            }
            let pin = super::field(v, "pin")?;
            validate_pin(pin)?;
            stop();
            let epoch = SESSION.lock().unwrap_or_else(|e| e.into_inner()).epoch;
            let mut snapshot = snapshot(root, device)?;
            let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(10))
                .map_err(|_| "Nearby connection failed")?;
            register(epoch, &stream)?;
            stream.set_write_timeout(Some(Duration::from_secs(30))).ok();
            let stage = snapshot.root.to_str().unwrap();
            let (mut stream, key, mut send, mut receive) = connect_with_stream(
                stream,
                &mut snapshot.store,
                stage,
                v["peer_id"].as_str(),
                Some(pin),
            )?;
            auto_sync_session(
                &mut stream,
                &key,
                &mut send,
                &mut receive,
                &mut snapshot.store,
                stage,
                snapshot.root.join("books").to_str().unwrap(),
                &[],
            )?;
            let result = apply(
                root,
                device,
                epoch,
                &snapshot,
                &snapshot.store,
                v["open_path"].as_str().unwrap_or(""),
            );
            let mut s = SESSION.lock().unwrap_or_else(|e| e.into_inner());
            if s.epoch == epoch {
                s.streams.clear();
                s.status = if result.is_ok() {
                    "synced".into()
                } else {
                    "failed".into()
                };
            }
            result
        }
        _ => Err("Unknown nearby operation".into()),
    }
}

#[cfg(test)]
mod activity_tests {
    use super::*;
    #[test]
    fn staged_nearby_activity_uses_logical_identity_and_preserves_live_writes() {
        let live = tempfile::tempdir().unwrap();
        let stage = tempfile::tempdir().unwrap();
        let root = live.path().to_str().unwrap();
        let staging = stage.path().to_str().unwrap();
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../core/tests/fixtures/illustrations.epub");
        let mut library = Library::load_from(root);
        let entry = library.add_or_update(
            root,
            "Book".into(),
            fixture.to_string_lossy().into_owned(),
            0,
            None,
        );
        let now = 1_700_000_120u64;
        let today = reader_core::reading_activity::date_from_days((now / 86400) as i64);
        reader_core::reading_activity::checkpoint(
            root,
            "ios-logical",
            &entry.id,
            "session",
            10,
            now,
            0,
            false,
        )
        .unwrap();
        stage_activity(root, "ios-logical", staging).unwrap();
        let wire = reader_core::reading_activity::snapshots(staging, "ios-pairing").unwrap();
        let logical = wire.iter().find(|s| s.device_id == "ios-logical").unwrap();
        let mut remote = logical.clone();
        remote.device_id = "android-pairing".into();
        remote
            .activity
            .values_mut()
            .next()
            .unwrap()
            .days
            .get_mut(&today)
            .unwrap()
            .reading = 20;
        reader_core::reading_activity::import(staging, "ios-pairing", &[remote]).unwrap();
        reader_core::reading_activity::checkpoint(
            root,
            "ios-logical",
            &entry.id,
            "session",
            15,
            now + 5,
            0,
            false,
        )
        .unwrap();
        merge_activity(root, "ios-logical", staging, "ios-pairing").unwrap();
        merge_activity(root, "ios-logical", staging, "ios-pairing").unwrap();
        let result =
            reader_core::reading_activity::report(root, "ios-logical", &today, None).unwrap();
        assert_eq!(result.total, 35);
        assert_eq!(result.devices, 2);
    }
}
