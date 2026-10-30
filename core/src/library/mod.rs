//! Library management, book cataloging, and metadata parsing.
pub mod access;
pub mod archive;
pub mod categories;
pub mod covers;
mod types;
pub use types::*;

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::now_secs;

fn library_path(data_dir: &str) -> PathBuf {
    PathBuf::from(data_dir).join("library.json")
}

fn books_dir(data_dir: &str) -> PathBuf {
    PathBuf::from(data_dir).join("books")
}

fn epub_path_for(data_dir: &str, id: &str) -> PathBuf {
    books_dir(data_dir).join(format!("{id}.epub"))
}

fn config_path_for(data_dir: &str, id: &str) -> PathBuf {
    books_dir(data_dir).join(format!("{id}.json"))
}

fn is_uuid_like(s: &str) -> bool {
    Uuid::parse_str(s).is_ok()
}

fn generate_book_id() -> String {
    Uuid::new_v4().to_string()
}

fn derive_id_from_path(path: &str) -> Option<String> {
    let stem = Path::new(path).file_stem()?.to_str()?;
    if is_uuid_like(stem) {
        Some(stem.to_string())
    } else {
        None
    }
}

fn file_hash(path: &str) -> Option<String> {
    crate::file_hash(path).ok()
}

fn bytes_hash(data: &[u8]) -> String {
    crate::bytes_hash(data)
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct Library {
    pub books: Vec<BookEntry>,
    #[serde(default)]
    pub classification: categories::Classification,
    /// O(1) lookup: path → index in `books`. Rebuilt on load/mutation.
    #[serde(skip)]
    path_index: HashMap<String, usize>,
}

impl Library {
    // The root lease serializes mutations, but a previously loaded UI snapshot
    // can still be stale. Refresh before changing membership or locators.
    fn refresh_current(&mut self, data_dir: &str) -> Result<(), String> {
        if let Some(current) = crate::storage::load::<Self>(&library_path(data_dir))? {
            *self = current;
            self.rebuild_index();
        }
        Ok(())
    }
    /// Rebuild the path_index from the current books vec.
    fn rebuild_index(&mut self) {
        self.path_index.clear();
        for (i, b) in self.books.iter().enumerate() {
            self.path_index.insert(b.path.clone(), i);
        }
    }

    fn find_by_path(&self, path: &str) -> Option<usize> {
        self.path_index.get(path).copied()
    }
}

impl Library {
    /// Sharing reads a frozen catalog without running migration or rewriting configs.
    pub fn read_snapshot(data_dir: &str) -> Result<Self, String> {
        let _access = access::lock(data_dir)?;
        let data = match std::fs::read(library_path(data_dir)) {
            Ok(data) => data,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default())
            }
            Err(_) => return Err("Cannot read library catalog".into()),
        };
        let mut library: Self =
            serde_json::from_slice(&data).map_err(|_| "Cannot read library catalog")?;
        library.rebuild_index();
        Ok(library)
    }
    pub fn load_from(data_dir: &str) -> Result<Self, String> {
        let _access = access::lock(data_dir)?;
        let mut library: Self = crate::storage::load(&library_path(data_dir))?.unwrap_or_default();
        library.migrate_to_uuid_storage(data_dir)?;
        library.rebuild_index();
        Ok(library)
    }

    pub fn save_to(&self, data_dir: &str) -> Result<(), String> {
        let _access = access::lock(data_dir)?;
        crate::storage::save(&library_path(data_dir), self)
    }

    pub fn add_or_update(
        &mut self,
        data_dir: &str,
        title: String,
        source_path: String,
        chapter: usize,
        chapter_title: Option<String>,
    ) -> Result<BookEntry, String> {
        let _access = access::lock(data_dir)?;
        std::fs::create_dir_all(books_dir(data_dir))
            .map_err(|e| format!("Cannot create storage: {e}"))?;
        self.refresh_current(data_dir)?;
        let now = now_secs();

        let incoming_hash = file_hash(&source_path);
        let existing_idx = self.find_by_path(&source_path).or_else(|| {
            incoming_hash.as_ref().and_then(|h| {
                self.books
                    .iter()
                    .position(|b| file_hash(&b.path).as_ref() == Some(h))
            })
        });

        let mut id = existing_idx
            .and_then(|idx| {
                let existing = &self.books[idx];
                if existing.id.is_empty() {
                    derive_id_from_path(&existing.path)
                } else {
                    Some(existing.id.clone())
                }
            })
            .or_else(|| {
                derive_id_from_path(&source_path).filter(|id| {
                    Path::new(&source_path) == epub_path_for(data_dir, id)
                        && !self.books.iter().any(|entry| &entry.id == id)
                })
            })
            .unwrap_or_else(generate_book_id);
        if !is_uuid_like(&id) {
            id = generate_book_id();
        }

        let managed_epub = epub_path_for(data_dir, &id);
        let source = PathBuf::from(&source_path);
        let managed_epub_str = managed_epub.to_string_lossy().to_string();

        if source != managed_epub {
            crate::storage::copy_file(&source, &managed_epub)?;
        } else if !source.is_file() {
            return Err("Cannot read selected book".into());
        }

        let final_path = if managed_epub.exists() {
            managed_epub_str
        } else {
            source_path.clone()
        };

        if let Some(idx) = existing_idx {
            let entry = &mut self.books[idx];
            entry.id = id.clone();
            if entry.title.trim().is_empty() {
                entry.title = title;
            }
            entry.path = final_path;
            if entry.last_chapter == chapter && chapter_title.is_some() {
                entry.last_chapter_title = chapter_title;
            }
        } else {
            self.books.push(BookEntry {
                id,
                title,
                path: final_path,
                last_chapter: chapter,
                last_block: 0,
                last_char_offset: 0,
                last_chapter_title: chapter_title,
                last_opened: 0,
            });
        }

        let idx = self.books.len().saturating_sub(1);
        let idx = existing_idx.unwrap_or(idx);
        let entry = self.books[idx].clone();
        self.write_book_config(data_dir, &entry, Some(now))?;
        self.rebuild_index();
        self.save_to(data_dir)?;
        Ok(entry)
    }

    pub fn add_or_update_from_bytes(
        &mut self,
        data_dir: &str,
        title: String,
        bytes: &[u8],
        chapter: usize,
        chapter_title: Option<String>,
    ) -> Result<BookEntry, String> {
        let _access = access::lock(data_dir)?;
        std::fs::create_dir_all(books_dir(data_dir))
            .map_err(|e| format!("Cannot create storage: {e}"))?;
        self.refresh_current(data_dir)?;
        let now = now_secs();
        let incoming_hash = bytes_hash(bytes);

        let existing_idx = self.books.iter().position(|b| {
            file_hash(&b.path)
                .map(|h| h == incoming_hash)
                .unwrap_or(false)
        });

        let id = existing_idx
            .and_then(|idx| {
                let existing = &self.books[idx];
                if existing.id.is_empty() {
                    derive_id_from_path(&existing.path)
                } else {
                    Some(existing.id.clone())
                }
            })
            .unwrap_or_else(generate_book_id);

        let managed_epub = epub_path_for(data_dir, &id);
        crate::storage::replace(&managed_epub, bytes)?;

        if let Some(idx) = existing_idx {
            let entry = &mut self.books[idx];
            entry.id = id.clone();
            if entry.title.trim().is_empty() {
                entry.title = title;
            }
            entry.path = managed_epub.to_string_lossy().to_string();
            if entry.last_chapter == chapter && chapter_title.is_some() {
                entry.last_chapter_title = chapter_title;
            }
        } else {
            self.books.push(BookEntry {
                id,
                title,
                path: managed_epub.to_string_lossy().to_string(),
                last_chapter: chapter,
                last_block: 0,
                last_char_offset: 0,
                last_chapter_title: chapter_title,
                last_opened: 0,
            });
        }

        let idx = self.books.len().saturating_sub(1);
        let idx = existing_idx.unwrap_or(idx);
        let entry = self.books[idx].clone();
        self.write_book_config(data_dir, &entry, Some(now))?;
        self.rebuild_index();
        self.save_to(data_dir)?;
        Ok(entry)
    }

    pub fn hide_by_path(&mut self, data_dir: &str, path: &str) -> Result<(), String> {
        let _access = access::lock(data_dir)?;
        self.hide_paths(
            data_dir,
            &std::collections::HashSet::from([path.to_owned()]),
        )?;
        Ok(())
    }
    pub fn hide_paths(
        &mut self,
        data_dir: &str,
        paths: &std::collections::HashSet<String>,
    ) -> Result<(), String> {
        let _access = access::lock(data_dir)?;
        self.refresh_current(data_dir)?;
        let before = self.books.len();
        self.books.retain(|e| !paths.contains(&e.path));
        if self.books.len() != before {
            self.rebuild_index();
            self.save_to(data_dir)?;
        }
        Ok(())
    }
    pub fn restore_entry(&mut self, data_dir: &str, entry: BookEntry) -> Result<(), String> {
        let _access = access::lock(data_dir)?;
        self.refresh_current(data_dir)?;
        if !self.books.iter().any(|e| e.id == entry.id) {
            self.write_book_config(data_dir, &entry, None)?;
            self.books.push(entry);
        }
        self.rebuild_index();
        self.save_to(data_dir)?;
        Ok(())
    }

    pub fn remove(&mut self, data_dir: &str, idx: usize) -> Result<(), String> {
        let _access = access::lock(data_dir)?;
        let target = self.books.get(idx).map(|entry| entry.id.clone());
        self.refresh_current(data_dir)?;
        let Some(idx) = target.and_then(|id| self.books.iter().position(|entry| entry.id == id))
        else {
            return Ok(());
        };
        if idx < self.books.len() {
            let entry = self.books.remove(idx);
            self.rebuild_index();
            if let Err(error) = self.save_to(data_dir) {
                self.books.insert(idx, entry);
                self.rebuild_index();
                return Err(error);
            }
            self.remove_entry_files(data_dir, &entry);
        }
        Ok(())
    }

    /// Change the display title while retaining storage identity and reading data.
    pub fn rename_book(
        &mut self,
        data_dir: &str,
        id: &str,
        title: &str,
    ) -> Result<BookEntry, String> {
        let _access = access::lock(data_dir)?;
        self.refresh_current(data_dir)?;
        let title = title.trim();
        if title.is_empty() {
            return Err("Book name cannot be empty".into());
        }
        let idx = self
            .books
            .iter()
            .position(|e| e.id == id)
            .ok_or("Book is no longer in the library")?;
        let mut config =
            Self::read_book_config(data_dir, id).ok_or("Cannot read book configuration")?;
        let mut renamed = self.clone();
        renamed.books[idx].title = title.to_owned();
        config.title = title.to_owned();
        config.updated_at = now_secs();
        let config_path = config_path_for(data_dir, id);
        let old_config = std::fs::read(&config_path).map_err(|e| e.to_string())?;
        let result = (|| {
            crate::storage::save(&config_path, &config)?;
            if let Err(e) = renamed.save_to(data_dir) {
                let _ = write_rename_file(&config_path, &old_config);
                return Err(e.to_string());
            }
            Ok(())
        })();
        result?;
        *self = renamed;
        Ok(self.books[idx].clone())
    }

    /// Import verified share files as new identities. Existing content stays untouched.
    pub fn import_shared_books(
        &mut self,
        data_dir: &str,
        incoming: &[(String, String, PathBuf)],
    ) -> Result<(Vec<BookEntry>, usize), String> {
        let _access = access::lock(data_dir)?;
        self.import_shared_books_checked(data_dir, incoming, || Ok(()), |_| {})
    }
    pub fn import_shared_books_checked(
        &mut self,
        data_dir: &str,
        incoming: &[(String, String, PathBuf)],
        check: impl Fn() -> Result<(), String>,
        copied: impl Fn(u64),
    ) -> Result<(Vec<BookEntry>, usize), String> {
        let _access = access::lock(data_dir)?;
        self.refresh_current(data_dir)?;
        use sha2::{Digest, Sha256};
        use std::io::{Read, Write};
        let checked_hash = |path: &Path| -> Result<String, String> {
            let mut file =
                std::fs::File::open(path).map_err(|_| "Cannot read book for verification")?;
            let mut hasher = Sha256::new();
            let mut buffer = [0; 65536];
            loop {
                check()?;
                let n = file
                    .read(&mut buffer)
                    .map_err(|_| "Cannot read book for verification")?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
            }
            Ok(format!("{:x}", hasher.finalize()))
        };
        let mut known = HashSet::new();
        for entry in &self.books {
            check()?;
            if let Ok(hash) = checked_hash(Path::new(&entry.path)) {
                known.insert(hash);
            }
            check()?;
        }
        let mut next = self.clone();
        let mut created = Vec::new();
        let mut imported = Vec::new();
        let mut skipped = 0;
        std::fs::create_dir_all(books_dir(data_dir))
            .map_err(|_| "Cannot create library storage")?;
        let result = (|| -> Result<(), String> {
            for (title, hash, source) in incoming {
                check()?;
                if !crate::cloud_sync::valid_hash(hash) || &checked_hash(source)? != hash {
                    return Err("Received book checksum does not match".into());
                }
                if !known.insert(hash.clone()) {
                    skipped += 1;
                    continue;
                }
                let id = generate_book_id();
                let path = epub_path_for(data_dir, &id);
                let mut input =
                    std::fs::File::open(source).map_err(|_| "Cannot read received book")?;
                let mut output = std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&path)
                    .map_err(|_| "Cannot import received book")?;
                created.push((path.clone(), config_path_for(data_dir, &id)));
                let mut buffer = [0; 65536];
                loop {
                    check()?;
                    let n = input
                        .read(&mut buffer)
                        .map_err(|_| "Cannot import received book")?;
                    if n == 0 {
                        break;
                    }
                    output
                        .write_all(&buffer[..n])
                        .map_err(|_| "Cannot import received book")?;
                    copied(n as u64);
                }
                output
                    .sync_all()
                    .map_err(|_| "Cannot import received book")?;
                check()?;
                let entry = BookEntry {
                    id,
                    title: title.clone(),
                    path: path.to_string_lossy().into_owned(),
                    last_chapter: 0,
                    last_block: 0,
                    last_char_offset: 0,
                    last_chapter_title: None,
                    last_opened: 0,
                };
                next.books.push(entry.clone());
                next.write_book_config(data_dir, &entry, None)?;
                check()?;
                let cfg = Self::read_book_config(data_dir, &entry.id)
                    .ok_or("Cannot save received book configuration")?;
                if cfg.file_hash.as_ref() != Some(hash) {
                    return Err("Cannot verify imported book".into());
                }
                imported.push(entry);
            }
            check()?;
            next.save_to(data_dir)
        })();
        if let Err(error) = result {
            for (epub, config) in created {
                let _ = std::fs::remove_file(epub);
                let _ = std::fs::remove_file(crate::storage::backup_path(&config));
                let _ = std::fs::remove_file(config);
            }
            return Err(error);
        }
        next.rebuild_index();
        *self = next;
        Ok((imported, skipped))
    }

    pub fn remove_by_path(&mut self, data_dir: &str, path: &str) -> Result<(), String> {
        let _access = access::lock(data_dir)?;
        self.refresh_current(data_dir)?;
        if let Some(idx) = self.find_by_path(path) {
            let entry = self.books.remove(idx);
            self.rebuild_index();
            if let Err(error) = self.save_to(data_dir) {
                self.books.insert(idx, entry);
                self.rebuild_index();
                return Err(error);
            }
            self.remove_entry_files(data_dir, &entry);
        }
        Ok(())
    }

    pub fn update_chapter(
        &mut self,
        data_dir: &str,
        path: &str,
        chapter: usize,
        chapter_title: Option<String>,
    ) -> Result<(), String> {
        let _access = access::lock(data_dir)?;
        self.update_position(data_dir, path, chapter, chapter_title, 0, 0)?;
        Ok(())
    }

    pub fn update_position(
        &mut self,
        data_dir: &str,
        path: &str,
        chapter: usize,
        chapter_title: Option<String>,
        block: usize,
        char_offset: usize,
    ) -> Result<(), String> {
        let _access = access::lock(data_dir)?;
        self.refresh_current(data_dir)?;
        let changed = self.find_by_path(path).is_some_and(|i| {
            let e = &self.books[i];
            (e.last_chapter, e.last_block, e.last_char_offset) != (chapter, block, char_offset)
        });
        let edited_ms = crate::now_millis();
        if changed {
            if let Some(idx) = self.find_by_path(path) {
                self.books[idx].last_opened = self.books[idx].last_opened.max(edited_ms);
            }
        }
        self.apply_current_position(
            data_dir,
            path,
            chapter,
            chapter_title,
            block,
            char_offset,
            changed.then(|| crate::cloud_progress::ProgressVersion {
                modified_ms: edited_ms,
                device: String::new(),
                legacy: false,
            }),
        )?;
        Ok(())
    }

    /// Applying a locator from another device must not create a reading event.
    pub fn apply_position(
        &mut self,
        data_dir: &str,
        path: &str,
        chapter: usize,
        chapter_title: Option<String>,
        block: usize,
        char_offset: usize,
    ) -> Result<(), String> {
        let _access = access::lock(data_dir)?;
        self.refresh_current(data_dir)?;
        self.apply_current_position(
            data_dir,
            path,
            chapter,
            chapter_title,
            block,
            char_offset,
            None,
        )
    }
    /// Apply a selected reading edit with its captured clock, never a sync/save clock.
    /// Late queued edits cannot replace a newer persisted locator revision.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_position_at(
        &mut self,
        data_dir: &str,
        path: &str,
        chapter: usize,
        chapter_title: Option<String>,
        block: usize,
        char_offset: usize,
        modified_ms: u64,
    ) -> Result<bool, String> {
        let record = crate::cloud_progress::VersionedProgress {
            progress: crate::sharing::ProgressEntry {
                book_hash: String::new(),
                title: String::new(),
                chapter,
                block,
                char_offset,
                chapter_title,
                timestamp: modified_ms / 1000,
            },
            version: crate::cloud_progress::ProgressVersion {
                modified_ms,
                device: String::new(),
                legacy: false,
            },
        };
        self.apply_versioned_position(data_dir, path, &record)
    }

    /// Publish a selected edit without changing its original author or precision.
    /// Membership and version are rechecked under the same root lease as the write.
    pub fn apply_versioned_position(
        &mut self,
        data_dir: &str,
        path: &str,
        record: &crate::cloud_progress::VersionedProgress,
    ) -> Result<bool, String> {
        let _access = access::lock(data_dir)?;
        self.refresh_current(data_dir)?;
        let Some(index) = self.find_by_path(path) else {
            return Ok(false);
        };
        let stored = Self::load_book_config(data_dir, &self.books[index].id)?;
        if stored
            .as_ref()
            .and_then(BookConfig::reading_version)
            .is_some_and(|version| version > record.version)
        {
            return Ok(false);
        }
        let p = &record.progress;
        let entry = &self.books[index];
        if stored.as_ref().is_some_and(|config| {
            config.reading_version().as_ref() == Some(&record.version)
                && (
                    config.last_chapter,
                    config.last_block,
                    config.last_char_offset,
                ) == (p.chapter, p.block, p.char_offset)
        }) && (entry.last_chapter, entry.last_block, entry.last_char_offset)
            == (p.chapter, p.block, p.char_offset)
            && p.chapter_title
                .as_ref()
                .is_none_or(|title| entry.last_chapter_title.as_ref() == Some(title))
        {
            return Ok(true);
        }
        self.books[index].last_opened = self.books[index]
            .last_opened
            .max(record.version.modified_ms);
        self.apply_current_position(
            data_dir,
            path,
            p.chapter,
            p.chapter_title.clone(),
            p.block,
            p.char_offset,
            Some(record.version.clone()),
        )?;
        Ok(true)
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_current_position(
        &mut self,
        data_dir: &str,
        path: &str,
        chapter: usize,
        chapter_title: Option<String>,
        block: usize,
        char_offset: usize,
        version: Option<crate::cloud_progress::ProgressVersion>,
    ) -> Result<(), String> {
        if let Some(idx) = self.find_by_path(path) {
            let entry = &mut self.books[idx];
            entry.last_chapter = chapter;
            entry.last_block = block;
            entry.last_char_offset = char_offset;
            if chapter_title.is_some() {
                entry.last_chapter_title = chapter_title;
            }
            let entry_snapshot = entry.clone();
            self.write_book_config_with_clock(data_dir, &entry_snapshot, None, version)?;
            self.save_to(data_dir)?;
        }
        Ok(())
    }

    /// The caller supplies the originating reading time, never the sync time.
    pub fn set_last_opened(
        &mut self,
        data_dir: &str,
        path: &str,
        at_ms: u64,
    ) -> Result<(), String> {
        let _access = access::lock(data_dir)?;
        self.refresh_current(data_dir)?;
        if let Some(idx) = self.find_by_path(path) {
            if self.books[idx].last_opened >= at_ms {
                return Ok(());
            }
            self.books[idx].last_opened = at_ms;
            self.write_book_config(data_dir, &self.books[idx], None)?;
            self.save_to(data_dir)?;
        }
        Ok(())
    }

    /// Batch remote history projection: write the catalog once, not once per book.
    pub fn apply_reading_times(
        &mut self,
        data_dir: &str,
        times: &std::collections::BTreeMap<String, u64>,
    ) -> Result<(), String> {
        let _access = access::lock(data_dir)?;
        self.refresh_current(data_dir)?;
        let mut changed = false;
        for (path, at) in times {
            if let Some(idx) = self.find_by_path(path) {
                if self.books[idx].last_opened < *at {
                    self.books[idx].last_opened = *at;
                    self.write_book_config(data_dir, &self.books[idx], None)?;
                    changed = true;
                }
            }
        }
        if changed {
            self.save_to(data_dir)?;
        }
        Ok(())
    }

    pub fn sorted_indices_by_recent(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..self.books.len()).collect();
        indices.sort_by(|&a, &b| {
            self.books[b]
                .last_opened
                .cmp(&self.books[a].last_opened)
                .then_with(|| self.books[a].title.cmp(&self.books[b].title))
        });
        indices
    }

    pub fn read_book_config(data_dir: &str, id: &str) -> Option<BookConfig> {
        Self::load_book_config(data_dir, id).ok().flatten()
    }

    pub fn load_book_config(data_dir: &str, id: &str) -> Result<Option<BookConfig>, String> {
        let _access = access::lock(data_dir)?;
        crate::annotations::load_config(data_dir, id)
    }

    fn migrate_to_uuid_storage(&mut self, data_dir: &str) -> Result<(), String> {
        let mut changed = false;
        std::fs::create_dir_all(books_dir(data_dir))
            .map_err(|e| format!("Cannot create storage: {e}"))?;

        let mut seen_ids: HashSet<String> = HashSet::new();
        let mut seen_paths: HashSet<String> = HashSet::new();
        let mut migrated = Vec::new();

        for mut entry in std::mem::take(&mut self.books) {
            let original = entry.clone();
            let mut id = if !entry.id.is_empty() && is_uuid_like(&entry.id) {
                entry.id.clone()
            } else {
                derive_id_from_path(&entry.path).unwrap_or_else(generate_book_id)
            };
            let current = PathBuf::from(&entry.path);
            // An old catalog is not authority to overwrite an orphaned managed
            // book. Allocate another identity when those bytes differ.
            loop {
                let destination = epub_path_for(data_dir, &id);
                let occupied_by_other_bytes = current != destination
                    && current.is_file()
                    && destination.is_file()
                    && crate::file_hash(&current.to_string_lossy())?
                        != crate::file_hash(&destination.to_string_lossy())?;
                if !seen_ids.contains(&id) && !occupied_by_other_bytes {
                    break;
                }
                id = generate_book_id();
            }
            seen_ids.insert(id.clone());
            if id != entry.id {
                entry.id = id.clone();
                changed = true;
            }
            let managed_epub = epub_path_for(data_dir, &id);
            if current != managed_epub {
                if current.exists() {
                    crate::storage::copy_file(&current, &managed_epub)?;
                }
                if managed_epub.exists() {
                    entry.path = managed_epub.to_string_lossy().to_string();
                    changed = true;
                }
            }

            if !PathBuf::from(&entry.path).exists() {
                changed = true;
                continue;
            }

            if !seen_paths.insert(entry.path.clone()) {
                changed = true;
                continue;
            }

            if original.id != entry.id
                || original.path != entry.path
                || original.last_opened != entry.last_opened
                || !config_path_for(data_dir, &entry.id).exists()
            {
                self.write_book_config(data_dir, &entry, None)?;
            }
            migrated.push(entry);
        }

        self.books = migrated;
        if changed {
            self.save_to(data_dir)?;
        }
        Ok(())
    }

    fn write_book_config(
        &self,
        data_dir: &str,
        entry: &BookEntry,
        created_at_hint: Option<u64>,
    ) -> Result<(), String> {
        self.write_book_config_with_clock(data_dir, entry, created_at_hint, None)
    }
    fn write_book_config_with_clock(
        &self,
        data_dir: &str,
        entry: &BookEntry,
        created_at_hint: Option<u64>,
        version: Option<crate::cloud_progress::ProgressVersion>,
    ) -> Result<(), String> {
        std::fs::create_dir_all(books_dir(data_dir))
            .map_err(|e| format!("Cannot create storage: {e}"))?;

        let existing = Self::load_book_config(data_dir, &entry.id)?;

        let created_at = existing
            .as_ref()
            .map(|c| c.created_at)
            .or(created_at_hint)
            .unwrap_or_else(now_secs);
        let settings = existing
            .as_ref()
            .map(|c| c.settings.clone())
            .unwrap_or_default();

        // Reuse cached metadata/hash if the epub path hasn't changed
        let (cached_metadata, cached_hash) = existing
            .as_ref()
            .filter(|c| c.epub_path == entry.path)
            .map(|c| (c.metadata.clone(), c.file_hash.clone()))
            .unwrap_or((None, None));

        let file_hash = cached_hash.or_else(|| file_hash(&entry.path));
        let metadata =
            cached_metadata.or_else(|| crate::epub::EpubBook::read_metadata(&entry.path));

        // Preserve annotation data from existing config
        let bookmarks = existing
            .as_ref()
            .map(|c| c.bookmarks.clone())
            .unwrap_or_default();
        let highlights = existing
            .as_ref()
            .map(|c| c.highlights.clone())
            .unwrap_or_default();
        let notes = existing
            .as_ref()
            .map(|c| c.notes.clone())
            .unwrap_or_default();
        let corrections = existing
            .as_ref()
            .map(|c| c.corrections.clone())
            .unwrap_or_default();
        let reading_stats = existing.as_ref().and_then(|c| c.reading_stats.clone());

        let mut cfg = BookConfig {
            annotation_state: existing
                .as_ref()
                .map(|c| c.annotation_state.clone())
                .unwrap_or_default(),
            persisted: existing.as_ref().and_then(|c| c.persisted.clone()),
            id: entry.id.clone(),
            title: entry.title.clone(),
            epub_path: entry.path.clone(),
            last_chapter: entry.last_chapter,
            last_block: entry.last_block,
            last_char_offset: entry.last_char_offset,
            last_chapter_title: entry.last_chapter_title.clone(),
            last_opened: entry.last_opened,
            position_modified_ms: version.as_ref().map(|v| v.modified_ms).unwrap_or_else(|| {
                existing
                    .as_ref()
                    .map_or(0, |config| config.position_modified_ms)
            }),
            position_version: version
                .or_else(|| existing.as_ref().and_then(|c| c.position_version.clone())),
            created_at,
            updated_at: now_secs(),
            settings,
            file_hash,
            metadata,
            bookmarks,
            highlights,
            notes,
            corrections,
            reading_stats,
            last_contribute_prompt_count: existing
                .as_ref()
                .map_or(0, |c| c.last_contribute_prompt_count),
        };

        cfg.save(data_dir)
    }

    fn remove_entry_files(&self, data_dir: &str, entry: &BookEntry) {
        let epub_path = PathBuf::from(&entry.path);
        if let Err(e) = std::fs::remove_file(&epub_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!("[Library] failed to remove epub {:?}: {}", epub_path, e);
            }
        }

        let id = if !entry.id.is_empty() {
            entry.id.clone()
        } else {
            derive_id_from_path(&entry.path).unwrap_or_default()
        };

        if !id.is_empty() {
            let config = config_path_for(data_dir, &id);
            let _ = std::fs::remove_file(crate::storage::backup_path(&config));
            let _ = std::fs::remove_file(config);
            let managed_epub = epub_path_for(data_dir, &id);
            if managed_epub != epub_path {
                let _ = std::fs::remove_file(managed_epub);
            }
        }
    }
}

