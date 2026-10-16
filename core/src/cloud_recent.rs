//! Reading history is independent of locator revisions: reopening the same page
//! is an event; receiving another device's event is not.
use crate::{
    cloud_sync::{DeviceSnapshot, SyncJournal},
    library::Library,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReadingEvent {
    pub opened_ms: u64,
    pub device: String,
}

pub fn timestamp_millis(value: u64) -> u64 {
    if (1_000_000_000..100_000_000_000).contains(&value) {
        value.saturating_mul(1000)
    } else {
        value
    }
}

pub fn merge(target: &mut BTreeMap<String, ReadingEvent>, source: &BTreeMap<String, ReadingEvent>) {
    for (hash, event) in source {
        if target.get(hash).is_none_or(|old| event > old) {
            target.insert(hash.clone(), event.clone());
        }
    }
}

impl SyncJournal {
    /// Persist genuine local opens/reading saves even when cloud sync is disabled.
    /// Already-applied remote times are equal and keep their original author.
    pub fn capture_recent(&mut self, root: &str) {
        let Ok(library) = Library::read_snapshot(root) else {
            return;
        };
        for entry in library.books {
            let at = timestamp_millis(entry.last_opened);
            if at == 0 {
                continue;
            }
            let Some(hash) = Library::read_book_config(root, &entry.id).and_then(|c| c.file_hash)
            else {
                continue;
            };
            if self
                .own
                .recent_reading
                .get(&hash)
                .is_none_or(|old| at > old.opened_ms)
            {
                self.own.recent_reading.insert(
                    hash,
                    ReadingEvent {
                        opened_ms: at,
                        device: self.own.device_id.clone(),
                    },
                );
            }
        }
    }

    /// Apply after downloaded files have been installed. Unknown book events stay
    /// in the journal so the file phase/later retries can project them afterwards.
    pub fn apply_recent(
        &mut self,
        root: &str,
        library: &mut Library,
        snapshots: &[DeviceSnapshot],
    ) -> usize {
        for snapshot in snapshots {
            merge(&mut self.own.recent_reading, &snapshot.recent_reading);
        }
        let mut changes = BTreeMap::new();
        for entry in &library.books {
            let Some(hash) = Library::read_book_config(root, &entry.id).and_then(|c| c.file_hash)
            else {
                continue;
            };
            if let Some(event) = self.own.recent_reading.get(&hash) {
                if entry.last_opened != event.opened_ms {
                    changes.insert(entry.path.clone(), event.opened_ms);
                }
            }
        }
        let count = changes.len();
        library.apply_reading_times(root, &changes);
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn book(root: &str, title: &str) -> (String, String) {
        let mut library = Library::load_from(root);
        let entry = library.add_or_update_from_bytes(root, title.into(), title.as_bytes(), 0, None);
        (entry.path, crate::bytes_hash(title.as_bytes()))
    }
    #[test]
    fn legacy_unix_seconds_are_upgraded_only_once() {
        let value = serde_json::json!({"title":"Book","path":"book.epub","last_chapter":0,"last_opened":1_700_000_000u64});
        let entry: crate::library::BookEntry = serde_json::from_value(value).unwrap();
        assert_eq!(entry.last_opened, 1_700_000_000_000);
        let next: crate::library::BookEntry =
            serde_json::from_value(serde_json::to_value(entry).unwrap()).unwrap();
        assert_eq!(next.last_opened, 1_700_000_000_000);
    }
    #[test]
    fn import_and_remote_locator_do_not_create_reading_events() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let (path, _) = book(root, "Book");
        let mut lib = Library::load_from(root);
        assert_eq!(lib.books[0].last_opened, 0);
        lib.apply_position(root, &path, 4, None, 8, 1);
        assert_eq!(Library::load_from(root).books[0].last_opened, 0);
        let journal = SyncJournal::load(root, "phone");
        assert!(journal.own.recent_reading.is_empty());
    }
    #[test]
    fn same_page_reopen_syncs_without_changing_progress_revision_or_echoing() {
        let a = tempfile::tempdir().unwrap();
        let a = a.path().to_str().unwrap();
        let b = tempfile::tempdir().unwrap();
        let b = b.path().to_str().unwrap();
        let (pa, hash) = book(a, "Book");
        let (pb, _) = book(b, "Book");
        let mut la = Library::load_from(a);
        la.set_last_opened(a, &pa, 1_800_000_000_123);
        let mut ja = SyncJournal::load(a, "phone");
        let version = ja.own.progress_version(&hash);
        la.set_last_opened(a, &pa, 1_800_000_000_456);
        ja.capture_recent(a);
        ja.save(a).unwrap();
        assert_eq!(ja.own.progress_version(&hash), version);
        let mut jb = SyncJournal::load(b, "tablet");
        let mut lb = Library::load_from(b);
        assert_eq!(jb.apply_recent(b, &mut lb, &[ja.own.clone()]), 1);
        assert_eq!(lb.books[0].last_opened, 1_800_000_000_456);
        jb.save(b).unwrap();
        let jb = SyncJournal::load(b, "tablet");
        assert_eq!(jb.own.recent_reading[&hash].device, "phone");
        assert_eq!(jb.own.recent_reading[&hash], ja.own.recent_reading[&hash]);
        lb.apply_position(b, &pb, 3, None, 2, 0);
        assert_eq!(
            SyncJournal::load(b, "tablet").own.recent_reading[&hash],
            ja.own.recent_reading[&hash]
        );
    }
    #[test]
    fn convergence_is_independent_of_snapshot_order_and_download_order() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let mut one = DeviceSnapshot::new("one".into());
        let mut two = DeviceSnapshot::new("two".into());
        let older = crate::bytes_hash(b"Older");
        let newer = crate::bytes_hash(b"Newer");
        one.recent_reading.insert(
            older.clone(),
            ReadingEvent {
                opened_ms: 1_800_000_001_100,
                device: "one".into(),
            },
        );
        two.recent_reading.insert(
            newer.clone(),
            ReadingEvent {
                opened_ms: 1_800_000_001_900,
                device: "two".into(),
            },
        );
        let mut journal = SyncJournal::load(root, "desktop");
        journal.apply_recent(
            root,
            &mut Library::load_from(root),
            &[two.clone(), one.clone()],
        );
        book(root, "Newer");
        book(root, "Older");
        let mut lib = Library::load_from(root);
        journal.apply_recent(root, &mut lib, &[]);
        assert_eq!(lib.books[lib.sorted_indices_by_recent()[0]].title, "Newer");
        let mut reverse = SyncJournal::load(root, "third");
        reverse.own.recent_reading.clear();
        reverse.apply_recent(root, &mut lib, &[one, two]);
        assert_eq!(reverse.own.recent_reading, journal.own.recent_reading);
        let mut stale = DeviceSnapshot::new("stale".into());
        stale.recent_reading.insert(
            newer.clone(),
            ReadingEvent {
                opened_ms: 1,
                device: "stale".into(),
            },
        );
        journal.apply_recent(root, &mut lib, &[stale]);
        assert_eq!(
            journal.own.recent_reading[&newer].opened_ms,
            1_800_000_001_900
        );
    }
    #[test]
    fn optional_history_is_backward_compatible_validated_and_own_cas_safe() {
        let mut snapshot = DeviceSnapshot::new("phone".into());
        let hash = "a".repeat(64);
        snapshot.recent_reading.insert(
            hash.clone(),
            ReadingEvent {
                opened_ms: 123,
                device: "tablet".into(),
            },
        );
        snapshot.validate().unwrap();
        let mut own = DeviceSnapshot::new("phone".into());
        own.merge_own(&snapshot).unwrap();
        own.merge_own(&DeviceSnapshot::new("phone".into())).unwrap();
        assert_eq!(own.recent_reading[&hash].device, "tablet");
        let mut json = serde_json::to_value(&snapshot).unwrap();
        json.as_object_mut().unwrap().remove("recent_reading");
        let legacy: DeviceSnapshot = serde_json::from_value(json).unwrap();
        assert!(legacy.recent_reading.is_empty());
        snapshot.recent_reading.get_mut(&hash).unwrap().device = "../path".into();
        assert!(snapshot.validate().is_err());
    }
}
