use super::*;
use crate::library::{
    archive::{ArchiveStore, ArchivedBook, BookState},
    BookEntry, Library,
};
use std::{fs, path::Path};

fn entry(config: &BookConfig) -> BookEntry {
    BookEntry {
        id: config.id.clone(),
        title: config.title.clone(),
        path: config.epub_path.clone(),
        last_chapter: config.last_chapter,
        last_block: config.last_block,
        last_char_offset: config.last_char_offset,
        last_chapter_title: config.last_chapter_title.clone(),
        last_opened: config.last_opened,
    }
}
fn merge_config(current: BookConfig, incoming: &BookConfig) -> Result<BookConfig, String> {
    let mut state = current.annotation_state.clone();
    state.merge(&incoming.annotation_state)?;
    let mut merged = if incoming.updated_at > current.updated_at {
        incoming.clone()
    } else {
        current.clone()
    };
    merged.id = current.id.clone();
    merged.epub_path = current.epub_path.clone();
    // A portable snapshot must not rewind newer reading on the destination device.
    let clock = |config: &BookConfig| {
        config
            .reading_version()
            .unwrap_or_else(|| crate::cloud_progress::ProgressVersion {
                modified_ms: crate::cloud_recent::timestamp_millis(config.last_opened),
                device: String::new(),
                legacy: true,
            })
    };
    let progress = if clock(incoming) > clock(&current) {
        incoming
    } else {
        &current
    };
    merged.last_chapter = progress.last_chapter;
    merged.last_block = progress.last_block;
    merged.last_char_offset = progress.last_char_offset;
    merged.last_chapter_title = progress.last_chapter_title.clone();
    merged.position_modified_ms = progress.position_modified_ms;
    merged.position_version = progress.position_version.clone();
    merged.last_opened = current.last_opened.max(incoming.last_opened);
    merged.created_at = current.created_at.min(incoming.created_at);
    merged.updated_at = current.updated_at.max(incoming.updated_at);
    merged.persisted = None;
    state.materialize(&mut merged);
    merged.annotation_state = state;
    Ok(merged)
}
/// Merge a fully validated package. Existing content is matched by actual SHA-256,
/// never by an imported path/UUID. Duplicate restore is causally idempotent.
pub fn restore(root: &str, source: &Path) -> Result<BackupSummary, String> {
    restore_checked(root, source, None)
}
pub(super) fn restore_checked(
    root: &str,
    source: &Path,
    expected: Option<&str>,
) -> Result<BackupSummary, String> {
    let _lease = crate::library::access::lock(root)?;
    let root_path = Path::new(root)
        .canonicalize()
        .map_err(|_| "Cannot resolve restore storage")?;
    let scratch = package::Scratch::new(&root_path)?;
    let file = fs::File::open(source).map_err(|_| "Cannot open selected backup")?;
    let mut zip = zip::ZipArchive::new(file).map_err(|_| "Invalid library backup")?;
    let manifest = package::read_manifest(&mut zip)?;
    let verified = manifest.summary()?;
    if expected.is_some_and(|expected| verified.manifest_sha256 != expected) {
        return Err("Backup changed after verification; select and verify it again".into());
    }
    let mut extracted = BTreeMap::new();
    // No catalog migration, local configuration write or lifecycle merge happens
    // until every entry has passed checksum and EPUB admission.
    let total = manifest.books.iter().filter(|b| b.included).count();
    for (index, book) in manifest.books.iter().filter(|b| b.included).enumerate() {
        progress::update(root, "verifying", index, total);
        let path = scratch.0.join(format!("{}.epub", book.hash));
        package::admit(&mut zip, book, &path, &scratch.0.join("chapter-index"))?;
        extracted.insert(book.hash.clone(), path);
    }
    let library = Library::read_snapshot(root)?;
    let mut archive = ArchiveStore::load(root)?;
    let mut catalog: BTreeMap<_, _> = library
        .books
        .iter()
        .map(|b| (b.id.clone(), b.clone()))
        .collect();
    let mut by_hash = BTreeMap::new();
    let candidates = library
        .books
        .iter()
        .chain(archive.records.values().filter_map(|r| r.entry.as_ref()));
    for book in candidates {
        if uuid::Uuid::parse_str(&book.id).is_err() {
            return Err("Migrate the existing library before restoring a backup".into());
        }
        let path = managed_path(&root_path, &book.id, Path::new(&book.path))?;
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                by_hash
                    .entry(crate::file_hash(&book.path)?)
                    .or_insert_with(|| book.clone());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if let Some(hash) =
                    Library::load_book_config(root, &book.id)?.and_then(|c| c.file_hash)
                {
                    by_hash.entry(hash).or_insert_with(|| book.clone());
                }
            }
            _ => return Err("Cannot safely inspect existing publication".into()),
        }
    }
    // Lifecycle LWW events retain their original identity/time; restoring the same
    // archive does not create a fresh archive/delete action on every device.
    for (hash, incoming) in &manifest.lifecycles {
        if archive
            .records
            .get(hash)
            .is_none_or(|old| incoming.newer_than(&old.lifecycle))
        {
            let previous_entry = archive
                .records
                .get(hash)
                .and_then(|r| r.entry.clone())
                .or_else(|| by_hash.get(hash).cloned());
            archive.records.insert(
                hash.clone(),
                ArchivedBook {
                    lifecycle: incoming.clone(),
                    entry: previous_entry,
                },
            );
        }
    }
    let mut updates = Vec::new();
    for (index, book) in manifest.books.iter().enumerate() {
        progress::update(root, "merging", index, manifest.books.len());
        let lifecycle = archive.records.get(&book.hash).map(|r| r.lifecycle.state);
        if lifecycle == Some(BookState::Deleted) {
            continue;
        }
        let existing = by_hash.get(&book.hash);
        let id = match existing {
            Some(e) => e.id.clone(),
            None => loop {
                let id = uuid::Uuid::new_v4().to_string();
                if !root_path.join("books").join(format!("{id}.json")).exists()
                    && !root_path.join("books").join(format!("{id}.epub")).exists()
                {
                    break id;
                }
            },
        };
        // Store the application's chosen root spelling. Canonical paths are for
        // ownership checks; changing /var to /private/var triggers needless legacy
        // catalog relocation on every load on Apple platforms.
        let path = Path::new(root).join("books").join(format!("{id}.epub"));
        let mut config = if let Some(existing) = existing {
            let current = Library::load_book_config(root, &existing.id)?
                .ok_or("Existing publication configuration is missing")?;
            merge_config(current, &book.config)?
        } else {
            book.config.clone()
        };
        config.id = id.clone();
        config.epub_path = path.to_string_lossy().into_owned();
        config.persisted = None;
        config.file_hash = Some(book.hash.clone());
        let staged_config = scratch.0.join(format!("{id}.json"));
        crate::storage::replace(
            &staged_config,
            &serde_json::to_vec(&config).map_err(|_| "Cannot stage restored configuration")?,
        )?;
        updates.push((format!("books/{id}.json"), staged_config));
        if !path.exists() {
            if let Some(staged) = extracted.get(&book.hash) {
                updates.push((format!("books/{id}.epub"), staged.clone()));
            }
        }
        let local_entry = entry(&config);
        let active = lifecycle.map_or(book.active, |s| s == BookState::Active);
        if active && (path.exists() || extracted.contains_key(&book.hash)) {
            catalog.insert(id, local_entry.clone());
        } else {
            catalog.remove(&id);
        }
        if let Some(record) = archive.records.get_mut(&book.hash) {
            record.entry = Some(local_entry);
        }
    }
    // Apply incoming tombstones/archive events even when the package intentionally
    // carries no original file for that book. Files remain local until explicit purge.
    catalog.retain(|_, e| {
        !archive.records.values().any(|r| {
            r.lifecycle.state != BookState::Active
                && r.entry.as_ref().is_some_and(|archived| archived.id == e.id)
        })
    });
    let mut output = library;
    output.books = catalog.into_values().collect();
    output.books.sort_by(|a, b| {
        b.last_opened
            .cmp(&a.last_opened)
            .then_with(|| a.id.cmp(&b.id))
    });
    for (name, value) in [
        ("library.json", serde_json::to_vec(&output)),
        ("book-lifecycle.json", serde_json::to_vec(&archive)),
    ] {
        let staged = scratch.0.join(name);
        crate::storage::replace(
            &staged,
            &value.map_err(|_| "Cannot encode restored library")?,
        )?;
        updates.push((name.into(), staged));
    }
    let activity = scratch.0.join("reading-activity.json");
    crate::storage::replace(
        &activity,
        &crate::reading_activity::merged_portable(root, &manifest.activity)?,
    )?;
    updates.push(("reading-activity.json".into(), activity));
    transaction::publish(root, &updates)?;
    Ok(verified)
}
