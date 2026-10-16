//! The core reader library exposing API for the desktop app and the Android bridge.
pub mod appearance;
pub mod book_share;
pub mod cloud_progress;
pub mod cloud_sync;
pub mod csc;
pub mod epub;
pub mod export;
#[cfg(feature = "github-sync")]
pub mod github_sync;
pub mod i18n;
pub mod library;
pub mod search;
pub mod sharing;
pub mod txt;

use std::time::{SystemTime, UNIX_EPOCH};

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
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut reader = std::io::BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Compute SHA-256 hash of in-memory bytes.
pub fn bytes_hash(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}
pub mod reading_activity;
