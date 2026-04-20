//! Contains HTML and XHTML parsing logic for EPUB content.
use std::collections::HashMap;

use scraper::{ElementRef, Html, Selector};

use super::image::resolve_image_data;
use crate::epub::{ContentBlock, InlineStyle, TextSpan};

pub(super) fn parse_html_blocks(
    html: &str,
    chapter_path: &str,
    image_resources: &HashMap<String, Vec<u8>>,
) -> Vec<ContentBlock> {
    let document = Html::parse_document(html);
    let body_sel = Selector::parse("body").expect("valid selector");
    let start_elem = document
        .select(&body_sel)
        .next()
        .unwrap_or_else(|| document.root_element());

    let mut blocks = Vec::new();
    let mut no_id = None;
    collect_blocks(start_elem, &mut blocks, chapter_path, image_resources, &mut no_id);

    while matches!(blocks.first(), Some(ContentBlock::BlankLine)) {
        blocks.remove(0);
    }
    while matches!(blocks.last(), Some(ContentBlock::BlankLine)) {
        blocks.pop();
    }

    blocks
}

fn collect_blocks(
    parent: ElementRef,
    blocks: &mut Vec<ContentBlock>,
    chapter_path: &str,
    image_resources: &HashMap<String, Vec<u8>>,
    inherited_id: &mut Option<String>,
) {
    for child in parent.children() {
        match child.value() {
            scraper::Node::Element(elem) => {
                let tag: &str = &elem.name.local;
                if let Some(elem_ref) = ElementRef::wrap(child) {
                    match tag {
                        "p" | "figcaption" | "cite" => {
                            let spans = collect_spans(elem_ref, InlineStyle::Normal, None);
                            let anchor_id = elem
                                .attr("id")
                                .map(|s| s.to_string())
                                .or_else(|| inherited_id.take());
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
                            let anchor_id = elem
                                .attr("id")
                                .map(|s| s.to_string())
                                .or_else(|| inherited_id.take());
                            if has_visible_text(&spans) {
                                blocks.push(ContentBlock::Heading { level, spans, anchor_id });
                            } else if anchor_id.is_some() {
                                blocks.push(ContentBlock::Heading { level, spans, anchor_id });
                            }
                        }
                        "hr" => blocks.push(ContentBlock::Separator),
                        "br" => blocks.push(ContentBlock::BlankLine),
                        "div" | "section" | "article" | "main" | "body" | "html" | "nav"
                        | "header" | "footer" | "aside" => {
                            let mut container_id = elem.attr("id").map(|s| s.to_string());
                            collect_blocks(
                                elem_ref,
                                blocks,
                                chapter_path,
                                image_resources,
                                &mut container_id,
                            );
                        }
                        "ul" | "ol" => {
                            let mut list_id = elem.attr("id").map(|s| s.to_string());
                            collect_list(elem_ref, blocks, tag == "ol", &mut list_id);
                        }
                        "blockquote" => {
                            let mut inner = Vec::new();
                            let mut bq_id = elem.attr("id").map(|s| s.to_string());
                            collect_blocks(
                                elem_ref,
                                &mut inner,
                                chapter_path,
                                image_resources,
                                &mut bq_id,
                            );
                            for block in inner {
                                if let ContentBlock::Paragraph { mut spans, anchor_id } = block {
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
                                let anchor_id = elem
                                    .attr("id")
                                    .map(|s| s.to_string())
                                    .or_else(|| inherited_id.take());
                                blocks.push(ContentBlock::Paragraph {
                                    spans: vec![TextSpan {
                                        text,
                                        style: InlineStyle::Normal,
                                        link_url: None,
                                        correction: None,
                                    }],
                                    anchor_id,
                                });
                            }
                        }
                        "table" => {
                            let mut table_id = elem.attr("id").map(|s| s.to_string());
                            collect_table(elem_ref, blocks, &mut table_id);
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
                                    let alt =
                                        img_node.value().attr("alt").map(|s| s.to_string());
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
                                let mut fallback_id = elem.attr("id").map(|s| s.to_string());
                                collect_blocks(
                                    elem_ref,
                                    blocks,
                                    chapter_path,
                                    image_resources,
                                    &mut fallback_id,
                                );
                            } else {
                                let spans = collect_spans(elem_ref, InlineStyle::Normal, None);
                                let anchor_id = elem
                                    .attr("id")
                                    .map(|s| s.to_string())
                                    .or_else(|| inherited_id.take());
                                if has_visible_text(&spans) {
                                    blocks.push(ContentBlock::Paragraph { spans, anchor_id });
                                }
                            }
                        }
                    }
                }
            }
            scraper::Node::Text(text) => {
                let s = text.trim();
                if !s.is_empty() {
                    let anchor_id = inherited_id.take();
                    blocks.push(ContentBlock::Paragraph {
                        spans: vec![TextSpan {
                            text: s.to_string(),
                            style: InlineStyle::Normal,
                            link_url: None,
                            correction: None,
                        }],
                        anchor_id,
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

fn collect_list(
    parent: ElementRef,
    blocks: &mut Vec<ContentBlock>,
    ordered: bool,
    inherited_id: &mut Option<String>,
) {
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
                        let anchor_id = inherited_id.take();
                        blocks.push(ContentBlock::Paragraph { spans, anchor_id });
                    }
                    index += 1;
                }
            }
        }
    }
}

fn collect_table(
    parent: ElementRef,
    blocks: &mut Vec<ContentBlock>,
    inherited_id: &mut Option<String>,
) {
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
            let anchor_id = inherited_id.take();
            blocks.push(ContentBlock::Paragraph {
                spans: row_spans,
                anchor_id,
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
