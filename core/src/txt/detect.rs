//! Bounded, strict decoding. Never silently replace undecodable book text.
use super::TxtError;
use chardetng::EncodingDetector;
use std::{fs::File, io::Read, path::Path};
pub const MAX_TXT_BYTES: u64 = 64 * 1024 * 1024;
pub struct DecodedText {
    pub text: String,
    pub encoding: String,
}
pub fn read_txt_file(path: &Path) -> Result<DecodedText, TxtError> {
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if metadata.len() > MAX_TXT_BYTES {
        return Err(TxtError::Parse("TXT_TOO_LARGE".into()));
    }
    if !metadata.is_file() {
        return Err(TxtError::Parse("TXT_NOT_REGULAR_FILE".into()));
    }
    let mut raw = Vec::new();
    file.take(MAX_TXT_BYTES + 1).read_to_end(&mut raw)?;
    if raw.len() as u64 > MAX_TXT_BYTES {
        return Err(TxtError::Parse("TXT_TOO_LARGE".into()));
    }
    let (encoding, content) = if raw.starts_with(&[0xFF, 0xFE]) {
        (encoding_rs::UTF_16LE, &raw[2..])
    } else if raw.starts_with(&[0xFE, 0xFF]) {
        (encoding_rs::UTF_16BE, &raw[2..])
    } else if raw.starts_with(&[0xEF, 0xBB, 0xBF]) {
        (encoding_rs::UTF_8, &raw[3..])
    } else {
        let content = &raw[..];
        if let Ok(text) = std::str::from_utf8(content) {
            return Ok(DecodedText {
                text: text.into(),
                encoding: "UTF-8".into(),
            });
        }
        let mut detector = EncodingDetector::new();
        detector.feed(content, true);
        (detector.guess(Some(b"cn"), true), content)
    };
    let (text, errors) = encoding.decode_without_bom_handling(content);
    if errors {
        return Err(TxtError::Encoding("TXT_ENCODING_FAILED".into()));
    }
    Ok(DecodedText {
        text: text.into_owned(),
        encoding: encoding.name().into(),
    })
}