fn write_rename_file(path: &Path, data: &[u8]) -> Result<(), String> {
    crate::storage::replace(path, data)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_entry(id: &str, path: &Path) -> BookEntry {
        BookEntry {
            id: id.into(),
            title: "Legacy book".into(),
            path: path.to_string_lossy().into(),
            last_chapter: 0,
            last_block: 0,
            last_char_offset: 0,
            last_chapter_title: None,
            last_opened: 0,
        }
    }
    #[test]
    fn migration_preserves_an_existing_managed_file_with_different_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let id = "f55b0ce4-7b45-4e58-83f1-164dfb8b19ee";
        std::fs::create_dir_all(books_dir(root)).unwrap();
        let external = dir.path().join("legacy.epub");
        std::fs::write(&external, b"legacy bytes").unwrap();
        let occupied = epub_path_for(root, id);
        std::fs::write(&occupied, b"owned bytes").unwrap();
        let mut library = Library::default();
        library.books.push(legacy_entry(id, &external));
        library.save_to(root).unwrap();
        let migrated = Library::load_from(root).unwrap();
        assert_eq!(migrated.books.len(), 1);
        assert_ne!(migrated.books[0].id, id);
        assert_eq!(std::fs::read(&occupied).unwrap(), b"owned bytes");
        assert_eq!(
            std::fs::read(&migrated.books[0].path).unwrap(),
            b"legacy bytes"
        );
        assert_eq!(std::fs::read(&external).unwrap(), b"legacy bytes");
        assert_eq!(
            Library::load_from(root).unwrap().books[0].id,
            migrated.books[0].id
        );
    }
    #[test]
    fn migration_copy_failure_is_returned_and_catalog_is_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let id = "ab09de94-45b4-48f6-9ad8-7c1a13400df5";
        std::fs::create_dir_all(books_dir(root)).unwrap();
        let external = dir.path().join("legacy.epub");
        std::fs::write(&external, b"retained bytes").unwrap();
        // A directory at the destination makes atomic publication fail without
        // relying on platform-specific permissions or exhausting the device.
        let destination = epub_path_for(root, id);
        std::fs::create_dir(&destination).unwrap();
        let mut library = Library::default();
        library.books.push(legacy_entry(id, &external));
        library.save_to(root).unwrap();
        let before = std::fs::read(library_path(root)).unwrap();
        assert!(Library::load_from(root).is_err());
        assert_eq!(std::fs::read(library_path(root)).unwrap(), before);
        assert_eq!(std::fs::read(&external).unwrap(), b"retained bytes");
        assert!(destination.is_dir());
        assert!(!config_path_for(root, id).exists());
    }

    #[test]
    fn external_uuid_filename_cannot_overwrite_another_owned_book() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let mut library = Library::load_from(root).unwrap();
        let original = library
            .add_or_update_from_bytes(root, "Original".into(), b"original bytes", 0, None)
            .unwrap();
        let incoming = dir.path().join(format!("{}.epub", original.id));
        std::fs::write(&incoming, b"different bytes").unwrap();
        let added = library
            .add_or_update(
                root,
                "Incoming".into(),
                incoming.to_string_lossy().into_owned(),
                0,
                None,
            )
            .unwrap();
        assert_ne!(original.id, added.id);
        assert_eq!(std::fs::read(&original.path).unwrap(), b"original bytes");
        assert_eq!(std::fs::read(added.path).unwrap(), b"different bytes");
        assert_eq!(Library::load_from(root).unwrap().books.len(), 2);
    }

    #[test]
    fn corrupt_catalog_recovers_annotations_and_never_becomes_an_empty_shelf() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let mut library = Library::load_from(root).unwrap();
        let entry = library
            .add_or_update_from_bytes(root, "Saved".into(), b"owned book", 0, None)
            .unwrap();
        let mut config = Library::load_book_config(root, &entry.id).unwrap().unwrap();
        config.bookmarks.push(Bookmark {
            chapter: 0,
            block: 1,
            created_at: 1,
        });
        config.save(root).unwrap();
        config.notes.push(Note {
            highlight_id: "saved".into(),
            content: "retained".into(),
            created_at: 1,
            updated_at: 1,
        });
        config.save(root).unwrap();
        std::fs::write(library_path(root), b"interrupted catalog").unwrap();
        std::fs::write(config_path_for(root, &entry.id), b"interrupted annotations").unwrap();
        let restored = Library::load_from(root).unwrap();
        assert_eq!(restored.books.len(), 1);
        let restored = Library::load_book_config(root, &entry.id).unwrap().unwrap();
        assert_eq!(restored.bookmarks.len(), 1);
        assert!(restored.notes.is_empty()); // The last complete backup precedes the final edit.
        assert_eq!(std::fs::read(entry.path).unwrap(), b"owned book");
    }

    #[test]
    fn unrecoverable_catalog_refuses_import_without_writing_owned_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        std::fs::write(library_path(root), b"damaged").unwrap();
        assert!(Library::load_from(root).is_err());
        let mut library = Library::default();
        assert!(library
            .add_or_update_from_bytes(root, "New".into(), b"book", 0, None)
            .is_err());
        assert!(library.books.is_empty());
        assert_eq!(std::fs::read(library_path(root)).unwrap(), b"damaged");
        assert_eq!(std::fs::read_dir(books_dir(root)).unwrap().count(), 0);
    }

    #[test]
    fn failed_catalog_save_cannot_delete_a_book_or_its_annotations() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let mut library = Library::load_from(root).unwrap();
        let entry = library
            .add_or_update_from_bytes(root, "Saved".into(), b"owned book", 0, None)
            .unwrap();
        std::fs::remove_file(crate::storage::backup_path(&library_path(root))).unwrap();
        std::fs::create_dir(crate::storage::backup_path(&library_path(root))).unwrap();
        assert!(library.remove(root, 0).is_err());
        assert_eq!(library.books.len(), 1);
        assert!(config_path_for(root, &entry.id).is_file());
        assert_eq!(std::fs::read(entry.path).unwrap(), b"owned book");
    }

    fn test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "rust_epub_reader_{name}_{}_{}",
            std::process::id(),
            now_secs()
        ))
    }

    #[test]
    fn old_library_json_defaults_locator_to_chapter_start() {
        let json = r#"{
            "books": [{
                "id": "",
                "title": "Legacy",
                "path": "legacy.epub",
                "last_chapter": 7,
                "last_opened": 1
            }]
        }"#;
        let library: Library = serde_json::from_str(json).unwrap();
        assert_eq!(library.books[0].last_chapter, 7);
        assert_eq!(library.books[0].last_block, 0);
        assert_eq!(library.books[0].last_char_offset, 0);
    }

    #[test]
    fn rename_survives_reopen_and_retains_identity_and_reading_data() {
        let dir = test_dir("rename_round_trip");
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source.epub");
        std::fs::write(&source, b"unchanged epub contents").unwrap();
        let root = dir.to_string_lossy().to_string();
        let mut library = Library::default();
        let entry = library
            .add_or_update(
                &root,
                "Original".into(),
                source.to_string_lossy().into(),
                2,
                None,
            )
            .unwrap();
        library
            .update_position(&root, &entry.path, 2, Some("Chapter".into()), 9, 3)
            .unwrap();
        let before =
            serde_json::to_value(Library::read_book_config(&root, &entry.id).unwrap()).unwrap();
        let renamed = library
            .rename_book(&root, &entry.id, "  中文 Book 📚  ")
            .unwrap();
        assert_eq!(renamed.id, entry.id);
        assert_eq!(renamed.path, entry.path);
        assert_eq!(renamed.last_block, 9);
        assert_eq!(renamed.last_char_offset, 3);
        let mut after =
            serde_json::to_value(Library::read_book_config(&root, &entry.id).unwrap()).unwrap();
        assert_eq!(after["title"], "中文 Book 📚");
        after["title"] = before["title"].clone();
        after["updated_at"] = before["updated_at"].clone();
        assert_eq!(after, before);
        let mut reloaded = Library::load_from(&root).unwrap();
        let reopened = reloaded
            .add_or_update(&root, "Original".into(), entry.path.clone(), 2, None)
            .unwrap();
        assert_eq!(reopened.title, "中文 Book 📚");
        assert_eq!(
            std::fs::read(&entry.path).unwrap(),
            b"unchanged epub contents"
        );
        assert!(reloaded.rename_book(&root, &entry.id, "  ").is_err());
        assert_eq!(reloaded.books[0].title, "中文 Book 📚");
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn add_snapshot_book(library: &mut Library, root: &str, name: &str) -> BookEntry {
        library
            .add_or_update_from_bytes(root, name.into(), name.as_bytes(), 0, None)
            .unwrap()
    }
    #[test]
    fn reopening_or_reimporting_from_a_stale_snapshot_keeps_the_latest_locator() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let mut stale = Library::load_from(root).unwrap();
        let entry = add_snapshot_book(&mut stale, root, "alpha");
        let mut current = Library::load_from(root).unwrap();
        current
            .apply_position_at(root, &entry.path, 7, Some("Latest".into()), 3, 8, 2000)
            .unwrap();
        let reopened = stale
            .add_or_update(
                root,
                "alpha".into(),
                entry.path.clone(),
                0,
                Some("Old".into()),
            )
            .unwrap();
        assert_eq!(
            (
                reopened.last_chapter,
                reopened.last_block,
                reopened.last_char_offset
            ),
            (7, 3, 8)
        );
        assert_eq!(reopened.last_chapter_title.as_deref(), Some("Latest"));
        let duplicate = stale
            .add_or_update_from_bytes(root, "alpha".into(), b"alpha", 0, None)
            .unwrap();
        assert_eq!(
            (
                duplicate.last_chapter,
                duplicate.last_block,
                duplicate.last_char_offset
            ),
            (7, 3, 8)
        );
    }

    #[test]
    fn typed_reading_versions_resolve_ties_and_legacy_future_clocks_without_reopen_promotion() {
        use crate::{
            cloud_progress::{ProgressVersion, VersionedProgress},
            cloud_sync::SyncJournal,
            sharing::ProgressEntry,
        };
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let mut library = Library::load_from(root).unwrap();
        let entry = add_snapshot_book(&mut library, root, "alpha");
        let hash = Library::read_book_config(root, &entry.id)
            .unwrap()
            .file_hash
            .unwrap();
        let mut record = VersionedProgress {
            progress: ProgressEntry {
                book_hash: hash.clone(),
                title: "Alpha".into(),
                chapter: 9,
                block: 0,
                char_offset: 0,
                chapter_title: None,
                timestamp: 9999999999,
            },
            version: ProgressVersion {
                modified_ms: 9999999999000,
                device: "old-device".into(),
                legacy: true,
            },
        };
        assert!(library
            .apply_versioned_position(root, &entry.path, &record)
            .unwrap());
        record.version = ProgressVersion {
            modified_ms: 2000,
            device: "A".into(),
            legacy: false,
        };
        record.progress.chapter = 2;
        assert!(
            library
                .apply_versioned_position(root, &entry.path, &record)
                .unwrap(),
            "Real reading must supersede inflated legacy clock"
        );
        record.version.device = "B".into();
        record.progress.chapter = 3;
        assert!(library
            .apply_versioned_position(root, &entry.path, &record)
            .unwrap());
        let winner = record.clone();
        record.version.device = "A".into();
        record.progress.chapter = 8;
        assert!(!library
            .apply_versioned_position(root, &entry.path, &record)
            .unwrap());
        library
            .set_last_opened(root, &entry.path, 9999999999999)
            .unwrap();
        let mut journal = SyncJournal::load(root, "relay");
        journal.seed_entry(root, &library.books[0]);
        assert_eq!(journal.own.progress_version(&hash), winner.version);
        assert_eq!(journal.own.progress[&hash].chapter, 3);
    }

    #[test]
    fn position_edit_clock_is_independent_from_reopen_time_and_rejects_delayed_edits() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().to_str().unwrap();
        let mut library = Library::default();
        let entry = add_snapshot_book(&mut library, root, "alpha");
        assert!(library
            .apply_position_at(root, &entry.path, 1, None, 2, 3, 1000)
            .unwrap());
        library.set_last_opened(root, &entry.path, 9000).unwrap();
        let config = Library::load_book_config(root, &entry.id).unwrap().unwrap();
        assert_eq!(
            (config.position_modified_ms, config.last_opened),
            (1000, 9000)
        );
        assert!(!library
            .apply_position_at(root, &entry.path, 9, None, 9, 9, 500)
            .unwrap());
        let config = Library::load_book_config(root, &entry.id).unwrap().unwrap();
        assert_eq!(config.last_chapter, 1);
        assert!(library
            .apply_position_at(root, &entry.path, 2, None, 3, 4, 2000)
            .unwrap());
        let config = Library::load_book_config(root, &entry.id).unwrap().unwrap();
        assert_eq!(
            (config.position_modified_ms, config.last_opened),
            (2000, 9000)
        );
        library
            .update_position(root, &entry.path, 2, None, 3, 4)
            .unwrap();
        assert_eq!(
            Library::load_book_config(root, &entry.id)
                .unwrap()
                .unwrap()
                .position_modified_ms,
            2000,
            "Reflowing the same locator cannot create a new edit"
        );
    }

    #[test]
    fn stale_reader_mutation_preserves_background_imports_and_cannot_resurrect_a_removed_book() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().to_str().unwrap();
        let mut writer = Library::default();
        let first = add_snapshot_book(&mut writer, root, "alpha");
        let mut stale = writer.clone();
        let second = add_snapshot_book(&mut writer, root, "beta");
        stale
            .update_position(root, &first.path, 2, None, 3, 4)
            .unwrap();
        let live = Library::load_from(root).unwrap();
        assert_eq!(live.books.len(), 2);
        assert!(live.books.iter().any(|entry| entry.id == second.id));
        assert_eq!(
            live.books
                .iter()
                .find(|entry| entry.id == first.id)
                .unwrap()
                .last_block,
            3
        );
        writer.remove_by_path(root, &first.path).unwrap();
        stale
            .update_position(root, &first.path, 9, None, 9, 9)
            .unwrap();
        let live = Library::load_from(root).unwrap();
        assert_eq!(live.books.len(), 1);
        assert_eq!(live.books[0].id, second.id);
        assert!(stale
            .rename_book(root, &first.id, "Do not resurrect")
            .is_err());
    }
    #[test]
    fn a_stale_display_index_removes_its_original_identity_after_another_snapshot_changes_membership(
    ) {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().to_str().unwrap();
        let mut writer = Library::default();
        let first = add_snapshot_book(&mut writer, root, "alpha");
        let second = add_snapshot_book(&mut writer, root, "beta");
        let mut stale = writer.clone();
        writer.hide_by_path(root, &first.path).unwrap();
        let third = add_snapshot_book(&mut writer, root, "gamma");
        stale.remove(root, 1).unwrap();
        let live = Library::load_from(root).unwrap();
        assert_eq!(live.books.len(), 1);
        assert_eq!(live.books[0].id, third.id);
        assert!(std::path::Path::new(&third.path).exists());
        assert!(!std::path::Path::new(&second.path).exists());
    }
    #[test]
    fn two_concurrent_loaded_snapshots_import_without_losing_each_others_catalog_entries() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().to_string_lossy().into_owned();
        let mut initial = Library::default();
        add_snapshot_book(&mut initial, &root, "alpha");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let workers: Vec<_> = ["beta", "gamma"]
            .into_iter()
            .map(|name| {
                let mut snapshot = initial.clone();
                let root = root.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    add_snapshot_book(&mut snapshot, &root, name)
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        let live = Library::load_from(&root).unwrap();
        assert_eq!(live.books.len(), 3);
    }

    #[test]
    fn update_position_round_trips_library_and_book_config() {
        let dir = test_dir("position_round_trip");
        let books = books_dir(dir.to_string_lossy().as_ref());
        std::fs::create_dir_all(&books).unwrap();
        let epub = books.join("00000000-0000-4000-8000-000000000001.epub");
        std::fs::write(&epub, b"placeholder").unwrap();
        let data_dir = dir.to_string_lossy().to_string();
        let path = epub.to_string_lossy().to_string();

        let mut library = Library::default();
        let entry = library
            .add_or_update(
                &data_dir,
                "Book".into(),
                path.clone(),
                2,
                Some("Chapter".into()),
            )
            .unwrap();
        library
            .update_position(&data_dir, &entry.path, 2, Some("Chapter".into()), 9, 3)
            .unwrap();

        let reloaded = Library::load_from(&data_dir).unwrap();
        assert_eq!(reloaded.books[0].last_block, 9);
        assert_eq!(reloaded.books[0].last_char_offset, 3);
        let config = Library::read_book_config(&data_dir, &entry.id).unwrap();
        assert_eq!(config.last_chapter, 2);
        assert_eq!(config.last_block, 9);
        assert_eq!(config.last_char_offset, 3);

        let _ = std::fs::remove_dir_all(dir);
    }
}
