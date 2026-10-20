//! Replicated book lifecycle, separate from file availability and reading position.
//! Hidden entries retain their UUID/config locally; snapshots contain no local paths.
use super::{BookEntry, Library};
use crate::cloud_sync::{valid_hash, CloudBook, DeviceSnapshot};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BookState {
    Active,
    Archived,
    Deleted,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BookLifecycle {
    pub book: CloudBook,
    pub state: BookState,
    pub revision: u64,
    pub event: String,
}
impl BookLifecycle {
    pub fn valid(&self, hash: &str) -> bool {
        valid_hash(hash)
            && self.book.hash == hash
            && self.book.title.len() <= 4096
            && self.book.size <= 1024 * 1024 * 1024
            && self.revision > 0
            && uuid::Uuid::parse_str(&self.event).is_ok()
    }
    pub fn newer_than(&self, other: &Self) -> bool {
        (self.revision, &self.event) > (other.revision, &other.event)
    }
}
pub fn merge_states(
    into: &mut BTreeMap<String, BookLifecycle>,
    from: &BTreeMap<String, BookLifecycle>,
) {
    for (hash, state) in from {
        if into.get(hash).is_none_or(|old| state.newer_than(old)) {
            into.insert(hash.clone(), state.clone());
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArchivedBook {
    #[serde(flatten)]
    pub lifecycle: BookLifecycle,
    /// Local-only backup, including UUID, progress and the owned storage path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<BookEntry>,
}
#[derive(Default, Serialize, Deserialize)]
pub struct ArchiveStore {
    pub records: BTreeMap<String, ArchivedBook>,
}
impl ArchiveStore {
    pub fn load(root: &str) -> Result<Self, String> {
        let _access = super::access::lock(root)?;
        match std::fs::read(Path::new(root).join("book-lifecycle.json")) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(_) => Err("Cannot read book archive".into()),
            Ok(bytes) => {
                if bytes.len() > 16 * 1024 * 1024 {
                    return Err("Book archive exceeds size limit".into());
                }
                let mut store: Self =
                    serde_json::from_slice(&bytes).map_err(|_| "Invalid book archive")?;
                if store.records.len() > 5000
                    || store.records.iter().any(|(h, r)| !r.lifecycle.valid(h))
                {
                    return Err("Invalid book archive".into());
                }
                // iOS may relocate the sandbox when the app is updated. Rebase
                // only owned UUID filenames, never an external import path.
                for record in store.records.values_mut() {
                    if let Some(entry) = &mut record.entry {
                        if super::is_uuid_like(&entry.id)
                            && Path::new(&entry.path).file_name().and_then(|n| n.to_str())
                                == Some(&format!("{}.epub", entry.id))
                        {
                            entry.path = Path::new(root)
                                .join("books")
                                .join(format!("{}.epub", entry.id))
                                .to_string_lossy()
                                .into_owned();
                        }
                    }
                }
                Ok(store)
            }
        }
    }
    fn save(&self, root: &str) -> Result<(), String> {
        let _access = super::access::lock(root)?;
        std::fs::create_dir_all(root).map_err(|_| "Cannot save book archive")?;
        super::write_rename_file(
            &Path::new(root).join("book-lifecycle.json"),
            &serde_json::to_vec(self).map_err(|_| "Cannot encode book archive")?,
        )
    }
    pub fn states(&self) -> BTreeMap<String, BookLifecycle> {
        self.records
            .iter()
            .map(|(h, r)| (h.clone(), r.lifecycle.clone()))
            .collect()
    }
    pub fn hidden(&self, hash: &str) -> bool {
        self.records
            .get(hash)
            .is_some_and(|r| r.lifecycle.state != BookState::Active)
    }
    pub fn list(&self) -> Vec<ArchivedBook> {
        let mut books: Vec<_> = self
            .records
            .values()
            .filter(|r| r.lifecycle.state == BookState::Archived)
            .cloned()
            .collect();
        books.sort_by_key(|r| std::cmp::Reverse(r.lifecycle.revision));
        books
    }
    fn revise(&mut self, info: CloudBook, state: BookState, entry: Option<BookEntry>) {
        let wall = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let revision = wall.max(
            self.records
                .values()
                .map(|r| r.lifecycle.revision)
                .max()
                .unwrap_or(0)
                .saturating_add(1),
        );
        let hash = info.hash.clone();
        self.records.insert(
            hash,
            ArchivedBook {
                lifecycle: BookLifecycle {
                    book: info,
                    state,
                    revision,
                    event: uuid::Uuid::new_v4().to_string(),
                },
                entry,
            },
        );
    }
    /// Validate the entire selection before persisting any changes. Archiving never unlinks files.
    pub fn archive(root: &str, library: &mut Library, paths: &[String]) -> Result<usize, String> {
        let _access = super::access::lock(root)?;
        if paths.is_empty() || paths.len() > 1000 {
            return Err("Invalid book selection".into());
        }
        let mut store = Self::load(root)?;
        let selected = paths
            .iter()
            .map(|path| {
                let entry = library
                    .books
                    .iter()
                    .find(|e| &e.path == path)
                    .ok_or("Book is no longer in the library")?
                    .clone();
                let size = std::fs::metadata(path)
                    .map_err(|_| "Cannot read local book")?
                    .len();
                let hash = Library::read_book_config(root, &entry.id)
                    .and_then(|c| c.file_hash)
                    .or_else(|| crate::file_hash(path).ok())
                    .ok_or("Cannot identify book")?;
                Ok((
                    entry,
                    CloudBook {
                        hash,
                        title: library
                            .books
                            .iter()
                            .find(|e| &e.path == path)
                            .unwrap()
                            .title
                            .clone(),
                        size,
                    },
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        for (entry, info) in &selected {
            store.revise(info.clone(), BookState::Archived, Some(entry.clone()));
        }
        store.save(root)?;
        library.hide_paths(
            root,
            &selected
                .iter()
                .map(|(entry, _)| entry.path.clone())
                .collect(),
        )?;
        Ok(selected.len())
    }
    pub fn change(root: &str, hash: &str, state: BookState) -> Result<(), String> {
        let _access = super::access::lock(root)?;
        if !matches!(state, BookState::Active | BookState::Deleted) {
            return Err("Invalid archive action".into());
        }
        let mut store = Self::load(root)?;
        let record = store
            .records
            .get(hash)
            .filter(|r| r.lifecycle.state == BookState::Archived)
            .cloned()
            .ok_or("Book is no longer archived")?;
        store.revise(record.lifecycle.book, state, record.entry);
        store.save(root)?;
        let mut library = Library::load_from(root)?;
        store.reconcile(root, &mut library, &Default::default())?;
        Ok(())
    }
    /// Merge live local actions last. An in-flight download must not resurrect an archived book.
    pub fn apply(
        root: &str,
        library: &mut Library,
        snapshots: &[DeviceSnapshot],
        own: &DeviceSnapshot,
    ) -> Result<(), String> {
        let _access = super::access::lock(root)?;
        Self::apply_verified(root, library, snapshots, own, &Default::default())
    }
    pub fn apply_verified(
        root: &str,
        library: &mut Library,
        snapshots: &[DeviceSnapshot],
        own: &DeviceSnapshot,
        confirmed: &std::collections::BTreeSet<String>,
    ) -> Result<(), String> {
        let _access = super::access::lock(root)?;
        let mut states = own.lifecycle.clone();
        for snapshot in snapshots {
            merge_states(&mut states, &snapshot.lifecycle);
        }
        let mut store = Self::load(root)?;
        for (hash, state) in states {
            let record = store.records.entry(hash).or_insert_with(|| ArchivedBook {
                lifecycle: state.clone(),
                entry: None,
            });
            if state.newer_than(&record.lifecycle) {
                record.lifecycle = state;
            }
        }
        store.save(root)?;
        store.reconcile(root, library, confirmed)
    }
    fn reconcile(
        &mut self,
        root: &str,
        library: &mut Library,
        confirmed: &std::collections::BTreeSet<String>,
    ) -> Result<(), String> {
        // Persist UUID/config backups before hiding anything or unlinking a file.
        let visible: BTreeMap<_, _> = library
            .books
            .iter()
            .filter_map(|entry| {
                Library::read_book_config(root, &entry.id)
                    .and_then(|c| c.file_hash)
                    .map(|hash| (hash, entry.clone()))
            })
            .collect();
        for (hash, record) in &mut self.records {
            if record.lifecycle.state != BookState::Active {
                if let Some(entry) = visible.get(hash) {
                    record.entry = Some(entry.clone());
                }
            }
        }
        self.save(root)?;
        let hidden = self
            .records
            .values()
            .filter(|r| r.lifecycle.state != BookState::Active)
            .filter_map(|r| r.entry.as_ref().map(|e| e.path.clone()))
            .collect();
        library.hide_paths(root, &hidden)?;
        for (hash, record) in &self.records {
            if record.lifecycle.state != BookState::Active {
                if let Some(entry) = &record.entry {
                    let release =
                        record.lifecycle.state == BookState::Deleted || confirmed.contains(hash);
                    if release {
                        if record.lifecycle.state == BookState::Archived
                            && Path::new(&entry.path).is_file()
                            && crate::file_hash(&entry.path).ok().as_ref() != Some(hash)
                        {
                            return Err("Local archived book changed; file retained".into());
                        }
                        unlink_owned(root, entry)?;
                    }
                }
            } else if let Some(entry) = &record.entry {
                if Path::new(&entry.path).is_file() {
                    library.restore_entry(root, entry.clone())?;
                }
            }
        }
        Ok(())
    }
    /// Include retained offline archives in uploads before releasing their local files.
    pub fn retained(&self) -> Vec<(CloudBook, PathBuf)> {
        self.records
            .values()
            .filter(|r| r.lifecycle.state == BookState::Archived)
            .filter_map(|r| {
                let path = PathBuf::from(&r.entry.as_ref()?.path);
                path.is_file().then(|| (r.lifecycle.book.clone(), path))
            })
            .collect()
    }
}
fn unlink_owned(root: &str, entry: &BookEntry) -> Result<(), String> {
    let expected = Path::new(root)
        .join("books")
        .join(format!("{}.epub", entry.id));
    if !super::is_uuid_like(&entry.id) || Path::new(&entry.path) != expected {
        return Err("Book is outside library storage".into());
    }
    if std::fs::symlink_metadata(Path::new(root).join("books"))
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err("Book is outside library storage".into());
    }
    // Unlink only the managed directory entry, never a symlink target or external import.
    match std::fs::remove_file(expected) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Cannot release archived book file".into()),
    }
}

/// Restore a downloaded archive to its original UUID/config, retaining annotations.
/// None means it is a normal first import; hidden books are never reintroduced.
pub fn restore_download(
    root: &str,
    library: &mut Library,
    hash: &str,
    bytes: &[u8],
) -> Result<Option<BookEntry>, String> {
    let _access = super::access::lock(root)?;
    let store = ArchiveStore::load(root)?;
    let Some(record) = store.records.get(hash) else {
        return Ok(None);
    };
    if record.lifecycle.state != BookState::Active {
        return Err("Book is archived".into());
    }
    let Some(entry) = &record.entry else {
        return Ok(None);
    };
    let expected = Path::new(root)
        .join("books")
        .join(format!("{}.epub", entry.id));
    if !super::is_uuid_like(&entry.id)
        || expected != Path::new(&entry.path)
        || crate::bytes_hash(bytes) != hash
    {
        return Err("Cannot restore archived book".into());
    }
    super::write_rename_file(&expected, bytes)?;
    library.restore_entry(root, entry.clone())?;
    Ok(Some(entry.clone()))
}

/// Restore an owned archived identity from a verified staged file without
/// materializing the entire publication in memory.
pub fn restore_download_file(
    root: &str,
    library: &mut Library,
    hash: &str,
    source: &Path,
) -> Result<Option<BookEntry>, String> {
    let _access = super::access::lock(root)?;
    let store = ArchiveStore::load(root)?;
    let Some(record) = store.records.get(hash) else {
        return Ok(None);
    };
    if record.lifecycle.state != BookState::Active {
        return Err("Book is archived".into());
    }
    let Some(entry) = &record.entry else {
        return Ok(None);
    };
    let expected = Path::new(root)
        .join("books")
        .join(format!("{}.epub", entry.id));
    if !super::is_uuid_like(&entry.id)
        || expected != Path::new(&entry.path)
        || crate::file_hash(&source.to_string_lossy())? != hash
    {
        return Err("Cannot restore archived book".into());
    }
    if crate::file_hash(&expected.to_string_lossy()).is_ok_and(|v| v == hash) {
        library.restore_entry(root, entry.clone())?;
        return Ok(Some(entry.clone()));
    }
    let temporary = expected.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        std::fs::copy(source, &temporary).map_err(|_| "Cannot copy restored book")?;
        if crate::file_hash(&temporary.to_string_lossy())? != hash {
            return Err("Restored book integrity check failed".into());
        }
        std::fs::rename(&temporary, &expected).map_err(|_| "Cannot finalize restored book")?;
        Ok::<_, String>(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result?;
    library.restore_entry(root, entry.clone())?;
    Ok(Some(entry.clone()))
}

pub fn dispatch(root: &str, request: &serde_json::Value) -> Result<serde_json::Value, String> {
    let _access = super::access::lock(root)?;
    match request["action"].as_str().unwrap_or("list") {
        "list" => {
            let values = ArchiveStore::load(root)?
                .list()
                .into_iter()
                .map(|r| {
                    let local = r
                        .entry
                        .as_ref()
                        .is_some_and(|e| Path::new(&e.path).is_file());
                    let mut value = serde_json::to_value(r).unwrap();
                    value["local_available"] = serde_json::json!(local);
                    value
                })
                .collect::<Vec<_>>();
            Ok(serde_json::json!(values))
        }
        "archive" => {
            let paths = request["paths"]
                .as_array()
                .ok_or("Missing book selection")?
                .iter()
                .map(|v| v.as_str().map(String::from).ok_or("Invalid book selection"))
                .collect::<Result<Vec<_>, _>>()?;
            let count = ArchiveStore::archive(root, &mut Library::load_from(root)?, &paths)?;
            Ok(serde_json::json!({"count":count}))
        }
        "restore" | "delete" => {
            let state = if request["action"] == "restore" {
                BookState::Active
            } else {
                BookState::Deleted
            };
            ArchiveStore::change(
                root,
                request["hash"].as_str().ok_or("Missing book hash")?,
                state,
            )?;
            Ok(serde_json::json!({"count":1}))
        }
        _ => Err("Invalid archive action".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (tempfile::TempDir, Library, BookEntry, Vec<u8>) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let bytes = b"archive fixture".to_vec();
        let mut library = Library::default();
        let entry = library
            .add_or_update_from_bytes(root, "Book".into(), &bytes, 3, Some("Chapter".into()))
            .unwrap();
        (dir, library, entry, bytes)
    }
    #[test]
    fn offline_archive_hides_but_retains_file_and_restores_uuid_position() {
        let (dir, mut library, entry, _) = setup();
        let root = dir.path().to_str().unwrap();
        ArchiveStore::archive(root, &mut library, std::slice::from_ref(&entry.path)).unwrap();
        assert!(Library::load_from(root).unwrap().books.is_empty());
        assert!(Path::new(&entry.path).exists());
        let store = ArchiveStore::load(root).unwrap();
        let hash = store.records.keys().next().unwrap();
        ArchiveStore::change(root, hash, BookState::Active).unwrap();
        let restored = Library::load_from(root).unwrap().books.remove(0);
        assert_eq!(restored.id, entry.id);
        assert_eq!(restored.last_chapter, 3);
    }
    #[test]
    fn archive_propagates_without_losing_only_copy_and_cloud_restore_preserves_config() {
        let (dir, mut library, entry, bytes) = setup();
        let root = dir.path().to_str().unwrap();
        let hash = crate::bytes_hash(&bytes);
        let mut remote = DeviceSnapshot::new("remote".into());
        let mut store = ArchiveStore::default();
        store.revise(
            CloudBook {
                hash: hash.clone(),
                title: entry.title.clone(),
                size: bytes.len() as u64,
            },
            BookState::Archived,
            None,
        );
        remote.lifecycle = store.states();
        let own = DeviceSnapshot::new("local".into());
        ArchiveStore::apply(root, &mut library, &[remote.clone()], &own).unwrap();
        assert!(library.books.is_empty());
        assert!(Path::new(&entry.path).exists());
        remote
            .books
            .insert(hash.clone(), remote.lifecycle[&hash].book.clone());
        ArchiveStore::apply_verified(
            root,
            &mut library,
            &[remote],
            &own,
            &std::collections::BTreeSet::from([hash.clone()]),
        )
        .unwrap();
        assert!(!Path::new(&entry.path).exists());
        assert_eq!(
            Library::read_book_config(root, &entry.id)
                .unwrap()
                .last_chapter,
            3
        );
        ArchiveStore::change(root, &hash, BookState::Active).unwrap();
        let restored = restore_download(root, &mut library, &hash, &bytes)
            .unwrap()
            .unwrap();
        assert_eq!(restored.id, entry.id);
        assert_eq!(restored.last_chapter, 3);
    }
    #[test]
    fn stale_network_result_cannot_resurrect_local_archive_or_delete() {
        let (dir, mut library, entry, bytes) = setup();
        let root = dir.path().to_str().unwrap();
        let hash = crate::bytes_hash(&bytes);
        ArchiveStore::archive(root, &mut library, std::slice::from_ref(&entry.path)).unwrap();
        let mut snapshot = DeviceSnapshot::new("old".into());
        snapshot.books.insert(
            hash.clone(),
            CloudBook {
                hash: hash.clone(),
                title: entry.title,
                size: bytes.len() as u64,
            },
        );
        ArchiveStore::apply(
            root,
            &mut library,
            &[snapshot.clone()],
            &DeviceSnapshot::new("local".into()),
        )
        .unwrap();
        assert!(library.books.is_empty());
        ArchiveStore::change(root, &hash, BookState::Deleted).unwrap();
        ArchiveStore::apply(
            root,
            &mut library,
            &[snapshot],
            &DeviceSnapshot::new("local".into()),
        )
        .unwrap();
        assert!(ArchiveStore::load(root).unwrap().list().is_empty());
        assert!(restore_download(root, &mut library, &hash, &bytes).is_err());
    }
    #[test]
    fn staged_file_restore_keeps_identity_and_rejects_corrupt_downloads() {
        let (dir, mut library, entry, bytes) = setup();
        let root = dir.path().to_str().unwrap();
        let hash = crate::bytes_hash(&bytes);
        ArchiveStore::archive(root, &mut library, std::slice::from_ref(&entry.path)).unwrap();
        ArchiveStore::change(root, &hash, BookState::Active).unwrap();
        library = Library::load_from(root).unwrap();
        library
            .hide_paths(root, &std::collections::HashSet::from([entry.path.clone()]))
            .unwrap();
        std::fs::remove_file(&entry.path).unwrap();
        let staged = dir.path().join("download.partial");
        std::fs::write(&staged, b"corrupt").unwrap();
        assert!(restore_download_file(root, &mut library, &hash, &staged).is_err());
        assert!(!Path::new(&entry.path).exists());
        assert!(library.books.is_empty());
        std::fs::write(&staged, bytes).unwrap();
        let restored = restore_download_file(root, &mut library, &hash, &staged)
            .unwrap()
            .unwrap();
        assert_eq!(restored.id, entry.id);
        assert_eq!(restored.last_chapter, entry.last_chapter);
        assert_eq!(restored.path, entry.path);
        assert_eq!(crate::file_hash(&restored.path).unwrap(), hash);
        assert_eq!(
            Library::read_book_config(root, &entry.id)
                .unwrap()
                .last_chapter,
            3
        );
        assert_eq!(library.books.len(), 1);
        assert!(staged.exists());
    }
    #[test]
    fn selection_validation_is_atomic_and_never_unlinks_external_files() {
        let (dir, mut library, entry, _) = setup();
        let root = dir.path().to_str().unwrap();
        assert!(
            ArchiveStore::archive(root, &mut library, &[entry.path.clone(), "missing".into()])
                .is_err()
        );
        assert_eq!(library.books.len(), 1);
        assert!(ArchiveStore::load(root).unwrap().list().is_empty());
        let mut external = entry.clone();
        external.path = dir.path().join("external.epub").to_string_lossy().into();
        std::fs::write(&external.path, b"external").unwrap();
        assert!(unlink_owned(root, &external).is_err());
        assert!(Path::new(&external.path).exists());
    }
    #[test]
    fn merge_is_order_independent_and_legacy_snapshot_defaults_empty() {
        let (dir, mut library, entry, _) = setup();
        let root = dir.path().to_str().unwrap();
        ArchiveStore::archive(root, &mut library, &[entry.path]).unwrap();
        let old = ArchiveStore::load(root).unwrap().states();
        let hash = old.keys().next().unwrap().clone();
        ArchiveStore::change(root, &hash, BookState::Deleted).unwrap();
        let new = ArchiveStore::load(root).unwrap().states();
        let mut a = old.clone();
        merge_states(&mut a, &new);
        let mut b = new;
        merge_states(&mut b, &old);
        assert_eq!(a, b);
        let legacy: DeviceSnapshot = serde_json::from_value(
            serde_json::json!({"schema_version":1,"device_id":"legacy","books":{},"progress":{}}),
        )
        .unwrap();
        assert!(legacy.lifecycle.is_empty());
    }
    #[test]
    fn archived_files_restore_after_ios_sandbox_relocation() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("before");
        let new = dir.path().join("after");
        let mut library = Library::default();
        let entry = library
            .add_or_update_from_bytes(old.to_str().unwrap(), "Book".into(), b"relocation", 3, None)
            .unwrap();
        ArchiveStore::archive(old.to_str().unwrap(), &mut library, &[entry.path]).unwrap();
        let hash = crate::bytes_hash(b"relocation");
        std::fs::rename(&old, &new).unwrap();
        ArchiveStore::change(new.to_str().unwrap(), &hash, BookState::Active).unwrap();
        let restored = Library::load_from(new.to_str().unwrap())
            .unwrap()
            .books
            .remove(0);
        assert_eq!(restored.id, entry.id);
        assert!(Path::new(&restored.path).is_file());
        assert_eq!(restored.last_chapter, 3);
        assert_eq!(
            Library::read_book_config(new.to_str().unwrap(), &entry.id)
                .unwrap()
                .epub_path,
            restored.path
        );
    }
}
