//! Receiving a progress record is distinct from publishing it and moving a viewport.
//! Call on a worker: EPUB validation never belongs in a UI frame or draw callback.
use super::PeerStore;
use crate::{cloud_progress::VersionedProgress, library::Library};

#[derive(Clone, Debug)]
pub struct CommittedProgress {
    pub path: String,
    pub record: VersionedProgress,
}

/// Apply only valid, current members. Recheck membership/version at publication so a
/// transfer cannot resurrect an archived book or overwrite a newer local reading edit.
/// Does not save a sync journal: the owner merges these notices into its live journal.
pub fn apply_received_progress(
    root: &str,
    store: &PeerStore,
) -> Result<Vec<CommittedProgress>, String> {
    let mut library = Library::read_snapshot(root)?;
    let mut index = std::collections::BTreeMap::<String, Vec<_>>::new();
    for entry in &library.books {
        let Some(config) = Library::load_book_config(root, &entry.id)? else {
            continue;
        };
        if let Some(hash) = config.file_hash.clone() {
            index.entry(hash).or_default().push((entry.clone(), config));
        }
    }
    let mut committed = Vec::new();
    for p in &store.progress {
        if !crate::cloud_sync::valid_hash(&p.book_hash) {
            continue;
        }
        let version = store.progress_version(&p.book_hash);
        if version.modified_ms == 0 || !crate::cloud_sync::valid_device_id(&version.device) {
            continue;
        }
        let record = VersionedProgress {
            progress: p.clone(),
            version,
        };
        for (entry, config) in index.get(&p.book_hash).into_iter().flatten() {
            if config.reading_version().is_some_and(|v| v > record.version) {
                continue;
            }
            let changed = (entry.last_chapter, entry.last_block, entry.last_char_offset)
                != (p.chapter, p.block, p.char_offset);
            if changed
                && !crate::epub::EpubBook::open_cached(
                    &entry.path,
                    &std::path::Path::new(root).join(".cache/chapter-index"),
                )
                .is_ok_and(|book| crate::cloud_progress::validate_position(&book, p))
            {
                continue;
            }
            if library.apply_versioned_position(root, &entry.path, &record)? {
                committed.push(CommittedProgress {
                    path: entry.path.clone(),
                    record: record.clone(),
                });
            }
        }
    }
    Ok(committed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cloud_progress::{ProgressVersion, VersionedProgress},
        sharing::{Message, ProgressEntry},
    };
    use std::collections::BTreeMap;
    const EPUB: &[u8] = include_bytes!("../../../ios/Tests/fixtures/native-reading-modes.epub");
    const TIME: u64 = 1_700_000_000_123;
    fn record(
        hash: &str,
        chapter: usize,
        offset: usize,
        time: u64,
        device: &str,
    ) -> VersionedProgress {
        VersionedProgress {
            progress: ProgressEntry {
                book_hash: hash.into(),
                title: "Book".into(),
                chapter,
                block: 3,
                char_offset: offset,
                chapter_title: None,
                timestamp: time / 1000,
            },
            version: ProgressVersion {
                modified_ms: time,
                device: device.into(),
                legacy: false,
            },
        }
    }
    fn fixture() -> (tempfile::TempDir, crate::library::BookEntry, String) {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().to_str().unwrap();
        let mut library = Library::load_from(root).unwrap();
        let entry = library
            .add_or_update_from_bytes(root, "Book".into(), EPUB, 0, None)
            .unwrap();
        let hash = crate::epub::EpubBook::file_hash(&entry.path).unwrap();
        (directory, entry, hash)
    }
    fn store(record: &VersionedProgress) -> PeerStore {
        let mut store = PeerStore::default();
        store.merge_progress_versioned(
            std::slice::from_ref(&record.progress),
            &BTreeMap::from([(record.progress.book_hash.clone(), record.version.clone())]),
        );
        store
    }
    #[test]
    fn receiving_keeps_origin_clock_reopen_independent_replay_idempotent_and_membership_current() {
        let (directory, entry, hash) = fixture();
        let root = directory.path().to_str().unwrap();
        let mut library = Library::load_from(root).unwrap();
        library
            .set_last_opened(root, &entry.path, TIME + 10000)
            .unwrap();
        let remote = record(&hash, 21, 3, TIME, "phone");
        let peer = store(&remote);
        assert_eq!(apply_received_progress(root, &peer).unwrap().len(), 1);
        let config = Library::load_book_config(root, &entry.id).unwrap().unwrap();
        assert_eq!(config.reading_version(), Some(remote.version.clone()));
        assert_eq!(
            (
                config.last_chapter,
                config.last_char_offset,
                config.last_opened
            ),
            (21, 3, TIME + 10000)
        );
        let path = directory
            .path()
            .join("books")
            .join(format!("{}.json", entry.id));
        let before = std::fs::read(&path).unwrap();
        assert_eq!(apply_received_progress(root, &peer).unwrap().len(), 1);
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "Replay rewrote unchanged config"
        );
        let local = record(&hash, 1, 1, TIME + 1, "tablet");
        assert!(library
            .apply_versioned_position(root, &entry.path, &local)
            .unwrap());
        assert!(apply_received_progress(root, &peer).unwrap().is_empty());
        assert_eq!(
            Library::load_book_config(root, &entry.id)
                .unwrap()
                .unwrap()
                .last_chapter,
            1
        );
        library
            .hide_paths(root, &std::iter::once(entry.path.clone()).collect())
            .unwrap();
        assert!(
            apply_received_progress(root, &store(&record(&hash, 22, 3, TIME + 2, "phone")))
                .unwrap()
                .is_empty()
        );
        assert!(Library::load_from(root).unwrap().books.is_empty());
    }
    #[test]
    fn invalid_canonical_coordinates_never_publish_or_notify() {
        let (directory, entry, hash) = fixture();
        let root = directory.path().to_str().unwrap();
        for invalid in [
            record(&hash, 99999, 0, TIME, "phone"),
            record(&hash, 21, 999999, TIME, "phone"),
        ] {
            assert!(apply_received_progress(root, &store(&invalid))
                .unwrap()
                .is_empty());
        }
        let config = Library::load_book_config(root, &entry.id).unwrap().unwrap();
        assert_eq!(config.last_chapter, 0);
        assert!(config.reading_version().is_none());
    }
    #[test]
    fn forwarding_same_second_edits_and_merging_a_session_preserves_latest_local_edit() {
        let hash = "a".repeat(64);
        let old = record(&hash, 20, 3, TIME, "A");
        let backwards = record(&hash, 2, 1, TIME + 1, "B");
        let latest = record(&hash, 4, 2, TIME + 2, "A");
        let mut live = store(&old);
        let mut session = live.clone();
        session.merge_progress_versioned(
            &[backwards.progress],
            &BTreeMap::from([(hash.clone(), backwards.version)]),
        );
        live.merge_progress_versioned(
            &[latest.progress],
            &BTreeMap::from([(hash.clone(), latest.version.clone())]),
        );
        let directory = tempfile::tempdir().unwrap();
        live.merge_session(&session, directory.path().to_str().unwrap());
        assert_eq!(live.progress[0].chapter, 4);
        assert_eq!(live.progress_version(&hash), latest.version);
        assert!(live
            .merge_progress_versioned(&session.progress, &session.progress_versions)
            .is_empty());
        let saved: PeerStore =
            serde_json::from_slice(&std::fs::read(directory.path().join("peers.json")).unwrap())
                .unwrap();
        assert_eq!(saved.progress_version(&hash), latest.version);
    }
    #[test]
    fn additive_wire_versions_keep_old_message_compatibility_and_exact_author() {
        for tag in ["SyncProgress", "ProgressResponse"] {
            let old = format!(r#"{{"type":"{tag}","entries":[]}}"#);
            let decoded: Message = serde_json::from_str(&old).unwrap();
            match decoded {
                Message::SyncProgress { versions, .. }
                | Message::ProgressResponse { versions, .. } => assert!(versions.is_empty()),
                _ => unreachable!(),
            }
        }
        let hash = "a".repeat(64);
        let r = record(&hash, 2, 3, TIME, "origin");
        let message = Message::SyncProgress {
            entries: vec![r.progress],
            versions: BTreeMap::from([(hash.clone(), r.version.clone())]),
            activity: vec![],
        };
        let value = serde_json::to_value(&message).unwrap();
        #[derive(serde::Deserialize)]
        struct Old {
            entries: Vec<ProgressEntry>,
        }
        let old: Old = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(old.entries[0].chapter, 2);
        let decoded: Message = serde_json::from_value(value).unwrap();
        match decoded {
            Message::SyncProgress { versions, .. } => assert_eq!(versions[&hash], r.version),
            _ => unreachable!(),
        }
    }
    #[cfg(not(feature = "keychain"))]
    #[test]
    fn encrypted_lan_transfers_the_exact_revision_and_commits_validated_progress() {
        use crate::sharing::{auto_sync_session, connect_to_peer, handle_client, start_server};
        use std::sync::{Arc, Mutex};
        let (server_dir, server_book, hash) = fixture();
        let (client_dir, _, client_hash) = fixture();
        assert_eq!(hash, client_hash);
        let server_root = server_dir.path().to_str().unwrap();
        let client_root = client_dir.path().to_str().unwrap();
        let original = record(&hash, 21, 3, TIME, "original-reader");
        let mut server = PeerStore::load(server_root);
        server.merge_progress_versioned(
            &[original.progress.clone()],
            &BTreeMap::from([(hash.clone(), original.version.clone())]),
        );
        let server_id = server.device_id.clone();
        let shared = Arc::new(Mutex::new(server));
        let (listener, address) = start_server(
            "127.0.0.1:0",
            server_root,
            server_root,
            "1234",
            shared.clone(),
        )
        .unwrap();
        let root = server_root.to_owned();
        let thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .unwrap();
            handle_client(
                &mut stream,
                &root,
                &format!("{root}/books"),
                "1234",
                shared,
                &[],
            )
        });
        let mut client = PeerStore::load(client_root);
        let (mut stream, key, mut sent, mut received) = connect_to_peer(
            &address,
            &mut client,
            client_root,
            Some(&server_id),
            Some("1234"),
        )
        .unwrap();
        auto_sync_session(
            &mut stream,
            &key,
            &mut sent,
            &mut received,
            &mut client,
            client_root,
            client_dir.path().join("books").to_str().unwrap(),
            &[],
        )
        .unwrap();
        crate::sharing::crypto::write_encrypted_message(
            &mut stream,
            &Message::Goodbye,
            &key,
            &mut sent,
        )
        .unwrap();
        thread.join().unwrap().unwrap();
        assert_eq!(client.progress_version(&hash), original.version);
        let notices = apply_received_progress(client_root, &client).unwrap();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].record.version, original.version);
        assert_eq!(
            Library::load_from(client_root).unwrap().books[0].last_chapter,
            21
        );
        assert_eq!(
            Library::load_from(server_root).unwrap().books[0].id,
            server_book.id
        );
    }
}
