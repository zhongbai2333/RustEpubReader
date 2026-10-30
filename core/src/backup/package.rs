use super::*;
use crate::library::{archive::ArchiveStore, Library};
use std::{
    fs,
    io::{Read, Seek, Write},
    path::{Path, PathBuf},
};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackupSummary {
    pub manifest_sha256: String,
    pub schema_version: u32,
    pub books: usize,
    pub archived: usize,
    pub files: usize,
    pub file_bytes: u64,
    pub created_at: u64,
}
impl Manifest {
    pub(super) fn summary(&self) -> Result<BackupSummary, String> {
        Ok(BackupSummary {
            manifest_sha256: crate::bytes_hash(
                &serde_json::to_vec(self).map_err(|_| "Cannot identify backup manifest")?,
            ),
            schema_version: self.schema_version,
            books: self.books.iter().filter(|b| b.active).count(),
            archived: self.books.iter().filter(|b| !b.active).count(),
            files: self.books.iter().filter(|b| b.included).count(),
            file_bytes: self
                .books
                .iter()
                .filter(|b| b.included)
                .map(|b| b.size)
                .sum(),
            created_at: self.created_at,
        })
    }
}
pub(super) struct Scratch(pub PathBuf);
impl Scratch {
    pub(super) fn new(parent: &Path) -> Result<Self, String> {
        fs::create_dir_all(parent).map_err(|_| "Cannot create backup staging directory")?;
        let path = parent.join(format!(".backup-staging-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).map_err(|_| "Cannot create backup staging directory")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
                .map_err(|_| "Cannot protect backup staging")?;
        }
        Ok(Self(path))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn archive_error(error: impl std::fmt::Display) -> String {
    format!("Cannot read library backup: {error}")
}
struct MetadataBudget(u64);
impl Write for MetadataBudget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let next = self
            .0
            .checked_add(bytes.len() as u64)
            .filter(|n| *n <= MAX_MANIFEST)
            .ok_or_else(|| std::io::Error::other("Backup metadata exceeds size limit"))?;
        self.0 = next;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl MetadataBudget {
    fn claim(&mut self, value: &impl Serialize) -> Result<(), String> {
        serde_json::to_writer(self, value).map_err(|_| "Backup metadata exceeds size limit".into())
    }
}

pub fn export(root: &str, destination: &Path) -> Result<BackupSummary, String> {
    let _lease = crate::library::access::lock(root)?;
    let root_path = Path::new(root)
        .canonicalize()
        .map_err(|_| "Cannot resolve library")?;
    let parent = destination.parent().ok_or("Invalid backup destination")?;
    let parent = parent
        .canonicalize()
        .map_err(|_| "Cannot resolve backup destination")?;
    if parent == root_path || parent.starts_with(root_path.join("books")) {
        return Err("Choose a backup destination outside library storage".into());
    }
    let library = Library::load_from(root)?;
    let archive = ArchiveStore::load(root)?;
    let local_archived = archive
        .records
        .values()
        .filter(|r| {
            r.lifecycle.state == crate::library::archive::BookState::Archived && r.entry.is_some()
        })
        .count();
    if library.books.len() + local_archived > MAX_BOOKS {
        return Err("Library backup exceeds book count limit".into());
    }
    let mut manifest = Manifest {
        format: FORMAT.into(),
        schema_version: VERSION,
        created_at: crate::now_secs(),
        books: Vec::new(),
        lifecycles: archive.states(),
        classification: library.classification.clone(),
        activity: Default::default(),
    };
    let mut paths = BTreeMap::new();
    let mut budget = MetadataBudget(1024);
    budget.claim(&manifest.lifecycles)?;
    let mut entries: Vec<_> = library
        .books
        .iter()
        .map(|e| (e.clone(), true, None))
        .collect();
    for record in archive
        .records
        .values()
        .filter(|r| r.lifecycle.state == crate::library::archive::BookState::Archived)
    {
        if let Some(entry) = &record.entry {
            entries.push((entry.clone(), false, Some(record.lifecycle.book.size)));
        }
    }
    let total = entries.len();
    for (index, (entry, active, missing_size)) in entries.into_iter().enumerate() {
        progress::update(root, "collecting", index, total);
        for path in [
            root_path.join("books").join(format!("{}.json", entry.id)),
            root_path
                .join("books")
                .join(format!("{}.json.bak", entry.id)),
        ] {
            if fs::metadata(path).is_ok_and(|metadata| metadata.len() > 16 * 1024 * 1024) {
                return Err("Book configuration exceeds backup limit".into());
            }
        }
        let mut config = Library::load_book_config(root, &entry.id)?
            .ok_or("Book configuration missing; backup cancelled")?;
        let owned = managed_path(&root_path, &entry.id, Path::new(&entry.path))?;
        let path = owned.as_path();
        // Never use a portable catalog to read arbitrary external files.
        let included = match fs::symlink_metadata(path) {
            Ok(m) if m.is_file() && !m.file_type().is_symlink() => true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !active => false,
            _ => return Err("Cannot read managed publication; backup cancelled".into()),
        };
        let (hash, size) = if included {
            (
                crate::file_hash(&entry.path)?,
                fs::metadata(path)
                    .map_err(|_| "Cannot read book size")?
                    .len(),
            )
        } else {
            (
                config
                    .file_hash
                    .clone()
                    .ok_or("Archived publication has no hash")?,
                missing_size.ok_or("Archived publication has no size")?,
            )
        };
        config.id = entry.id;
        config.title = entry.title;
        config.epub_path.clear();
        config.persisted = None;
        config.file_hash = Some(hash.clone());
        if included {
            paths.insert(hash.clone(), path.to_path_buf());
        }
        let publication = Publication {
            hash,
            size,
            included,
            active,
            config,
        };
        budget.claim(&publication)?;
        manifest.books.push(publication);
    }
    manifest
        .classification
        .assignments
        .retain(|id, _| manifest.books.iter().any(|book| &book.config.id == id));
    budget.claim(&manifest.classification)?;
    manifest.books.sort_by(|a, b| a.hash.cmp(&b.hash));
    manifest.activity =
        crate::reading_activity::portable(root, manifest.books.iter().map(|book| &book.config))?;
    budget.claim(&manifest.activity)?;
    manifest.validate()?;
    let json = serde_json::to_vec(&manifest).map_err(|_| "Cannot encode library backup")?;
    if json.len() as u64 > MAX_MANIFEST {
        return Err("Backup metadata exceeds size limit".into());
    }
    // Keep staging under the leased private root. A later access can reclaim it
    // after process termination; never leave hidden book-sized copies in Downloads.
    let scratch = Scratch::new(&root_path)?;
    let staged = scratch.0.join("library.readerbackup");
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut zip = ZipWriter::new(
        options
            .open(&staged)
            .map_err(|_| "Cannot create backup file")?,
    );
    let compression = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o600)
        .large_file(true);
    zip.start_file("manifest.json", compression)
        .map_err(archive_error)?;
    zip.write_all(&json).map_err(archive_error)?;
    for (index, book) in manifest.books.iter().filter(|b| b.included).enumerate() {
        progress::update(root, "packing", index, paths.len());
        zip.start_file(format!("books/{}.epub", book.hash), compression)
            .map_err(archive_error)?;
        let mut source =
            fs::File::open(&paths[&book.hash]).map_err(|_| "Cannot read publication")?;
        let count = std::io::copy(&mut source, &mut zip).map_err(archive_error)?;
        if count != book.size
            || crate::file_hash(&paths[&book.hash].to_string_lossy())? != book.hash
        {
            return Err("Publication changed during backup; retry".into());
        }
    }
    zip.finish()
        .map_err(archive_error)?
        .sync_all()
        .map_err(|_| "Cannot flush backup file")?;
    // Reopen and verify the exact package that will be published, not just inputs.
    inspect_with(&staged, Some(root))?;
    progress::update(root, "publishing", 0, 0);
    crate::storage::copy_file(&staged, destination)?;
    manifest.summary()
}
pub(super) fn read_manifest<R: Read + Seek>(zip: &mut ZipArchive<R>) -> Result<Manifest, String> {
    if zip.len() > MAX_BOOKS + 1 {
        return Err("Too many files in library backup".into());
    }
    let mut entry = zip.by_name("manifest.json").map_err(archive_error)?;
    if entry.size() > MAX_MANIFEST {
        return Err("Backup metadata exceeds size limit".into());
    }
    let mut data = Vec::new();
    (&mut entry)
        .take(MAX_MANIFEST + 1)
        .read_to_end(&mut data)
        .map_err(archive_error)?;
    if data.len() as u64 > MAX_MANIFEST {
        return Err("Backup metadata exceeds size limit".into());
    }
    let manifest: Manifest = serde_json::from_slice(&data).map_err(archive_error)?;
    manifest.validate()?;
    drop(entry);
    let expected: BTreeMap<_, _> = manifest
        .books
        .iter()
        .filter(|b| b.included)
        .map(|b| (format!("books/{}.epub", b.hash), b.size))
        .chain(std::iter::once((
            "manifest.json".to_owned(),
            data.len() as u64,
        )))
        .collect();
    if zip.len() != expected.len() {
        return Err("Unexpected or duplicated backup entries".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for index in 0..zip.len() {
        let file = zip.by_index(index).map_err(archive_error)?;
        if file.is_dir()
            || !seen.insert(file.name().to_owned())
            || expected.get(file.name()) != Some(&file.size())
            || file
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 != 0 && mode & 0o170000 != 0o100000)
        {
            return Err("Unexpected backup path, type or size".into());
        }
    }
    Ok(manifest)
}
pub(super) fn verify_file<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    book: &Publication,
    mut output: impl Write,
) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    let mut input = zip
        .by_name(&format!("books/{}.epub", book.hash))
        .map_err(archive_error)?;
    let mut buffer = [0u8; 64 * 1024];
    let mut digest = Sha256::new();
    let mut total = 0u64;
    loop {
        let size = input.read(&mut buffer).map_err(archive_error)?;
        if size == 0 {
            break;
        }
        total = total
            .checked_add(size as u64)
            .ok_or("Oversized backup publication")?;
        if total > book.size {
            return Err("Backup publication exceeds declared size".into());
        }
        digest.update(&buffer[..size]);
        output.write_all(&buffer[..size]).map_err(archive_error)?;
    }
    if total != book.size || format!("{:x}", digest.finalize()) != book.hash {
        return Err("Backup publication checksum mismatch".into());
    }
    Ok(())
}
pub fn inspect(source: &Path) -> Result<BackupSummary, String> {
    inspect_with(source, None)
}
pub(super) fn inspect_with(source: &Path, root: Option<&str>) -> Result<BackupSummary, String> {
    let mut zip =
        ZipArchive::new(fs::File::open(source).map_err(archive_error)?).map_err(archive_error)?;
    let manifest = read_manifest(&mut zip)?;
    let scratch = Scratch::new(&std::env::temp_dir())?;
    let total = manifest.books.iter().filter(|b| b.included).count();
    for (index, book) in manifest.books.iter().filter(|b| b.included).enumerate() {
        if let Some(root) = root {
            progress::update(root, "verifying", index, total);
        }
        let path = scratch.0.join(format!("{}.epub", book.hash));
        admit(&mut zip, book, &path, &scratch.0.join("chapter-index"))?;
        fs::remove_file(path).map_err(|_| "Cannot remove checked backup staging file")?;
    }
    manifest.summary()
}
pub(super) fn admit<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    book: &Publication,
    path: &Path,
    cache: &Path,
) -> Result<(), String> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut output = options
        .open(path)
        .map_err(|_| "Cannot stage restored publication")?;
    verify_file(zip, book, &mut output)?;
    output
        .sync_all()
        .map_err(|_| "Cannot flush restored publication")?;
    let parsed = crate::epub::EpubBook::open_cached(path, cache)?;
    if parsed.chapters.is_empty() {
        return Err("Backup publication has no readable chapters".into());
    }
    Ok(())
}
