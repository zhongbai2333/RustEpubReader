//! Data structures and logic related to EPUB chapters.
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum InlineStyle {
    Normal,
    Bold,
    Italic,
    BoldItalic,
}

impl InlineStyle {
    /// Stable string representation for cross-platform serialization.
    pub fn as_str(&self) -> &'static str {
        match self {
            InlineStyle::Normal => "Normal",
            InlineStyle::Bold => "Bold",
            InlineStyle::Italic => "Italic",
            InlineStyle::BoldItalic => "BoldItalic",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum CorrectionStatus {
    #[default]
    Pending,
    Accepted,
    Rejected,
    Ignored,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CorrectionInfo {
    pub original: String,
    pub corrected: String,
    pub confidence: f32,
    /// Character offset within the block text (concatenated spans).
    #[serde(default)]
    pub char_offset: usize,
    pub status: CorrectionStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextSpan {
    pub text: String,
    pub style: InlineStyle,
    pub link_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correction: Option<CorrectionInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    Heading {
        level: u8,
        spans: Vec<TextSpan>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        anchor_id: Option<String>,
    },
    Paragraph {
        spans: Vec<TextSpan>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        anchor_id: Option<String>,
    },
    Image {
        data: Arc<Vec<u8>>,
        alt: Option<String>,
    },
    Separator,
    BlankLine,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Chapter {
    pub title: String,
    pub blocks: Vec<ContentBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_href: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TocEntry {
    pub title: String,
    pub chapter_index: usize,
}
