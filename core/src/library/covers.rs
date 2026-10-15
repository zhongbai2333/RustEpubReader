//! Shelf-only thumbnails. Never parse a book's chapters or populate its reader cache.
use crate::epub::EpubBook;
use image::{codecs::jpeg::JpegEncoder, ImageReader};
use std::{io::Cursor, path::Path, time::UNIX_EPOCH};

const MAX_ENCODED: u64 = 1024 * 1024;
pub const WIDTH: u32 = 480;
pub const HEIGHT: u32 = 720;

/// Call on a background worker. The source's size/mtime invalidate positive and
/// negative disk-cache entries without hashing the entire EPUB on each launch.
pub fn thumbnail(source: &Path, cache: &Path) -> Option<Vec<u8>> {
    let metadata = source.metadata().ok()?;
    let modified = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    let identity = crate::bytes_hash(
        format!(
            "{}:{}:{}",
            source.display(),
            metadata.len(),
            modified.as_nanos()
        )
        .as_bytes(),
    );
    let destination = cache.join(format!("{identity}.jpg"));
    if let Ok(meta) = destination.metadata() {
        if meta.len() <= MAX_ENCODED {
            if let Ok(bytes) = std::fs::read(&destination) {
                if bytes.is_empty() {
                    return None;
                }
                if let Some((w, h)) = ImageReader::new(Cursor::new(&bytes))
                    .with_guessed_format()
                    .ok()
                    .and_then(|r| r.into_dimensions().ok())
                {
                    if w > 0 && h > 0 && w <= WIDTH && h <= HEIGHT {
                        return Some(bytes);
                    }
                }
            }
        }
    }
    let data = EpubBook::read_cover(source).ok()?;
    let bytes = data.and_then(|data| {
        if data.len() > 16 * 1024 * 1024 {
            return None;
        }
        let mut reader = ImageReader::new(Cursor::new(data))
            .with_guessed_format()
            .ok()?;
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        let image = reader.decode().ok()?;
        let image = if image.width() > WIDTH || image.height() > HEIGHT {
            image.thumbnail(WIDTH, HEIGHT)
        } else {
            image
        }
        .to_rgb8();
        let mut bytes = Vec::new();
        JpegEncoder::new_with_quality(&mut bytes, 82)
            .encode_image(&image)
            .ok()?;
        (bytes.len() as u64 <= MAX_ENCODED).then_some(bytes)
    });
    // Empty files cache a genuinely missing/unusable cover. Failed EPUB reads
    // return above and are retried on the next request rather than persisted.
    if std::fs::create_dir_all(cache).is_ok() {
        let temporary = cache.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
        if std::fs::write(&temporary, bytes.as_deref().unwrap_or_default()).is_ok() {
            let _ = std::fs::rename(&temporary, &destination);
        }
        let _ = std::fs::remove_file(temporary);
    }
    bytes
}
