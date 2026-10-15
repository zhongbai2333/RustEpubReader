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
        #[serde(default)]
        native_style: super::native::NativeStyle,
        level: u8,
        spans: Vec<TextSpan>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        anchor_id: Option<String>,
    },
    Paragraph {
        #[serde(default)]
        native_style: super::native::NativeStyle,
        spans: Vec<TextSpan>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        anchor_id: Option<String>,
    },
    Image {
        #[serde(default)]
        native_style: super::native::NativeStyle,
        data: Arc<Vec<u8>>,
        alt: Option<String>,
    },
    Separator,
    BlankLine,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Chapter {
    #[serde(skip)]
    pub native_document: std::sync::Arc<super::native::NativeDocument>,
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

impl ContentBlock {
    pub fn native_style(&self) -> super::native::NativeStyle {
        match self {
            Self::Paragraph { native_style, .. }
            | Self::Heading { native_style, .. }
            | Self::Image { native_style, .. } => native_style.clone(),
            _ => Default::default(),
        }
    }
    pub fn set_native_style(&mut self, style: super::native::NativeStyle) {
        match self {
            Self::Paragraph { native_style, .. }
            | Self::Heading { native_style, .. }
            | Self::Image { native_style, .. } => *native_style = style,
            _ => {}
        }
    }
}
impl Chapter {
    pub fn apply_presentation(
        &mut self,
        policy: super::native::CssPolicy,
        scripts: bool,
        events: &[super::native::NativeEvent],
    ) -> Vec<String> {
        let result = self.native_document.present(policy, scripts, events);
        for (block, style) in self.blocks.iter_mut().zip(result.styles) {
            block.set_native_style(style);
        }
        result.errors
    }
}
