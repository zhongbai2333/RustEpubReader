use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::now_secs;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BookEntry {
    #[serde(default)]
    pub id: String,
    pub title: String,
    pub path: String,
    pub last_chapter: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_chapter_title: Option<String>,
    pub last_opened: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct BookSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bookmark: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_override: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BookConfig {
    pub id: String,
    pub title: String,
    pub epub_path: String,
    pub last_chapter: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_chapter_title: Option<String>,
    pub last_opened: u64,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default)]
    pub settings: BookSettings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<crate::epub::EpubMetadata>,
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
    crate::epub::EpubBook::file_hash(path).ok()
}

fn bytes_hash(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

impl Library {
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
            entry.id = id.clone();
            entry.title = title;
            entry.path = final_path;
            entry.last_chapter = chapter;
            if chapter_title.is_some() {
                entry.last_chapter_title = chapter_title;
            }
            entry.last_opened = now;
        } else {
            self.books.push(BookEntry {
                id,
                title,
                path: final_path,
                last_chapter: chapter,
                last_chapter_title: chapter_title,
                last_opened: now,
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
            entry.id = id.clone();
            entry.title = title;
            entry.path = managed_epub.to_string_lossy().to_string();
            entry.last_chapter = chapter;
            if chapter_title.is_some() {
                entry.last_chapter_title = chapter_title;
            }
            entry.last_opened = now;
        } else {
            self.books.push(BookEntry {
                id,
                title,
                path: managed_epub.to_string_lossy().to_string(),
                last_chapter: chapter,
                last_chapter_title: chapter_title,
                last_opened: now,
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

    pub fn remove(&mut self, data_dir: &str, idx: usize) {
        if idx < self.books.len() {
            let entry = self.books.remove(idx);
            self.remove_entry_files(data_dir, &entry);
            self.rebuild_index();
            self.save_to(data_dir);
        }
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
        if let Some(idx) = self.find_by_path(path) {
            let entry = &mut self.books[idx];
            entry.last_chapter = chapter;
            if chapter_title.is_some() {
                entry.last_chapter_title = chapter_title;
            }
            entry.last_opened = now_secs();
            let entry_snapshot = entry.clone();
            self.write_book_config(data_dir, &entry_snapshot, None);
            self.save_to(data_dir);
        }
    }

    pub fn sorted_indices_by_recent(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..self.books.len()).collect();
        indices.sort_by(|&a, &b| self.books[b].last_opened.cmp(&self.books[a].last_opened));
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

            if entry.last_opened == 0 {
                entry.last_opened = now_secs();
                changed = true;
            }

            self.write_book_config(data_dir, &entry, None);
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

        let cfg = BookConfig {
            id: entry.id.clone(),
            title: entry.title.clone(),
            epub_path: entry.path.clone(),
            last_chapter: entry.last_chapter,
            last_chapter_title: entry.last_chapter_title.clone(),
            last_opened: entry.last_opened,
            created_at,
            updated_at: now_secs(),
            settings,
            file_hash,
            metadata,
        };

        if let Ok(data) = serde_json::to_string_pretty(&cfg) {
            if let Err(e) = std::fs::write(&config_path, &data) {
                eprintln!("[Library] failed to write config {:?}: {}", config_path, e);
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
