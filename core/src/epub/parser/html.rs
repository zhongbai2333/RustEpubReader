//! Contains HTML and XHTML parsing logic for EPUB content.
use std::collections::{HashMap, HashSet};

use scraper::{ElementRef, Html, Selector};

use super::image::resolve_image_data;
use crate::epub::{ContentBlock, InlineStyle, TextSpan};

pub(super) fn parse_html_blocks(
    html: &str,
    chapter_path: &str,
    image_resources: &HashMap<String, Vec<u8>>,
) -> Vec<ContentBlock> {
    parse_blocks(html, chapter_path, image_resources, false)
        .into_iter()
        .map(|(_, _, block)| block)
        .collect()
}

/// Supplement media for rendering without renumbering persisted content blocks.
/// Images inside a paragraph are displayed before its original text; the shared
/// anchor still refers to that original text block (or the preceding block's end).
pub(super) fn parse_render_blocks(
    html: &str,
    chapter_path: &str,
    image_resources: &HashMap<String, Vec<u8>>,
) -> Vec<(usize, usize, ContentBlock)> {
    parse_blocks(html, chapter_path, image_resources, true)
}
fn parse_blocks(
    html: &str,
    chapter_path: &str,
    image_resources: &HashMap<String, Vec<u8>>,
    render: bool,
) -> Vec<(usize, usize, ContentBlock)> {
    let document = Html::parse_document(html);
    let body_sel = Selector::parse("body").expect("valid selector");
    let start_elem = document
        .select(&body_sel)
        .next()
        .unwrap_or_else(|| document.root_element());
    let mut blocks = Vec::new();
    let mut media = Vec::new();
    collect_blocks(
        start_elem,
        &mut blocks,
        chapter_path,
        image_resources,
        render,
        &mut media,
    );
    let mut media: HashSet<usize> = media.into_iter().collect();
    // Leading/trailing canonical blank lines stay absent from shared coordinates,
    // even when a supplemental image appears before/after them in the render list.
    let first = (0..blocks.len())
        .find(|i| !media.contains(i) && !matches!(blocks[*i], ContentBlock::BlankLine));
    let last = (0..blocks.len())
        .rev()
        .find(|i| !media.contains(i) && !matches!(blocks[*i], ContentBlock::BlankLine));
    for (i, block) in blocks.iter().enumerate() {
        if matches!(block, ContentBlock::BlankLine)
            && (first.is_none_or(|first| i < first) || last.is_none_or(|last| i > last))
        {
            media.insert(i);
        }
    }
    let leading = blocks
        .iter()
        .take_while(|b| matches!(b, ContentBlock::BlankLine))
        .count();
    let end = (blocks.len()
        - blocks
            .iter()
            .rev()
            .take_while(|b| matches!(b, ContentBlock::BlankLine))
            .count())
    .max(leading);
    let source_count = (leading..end).filter(|i| !media.contains(i)).count();
    let last_offset = (leading..end)
        .rev()
        .find(|i| !media.contains(i))
        .map(|i| match &blocks[i] {
            ContentBlock::Paragraph { spans, .. } | ContentBlock::Heading { spans, .. } => {
                spans.iter().map(|s| s.text.chars().count()).sum()
            }
            _ => 0,
        })
        .unwrap_or(0);
    let mut source = 0;
    blocks
        .into_iter()
        .enumerate()
        .filter(|(i, _)| *i >= leading && *i < end)
        .map(|(index, block)| {
            let extra = media.contains(&index);
            let result = (
                source.min(source_count.saturating_sub(1)),
                if source >= source_count {
                    last_offset
                } else {
                    0
                },
                block,
            );
            if !extra {
                source += 1;
            }
            result
        })
        .collect()
}
fn supplement_images(
    parent: ElementRef,
    blocks: &mut Vec<ContentBlock>,
    chapter_path: &str,
    resources: &HashMap<String, Vec<u8>>,
    media: &mut Vec<usize>,
) {
    let selector = Selector::parse("img, image").expect("valid selector");
    for image in parent.select(&selector) {
        let node = image.value();
        let src = node
            .attr("src")
            .or_else(|| node.attr("data-src"))
            .or_else(|| node.attr("href"))
            .or_else(|| node.attr("xlink:href"))
            .unwrap_or("");
        if let Some(data) = resolve_image_data(src, chapter_path, resources) {
            media.push(blocks.len());
            blocks.push(ContentBlock::Image {
                data,
                alt: node.attr("alt").map(str::to_owned),
            });
        }
    }
}

