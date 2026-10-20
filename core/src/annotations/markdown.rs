//! Offline export; only chapters referenced by an annotation are requested.
use super::*;
use crate::epub::{Chapter, ContentBlock, EpubBook};
use serde::Serialize;

#[derive(Serialize)]
pub struct MarkdownExport {
    pub filename: String,
    pub markdown: String,
}
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if "\\`*_{}[]<>|#!~".contains(c) {
            out.push('\\');
        }
        if c == '\r' {
            continue;
        }
        out.push(c);
    }
    out
}
fn header(text: &str) -> String {
    escape(&text.replace(['\n', '\r'], " "))
}
fn text(block: &ContentBlock) -> String {
    match block {
        ContentBlock::Paragraph { spans, .. } | ContentBlock::Heading { spans, .. } => {
            spans.iter().map(|s| s.text.as_str()).collect()
        }
        ContentBlock::Image { alt, .. } => alt.clone().unwrap_or_default(),
        _ => String::new(),
    }
}
fn excerpt(chapter: &Chapter, highlight: &Highlight) -> Result<String, String> {
    if highlight.start_block > highlight.end_block || highlight.end_block >= chapter.blocks.len() {
        return Err("Highlight is outside the exported chapter".into());
    }
    let mut parts = Vec::new();
    for index in highlight.start_block..=highlight.end_block {
        let text = text(&chapter.blocks[index]);
        let count = text.chars().count();
        let start = if index == highlight.start_block {
            highlight.start_offset
        } else {
            0
        };
        let end = if index == highlight.end_block {
            highlight.end_offset
        } else {
            count
        };
        if start > end || end > count {
            return Err("Highlight text offsets are outside the exported chapter".into());
        }
        parts.push(
            text.chars()
                .skip(start)
                .take(end - start)
                .collect::<String>(),
        );
    }
    Ok(parts.join("\n"))
}
fn render(
    config: &BookConfig,
    mut chapter: impl FnMut(usize) -> Result<Arc<Chapter>, String>,
) -> Result<String, String> {
    let mut output = format!("# {}\n\n", header(&config.title));
    if let Some(author) = config
        .metadata
        .as_ref()
        .and_then(|m| m.author.as_ref())
        .filter(|a| !a.is_empty())
    {
        output.push_str(&format!("{}\n\n", header(author)));
    }
    let mut locations: std::collections::BTreeSet<_> =
        config.bookmarks.iter().map(|b| b.chapter).collect();
    locations.extend(config.highlights.iter().map(|h| h.chapter));
    for location in locations {
        let chapter = chapter(location)?;
        output.push_str(&format!("## {}\n\n", header(&chapter.title)));
        for b in config.bookmarks.iter().filter(|b| b.chapter == location) {
            output.push_str(&format!("- Bookmark — block {}\n", b.block + 1));
        }
        output.push('\n');
        for h in config.highlights.iter().filter(|h| h.chapter == location) {
            let quote = excerpt(&chapter, h)?;
            for line in escape(&quote).lines() {
                output.push_str("> ");
                output.push_str(line);
                output.push('\n');
            }
            output.push('\n');
            for note in config.notes.iter().filter(|n| n.highlight_id == h.id) {
                output.push_str(&escape(&note.content));
                output.push_str("\n\n");
            }
            if output.len() > 16 * 1024 * 1024 {
                return Err("Markdown export exceeds size limit".into());
            }
        }
    }
    Ok(output)
}
pub fn export_markdown(root: &str, id: &str) -> Result<MarkdownExport, String> {
    let config = load_config(root, id)?.ok_or("Book is no longer available")?;
    let book = EpubBook::open_cached(
        &config.epub_path,
        &Path::new(root).join(".cache/chapter-index"),
    )?;
    let markdown = render(&config, |index| book.chapter(index))?;
    let display_name = crate::export::book_filename(&config.title);
    let stem = display_name
        .strip_suffix(".epub")
        .ok_or("Invalid export filename")?;
    Ok(MarkdownExport {
        filename: format!("{stem}-annotations.md"),
        markdown,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_native_coordinate_contract_uses_scalar_ranges_in_markdown() {
        let vectors: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/reader-coordinates.json"
        ))
        .unwrap();
        assert_eq!(vectors["persisted_unit"], "unicode_scalar");
        for case in vectors["cases"].as_array().unwrap() {
            let source = case["source"].as_str().unwrap();
            let chapter = Chapter {
                native_document: Default::default(),
                title: "Contract".into(),
                source_href: None,
                blocks: vec![ContentBlock::Paragraph {
                    native_style: Default::default(),
                    anchor_id: None,
                    spans: vec![crate::epub::TextSpan {
                        text: source.into(),
                        style: crate::epub::InlineStyle::Normal,
                        link_url: None,
                        correction: None,
                    }],
                }],
            };
            for range in case["ranges"].as_array().unwrap() {
                let highlight = Highlight {
                    id: "contract".into(),
                    chapter: 0,
                    start_block: 0,
                    end_block: 0,
                    start_offset: range["start"].as_u64().unwrap() as usize,
                    end_offset: range["end"].as_u64().unwrap() as usize,
                    color: Default::default(),
                    created_at: 1,
                };
                assert_eq!(
                    excerpt(&chapter, &highlight).unwrap(),
                    range["text"].as_str().unwrap()
                );
            }
        }
    }

    #[test]
    fn export_is_unicode_safe_escapes_book_text_and_only_loads_referenced_chapters() {
        let (dir, id) = super::super::tests::book();
        let mut config = load_config(dir.path().to_str().unwrap(), &id)
            .unwrap()
            .unwrap();
        config.bookmarks.push(Bookmark {
            chapter: 7,
            block: 1,
            created_at: 1,
        });
        config.highlights.push(Highlight {
            id: "h".into(),
            chapter: 7,
            start_block: 0,
            start_offset: 1,
            end_block: 0,
            end_offset: 4,
            color: Default::default(),
            created_at: 1,
        });
        config.notes.push(Note {
            highlight_id: "h".into(),
            content: "![remote](https://example.invalid/pixel)\n<em>note</em>".into(),
            created_at: 1,
            updated_at: 1,
        });
        let mut requested = Vec::new();
        let result = render(&config, |index| {
            requested.push(index);
            Ok(Arc::new(Chapter {
                native_document: Default::default(),
                title: "章 *七*".into(),
                source_href: None,
                blocks: vec![ContentBlock::Paragraph {
                    native_style: Default::default(),
                    spans: vec![crate::epub::TextSpan {
                        text: "甲😀乙丙丁".into(),
                        style: crate::epub::InlineStyle::Normal,
                        link_url: None,
                        correction: None,
                    }],
                    anchor_id: None,
                }],
            }))
        })
        .unwrap();
        assert_eq!(requested, vec![7]);
        assert!(result.contains("> 😀乙丙"));
        assert!(result.contains("章 \\*七\\*"));
        assert!(result.contains("\\!\\[remote\\]"));
        assert!(result.contains("\\<em\\>note"));
    }
}
