use super::{ticket::Ticket, Work};
use crate::{
    epub::EpubBook,
    library::{BookEntry, Library},
    sharing::{aes_decrypt, aes_encrypt},
};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

pub const MAX_BOOK: u64 = 128 * 1024 * 1024;
pub const MAX_TOTAL: u64 = 512 * 1024 * 1024;
pub const MAX_PACKAGE: u64 = MAX_TOTAL + 2 * 1024 * 1024;
const CHUNK: usize = 65536;
const MAGIC: &[u8; 9] = b"RERSHARE1";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BookInfo {
    pub title: String,
    pub hash: String,
    pub size: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub version: u8,
    pub books: Vec<BookInfo>,
}
impl Manifest {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.books.is_empty() || self.books.len() > 100 {
            return Err("Invalid share book list".into());
        }
        let mut total = 0u64;
        for book in &self.books {
            if book.title.trim().is_empty()
                || book.title.len() > 4096
                || !crate::cloud_sync::valid_hash(&book.hash)
                || book.size == 0
                || book.size > MAX_BOOK
            {
                return Err("Invalid shared book".into());
            }
            total = total
                .checked_add(book.size)
                .ok_or("Share package is too large")?;
            if total > MAX_TOTAL {
                return Err("Share package is too large".into());
            }
        }
        Ok(())
    }
}
/// Authenticate the bounded manifest before accepting a large anonymous body.
pub fn inspect_prefix(prefix: &[u8], ticket: &Ticket) -> Result<Option<u64>, String> {
    if prefix.len() < 9 {
        return Ok(None);
    }
    if &prefix[..9] != MAGIC {
        return Err("Unsupported share package format".into());
    }
    if prefix.len() < 13 {
        return Ok(None);
    }
    let length = u32::from_be_bytes(prefix[9..13].try_into().unwrap()) as usize;
    if !(16..=512 * 1024 + 16).contains(&length) {
        return Err("Invalid encrypted book list".into());
    }
    if prefix.len() < 13 + length {
        return Ok(None);
    }
    let bytes = aes_decrypt(&ticket.secret()?, 0, &prefix[13..13 + length])
        .map_err(|_| "Share package could not be verified; the key or file is incorrect")?;
    let manifest: Manifest =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid encrypted book list")?;
    manifest.validate()?;
    let length = 13
        + length as u64
        + manifest
            .books
            .iter()
            .map(|b| b.size + b.size.div_ceil(CHUNK as u64) * 20)
            .sum::<u64>();
    Ok(Some(length))
}
fn validate_epub(path: &Path) -> Result<(), String> {
    let input = File::open(path).map_err(|_| "Cannot validate shared EPUB")?;
    let mut archive =
        zip::ZipArchive::new(input).map_err(|_| "Shared file is not a valid EPUB archive")?;
    if archive.len() > 20_000 {
        return Err("Shared EPUB has too many resources".into());
    }
    let mut expanded = 0u64;
    for index in 0..archive.len() {
        let file = archive
            .by_index_raw(index)
            .map_err(|_| "Cannot validate shared EPUB resources")?;
        expanded = expanded
            .checked_add(file.size())
            .ok_or("Shared EPUB resources are too large")?;
        if file.size() > MAX_BOOK || expanded > MAX_TOTAL {
            return Err("Shared EPUB resources are too large".into());
        }
    }
    Ok(())
}
pub struct Scratch(pub PathBuf);
impl Scratch {
    pub fn new(root: &str) -> Result<Self, String> {
        let parent = Path::new(root).join(".book-share-tmp");
        if std::fs::symlink_metadata(&parent).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("Cannot protect share storage".into());
        }
        let path = Path::new(root)
            .join(".book-share-tmp")
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&path).map_err(|_| "Cannot prepare share storage")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                .map_err(|_| "Cannot protect share storage")?;
        }
        let marker = path.join(".owner.json");
        let mut owner = create_private(&marker)?;
        owner.write_all(&serde_json::to_vec(&serde_json::json!({"purpose":"reader-book-share-staging-v1","pid":std::process::id(),"created":crate::now_secs()})).map_err(|_|"Cannot protect share storage")?).map_err(|_|"Cannot protect share storage")?;
        Ok(Self(path))
    }
}
/// Only our marked UUID directories from dead processes are recoverable cache.
/// Never follows symlinks or traverses book/library/evidence directories.
pub fn cleanup_abandoned(root: &str) {
    let parent = Path::new(root).join(".book-share-tmp");
    if !std::fs::symlink_metadata(&parent).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
    {
        return;
    }
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten().take(1000) {
        if uuid::Uuid::parse_str(&entry.file_name().to_string_lossy()).is_err()
            || !entry.file_type().is_ok_and(|t| t.is_dir())
        {
            continue;
        }
        let marker = entry.path().join(".owner.json");
        if !std::fs::symlink_metadata(&marker).is_ok_and(|m| m.is_file() && m.len() <= 1024) {
            continue;
        }
        let Some(value) = std::fs::read(&marker)
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        else {
            continue;
        };
        if value["purpose"].as_str() != Some("reader-book-share-staging-v1") {
            continue;
        }
        let Some(pid) = value["pid"]
            .as_u64()
            .and_then(|p| i32::try_from(p).ok())
            .filter(|p| *p > 0)
        else {
            continue;
        };
        #[cfg(unix)]
        let alive = unsafe { libc::kill(pid, 0) == 0 }
            || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM);
        #[cfg(not(unix))]
        let alive = value["created"]
            .as_u64()
            .is_none_or(|time| crate::now_secs().saturating_sub(time) < 86400);
        if !alive {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub fn create_private(path: &Path) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .map_err(|_| "Cannot create share file".into())
}
pub struct Package {
    pub _scratch: Scratch,
    pub path: PathBuf,
    pub key: [u8; 32],
    pub manifest: Manifest,
}
pub struct Received {
    pub _scratch: Scratch,
    pub manifest: Manifest,
    pub paths: Vec<PathBuf>,
}
fn frame_write(
    file: &mut File,
    key: &[u8; 32],
    counter: &mut u64,
    data: &[u8],
) -> Result<(), String> {
    let encrypted = aes_encrypt(key, *counter, data)?;
    *counter += 1;
    file.write_all(&(encrypted.len() as u32).to_be_bytes())
        .and_then(|_| file.write_all(&encrypted))
        .map_err(|_| "Cannot write encrypted package".into())
}
fn frame_read(
    file: &mut File,
    key: &[u8; 32],
    counter: &mut u64,
    maximum: usize,
) -> Result<Vec<u8>, String> {
    let mut len = [0; 4];
    file.read_exact(&mut len)
        .map_err(|_| "Incomplete encrypted package")?;
    let length = u32::from_be_bytes(len) as usize;
    if length < 16 || length > maximum + 16 {
        return Err("Invalid encrypted package frame".into());
    }
    let mut bytes = vec![0; length];
    file.read_exact(&mut bytes)
        .map_err(|_| "Incomplete encrypted package")?;
    let result = aes_decrypt(key, *counter, &bytes)
        .map_err(|_| "Share package could not be verified; the key or file is incorrect")?;
    *counter += 1;
    Ok(result)
}
pub fn prepare(root: &str, ids: &[String], work: &Arc<Work>) -> Result<Package, String> {
    if ids.is_empty() || ids.len() > 100 {
        return Err("Select between 1 and 100 books".into());
    }
    let library = Library::read_snapshot(root)?;
    let books = ids
        .iter()
        .map(|id| {
            library
                .books
                .iter()
                .find(|b| &b.id == id)
                .cloned()
                .ok_or("Book is no longer in the library".into())
        })
        .collect::<Result<Vec<BookEntry>, String>>()?;
    let mut manifest = Manifest {
        version: 1,
        books: Vec::new(),
    };
    for book in &books {
        work.check()?;
        validate_epub(Path::new(&book.path))?;
        if EpubBook::read_metadata(&book.path).is_none() {
            return Err("Selected file is not a readable EPUB".into());
        }
        manifest.books.push(BookInfo {
            title: book.title.clone(),
            hash: crate::file_hash(&book.path)?,
            size: std::fs::metadata(&book.path)
                .map_err(|_| "Cannot read selected book")?
                .len(),
        });
    }
    manifest.validate()?;
    let total = manifest.books.iter().map(|b| b.size).sum();
    work.phase("preparing", total);
    let scratch = Scratch::new(root)?;
    let path = scratch.0.join("package.rershare");
    let mut file = create_private(&path)?;
    let mut key = [0; 32];
    OsRng.fill_bytes(&mut key);
    let mut counter = 0;
    file.write_all(MAGIC)
        .map_err(|_| "Cannot write share package")?;
    let metadata = serde_json::to_vec(&manifest).map_err(|_| "Cannot encode share book list")?;
    if metadata.len() > 512 * 1024 {
        return Err("Share book list is too large".into());
    }
    frame_write(&mut file, &key, &mut counter, &metadata)?;
    let mut buffer = [0; CHUNK];
    for (book, info) in books.iter().zip(&manifest.books) {
        let mut source = File::open(&book.path).map_err(|_| "Cannot open selected book")?;
        let mut remaining = info.size;
        let mut hash = Sha256::new();
        while remaining > 0 {
            work.check()?;
            let n = (remaining as usize).min(CHUNK);
            source
                .read_exact(&mut buffer[..n])
                .map_err(|_| "Selected book changed during preparation")?;
            hash.update(&buffer[..n]);
            frame_write(&mut file, &key, &mut counter, &buffer[..n])?;
            remaining -= n as u64;
            work.advance(n as u64);
        }
        let mut extra = [0];
        if source
            .read(&mut extra)
            .map_err(|_| "Cannot read selected book")?
            != 0
            || format!("{:x}", hash.finalize()) != info.hash
        {
            return Err("Selected book changed during preparation".into());
        }
    }
    file.sync_all()
        .map_err(|_| "Cannot save encrypted package")?;
    Ok(Package {
        _scratch: scratch,
        path,
        key,
        manifest,
    })
}
pub fn decode(
    root: &str,
    path: &Path,
    ticket: &Ticket,
    work: &Arc<Work>,
) -> Result<Received, String> {
    if std::fs::metadata(path)
        .map_err(|_| "Cannot read encrypted package")?
        .len()
        > MAX_PACKAGE
    {
        return Err("Share package is too large".into());
    }
    let mut file = File::open(path).map_err(|_| "Cannot read encrypted package")?;
    let mut magic = [0; 9];
    file.read_exact(&mut magic)
        .map_err(|_| "Incomplete share package")?;
    if &magic != MAGIC {
        return Err("Unsupported share package format".into());
    }
    let key = ticket.secret()?;
    let mut counter = 0;
    let manifest: Manifest =
        serde_json::from_slice(&frame_read(&mut file, &key, &mut counter, 512 * 1024)?)
            .map_err(|_| "Invalid encrypted book list")?;
    manifest.validate()?;
    work.phase("verifying", manifest.books.iter().map(|b| b.size).sum());
    let scratch = Scratch::new(root)?;
    let mut paths = Vec::new();
    for (index, book) in manifest.books.iter().enumerate() {
        work.check()?;
        let path = scratch.0.join(format!("{index}.epub"));
        let mut output = create_private(&path)?;
        let mut remaining = book.size;
        let mut hash = Sha256::new();
        while remaining > 0 {
            work.check()?;
            let bytes = frame_read(&mut file, &key, &mut counter, CHUNK)?;
            if bytes.len() as u64 != remaining.min(CHUNK as u64) {
                return Err("Invalid shared book length".into());
            }
            hash.update(&bytes);
            output
                .write_all(&bytes)
                .map_err(|_| "Cannot save received book")?;
            remaining -= bytes.len() as u64;
            work.advance(bytes.len() as u64);
        }
        output.sync_all().map_err(|_| "Cannot save received book")?;
        if format!("{:x}", hash.finalize()) != book.hash {
            return Err("Shared book checksum does not match".into());
        }
        work.check()?;
        validate_epub(&path)?;
        let parsed = EpubBook::open(path.to_str().ok_or("Invalid received book path")?)?;
        if parsed.chapters.is_empty() {
            return Err("Shared EPUB has no readable chapters".into());
        }
        paths.push(path);
    }
    let mut extra = [0];
    if file
        .read(&mut extra)
        .map_err(|_| "Cannot verify share package")?
        != 0
    {
        return Err("Unexpected data after share package".into());
    }
    Ok(Received {
        _scratch: scratch,
        manifest,
        paths,
    })
}