fn collect_blocks(
    parent: ElementRef,
    blocks: &mut Vec<ContentBlock>,
    chapter_path: &str,
    image_resources: &HashMap<String, Vec<u8>>,
    render: bool,
    media: &mut Vec<usize>,
) {
    for child in parent.children() {
        match child.value() {
            scraper::Node::Element(elem) => {
                let tag: &str = &elem.name.local;
                if let Some(elem_ref) = ElementRef::wrap(child) {
                    match tag {
                        "p" | "figcaption" | "cite" => {
                            if render {
                                supplement_images(
                                    elem_ref,
                                    blocks,
                                    chapter_path,
                                    image_resources,
                                    media,
                                );
                            }
                            let spans = collect_spans(elem_ref, InlineStyle::Normal, None);
                            let anchor_id = elem.attr("id").map(|s| s.to_string());
                            if has_visible_text(&spans) {
                                blocks.push(ContentBlock::Paragraph { spans, anchor_id });
                            } else if anchor_id.is_some() {
                                // Keep empty paragraph if it has an anchor id (e.g. review placeholder)
                                blocks.push(ContentBlock::Paragraph { spans, anchor_id });
                            }
                        }
                        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                            let level = (tag.as_bytes()[1] - b'0').clamp(1, 6);
                            let spans = collect_spans(elem_ref, InlineStyle::Bold, None);
                            let anchor_id = elem.attr("id").map(|s| s.to_string());
                            if has_visible_text(&spans) || anchor_id.is_some() {
                                blocks.push(ContentBlock::Heading {
                                    level,
                                    spans,
                                    anchor_id,
                                });
                            }
                        }
                        "hr" => blocks.push(ContentBlock::Separator),
                        "br" => blocks.push(ContentBlock::BlankLine),
                        "div" | "section" | "article" | "main" | "body" | "html" | "nav"
                        | "header" | "footer" | "aside" => {
                            collect_blocks(
                                elem_ref,
                                blocks,
                                chapter_path,
                                image_resources,
                                render,
                                media,
                            );
                        }
                        "ul" | "ol" => {
                            collect_list(elem_ref, blocks, tag == "ol");
                        }
                        "blockquote" => {
                            let mut inner = Vec::new();
                            let mut nested_media = Vec::new();
                            collect_blocks(
                                elem_ref,
                                &mut inner,
                                chapter_path,
                                image_resources,
                                render,
                                &mut nested_media,
                            );
                            media.extend(nested_media.iter().map(|i| blocks.len() + i));
                            for block in inner {
                                if let ContentBlock::Paragraph {
                                    mut spans,
                                    anchor_id,
                                } = block
                                {
                                    spans.insert(
                                        0,
                                        TextSpan {
                                            text: "\u{2502} ".to_string(),
                                            style: InlineStyle::Normal,
                                            link_url: None,
                                            correction: None,
                                        },
                                    );
                                    blocks.push(ContentBlock::Paragraph { spans, anchor_id });
                                } else {
                                    blocks.push(block);
                                }
                            }
                        }
                        "pre" | "code" => {
                            let text = elem_ref.text().collect::<String>();
                            if !text.trim().is_empty() {
                                blocks.push(ContentBlock::Paragraph {
                                    spans: vec![TextSpan {
                                        text,
                                        style: InlineStyle::Normal,
                                        link_url: None,
                                        correction: None,
                                    }],
                                    anchor_id: None,
                                });
                            }
                        }
                        "table" => {
                            collect_table(elem_ref, blocks);
                        }
                        "script" | "style" | "meta" | "link" | "title" | "head" => {}
                        "img" | "image" => {
                            let src = elem
                                .attr("src")
                                .or_else(|| elem.attr("data-src"))
                                .or_else(|| elem.attr("xlink:href"))
                                .unwrap_or("");
                            if let Some(data) =
                                resolve_image_data(src, chapter_path, image_resources)
                            {
                                let alt = elem.attr("alt").map(|s| s.to_string());
                                blocks.push(ContentBlock::Image { data, alt });
                            }
                        }
                        "svg" => {
                            let image_sel = Selector::parse("image").expect("valid selector");
                            for img_node in elem_ref.select(&image_sel) {
                                let src = img_node
                                    .value()
                                    .attr("href")
                                    .or_else(|| img_node.value().attr("xlink:href"))
                                    .unwrap_or("");
                                if let Some(data) =
                                    resolve_image_data(src, chapter_path, image_resources)
                                {
                                    let alt = img_node.value().attr("alt").map(|s| s.to_string());
                                    blocks.push(ContentBlock::Image { data, alt });
                                }
                            }
                        }
                        _ => {
                            let has_block_children = elem_ref.children().any(|c| {
                                if let scraper::Node::Element(e) = c.value() {
                                    let t: &str = &e.name.local;
                                    matches!(
                                        t,
                                        "p" | "div"
                                            | "h1"
                                            | "h2"
                                            | "h3"
                                            | "h4"
                                            | "h5"
                                            | "h6"
                                            | "ul"
                                            | "ol"
                                            | "blockquote"
                                            | "section"
                                            | "article"
                                            | "table"
                                    )
                                } else {
                                    false
                                }
                            });
                            if has_block_children {
                                collect_blocks(
                                    elem_ref,
                                    blocks,
                                    chapter_path,
                                    image_resources,
                                    render,
                                    media,
                                );
                            } else {
                                if render {
                                    supplement_images(
                                        elem_ref,
                                        blocks,
                                        chapter_path,
                                        image_resources,
                                        media,
                                    );
                                }
                                let spans = collect_spans(elem_ref, InlineStyle::Normal, None);
                                if has_visible_text(&spans) {
                                    blocks.push(ContentBlock::Paragraph {
                                        spans,
                                        anchor_id: None,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            scraper::Node::Text(text) => {
                let s = text.trim();
                if !s.is_empty() {
                    blocks.push(ContentBlock::Paragraph {
                        spans: vec![TextSpan {
                            text: s.to_string(),
                            style: InlineStyle::Normal,
                            link_url: None,
                            correction: None,
                        }],
                        anchor_id: None,
                    });
                }
            }
            _ => {}
        }
    }
}

fn collect_spans(
    parent: ElementRef,
    inherited_style: InlineStyle,
    link_url: Option<&str>,
) -> Vec<TextSpan> {
    let mut spans = Vec::new();
    for child in parent.children() {
        match child.value() {
            scraper::Node::Text(text) => {
                let s = normalize_whitespace(text);
                if !s.is_empty() {
                    spans.push(TextSpan {
                        text: s,
                        style: inherited_style.clone(),
                        link_url: link_url.map(|u| u.to_string()),
                        correction: None,
                    });
                }
            }
            scraper::Node::Element(elem) => {
                if let Some(elem_ref) = ElementRef::wrap(child) {
                    let tag: &str = &elem.name.local;
                    if tag == "br" {
                        spans.push(TextSpan {
                            text: "\n".to_string(),
                            style: InlineStyle::Normal,
                            link_url: None,
                            correction: None,
                        });
                        continue;
                    }
                    if matches!(tag, "script" | "style") {
                        continue;
                    }
                    let new_style = match tag {
                        "b" | "strong" => match &inherited_style {
                            InlineStyle::Italic | InlineStyle::BoldItalic => {
                                InlineStyle::BoldItalic
                            }
                            _ => InlineStyle::Bold,
                        },
                        "i" | "em" | "cite" => match &inherited_style {
                            InlineStyle::Bold | InlineStyle::BoldItalic => InlineStyle::BoldItalic,
                            _ => InlineStyle::Italic,
                        },
                        _ => inherited_style.clone(),
                    };
                    let new_link = if tag == "a" {
                        elem.attr("href").or(link_url)
                    } else {
                        link_url
                    };
                    spans.extend(collect_spans(elem_ref, new_style, new_link));
                }
            }
            _ => {}
        }
    }
    spans
}

fn collect_list(parent: ElementRef, blocks: &mut Vec<ContentBlock>, ordered: bool) {
    let mut index = 1;
    for child in parent.children() {
        if let scraper::Node::Element(elem) = child.value() {
            let tag: &str = &elem.name.local;
            if tag == "li" {
                if let Some(li_ref) = ElementRef::wrap(child) {
                    let prefix = if ordered {
                        format!("{}. ", index)
                    } else {
                        "  \u{2022} ".to_string()
                    };
                    let mut spans = vec![TextSpan {
                        text: prefix,
                        style: InlineStyle::Normal,
                        link_url: None,
                        correction: None,
                    }];
                    spans.extend(collect_spans(li_ref, InlineStyle::Normal, None));
                    if spans.len() > 1 {
                        blocks.push(ContentBlock::Paragraph {
                            spans,
                            anchor_id: None,
                        });
                    }
                    index += 1;
                }
            }
        }
    }
}

fn collect_table(parent: ElementRef, blocks: &mut Vec<ContentBlock>) {
    let row_sel = Selector::parse("tr").expect("valid selector");
    for row in parent.select(&row_sel) {
        let mut row_spans = Vec::new();
        for cell_child in row.children() {
            if let scraper::Node::Element(elem) = cell_child.value() {
                let tag: &str = &elem.name.local;
                if matches!(tag, "td" | "th") {
                    if let Some(cell_ref) = ElementRef::wrap(cell_child) {
                        if !row_spans.is_empty() {
                            row_spans.push(TextSpan {
                                text: " | ".to_string(),
                                style: InlineStyle::Normal,
                                link_url: None,
                                correction: None,
                            });
                        }
                        let style = if tag == "th" {
                            InlineStyle::Bold
                        } else {
                            InlineStyle::Normal
                        };
                        row_spans.extend(collect_spans(cell_ref, style, None));
                    }
                }
            }
        }
        if has_visible_text(&row_spans) {
            blocks.push(ContentBlock::Paragraph {
                spans: row_spans,
                anchor_id: None,
            });
        }
    }
}

fn has_visible_text(spans: &[TextSpan]) -> bool {
    spans.iter().any(|s| !s.text.trim().is_empty())
}

fn normalize_whitespace(s: &str) -> String {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.is_empty() {
        return String::new();
    }
    let mut result = String::new();
    if s.starts_with(char::is_whitespace) {
        result.push(' ');
    }
    result.push_str(&parts.join(" "));
    if s.ends_with(char::is_whitespace) && s.len() > 1 {
        result.push(' ');
    }
    result
}

#[cfg(test)]
mod rendering_tests {
    use super::*;
    fn text(block: &ContentBlock) -> String {
        match block {
            ContentBlock::Paragraph { spans, .. } | ContentBlock::Heading { spans, .. } => {
                spans.iter().map(|s| s.text.as_str()).collect()
            }
            _ => String::new(),
        }
    }
    #[test]
    fn supplemental_paragraph_images_keep_canonical_block_numbers_and_text() {
        let mut resources = HashMap::new();
        resources.insert("OPS/img.png".into(), vec![1, 2, 3]);
        let html = "<body><br/><h1>Title</h1><p><img src='img.png'/></p><p id='one'>中<b>😀</b>é</p><img src='img.png'/><p>Next</p><p><img src='img.png'/></p><br/></body>";
        let original = parse_html_blocks(html, "OPS/chapter.xhtml", &resources);
        assert_eq!(original.len(), 4);
        assert_eq!(text(&original[0]), "Title");
        assert_eq!(text(&original[1]), "中😀é");
        assert!(matches!(original[2], ContentBlock::Image { .. }));
        let rendered = parse_render_blocks(html, "OPS/chapter.xhtml", &resources);
        assert_eq!(rendered.len(), 6);
        assert_eq!(
            rendered
                .iter()
                .map(|(index, _, _)| *index)
                .collect::<Vec<_>>(),
            vec![0, 1, 1, 2, 3, 3]
        );
        assert_eq!(rendered.last().unwrap().1, 4); // image after final paragraph anchors at its scalar end
        assert_eq!(text(&rendered[2].2), text(&original[1]));
        assert_eq!(text(&rendered[4].2), text(&original[3]));
        assert!(matches!(rendered[1].2, ContentBlock::Image { .. }));
    }
    #[test]
    fn media_does_not_resurrect_trimmed_canonical_blank_lines() {
        let mut images = HashMap::new();
        images.insert("img.png".into(), vec![1]);
        let html = "<body><p><img src='img.png'/></p><br/><p>Body</p><br/><p><img src='img.png'/></p></body>";
        assert_eq!(parse_html_blocks(html, "c.xhtml", &images).len(), 1);
        let rendered = parse_render_blocks(html, "c.xhtml", &images);
        assert!(rendered.iter().all(|(i, _, _)| *i == 0));
        assert_eq!(rendered.last().unwrap().1, 4);
    }
    #[test]
    fn empty_or_blank_only_chapters_are_safe_in_both_parsers() {
        for html in ["<body></body>", "<body><br/><br/></body>"] {
            assert!(parse_html_blocks(html, "c.xhtml", &HashMap::new()).is_empty());
            assert!(parse_render_blocks(html, "c.xhtml", &HashMap::new()).is_empty());
        }
    }
}
