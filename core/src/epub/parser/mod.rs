//! EPUB parser module containing sub-parsers for HTML, images, and other aspects.
mod html;
mod image;

use std::collections::{HashMap, HashSet};
use std::path::Path;

use epub::doc::EpubDoc;

use super::{Chapter, ContentBlock, TocEntry};
use html::parse_html_blocks;
use image::load_referenced_images;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct EpubBook {
    pub title: String,
    pub chapters: Vec<Chapter>,
    pub toc: Vec<TocEntry>,
    #[serde(skip)]
    pub cover_data: Option<Vec<u8>>,
    #[serde(skip)]
    pub fonts: Vec<(String, Vec<u8>)>,
    /// Mapping from main chapter index to its review chapter index.
    #[serde(default)]
    pub chapter_reviews: HashMap<usize, usize>,
    /// Set of chapter indices that are review chapters.
    #[serde(default)]
    pub review_chapter_indices: HashSet<usize>,
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
        let mut doc = EpubDoc::new(path).map_err(|e| format!("无法打开 EPUB 文件: {e}"))?;

        let title = doc
            .mdata("title")
            .map(|item| item.value.clone())
            .unwrap_or_else(|| "未知书名".to_string());

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
                    .ok_or_else(|| format!("无法读取资源: {}", res.path.display()))?;

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

                let chapter_idx = chapters.len();

                // Avoid completely empty chapters (keeps index stable but shows something)
                let blocks = if blocks.is_empty() {
                    vec![ContentBlock::BlankLine]
                } else {
                    blocks
                };

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

        // Identify review chapters (段评) and build mapping
        let mut chapter_reviews = HashMap::new();
        let mut review_chapter_indices = HashSet::new();
        const REVIEW_SUFFIX: &str = " - 段评";
        for (idx, ch) in chapters.iter().enumerate() {
            if ch.title.ends_with(REVIEW_SUFFIX) {
                review_chapter_indices.insert(idx);
                let base_title = &ch.title[..ch.title.len() - REVIEW_SUFFIX.len()];
                // Match to the Nth main chapter with the same title
                let review_count = chapters[..idx].iter().filter(|c| c.title == ch.title).count();
                if let Some(main_idx) = chapters
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.title == base_title)
                    .nth(review_count)
                    .map(|(i, _)| i)
                {
                    chapter_reviews.insert(main_idx, idx);
                }
            }
        }

        Ok(EpubBook {
            title,
            chapters,
            toc,
            cover_data,
            fonts,
            chapter_reviews,
            review_chapter_indices,
        })
    }

    /// Compute SHA-256 hash of an epub file for cross-device identification
    pub fn file_hash(path: &str) -> Result<String, String> {
        crate::file_hash(path)
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
