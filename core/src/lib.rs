//! The core reader library exposing API for the desktop app and the Android bridge.
pub mod csc;
pub mod epub;
pub mod export;
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
/// Rejects path separators, control characters, and reserved filenames.
pub fn sanitize_filename(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .take(200)
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c > '\x7F' {
                // Reject Unicode characters that resemble path separators
                if c == '\u{FF0F}' || c == '\u{FF3C}' || c == '\u{2044}' || c == '\u{2215}' {
                    '_'
                } else {
                    c
                }
            } else {
                '_'
            }
        })
        .collect();
    let sanitized = sanitized.trim_matches('.').to_string();
    // Reject empty, path traversal sequences, and Windows reserved names
    let upper = sanitized.to_uppercase();
    let is_reserved = matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" |
        "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9" |
        "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9"
    );
    if sanitized.is_empty() || sanitized.contains("..") || is_reserved {
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
