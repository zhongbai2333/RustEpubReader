//! Latest actual reading edit wins. Applying a remote edit never creates a new edit.
use crate::{
    cloud_sync::{DeviceSnapshot, SyncJournal},
    sharing::ProgressEntry,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProgressVersion {
    pub modified_ms: u64,
    pub device: String,
    #[serde(default)]
    pub legacy: bool,
}
impl Ord for ProgressVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (!self.legacy, self.modified_ms, &self.device).cmp(&(
            !other.legacy,
            other.modified_ms,
            &other.device,
        ))
    }
}
impl PartialOrd for ProgressVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
#[derive(Clone, Debug)]
pub struct VersionedProgress {
    pub progress: ProgressEntry,
    pub version: ProgressVersion,
}
impl DeviceSnapshot {
    pub fn progress_version(&self, hash: &str) -> ProgressVersion {
        self.progress_versions
            .get(hash)
            .cloned()
            .unwrap_or_else(|| ProgressVersion {
                modified_ms: self.progress.get(hash).map_or(0, |p| {
                    // Legacy snapshots used seconds. Tolerate prior accidental millis.
                    if p.timestamp < 100_000_000_000 {
                        p.timestamp.saturating_mul(1000)
                    } else {
                        p.timestamp
                    }
                }),
                device: self.device_id.clone(),
                legacy: true,
            })
    }
}
pub fn latest_records(
    snapshots: &[DeviceSnapshot],
    own: &DeviceSnapshot,
) -> BTreeMap<String, VersionedProgress> {
    let mut result: BTreeMap<String, VersionedProgress> = BTreeMap::new();
    for snapshot in std::iter::once(own).chain(snapshots) {
        for (hash, progress) in &snapshot.progress {
            let version = snapshot.progress_version(hash);
            if result.get(hash).is_none_or(|old| version > old.version) {
                result.insert(
                    hash.clone(),
                    VersionedProgress {
                        progress: progress.clone(),
                        version,
                    },
                );
            }
        }
    }
    result
}
pub fn same_position(a: &ProgressEntry, b: &ProgressEntry) -> bool {
    (a.chapter, a.block, a.char_offset) == (b.chapter, b.block, b.char_offset)
}
impl SyncJournal {
    /// Install the winning record with its original author/time, including into our
    /// own snapshot. Re-uploading it is idempotent and cannot bounce it back newer.
    pub fn accept(&mut self, record: &VersionedProgress) {
        let hash = &record.progress.book_hash;
        if self
            .own
            .progress
            .get(hash)
            .is_none_or(|_| record.version >= self.own.progress_version(hash))
        {
            self.own
                .progress
                .insert(hash.clone(), record.progress.clone());
            self.own
                .progress_versions
                .insert(hash.clone(), record.version.clone());
            self.seen.insert(hash.clone());
        }
    }
    /// iOS saves are queued off the UI thread. Use the event's capture time,
    /// and reject an old queued edit after a newer remote revision was accepted.
    pub fn record_at(&mut self, progress: ProgressEntry, edited_ms: u64) -> bool {
        let hash = &progress.book_hash;
        if self
            .own
            .progress
            .get(hash)
            .is_some_and(|old| same_position(old, &progress))
        {
            return false;
        }
        let previous = self.own.progress_version(hash);
        let proposed = ProgressVersion {
            modified_ms: edited_ms,
            device: self.own.device_id.clone(),
            legacy: false,
        };
        if self.own.progress.contains_key(hash) && proposed < previous {
            return false;
        }
        self.own.progress_versions.insert(
            hash.clone(),
            ProgressVersion {
                modified_ms: if proposed == previous {
                    edited_ms.saturating_add(1)
                } else {
                    edited_ms
                },
                device: self.own.device_id.clone(),
                legacy: false,
            },
        );
        self.own.progress.insert(hash.clone(), progress.clone());
        self.seen.insert(hash.clone());
        true
    }
    pub fn seed_entry(&mut self, root: &str, entry: &crate::library::BookEntry) {
        if let Some(hash) =
            crate::library::Library::read_book_config(root, &entry.id).and_then(|c| c.file_hash)
        {
            self.seed(ProgressEntry {
                book_hash: hash,
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
    }
    /// Seed a pre-existing locator without treating open/import/reflow as reading.
    pub fn seed(&mut self, progress: ProgressEntry) {
        self.own
            .progress
            .entry(progress.book_hash.clone())
            .or_insert(progress);
    }
}
pub fn validate_position(book: &crate::epub::EpubBook, progress: &ProgressEntry) -> bool {
    let Some(chapter) = book.chapters.get(progress.chapter) else {
        return false;
    };
    if chapter.blocks.is_empty() {
        return progress.block == 0 && progress.char_offset == 0;
    }
    let Some(block) = chapter.blocks.get(progress.block) else {
        return false;
    };
    let chars = match block {
        crate::epub::ContentBlock::Heading { spans, .. }
        | crate::epub::ContentBlock::Paragraph { spans, .. } => {
            spans.iter().map(|s| s.text.chars().count()).sum()
        }
        _ => 0,
    };
    progress.char_offset <= chars
}
#[cfg(test)]
mod tests {
    use super::*;
    fn p(chapter: usize, time: u64) -> ProgressEntry {
        ProgressEntry {
            book_hash: "a".repeat(64),
            title: "Book".into(),
            chapter,
            block: 0,
            char_offset: 0,
            chapter_title: None,
            timestamp: time,
        }
    }
    #[test]
    fn three_devices_converge_with_backwards_reread_no_echo_and_order_independent_ties() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let mut a = SyncJournal::load(root, "A");
        let mut b = SyncJournal::load(root, "B");
        let mut c = SyncJournal::load(root, "C");
        a.record(p(15, 100));
        b.record(p(3, 101));
        c.record(p(7, 102));
        // Explicit fixture times, independent of test scheduling.
        for (j, ms) in [(&mut a, 100000), (&mut b, 101000), (&mut c, 102000)] {
            j.own
                .progress_versions
                .get_mut(&"a".repeat(64))
                .unwrap()
                .modified_ms = ms;
        }
        let snapshots = vec![a.own.clone(), b.own.clone(), c.own.clone()];
        for journal in [&mut a, &mut b, &mut c] {
            let winning = latest_records(&snapshots, &journal.own)
                .remove(&"a".repeat(64))
                .unwrap();
            journal.accept(&winning);
            assert_eq!(journal.own.progress[&"a".repeat(64)].chapter, 7);
            assert!(
                !journal.record(p(7, 999)),
                "remote apply/close must not manufacture a newer edit"
            );
            assert_eq!(
                journal.own.progress_version(&"a".repeat(64)),
                winning.version
            );
        }
        a.record(p(1, 103));
        let winner = latest_records(&[a.own.clone(), b.own.clone()], &c.own)
            .remove(&"a".repeat(64))
            .unwrap();
        assert_eq!(
            winner.progress.chapter, 1,
            "newer rereading wins over a further chapter"
        );
        let mut order = snapshots.clone();
        order.reverse();
        assert_eq!(
            latest_records(&snapshots, &a.own)[&"a".repeat(64)].version,
            latest_records(&order, &a.own)[&"a".repeat(64)].version
        );
    }
    #[test]
    fn equal_second_real_edits_have_order_and_live_edits_beat_inflight_response() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let mut a = SyncJournal::load(root, "A");
        a.record(p(2, 10));
        let stale = a.own.clone();
        a.record(p(3, 10));
        let old = stale.progress_version(&"a".repeat(64));
        let new = a.own.progress_version(&"a".repeat(64));
        assert!(new > old);
        a.own.merge_own(&stale).unwrap();
        assert_eq!(a.own.progress[&"a".repeat(64)].chapter, 3);
    }
    #[test]
    fn legacy_ties_are_deterministic_across_device_order() {
        let mut a = DeviceSnapshot::new("A".into());
        let mut b = DeviceSnapshot::new("B".into());
        a.progress.insert("a".repeat(64), p(2, 10));
        b.progress.insert("a".repeat(64), p(4, 10));
        assert_eq!(
            latest_records(&[a.clone()], &b)[&"a".repeat(64)]
                .progress
                .chapter,
            latest_records(&[b], &a)[&"a".repeat(64)].progress.chapter
        );
    }
    #[test]
    fn queued_reading_event_cannot_become_newer_just_because_it_saves_later() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let mut own = SyncJournal::load(root, "ios");
        own.seed(p(1, 10));
        let remote = VersionedProgress {
            progress: p(4, 20),
            version: ProgressVersion {
                modified_ms: 20500,
                device: "desktop".into(),
                legacy: false,
            },
        };
        own.accept(&remote);
        assert!(!own.record_at(p(2, 30), 20000));
        assert_eq!(own.own.progress[&"a".repeat(64)].chapter, 4);
        assert!(own.record_at(p(2, 30), 21000));
        assert_eq!(own.own.progress[&"a".repeat(64)].chapter, 2);
    }
    #[test]
    fn untouched_bootstrap_remains_valid_and_does_not_override_real_reading() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let mut own = SyncJournal::load(root, "new-phone");
        own.seed(p(0, 0));
        let baseline = latest_records(&[], &own.own)
            .remove(&"a".repeat(64))
            .unwrap();
        own.accept(&baseline);
        own.save(root).unwrap();
        assert!(!own.record(p(0, 100)));
        let mut read = DeviceSnapshot::new("desktop".into());
        read.progress.insert("a".repeat(64), p(7, 20));
        assert_eq!(
            latest_records(&[read], &own.own)[&"a".repeat(64)]
                .progress
                .chapter,
            7
        );
    }

    #[test]
    fn new_actual_edit_supersedes_inflated_legacy_seconds_without_inheriting_future_time() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let mut own = SyncJournal::load(root, "ios");
        let mut old = DeviceSnapshot::new("old-desktop".into());
        old.progress.insert("a".repeat(64), p(9, 9_999_999_999));
        let legacy = latest_records(&[old.clone()], &own.own)
            .remove(&"a".repeat(64))
            .unwrap();
        own.accept(&legacy);
        assert!(own.record_at(p(3, 20), 20500));
        assert_eq!(own.own.progress_version(&"a".repeat(64)).modified_ms, 20500);
        assert_eq!(
            latest_records(&[old], &own.own)[&"a".repeat(64)]
                .progress
                .chapter,
            3
        );
    }
}
