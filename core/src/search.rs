//! Book content and library metadata search engine.
use crate::epub::{ContentBlock, EpubBook};

#[derive(Clone, Debug)]
pub struct SearchResult {
    pub chapter_index: usize,
    pub chapter_title: String,
    pub block_index: usize,
    pub context: String,
    pub match_start: usize,
    pub match_len: usize,
}

/// Search the full text of an EPUB book for a query string.
pub fn search_book(book: &EpubBook, query: &str, case_sensitive: bool) -> Vec<SearchResult> {
    let mut results = Vec::new();
    if query.is_empty() {
        return results;
    }

    let query_cmp = if case_sensitive {
        query.to_string()
    } else {
        query.to_lowercase()
    };

    for (ch_idx, chapter) in book.chapters.iter().enumerate() {
        for (blk_idx, block) in chapter.blocks.iter().enumerate() {
            let text = block_text(block);
            if text.is_empty() {
                continue;
            }

            let search_text = if case_sensitive {
                text.clone()
            } else {
                text.to_lowercase()
            };
            let mut start = 0;
            while let Some(pos) = search_text[start..].find(&query_cmp) {
                let abs_pos = start + pos;
                let ctx_start = text
                    .char_indices()
                    .rev()
                    .filter(|&(i, _)| i <= abs_pos)
                    .nth(20)
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                let ctx_end = text
                    .char_indices()
                    .filter(|&(i, _)| i >= abs_pos + query.len())
                    .nth(20)
                    .map(|(i, _)| i)
                    .unwrap_or(text.len());
                let context = text[ctx_start..ctx_end].to_string();

                results.push(SearchResult {
                    chapter_index: ch_idx,
                    chapter_title: chapter.title.clone(),
                    block_index: blk_idx,
                    context,
                    match_start: abs_pos,
                    match_len: query.len(),
                });
                start = abs_pos + query.len();
            }
        }
    }
    results
}

fn block_text(block: &ContentBlock) -> String {
    match block {
        ContentBlock::Paragraph { spans, .. } | ContentBlock::Heading { spans, .. } => {
            spans.iter().map(|s| s.text.as_str()).collect()
        }
        _ => String::new(),
    }
}
