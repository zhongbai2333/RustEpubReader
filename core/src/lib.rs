//! The core reader library exposing API for the desktop app and the Android bridge.
pub mod appearance;
pub mod book_share;
pub mod cloud_progress;
pub mod cloud_recent;
pub mod cloud_sync;
pub mod csc;
pub mod diagnostics;
pub mod epub;
pub mod export;
#[cfg(feature = "github-sync")]
pub mod github_sync;
pub mod i18n;
pub mod image_resources;
pub mod library;
pub mod search;
pub mod sharing;
pub mod storage;
pub mod txt;

use std::time::{SystemTime, UNIX_EPOCH};

/// Current time as milliseconds since UNIX epoch.
pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Current time as seconds since UNIX epoch.
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Sanitize a string for use as a filename, with length limit.
pub fn sanitize_filename(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .take(200)
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c > '\x7F' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let sanitized = sanitized.trim_matches('.').to_string();
    if sanitized.is_empty() || sanitized.contains("..") {
        format!("book_{}", now_secs())
    } else {
        sanitized
    }
}

pub fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

pub fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|e| format!("base64 decode: {e}"))
}

/// HTML/XML escape for use in XHTML output.
pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Compute SHA-256 hash of a file, streaming in 8 KiB chunks.
pub fn file_hash(path: &str) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    hash_reader(std::io::BufReader::new(file))
}
fn hash_reader(mut reader: impl std::io::Read) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        open_operation::check()?;
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub mod open_operation;

#[cfg(test)]
mod cancellation_io_tests {
    #[test]
    fn hashing_stops_after_the_inflight_chunk_and_preserves_ordinary_hashes() {
        struct Source {
            token: crate::open_operation::Cancellation,
            reads: usize,
        }
        impl std::io::Read for Source {
            fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
                self.reads += 1;
                assert_eq!(self.reads, 1, "Cancelled hashing read another chunk");
                bytes.fill(42);
                self.token.cancel();
                Ok(bytes.len())
            }
        }
        let token = crate::open_operation::Cancellation::default();
        let result = token.run(|| {
            crate::hash_reader(Source {
                token: token.clone(),
                reads: 0,
            })
        });
        assert_eq!(result.unwrap_err(), crate::open_operation::CANCELLED);
        assert_eq!(
            crate::hash_reader(&b"ordinary"[..]).unwrap(),
            crate::bytes_hash(b"ordinary")
        );
    }
}

/// Compute SHA-256 hash of in-memory bytes.
pub fn bytes_hash(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}
pub mod annotations;
pub mod backup;
pub mod reading_activity;
