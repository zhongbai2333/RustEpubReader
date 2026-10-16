//! Portable GitHub sync format. Remote data never contains local paths or credentials.
use crate::sharing::ProgressEntry;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_BOOK_BYTES: u64 = 20 * 1024 * 1024;
/// Upper bound for chunk-backed documents; independent of legacy wire limits.
pub const MAX_DOCUMENT_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudBook {
    pub hash: String,
    pub title: String,
    pub size: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceSnapshot {
    pub schema_version: u32,
    pub device_id: String,
    pub books: BTreeMap<String, CloudBook>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub large_books: BTreeMap<String, CloudBook>,
    pub progress: BTreeMap<String, ProgressEntry>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub recent_reading: BTreeMap<String, crate::cloud_recent::ReadingEvent>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub progress_versions: BTreeMap<String, crate::cloud_progress::ProgressVersion>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub lifecycle: BTreeMap<String, crate::library::archive::BookLifecycle>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub activity: BTreeMap<String, crate::reading_activity::DeviceActivity>,
}

pub fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

pub fn valid_device_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

impl DeviceSnapshot {
    pub fn new(device_id: String) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            device_id,
            books: BTreeMap::new(),
            large_books: BTreeMap::new(),
            progress: BTreeMap::new(),
            recent_reading: BTreeMap::new(),
            progress_versions: BTreeMap::new(),
            lifecycle: BTreeMap::new(),
            activity: BTreeMap::new(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION
            || !valid_device_id(&self.device_id)
            || self.books.len().saturating_add(self.large_books.len()) > 5000
            || self.recent_reading.len() > 5000
            || self.progress.len() > 5000
            || self.progress_versions.len() > 5000
            || self.activity.len() > 5000
            || self.lifecycle.len() > 5000
        {
            return Err("Unsupported or invalid sync snapshot".into());
        }
        for (hash, book) in &self.books {
            if !valid_hash(hash)
                || hash != &book.hash
                || book.size > MAX_BOOK_BYTES
                || book.title.len() > 4096
            {
                return Err("Invalid remote book metadata".into());
            }
        }
        for (hash, book) in &self.large_books {
            if !valid_hash(hash)
                || hash != &book.hash
                || book.size <= MAX_BOOK_BYTES
                || book.size > MAX_DOCUMENT_BYTES
                || book.title.len() > 4096
                || self.books.contains_key(hash)
            {
                return Err("Invalid chunked book metadata".into());
            }
        }
        for (hash, event) in &self.recent_reading {
            if !valid_hash(hash) || !valid_device_id(&event.device) || event.opened_ms == 0 {
                return Err("Invalid recent reading event".into());
            }
        }
        for (hash, p) in &self.progress {
            if !valid_hash(hash)
                || hash != &p.book_hash
                || p.chapter > 100_000
                || p.block > 10_000_000
                || p.char_offset > 100_000_000
                || p.title.len() > 4096
                || p.chapter_title.as_ref().is_some_and(|s| s.len() > 4096)
            {
                return Err("Invalid remote progress".into());
            }
        }
        for (hash, version) in &self.progress_versions {
            if !self.progress.contains_key(hash) || !valid_device_id(&version.device) {
                return Err("Invalid progress revision".into());
            }
        }
        for (hash, state) in &self.lifecycle {
            if !state.valid(hash) {
                return Err("Invalid remote book lifecycle".into());
            }
        }
        for (hash, activity) in &self.activity {
            if !valid_hash(hash) || !activity.validate() {
                return Err("Invalid remote reading activity".into());
            }
        }
        Ok(())
    }

    /// A device writes only its own file; CAS retries preserve a newer own-device record.
    pub fn merge_own(&mut self, other: &Self) -> Result<(), String> {
        other.validate()?;
        if other.device_id != self.device_id {
            return Err("Device identity mismatch".into());
        }
        crate::library::archive::merge_states(&mut self.lifecycle, &other.lifecycle);
        for (hash, book) in &other.books {
            self.books
                .entry(hash.clone())
                .or_insert_with(|| book.clone());
        }
        for (hash, book) in &other.large_books {
            self.large_books
                .entry(hash.clone())
                .or_insert_with(|| book.clone());
        }
        for (hash, p) in &other.progress {
            let version = other.progress_version(hash);
            if self
                .progress
                .get(hash)
                .is_none_or(|_| version > self.progress_version(hash))
            {
                self.progress.insert(hash.clone(), p.clone());
                if let Some(explicit) = other.progress_versions.get(hash) {
                    self.progress_versions
                        .insert(hash.clone(), explicit.clone());
                } else {
                    self.progress_versions.remove(hash);
                }
            }
        }
        crate::cloud_recent::merge(&mut self.recent_reading, &other.recent_reading);
        for (hash, activity) in &other.activity {
            self.activity
                .entry(hash.clone())
                .or_default()
                .merge(activity);
        }
        Ok(())
    }
}

/// Preserve every device snapshot; timestamp selects a suggestion, never the furthest page.
pub fn latest_progress(
    snapshots: &[DeviceSnapshot],
    own: &DeviceSnapshot,
) -> BTreeMap<String, ProgressEntry> {
    crate::cloud_progress::latest_records(snapshots, own)
        .into_iter()
        .map(|(hash, record)| (hash, record.progress))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn progress(chapter: usize, timestamp: u64) -> ProgressEntry {
        ProgressEntry {
            book_hash: "a".repeat(64),
            title: "Book".into(),
            chapter,
            block: 3,
            char_offset: 2,
            chapter_title: None,
            timestamp,
        }
    }
    #[test]
    fn latest_can_move_backwards_and_equal_time_is_device_deterministic() {
        let mut own = DeviceSnapshot::new("local".into());
        let p = progress(10, 20);
        own.progress.insert(p.book_hash.clone(), p);
        let mut remote = DeviceSnapshot::new("remote".into());
        let p = progress(2, 30);
        remote.progress.insert(p.book_hash.clone(), p);
        assert_eq!(
            latest_progress(&[remote.clone()], &own)[&"a".repeat(64)].chapter,
            2
        );
        remote.progress.get_mut(&"a".repeat(64)).unwrap().timestamp = 20;
        assert_eq!(latest_progress(&[remote], &own)[&"a".repeat(64)].chapter, 2);
    }
    #[test]
    fn rejects_paths_large_books_and_unknown_versions() {
        assert!(!valid_hash("../../secret"));
        assert!(!valid_device_id("../device"));
        let mut s = DeviceSnapshot::new("device".into());
        s.books.insert(
            "a".repeat(64),
            CloudBook {
                hash: "a".repeat(64),
                title: "Book".into(),
                size: MAX_BOOK_BYTES + 1,
            },
        );
        assert!(s.validate().is_err());
        s.books.clear();
        let mut p = progress(usize::MAX, 1);
        s.progress.insert(p.book_hash.clone(), p.clone());
        assert!(s.validate().is_err());
        p.chapter = 0;
        p.block = usize::MAX;
        s.progress.insert(p.book_hash.clone(), p);
        assert!(s.validate().is_err());
        s.progress.clear();
        s.schema_version = 2;
        assert!(s.validate().is_err());
    }
    #[test]
    fn own_merge_preserves_newer_progress_and_rejects_other_device() {
        let mut a = DeviceSnapshot::new("a".into());
        let p = progress(3, 30);
        a.progress.insert(p.book_hash.clone(), p);
        let mut b = DeviceSnapshot::new("a".into());
        let p = progress(8, 20);
        b.progress.insert(p.book_hash.clone(), p);
        a.merge_own(&b).unwrap();
        assert_eq!(a.progress[&"a".repeat(64)].chapter, 3);
        b.device_id = "b".into();
        assert!(a.merge_own(&b).is_err());
    }
    #[test]
    fn large_files_are_optional_metadata_and_do_not_break_v1_progress_readers() {
        let mut state = DeviceSnapshot::new("device".into());
        let p = progress(2, 10);
        state.progress.insert(p.book_hash.clone(), p);
        let book = CloudBook {
            hash: "a".repeat(64),
            title: "Large".into(),
            size: 21 * 1024 * 1024,
        };
        state.large_books.insert(book.hash.clone(), book.clone());
        state.validate().unwrap();
        #[derive(Deserialize)]
        struct Legacy {
            schema_version: u32,
            books: BTreeMap<String, CloudBook>,
            progress: BTreeMap<String, ProgressEntry>,
        }
        let legacy: Legacy = serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
        assert_eq!(legacy.schema_version, 1);
        assert!(legacy.books.is_empty());
        assert_eq!(legacy.progress.len(), 1);
        let mut merged = DeviceSnapshot::new("device".into());
        merged.merge_own(&state).unwrap();
        assert_eq!(merged.large_books[&book.hash], book);
        state.books.insert(book.hash.clone(), book);
        assert!(state.validate().is_err());
    }
    #[test]
    fn journal_round_trips_and_does_not_restore_another_device_identity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_str().unwrap();
        let mut journal = SyncJournal::load(path, "desktop");
        assert!(journal.record(progress(4, 20)));
        assert!(!journal.record(progress(4, 30)));
        journal.save(path).unwrap();
        let restored = SyncJournal::load(path, "desktop");
        assert_eq!(restored.own.progress[&"a".repeat(64)].timestamp, 20);
        assert!(SyncJournal::load(path, "new-phone").own.progress.is_empty());
    }
    #[test]
    fn same_second_reread_gets_a_newer_journal_revision() {
        let dir = tempfile::tempdir().unwrap();
        let mut journal = SyncJournal::load(dir.path().to_str().unwrap(), "android");
        journal.record(progress(10, 20));
        journal.record(progress(2, 20));
        assert_eq!(journal.own.progress[&"a".repeat(64)].chapter, 2);
        assert_eq!(journal.own.progress[&"a".repeat(64)].timestamp, 20);
        assert!(journal.own.progress_version(&"a".repeat(64)).modified_ms > 20_000);
    }
}

