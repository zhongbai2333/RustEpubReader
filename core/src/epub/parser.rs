use std::collections::HashMap;
use std::path::Path;

use base64::Engine;
use epub::doc::EpubDoc;
use scraper::{ElementRef, Html, Selector};

use super::{Chapter, ContentBlock, InlineStyle, TextSpan, TocEntry};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct EpubBook {
    pub title: String,
    pub chapters: Vec<Chapter>,
    pub toc: Vec<TocEntry>,
    #[serde(skip)]
    pub cover_data: Option<Vec<u8>>,
    #[serde(skip)]
    pub fonts: Vec<(String, Vec<u8>)>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct EpubMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rights: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contributor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chapter_count: Option<usize>,
}

impl EpubBook {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let mut doc = EpubDoc::new(path).map_err(|e| format!("鏃犳硶鎵撳紑 EPUB 鏂囦欢: {e}"))?;

        let title = doc
            .mdata("title")
            .map(|item| item.value.clone())
            .unwrap_or_else(|| "鏈煡涔﹀悕".to_string());

        let cover_path = doc
            .get_cover_id()
            .and_then(|id| doc.resources.get(&id).map(|r| r.path.clone()));
        let cover_data = cover_path.and_then(|p| doc.get_resource_by_path(&p));

        let mut chapters = Vec::new();
        let mut toc = Vec::new();

        // Build path index for image resources (lazy: data loaded only when referenced)
        let image_path_index: HashMap<String, std::path::PathBuf> = doc
            .resources
            .values()
            .filter(|r| r.mime.starts_with("image/"))
            .map(|r| (r.path.to_string_lossy().to_string(), r.path.clone()))
            .collect();
        let mut image_resources: HashMap<String, Vec<u8>> = HashMap::new();

        let toc_items: Vec<(String, String)> = doc
            .toc
            .iter()
            .map(|nav| {
                let label = nav.label.clone();
                let raw = nav.content.to_string_lossy().to_string();
                let path_part = raw.split('#').next().unwrap_or(&raw).to_string();
                (label, path_part)
            })
            .collect();

        let spine: Vec<_> = doc.spine.iter().map(|s| s.idref.clone()).collect();
        for spine_id in &spine {
            let resource = doc.resources.get(spine_id).cloned();
            if let Some(res) = resource {
                if !res.mime.starts_with("application/xhtml") && !res.mime.starts_with("text/html")
                {
                    continue;
                }

                let content_bytes = doc
                    .get_resource_by_path(&res.path)
                    .ok_or_else(|| format!("鏃犳硶璇诲彇璧勬簮: {}", res.path.display()))?;

                let html = String::from_utf8_lossy(&content_bytes).to_string();

                // Lazy-load: only fetch images referenced by this chapter (not all EPUB images)
                load_referenced_images(
                    &html,
                    &res.path.to_string_lossy(),
                    &image_path_index,
                    &mut image_resources,
                    &mut doc,
                );

                let blocks =
                    parse_html_blocks(&html, &res.path.to_string_lossy(), &image_resources);

                let path_str = res.path.to_string_lossy().to_string();
                let path_filename = res
                    .path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_default();

                let matches_toc = |p: &str| -> bool {
                    let toc_filename = Path::new(p)
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_default();
                    path_str.contains(p)
                        || p.contains(&path_str)
                        || (!toc_filename.is_empty() && toc_filename == path_filename)
                };

                let chapter_title = toc_items
                    .iter()
                    .find(|(_, p)| matches_toc(p))
                    .map(|(label, _)| label.clone())
                    .unwrap_or_else(|| format!("第 {} 章", chapters.len() + 1));

                if blocks.is_empty() {
                    continue;
                }

                let chapter_idx = chapters.len();

                chapters.push(Chapter {
                    title: chapter_title.clone(),
                    blocks,
                    source_href: Some(path_str.clone()),
                });

                if toc_items.iter().any(|(_, p)| matches_toc(p)) {
                    toc.push(TocEntry {
                        title: chapter_title,
                        chapter_index: chapter_idx,
                    });
                }
            }
        }

        if toc.is_empty() {
            toc = chapters
                .iter()
                .enumerate()
                .map(|(i, ch)| TocEntry {
                    title: ch.title.clone(),
                    chapter_index: i,
                })
                .collect();
        }

