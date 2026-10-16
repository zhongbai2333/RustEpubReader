//! Library management, book cataloging, and metadata parsing.
pub mod archive;
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
    /// O(1) lookup: path → index in `books`. Rebuilt on load/mutation.
    #[serde(skip)]
    path_index: HashMap<String, usize>,
}

impl Library {
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
    pub fn load_from(data_dir: &str) -> Self {
        let path = library_path(data_dir);
        let mut library = if let Ok(data) = std::fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Self::default()
        };

        library.migrate_to_uuid_storage(data_dir);
        library.rebuild_index();
        library
    }

    pub fn save_to(&self, data_dir: &str) {
        let dir = PathBuf::from(data_dir);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            eprintln!("[Library] failed to create data dir {:?}: {}", dir, e);
            return;
        }
        let path = library_path(data_dir);
        match serde_json::to_string_pretty(self) {
            Ok(data) => {
                if let Err(e) = std::fs::write(&path, data) {
                    eprintln!("[Library] failed to write {:?}: {}", path, e);
                }
            }
            Err(e) => eprintln!("[Library] failed to serialize library: {}", e),
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(mut perm) = std::fs::metadata(&path).map(|m| m.permissions()) {
                perm.set_mode(0o600);
                let _ = std::fs::set_permissions(&path, perm);
            }
        }
    }

    pub fn add_or_update(
        &mut self,
        data_dir: &str,
        title: String,
        source_path: String,
        chapter: usize,
        chapter_title: Option<String>,
    ) -> BookEntry {
        if let Err(e) = std::fs::create_dir_all(books_dir(data_dir)) {
            eprintln!("[Library] failed to create books dir: {}", e);
        }
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
            .or_else(|| derive_id_from_path(&source_path))
            .unwrap_or_else(generate_book_id);
        if !is_uuid_like(&id) {
            id = generate_book_id();
        }

        let managed_epub = epub_path_for(data_dir, &id);
        let source = PathBuf::from(&source_path);
        let managed_epub_str = managed_epub.to_string_lossy().to_string();

        if source.exists() && source != managed_epub {
            if let Err(e) = std::fs::copy(&source, &managed_epub) {
                eprintln!(
                    "[Library] failed to copy {:?} -> {:?}: {}",
                    source, managed_epub, e
                );
            }
        }

        let final_path = if managed_epub.exists() {
            managed_epub_str
        } else {
            source_path.clone()
        };

        if let Some(idx) = existing_idx {
            let entry = &mut self.books[idx];
            if entry.last_chapter != chapter {
                entry.last_block = 0;
                entry.last_char_offset = 0;
            }
            entry.id = id.clone();
            if entry.title.trim().is_empty() {
                entry.title = title;
            }
            entry.path = final_path;
            entry.last_chapter = chapter;
            if chapter_title.is_some() {
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
        self.write_book_config(data_dir, &entry, Some(now));
        self.rebuild_index();
        self.save_to(data_dir);
        entry
    }

    pub fn add_or_update_from_bytes(
        &mut self,
        data_dir: &str,
        title: String,
        bytes: &[u8],
        chapter: usize,
        chapter_title: Option<String>,
    ) -> BookEntry {
        if let Err(e) = std::fs::create_dir_all(books_dir(data_dir)) {
            eprintln!("[Library] failed to create books dir: {}", e);
        }
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
        if let Err(e) = std::fs::write(&managed_epub, bytes) {
            eprintln!("[Library] failed to write epub {:?}: {}", managed_epub, e);
        }

        if let Some(idx) = existing_idx {
            let entry = &mut self.books[idx];
            if entry.last_chapter != chapter {
                entry.last_block = 0;
                entry.last_char_offset = 0;
            }
            entry.id = id.clone();
            if entry.title.trim().is_empty() {
                entry.title = title;
            }
            entry.path = managed_epub.to_string_lossy().to_string();
            entry.last_chapter = chapter;
            if chapter_title.is_some() {
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
        self.write_book_config(data_dir, &entry, Some(now));
        self.rebuild_index();
        self.save_to(data_dir);
        entry
    }

    pub fn hide_by_path(&mut self, data_dir: &str, path: &str) {
        self.hide_paths(
            data_dir,
            &std::collections::HashSet::from([path.to_owned()]),
        );
    }
    pub fn hide_paths(&mut self, data_dir: &str, paths: &std::collections::HashSet<String>) {
        let before = self.books.len();
        self.books.retain(|e| !paths.contains(&e.path));
        if self.books.len() != before {
            self.rebuild_index();
            self.save_to(data_dir);
        }
    }
    pub fn restore_entry(&mut self, data_dir: &str, entry: BookEntry) {
        if !self.books.iter().any(|e| e.id == entry.id) {
            self.write_book_config(data_dir, &entry, None);
            self.books.push(entry);
        }
        self.rebuild_index();
        self.save_to(data_dir);
    }

    pub fn remove(&mut self, data_dir: &str, idx: usize) {
        if idx < self.books.len() {
            let entry = self.books.remove(idx);
            self.remove_entry_files(data_dir, &entry);
            self.rebuild_index();
            self.save_to(data_dir);
        }
    }

    /// Change the display title while retaining storage identity and reading data.
    pub fn rename_book(
        &mut self,
        data_dir: &str,
        id: &str,
        title: &str,
    ) -> Result<BookEntry, String> {
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
        let library_data = serde_json::to_vec_pretty(&renamed).map_err(|e| e.to_string())?;
        let config_data = serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?;
        let config_path = config_path_for(data_dir, id);
        let old_config = std::fs::read(&config_path).map_err(|e| e.to_string())?;
        let result = (|| {
            write_rename_file(&config_path, &config_data)?;
            if let Err(e) = write_rename_file(&library_path(data_dir), &library_data) {
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
        self.import_shared_books_checked(data_dir, incoming, || Ok(()), |_| {})
    }
    pub fn import_shared_books_checked(
        &mut self,
        data_dir: &str,
        incoming: &[(String, String, PathBuf)],
        check: impl Fn() -> Result<(), String>,
        copied: impl Fn(u64),
    ) -> Result<(Vec<BookEntry>, usize), String> {
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
                next.write_book_config(data_dir, &entry, None);
                check()?;
                let cfg = Self::read_book_config(data_dir, &entry.id)
                    .ok_or("Cannot save received book configuration")?;
                if cfg.file_hash.as_ref() != Some(hash) {
                    return Err("Cannot verify imported book".into());
                }
                imported.push(entry);
            }
            check()?;
            let bytes = serde_json::to_vec_pretty(&next).map_err(|_| "Cannot save library")?;
            write_rename_file(&library_path(data_dir), &bytes)
        })();
        if let Err(error) = result {
            for (epub, config) in created {
                let _ = std::fs::remove_file(epub);
                let _ = std::fs::remove_file(config);
            }
            return Err(error);
        }
        next.rebuild_index();
        *self = next;
        Ok((imported, skipped))
    }

    pub fn remove_by_path(&mut self, data_dir: &str, path: &str) {
        if let Some(idx) = self.find_by_path(path) {
            let entry = self.books.remove(idx);
            self.remove_entry_files(data_dir, &entry);
            self.rebuild_index();
            self.save_to(data_dir);
        }
    }

    pub fn update_chapter(
        &mut self,
        data_dir: &str,
        path: &str,
        chapter: usize,
        chapter_title: Option<String>,
    ) {
        self.update_position(data_dir, path, chapter, chapter_title, 0, 0);
    }

    pub fn update_position(
        &mut self,
        data_dir: &str,
        path: &str,
        chapter: usize,
        chapter_title: Option<String>,
        block: usize,
        char_offset: usize,
    ) {
        let changed = self.find_by_path(path).is_some_and(|i| {
            let e = &self.books[i];
            (e.last_chapter, e.last_block, e.last_char_offset) != (chapter, block, char_offset)
        });
        if changed {
            if let Some(idx) = self.find_by_path(path) {
                self.books[idx].last_opened = crate::now_millis();
            }
        }
        self.apply_position(data_dir, path, chapter, chapter_title, block, char_offset);
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
    ) {
        if let Some(idx) = self.find_by_path(path) {
            let entry = &mut self.books[idx];
            entry.last_chapter = chapter;
            entry.last_block = block;
            entry.last_char_offset = char_offset;
            if chapter_title.is_some() {
                entry.last_chapter_title = chapter_title;
            }
            let entry_snapshot = entry.clone();
            self.write_book_config(data_dir, &entry_snapshot, None);
            self.save_to(data_dir);
        }
    }

    /// The caller supplies the originating reading time, never the sync time.
    pub fn set_last_opened(&mut self, data_dir: &str, path: &str, at_ms: u64) {
        if let Some(idx) = self.find_by_path(path) {
            if self.books[idx].last_opened == at_ms {
                return;
            }
            self.books[idx].last_opened = at_ms;
            self.write_book_config(data_dir, &self.books[idx], None);
            self.save_to(data_dir);
        }
    }

    /// Batch remote history projection: write the catalog once, not once per book.
    pub fn apply_reading_times(
        &mut self,
        data_dir: &str,
        times: &std::collections::BTreeMap<String, u64>,
    ) {
        let mut changed = false;
        for (path, at) in times {
            if let Some(idx) = self.find_by_path(path) {
                if self.books[idx].last_opened != *at {
                    self.books[idx].last_opened = *at;
                    self.write_book_config(data_dir, &self.books[idx], None);
                    changed = true;
                }
            }
        }
        if changed {
            self.save_to(data_dir);
        }
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
        let path = config_path_for(data_dir, id);
        let data = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }

    fn migrate_to_uuid_storage(&mut self, data_dir: &str) {
        let mut changed = false;
        if let Err(e) = std::fs::create_dir_all(books_dir(data_dir)) {
            eprintln!("[Library] failed to create books dir: {}", e);
        }

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

            while !seen_ids.insert(id.clone()) {
                id = generate_book_id();
            }
            if id != entry.id {
                entry.id = id.clone();
                changed = true;
            }

            let managed_epub = epub_path_for(data_dir, &id);
            let current = PathBuf::from(&entry.path);

            if current != managed_epub {
                if current.exists() {
                    if let Err(e) = std::fs::copy(&current, &managed_epub) {
                        eprintln!(
                            "[Library] migration copy {:?} -> {:?} failed: {}",
                            current, managed_epub, e
                        );
                    }
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
                self.write_book_config(data_dir, &entry, None);
            }
            migrated.push(entry);
        }

        self.books = migrated;
        if changed {
            self.save_to(data_dir);
        }
    }

    fn write_book_config(&self, data_dir: &str, entry: &BookEntry, created_at_hint: Option<u64>) {
        if let Err(e) = std::fs::create_dir_all(books_dir(data_dir)) {
            eprintln!("[Library] failed to create books dir: {}", e);
        }
        let config_path = config_path_for(data_dir, &entry.id);

        let existing = std::fs::read_to_string(&config_path)
            .ok()
            .and_then(|s| serde_json::from_str::<BookConfig>(&s).ok());

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

        let cfg = BookConfig {
            id: entry.id.clone(),
            title: entry.title.clone(),
            epub_path: entry.path.clone(),
            last_chapter: entry.last_chapter,
            last_block: entry.last_block,
            last_char_offset: entry.last_char_offset,
            last_chapter_title: entry.last_chapter_title.clone(),
            last_opened: entry.last_opened,
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

        if let Ok(data) = serde_json::to_string_pretty(&cfg) {
            if let Err(e) = std::fs::write(&config_path, &data) {
                eprintln!("[Library] failed to write config {:?}: {}", config_path, e);
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(mut perm) = std::fs::metadata(&config_path).map(|m| m.permissions()) {
                    perm.set_mode(0o600);
                    let _ = std::fs::set_permissions(&config_path, perm);
                }
            }
        }
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
            let _ = std::fs::remove_file(config_path_for(data_dir, &id));
            let managed_epub = epub_path_for(data_dir, &id);
            if managed_epub != epub_path {
                let _ = std::fs::remove_file(managed_epub);
            }
        }
    }
}

fn write_rename_file(path: &Path, data: &[u8]) -> Result<(), String> {
    let temporary = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    let result = (|| -> std::io::Result<()> {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(data)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let entry = library.add_or_update(
            &root,
            "Original".into(),
            source.to_string_lossy().into(),
            2,
            None,
        );
        library.update_position(&root, &entry.path, 2, Some("Chapter".into()), 9, 3);
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
        let mut reloaded = Library::load_from(&root);
        let reopened =
            reloaded.add_or_update(&root, "Original".into(), entry.path.clone(), 2, None);
        assert_eq!(reopened.title, "中文 Book 📚");
        assert_eq!(
            std::fs::read(&entry.path).unwrap(),
            b"unchanged epub contents"
        );
        assert!(reloaded.rename_book(&root, &entry.id, "  ").is_err());
        assert_eq!(reloaded.books[0].title, "中文 Book 📚");
        std::fs::remove_dir_all(dir).unwrap();
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
        let entry = library.add_or_update(
            &data_dir,
            "Book".into(),
            path.clone(),
            2,
            Some("Chapter".into()),
        );
        library.update_position(&data_dir, &entry.path, 2, Some("Chapter".into()), 9, 3);

        let reloaded = Library::load_from(&data_dir);
        assert_eq!(reloaded.books[0].last_block, 9);
        assert_eq!(reloaded.books[0].last_char_offset, 3);
        let config = Library::read_book_config(&data_dir, &entry.id).unwrap();
        assert_eq!(config.last_chapter, 2);
        assert_eq!(config.last_block, 9);
        assert_eq!(config.last_char_offset, 3);

        let _ = std::fs::remove_dir_all(dir);
    }
}
