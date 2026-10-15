//! Core EPUB parsing, rendering, and book processing logic.
pub mod chapter;
pub mod native;
mod native_css;
mod native_typography;
pub mod parser;

pub use chapter::{
    Chapter, ContentBlock, CorrectionInfo, CorrectionStatus, InlineStyle, TextSpan, TocEntry,
};
pub use parser::{EpubBook, EpubMetadata};
