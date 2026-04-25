//! EPUB parser module containing sub-parsers for HTML, images, and other aspects.
mod html;
mod image;

use std::collections::{HashMap, HashSet};
use std::path::Path;

use rbook::ebook::resource::ResourceKey;
use rbook::epub::Epub as RbookEpub;

use super::{Chapter, TocEntry};
use html::parse_html_blocks;
use image::load_referenced_images;

fn resource_key_to_string(key: &ResourceKey<'_>) -> Option<String> {
    match key {
        ResourceKey::Value(value) => Some(value.as_ref().to_string()),
        ResourceKey::Position(_) => None,
    }
}

fn normalize_resource_path(path: &str) -> String {
    let clean = path
        .split('#')
        .next()
        .unwrap_or(path)
        .split('?')
        .next()
        .unwrap_or(path)
        .replace('\\', "/");
    let absolute = clean.starts_with('/');
    let mut parts = Vec::new();
    for part in clean.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    let joined = parts.join("/");
    if absolute {
        format!("/{joined}")
    } else {
        joined
    }
}

fn resource_path_matches(resource_path: &str, candidate_path: &str) -> bool {
    let resource_norm = normalize_resource_path(resource_path);
    let candidate_norm = normalize_resource_path(candidate_path);
    let resource_trim = resource_norm.trim_start_matches('/');
    let candidate_trim = candidate_norm.trim_start_matches('/');
    if resource_trim == candidate_trim
        || resource_trim.ends_with(candidate_trim)
        || candidate_trim.ends_with(resource_trim)
    {
        return true;
    }
    let resource_name = Path::new(resource_trim)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();
    let candidate_name = Path::new(candidate_trim)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();
    !resource_name.is_empty() && resource_name == candidate_name
}

fn collect_toc_items(epub: &RbookEpub) -> Vec<(String, String)> {
    let mut items = Vec::new();
    if let Some(root) = epub.toc().contents() {
        for entry in root.flatten() {
            let Some(manifest_entry) = entry.manifest_entry() else {
                continue;
            };
            let Some(path) = resource_key_to_string(manifest_entry.resource().key()) else {
                continue;
            };
            items.push((entry.label().to_string(), normalize_resource_path(&path)));
        }
    }
    items
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct EpubBook {
    pub title: String,
    pub chapters: Vec<Chapter>,
    pub toc: Vec<TocEntry>,
    #[serde(skip)]
    pub cover_data: Option<Vec<u8>>,
    #[serde(skip)]
    pub fonts: Vec<(String, Vec<u8>)>,
    #[serde(default)]
    pub chapter_reviews: HashMap<usize, usize>,
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
        let epub = RbookEpub::open(path).map_err(|e| format!("无法打开 EPUB 文件: {e}"))?;
        let metadata = epub.metadata();

        let title = metadata
            .title()
            .map(|item| item.value().to_string())
            .unwrap_or_else(|| "未知书名".to_string());

        let cover_data = epub
            .manifest()
            .cover_image()
            .and_then(|entry| entry.read_bytes().ok());

        let mut chapters = Vec::new();
        let mut toc = Vec::new();

        // Build image resource path index (lazy: bytes loaded only when referenced)
        let image_resource_paths: Vec<String> = epub
            .manifest()
            .images()
            .filter_map(|entry| resource_key_to_string(entry.resource().key()))
            .map(|path| normalize_resource_path(&path))
            .collect();
        let mut image_resources: HashMap<String, Vec<u8>> = HashMap::new();

        let toc_items = collect_toc_items(&epub);

        for spine_entry in epub.spine().iter() {
            let Some(manifest_entry) = spine_entry.manifest_entry() else {
                continue;
            };
            let kind = manifest_entry.kind().as_str().to_string();
            if !kind.starts_with("application/xhtml") && !kind.starts_with("text/html") {
                continue;
            }

            let Some(raw_path) = resource_key_to_string(manifest_entry.resource().key()) else {
                continue;
            };
            let path_str = normalize_resource_path(&raw_path);
            let html = manifest_entry
                .read_str()
                .map_err(|e| format!("无法读取资源: {path_str}: {e}"))?;

            // Lazy-load: only fetch images referenced by this chapter (not all EPUB images)
            load_referenced_images(
                &html,
                &path_str,
                &image_resource_paths,
                &mut image_resources,
                |resolved| epub.read_resource_bytes(resolved).ok(),
            );

            let blocks = parse_html_blocks(&html, &path_str, &image_resources);

            let chapter_title = toc_items
                .iter()
                .find(|(_, p)| resource_path_matches(&path_str, p))
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

            if toc_items
                .iter()
                .any(|(_, p)| resource_path_matches(&path_str, p))
            {
                toc.push(TocEntry {
                    title: chapter_title,
                    chapter_index: chapter_idx,
                });
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
        for entry in epub.manifest().fonts() {
            if let Ok(data) = entry.read_bytes() {
                let name = resource_key_to_string(entry.resource().key())
                    .and_then(|path| {
                        Path::new(&path)
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                    })
                    .unwrap_or_else(|| "EmbeddedFont".to_string());
                fonts.push((name, data));
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
                let review_count = chapters[..idx]
                    .iter()
                    .filter(|c| c.title == ch.title)
                    .count();
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
        let epub = RbookEpub::options()
            .skip_toc(true)
            .skip_manifest(true)
            .skip_spine(true)
            .open(path)
            .ok()?;
        epub.metadata().title().map(|item| item.value().to_string())
    }

    /// Read all available Dublin Core metadata from an epub file without full parsing.
    pub fn read_metadata<P: AsRef<Path>>(path: P) -> Option<EpubMetadata> {
        let epub = RbookEpub::options()
            .skip_toc(true)
            .skip_manifest(true)
            .skip_spine(true)
            .open(path)
            .ok()?;
        let metadata = epub.metadata();
        let title = metadata.title().map(|item| item.value().to_string());
        let author = metadata
            .creators()
            .next()
            .map(|item| item.value().to_string());
        let publisher = metadata
            .publishers()
            .next()
            .map(|item| item.value().to_string());
        let identifier = metadata.identifier().map(|item| item.value().to_string());
        let description = metadata.description().map(|item| item.value().to_string());
        let contributor = metadata
            .contributors()
            .next()
            .map(|item| item.value().to_string());
        let chapter_count = Some(epub.spine().len());

        Some(EpubMetadata {
            title,
            author,
            publisher,
            language: None,
            identifier,
            description,
            subject: None,
            date: None,
            rights: None,
            contributor,
            chapter_count,
        })
    }
}