        let mut fonts = Vec::new();
        let font_mimes = [
            "application/x-font-ttf",
            "font/ttf",
            "font/otf",
            "application/vnd.ms-opentype",
            "application/x-font-opentype",
            "font/sfnt",
        ];
        let resource_list: Vec<_> = doc
            .resources
            .values()
            .map(|r| (r.path.clone(), r.mime.clone()))
            .collect();
        for (path, mime) in &resource_list {
            if font_mimes.iter().any(|m| mime.eq_ignore_ascii_case(m)) {
                if let Some(data) = doc.get_resource_by_path(path) {
                    let name = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "EmbeddedFont".to_string());
                    fonts.push((name, data));
                }
            }
        }

        Ok(EpubBook {
            title,
            chapters,
            toc,
            cover_data,
            fonts,
        })
    }

    /// Compute SHA-256 hash of an epub file for cross-device identification
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

    /// Read only the title metadata from an epub file without full parsing.
    pub fn read_title<P: AsRef<Path>>(path: P) -> Option<String> {
        let doc = EpubDoc::new(path).ok()?;
        doc.mdata("title").map(|item| item.value.clone())
    }

    /// Read all available Dublin Core metadata from an epub file without full parsing.
    pub fn read_metadata<P: AsRef<Path>>(path: P) -> Option<EpubMetadata> {
        let doc = EpubDoc::new(path).ok()?;
        let get = |key: &str| doc.mdata(key).map(|item| item.value.clone());
        Some(EpubMetadata {
            title: get("title"),
            author: get("creator"),
            publisher: get("publisher"),
            language: get("language"),
            identifier: get("identifier"),
            description: get("description"),
            subject: get("subject"),
            date: get("date"),
            rights: get("rights"),
            contributor: get("contributor"),
            chapter_count: Some(doc.spine.len()),
        })
    }
}

/// Load only the images referenced by `<img>`/`<image>` tags in this chapter's HTML.
/// This avoids eagerly loading all images in the EPUB into memory.
fn load_referenced_images(
    html: &str,
    chapter_path: &str,
    image_path_index: &HashMap<String, std::path::PathBuf>,
    image_resources: &mut HashMap<String, Vec<u8>>,
    doc: &mut EpubDoc<std::io::BufReader<std::fs::File>>,
) {
    let document = Html::parse_document(html);
    let img_sel = Selector::parse("img, image").expect("valid selector");
    let chapter_dir = Path::new(chapter_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));

    for elem in document.select(&img_sel) {
        let src = elem
            .value()
            .attr("src")
            .or_else(|| elem.value().attr("data-src"))
            .or_else(|| elem.value().attr("href"))
            .or_else(|| elem.value().attr("xlink:href"))
            .unwrap_or("");
        let clean = src
            .split('#')
            .next()
            .unwrap_or(src)
            .split('?')
            .next()
            .unwrap_or(src)
            .trim();
        if clean.is_empty() || clean.starts_with("data:") {
            continue;
        }
        let resolved = if clean.starts_with('/') {
            clean.trim_start_matches('/').to_string()
        } else {
            chapter_dir.join(clean).to_string_lossy().to_string()
        };
        if image_resources.contains_key(&resolved) {
            continue;
        }
        // Try direct path match
        if let Some(epub_path) = image_path_index.get(&resolved) {
            if let Some(data) = doc.get_resource_by_path(epub_path) {
                image_resources.insert(resolved, data);
                continue;
            }
        }
        // Fallback: match by filename
        let file_name = Path::new(clean)
            .file_name()
            .map(|n| n.to_string_lossy().to_string());
        if let Some(file_name) = file_name {
            if let Some((key, epub_path)) = image_path_index.iter().find(|(k, _)| {
                Path::new(k.as_str())
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .as_deref()
                    == Some(&file_name)
            }) {
                if let Some(data) = doc.get_resource_by_path(epub_path) {
                    image_resources.insert(key.clone(), data);
                }
            }
        }
    }
}