/// Device-local journal shared by desktop and mobile adapters. Never contains a token.
#[derive(Serialize, Deserialize)]
pub struct SyncJournal {
    pub own: DeviceSnapshot,
    pub seen: std::collections::BTreeSet<String>,
    pub applied: BTreeMap<String, u64>,
}
impl SyncJournal {
    pub fn load(data_dir: &str, device_id: &str) -> Self {
        use std::io::Read;
        let path = std::path::Path::new(data_dir).join("github-sync-local.json");
        let loaded = std::fs::File::open(path)
            .ok()
            .and_then(|file| {
                let mut bytes = Vec::new();
                file.take(4 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .ok()?;
                if bytes.len() > 4 * 1024 * 1024 {
                    return None;
                }
                serde_json::from_slice::<Self>(&bytes).ok()
            })
            .filter(|j| j.own.device_id == device_id && j.own.validate().is_ok());
        let mut journal = loaded.unwrap_or_else(|| Self {
            own: DeviceSnapshot::new(device_id.into()),
            seen: Default::default(),
            applied: Default::default(),
        });
        if journal.own.progress.is_empty() {
            if let Ok(library) = crate::library::Library::read_snapshot(data_dir) {
                for entry in &library.books {
                    journal.seed_entry(data_dir, entry);
                }
            }
        }
        journal.capture_recent(data_dir);
        if let Ok(archive) = crate::library::archive::ArchiveStore::load(data_dir) {
            crate::library::archive::merge_states(&mut journal.own.lifecycle, &archive.states());
        }
        if let Ok(activity) = crate::reading_activity::export(data_dir, device_id) {
            for (hash, value) in activity {
                journal.own.activity.entry(hash).or_default().merge(&value);
            }
        }
        journal
    }
    pub fn save(&self, data_dir: &str) -> Result<(), String> {
        self.own.validate()?;
        let path = std::path::Path::new(data_dir).join("github-sync-local.json");
        let temporary = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec(self).map_err(|_| "Cannot encode local journal")?;
        std::fs::write(&temporary, bytes)
            .and_then(|_| std::fs::rename(&temporary, path))
            .map_err(|_| "Cannot save local sync journal".into())
    }
    pub fn record(&mut self, progress: ProgressEntry) -> bool {
        let hash = progress.book_hash.clone();
        if self
            .own
            .progress
            .get(&hash)
            .is_some_and(|old| crate::cloud_progress::same_position(old, &progress))
        {
            return false;
        }
        let wall = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let event_ms = if progress.timestamp < 100_000_000_000 {
            progress
                .timestamp
                .saturating_mul(1000)
                .saturating_add(wall % 1000)
        } else {
            progress.timestamp
        };
        let previous = self.own.progress_version(&hash);
        let modified_ms = if previous.legacy {
            event_ms
        } else {
            event_ms.max(previous.modified_ms.saturating_add(1))
        };
        self.own.progress_versions.insert(
            hash.clone(),
            crate::cloud_progress::ProgressVersion {
                modified_ms,
                device: self.own.device_id.clone(),
                legacy: false,
            },
        );
        self.seen.insert(hash.clone());
        self.own.progress.insert(hash, progress);
        true
    }
}
