//! Versioned, credential-free library packages and recoverable native restore.
use crate::library::{archive::BookLifecycle, BookConfig};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod package;
mod progress;
mod restore;
pub(crate) mod transaction;
pub use package::{export, inspect, BackupSummary};
pub use restore::restore;
#[cfg(test)]
mod tests;

const FORMAT: &str = "RustEpubReader library backup";
const VERSION: u32 = 1;
const MAX_BOOKS: usize = 5_000;
const MAX_MANIFEST: u64 = 64 * 1024 * 1024;
const MAX_TOTAL: u64 = 64 * 1024 * 1024 * 1024;

fn managed_path(
    root: &std::path::Path,
    id: &str,
    path: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    let books = root.join("books");
    let metadata =
        std::fs::symlink_metadata(&books).map_err(|_| "Cannot inspect managed book directory")?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || uuid::Uuid::parse_str(id).is_err()
        || path.file_name().and_then(|n| n.to_str()) != Some(&format!("{id}.epub"))
        || path
            .parent()
            .and_then(|parent| parent.canonicalize().ok())
            .as_ref()
            != Some(&books)
    {
        return Err("Backup requires managed book storage".into());
    }
    Ok(books.join(format!("{id}.epub")))
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Publication {
    hash: String,
    size: u64,
    included: bool,
    active: bool,
    // Typed whitelist. Local paths are cleared, installation identity and account
    // files are never read. Annotation causal actor identifiers are not secrets.
    config: BookConfig,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: String,
    schema_version: u32,
    created_at: u64,
    books: Vec<Publication>,
    lifecycles: BTreeMap<String, BookLifecycle>,
    #[serde(default)]
    activity: crate::reading_activity::PortableActivity,
}
impl Manifest {
    fn validate(&self) -> Result<(), String> {
        if self.format != FORMAT || self.schema_version != VERSION {
            return Err("Unsupported library backup version".into());
        }
        if self.books.len() > MAX_BOOKS || self.lifecycles.len() > MAX_BOOKS {
            return Err("Library backup exceeds book count limit".into());
        }
        let mut hashes = std::collections::BTreeSet::new();
        let mut ids = std::collections::BTreeSet::new();
        let mut total = 0u64;
        for book in &self.books {
            let c = &book.config;
            if !crate::cloud_sync::valid_hash(&book.hash)
                || !hashes.insert(&book.hash)
                || uuid::Uuid::parse_str(&c.id).is_err()
                || !ids.insert(&c.id)
                || !c.epub_path.is_empty()
                || c.file_hash.as_ref() != Some(&book.hash)
                || c.title.len() > 4096
                || c.last_chapter > 100_000
                || c.last_block > 10_000_000
                || c.last_char_offset > 100_000_000
                || c.last_chapter_title
                    .as_ref()
                    .is_some_and(|t| t.len() > 4096)
                || book.size == 0
                || book.size > crate::cloud_sync::MAX_DOCUMENT_BYTES
                || (book.active && !book.included)
                || serde_json::to_vec(c)
                    .map_err(|_| "Invalid backup configuration")?
                    .len()
                    > 16 * 1024 * 1024
            {
                return Err("Invalid library backup publication".into());
            }
            c.annotation_state.validate()?;
            let mut materialized = c.clone();
            c.annotation_state.materialize(&mut materialized);
            if c.bookmarks != materialized.bookmarks
                || c.highlights != materialized.highlights
                || c.notes != materialized.notes
            {
                return Err("Inconsistent backup annotations".into());
            }
            if let Some(lifecycle) = self.lifecycles.get(&book.hash) {
                if book.active != (lifecycle.state == crate::library::archive::BookState::Active)
                    || lifecycle.state == crate::library::archive::BookState::Deleted
                {
                    return Err("Inconsistent backup archive state".into());
                }
            } else if !book.active {
                return Err("Archived backup book has no lifecycle".into());
            }
            total = total
                .checked_add(if book.included { book.size } else { 0 })
                .ok_or("Oversized library backup")?;
        }
        if total > MAX_TOTAL || self.lifecycles.iter().any(|(h, l)| !l.valid(h)) {
            return Err("Invalid or oversized library backup".into());
        }
        crate::reading_activity::validate_portable(&self.activity)?;
        Ok(())
    }
}

/// Shared JSON bridge used by native clients; file picking/sharing stays native.
pub fn command(root: &str, request: &serde_json::Value) -> Result<serde_json::Value, String> {
    if request["action"].as_str() == Some("status") {
        return progress::status(root);
    }
    let path = request["path"]
        .as_str()
        .filter(|p| std::path::Path::new(p).is_absolute())
        .ok_or("An absolute backup file path is required")?;
    let mut operation =
        progress::Operation::begin(root, request["action"].as_str().unwrap_or("preparing"))?;
    let summary = match request["action"].as_str() {
        Some("export") => export(root, std::path::Path::new(path))?,
        Some("inspect") => package::inspect_with(std::path::Path::new(path), Some(root))?,
        Some("restore") => {
            let expected = request["expected_manifest"]
                .as_str()
                .filter(|h| crate::cloud_sync::valid_hash(h))
                .ok_or("Verify the selected backup before restoring")?;
            restore::restore_checked(root, std::path::Path::new(path), Some(expected))?
        }
        _ => return Err("Unknown backup action".into()),
    };
    operation.finish();
    serde_json::to_value(summary).map_err(|_| "Cannot encode backup result".into())
}