fn parse_html_blocks(
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
    collect_blocks(start_elem, &mut blocks, chapter_path, image_resources);

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
) {
    for child in parent.children() {
        match child.value() {
            scraper::Node::Element(elem) => {
                let tag: &str = &elem.name.local;
                if let Some(elem_ref) = ElementRef::wrap(child) {
                    match tag {
                        "p" | "figcaption" | "cite" => {
                            let spans = collect_spans(elem_ref, InlineStyle::Normal, None);
                            if has_visible_text(&spans) {
                                blocks.push(ContentBlock::Paragraph { spans });
                            }
                        }
                        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                            let level = (tag.as_bytes()[1] - b'0').clamp(1, 6);
                            let spans = collect_spans(elem_ref, InlineStyle::Bold, None);
                            if has_visible_text(&spans) {
                                blocks.push(ContentBlock::Heading { level, spans });
                            }
                        }
                        "hr" => blocks.push(ContentBlock::Separator),
                        "br" => blocks.push(ContentBlock::BlankLine),
                        "div" | "section" | "article" | "main" | "body" | "html" | "nav"
                        | "header" | "footer" | "aside" => {
                            collect_blocks(elem_ref, blocks, chapter_path, image_resources);
                        }
                        "ul" | "ol" => {
                            collect_list(elem_ref, blocks, tag == "ol");
                        }
                        "blockquote" => {
                            let mut inner = Vec::new();
                            collect_blocks(elem_ref, &mut inner, chapter_path, image_resources);
                            for block in inner {
                                if let ContentBlock::Paragraph { mut spans } = block {
                                    spans.insert(
                                        0,
                                        TextSpan {
                                            text: "鈹?".to_string(),
                                            style: InlineStyle::Normal,
                                            link_url: None,
                                        },
                                    );
                                    blocks.push(ContentBlock::Paragraph { spans });
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
                                    }],
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
                                collect_blocks(elem_ref, blocks, chapter_path, image_resources);
                            } else {
                                let spans = collect_spans(elem_ref, InlineStyle::Normal, None);
                                if has_visible_text(&spans) {
                                    blocks.push(ContentBlock::Paragraph { spans });
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
                        }],
                    });
                }
            }
            _ => {}
        }
    }
}

fn resolve_image_data(
    raw_src: &str,
    chapter_path: &str,
    image_resources: &HashMap<String, Vec<u8>>,
) -> Option<std::sync::Arc<Vec<u8>>> {
    let src = raw_src.trim();
    if src.is_empty() {
        return None;
    }

    if src.starts_with("data:") {
        return decode_data_uri(src).map(std::sync::Arc::new);
    }

    let lowered = src.to_ascii_lowercase();
    if lowered.starts_with("http://") || lowered.starts_with("https://") {
        return None;
    }

    let clean = src
        .split('#')
        .next()
        .unwrap_or(src)
        .split('?')
        .next()
        .unwrap_or(src)
        .trim();
    if clean.is_empty() {
        return None;
    }

    let chapter_dir = Path::new(chapter_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let resolved = if clean.starts_with('/') {
        clean.trim_start_matches('/').to_string()
    } else {
        chapter_dir.join(clean).to_string_lossy().to_string()
    };

    image_resources
        .get(&resolved)
        .cloned()
        .or_else(|| {
            let file_name = Path::new(clean)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())?;
            image_resources.iter().find_map(|(k, v)| {
                let kn = Path::new(k)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if kn == file_name {
                    Some(v.clone())
                } else {
                    None
                }
            })
        })
        .map(std::sync::Arc::new)
}

fn decode_data_uri(uri: &str) -> Option<Vec<u8>> {
    let comma = uri.find(',')?;
    if comma + 1 >= uri.len() {
        return None;
    }
    let (meta, payload) = uri.split_at(comma);
    let payload = &payload[1..];

    if meta.to_ascii_lowercase().contains(";base64") {
        base64::engine::general_purpose::STANDARD
            .decode(payload)
            .ok()
    } else {
        Some(payload.as_bytes().to_vec())
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
                        "  鈥?".to_string()
                    };
                    let mut spans = vec![TextSpan {
                        text: prefix,
                        style: InlineStyle::Normal,
                        link_url: None,
                    }];
                    spans.extend(collect_spans(li_ref, InlineStyle::Normal, None));
                    if spans.len() > 1 {
                        blocks.push(ContentBlock::Paragraph { spans });
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
            blocks.push(ContentBlock::Paragraph { spans: row_spans });
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
