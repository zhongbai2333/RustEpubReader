//! Essential types and data structures used by the library management.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub(crate) fn is_zero(v: &usize) -> bool {
    *v == 0
}

// ── Annotation / Stats types ──

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Bookmark {
    pub chapter: usize,
    pub block: usize,
    pub created_at: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub enum HighlightColor {
    #[default]
    Yellow,
    Green,
    Blue,
    Pink,
}

impl HighlightColor {
    pub fn css_class(&self) -> &'static str {
        match self {
            Self::Yellow => "highlight-yellow",
            Self::Green => "highlight-green",
            Self::Blue => "highlight-blue",
            Self::Pink => "highlight-pink",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Highlight {
    pub id: String,
    pub chapter: usize,
    pub start_block: usize,
    pub start_offset: usize,
    pub end_block: usize,
    pub end_offset: usize,
    pub color: HighlightColor,
    pub created_at: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Note {
    pub highlight_id: String,
    pub content: String,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CorrectionRecord {
    pub chapter: usize,
    pub block_idx: usize,
    pub char_offset: usize,
    pub original: String,
    pub corrected: String,
    pub status: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ReadingStats {
    pub total_seconds: u64,
    #[serde(default)]
    pub sessions: Vec<ReadingSession>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReadingSession {
    pub date: String,
    pub seconds: u64,
}

// ── Core types ──

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BookEntry {
    #[serde(default)]
    pub id: String,
    pub title: String,
    pub path: String,
    pub last_chapter: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub last_block: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub last_char_offset: usize,
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
    #[serde(default, skip_serializing_if = "is_zero")]
    pub last_block: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub last_char_offset: usize,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bookmarks: Vec<Bookmark>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub highlights: Vec<Highlight>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<Note>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub corrections: Vec<CorrectionRecord>,
    /// Number of resolved corrections at last contribution prompt.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub last_contribute_prompt_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reading_stats: Option<ReadingStats>,
}

impl BookConfig {
    pub fn save(&self, data_dir: &str) {
        let dir = PathBuf::from(data_dir).join("books");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("{}.json", self.id));
        if let Ok(data) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, data);
        }
    }
}
