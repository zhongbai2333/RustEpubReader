//! Chapter-backed publications. Only the catalogue and bounded resource caches survive open.
use super::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    io::Write,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Slot {
    path: String,
    title: String,
    before: Vec<String>,
    after: Vec<String>,
    illustrated: bool,
}
#[derive(Debug)]
pub struct LoadedChapter {
    pub chapter: Arc<Chapter>,
    pub images: Vec<SupplementalImage>,
    cost: usize,
}
#[derive(Debug)]
pub struct ChapterStore {
    archive: Mutex<Arc<RbookEpub>>,
    resources: Arc<super::super::native::NativeResources>,
    slots: Vec<Slot>,
    image_paths: Vec<String>,
    cache: Mutex<VecDeque<(usize, Arc<LoadedChapter>)>>,
    loads: AtomicUsize,
    probes: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Index {
    version: u32,
    hash: String,
    slots: Vec<Slot>,
    toc: Vec<TocEntry>,
}

impl ChapterStore {
    /// Warm one chapter without cloning the publication or changing its visible window.
    pub fn prefetch(&self, index: usize) -> Result<(), String> {
        self.load(index).map(|_| ())
    }
    pub(super) fn relocate(&self, path: &Path) -> Result<(), String> {
        let epub = Arc::new(RbookEpub::open(path).map_err(|_| "Cannot relocate EPUB archive")?);
        self.resources.relocate(path)?;
        *self.archive.lock().unwrap_or_else(|e| e.into_inner()) = epub;
        Ok(())
    }

    fn load(&self, index: usize) -> Result<Arc<LoadedChapter>, String> {
        crate::open_operation::check()?;
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(at) = cache.iter().position(|(i, _)| *i == index) {
            let value = cache.remove(at).unwrap();
            let result = value.1.clone();
            cache.push_back(value);
            return Ok(result);
        }
        let slot = self
            .slots
            .get(index)
            .ok_or("Reading chapter is out of range")?;
        let epub = self
            .archive
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let parse = |path: &str| -> Result<_, String> {
            let html = String::from_utf8(read_bounded(&epub, path, 64 * 1024 * 1024)?)
                .map_err(|_| "Invalid chapter text")?;
            let document = scraper::Html::parse_document(&html);
            crate::open_operation::check()?;
            let mut images = HashMap::new();
            let mut image_budget = 64 * 1024 * 1024;
            let mut resource_error = None;
            load_referenced_images(&document, path, &self.image_paths, &mut images, |p| {
                match read_bounded(&epub, p, image_budget.min(32 * 1024 * 1024)) {
                    Ok(bytes) => {
                        image_budget -= bytes.len();
                        Some(bytes)
                    }
                    Err(error) => {
                        if error.contains("exceeds") {
                            resource_error = Some(error);
                        }
                        None
                    }
                }
            });
            if let Some(error) = resource_error {
                return Err(error);
            }
            let canonical = html::parse_document_blocks(&document, path, &images, false);
            let media = html::supplemental_document_images(&document, path, &images, &canonical);
            Ok((
                html,
                canonical.into_iter().map(|(_, _, b)| b).collect::<Vec<_>>(),
                media,
            ))
        };
        let (html, mut blocks, mut images) = parse(&slot.path)?;
        let mut before = Vec::new();
        for path in &slot.before {
            let (_, _, media) = parse(path)?;
            before.extend(media.into_iter().map(|mut image| {
                image.block = 0;
                image.after = false;
                image
            }));
        }
        if slot.illustrated {
            images.extend(before);
        } else {
            before.append(&mut images);
            images = before;
        }
        for path in &slot.after {
            let (_, _, media) = parse(path)?;
            images.extend(media.into_iter().map(|mut image| {
                image.block = blocks.len().saturating_sub(1);
                image.after = true;
                image
            }));
        }
        if slot.illustrated {
            blocks = images
                .drain(..)
                .map(|image| super::super::ContentBlock::Image {
                    data: image.data,
                    alt: image.alt,
                    native_style: Default::default(),
                })
                .collect();
        }
        let cost = html.len().saturating_mul(12)
            + blocks
                .iter()
                .map(|b| match b {
                    super::super::ContentBlock::Image { data, .. } => data.len(),
                    super::super::ContentBlock::Paragraph { spans, .. }
                    | super::super::ContentBlock::Heading { spans, .. } => {
                        1024 + spans.iter().map(|s| s.text.len() * 2 + 128).sum::<usize>()
                    }
                    _ => 64,
                })
                .sum::<usize>()
            + images.iter().map(|i| i.data.len()).sum::<usize>();
        let native_document = if slot.illustrated {
            Default::default()
        } else {
            Arc::new(super::super::native::NativeDocument::deferred(
                html,
                slot.path.clone(),
                &blocks,
                self.resources.clone(),
            ))
        };
        let value = Arc::new(LoadedChapter {
            chapter: Arc::new(Chapter {
                title: slot.title.clone(),
                blocks,
                source_href: if slot.illustrated {
                    None
                } else {
                    Some(slot.path.clone())
                },
                native_document,
            }),
            images,
            cost,
        });
        self.loads.fetch_add(1, Ordering::Relaxed);
        crate::open_operation::check()?;
        cache.push_back((index, value.clone()));
        while cache.len() > 1
            && (cache.len() > 8
                || cache.iter().map(|(_, v)| v.cost).sum::<usize>() > 32 * 1024 * 1024)
        {
            cache.pop_front();
        }
        Ok(value)
    }
}

// Probe only enough prefix to establish that the legacy canonical chapter exists.
// No body, image data, native DOM or binding is retained by this index builder.
struct Probe<'a> {
    path: &'a str,
    images: &'a HashMap<String, Vec<u8>>,
    bytes: Vec<u8>,
    found: bool,
    next: usize,
}
impl Probe<'_> {
    fn inspect(&mut self) {
        let text = std::str::from_utf8(&self.bytes)
            .unwrap_or_else(|e| std::str::from_utf8(&self.bytes[..e.valid_up_to()]).unwrap());
        let text = if let Some(amp) = text.rfind('&') {
            if text[amp..].find(';').is_none() && text[amp..].find('>').is_none() {
                &text[..amp]
            } else {
                text
            }
        } else {
            text
        };
        let doc = scraper::Html::parse_document(text);
        self.found = html::parse_document_blocks(&doc, self.path, self.images, false)
            .iter()
            .any(|(_, _, block)| !matches!(block, super::super::ContentBlock::Image { .. }));
    }
}
impl Write for Probe<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        for part in bytes.chunks(1024) {
            crate::open_operation::check_io()?;
            self.bytes.extend_from_slice(part);
            if self.bytes.len() >= self.next {
                self.inspect();
                self.next = self.next.saturating_mul(2);
                if self.found {
                    return Err(std::io::Error::other("chapter-prefix-complete"));
                }
            }
            if self.bytes.len() > 64 * 1024 * 1024 {
                return Err(std::io::Error::other(
                    "Chapter index prefix exceeds safety limit",
                ));
            }
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl EpubBook {
    /// Cold open without retaining or parsing the publication's complete body.
    /// A compatibility catalogue probes chapter prefixes when no index is supplied.
    pub fn open_lazy<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        Self::open_chapter_backed(path.as_ref(), None)
    }
    /// The hash-keyed catalogue stores paths/titles only, never book text or images.
    pub fn open_cached<P: AsRef<Path>>(path: P, index_dir: &Path) -> Result<Self, String> {
        Self::open_chapter_backed(path.as_ref(), Some(index_dir))
    }
    fn open_chapter_backed(path: &Path, index_dir: Option<&Path>) -> Result<Self, String> {
        crate::open_operation::check()?;
        let epub = Arc::new(RbookEpub::open(path).map_err(|e| format!("Cannot open EPUB: {e}"))?);
        crate::open_operation::check()?;
        let title = epub
            .metadata()
            .title()
            .map(|v| v.value().to_string())
            .unwrap_or_else(|| "未知书名".into());
        let image_paths: Vec<_> = epub
            .manifest()
            .images()
            .filter_map(|v| resource_key_to_string(v.resource().key()))
            .collect();
        let hash = if index_dir.is_some() {
            Self::file_hash(&path.to_string_lossy())?
        } else {
            String::new()
        };
        let cache_path = index_dir.map(|p| p.join(format!("chapter-index-v1-{hash}.json")));
        let allowed: HashSet<String> = epub
            .spine()
            .iter()
            .filter_map(|e| e.manifest_entry())
            .filter_map(|e| resource_key_to_string(e.resource().key()))
            .map(|p| normalize_resource_path(&p))
            .collect();
        let cached = cache_path
            .as_ref()
            .and_then(|p| {
                std::fs::metadata(p)
                    .ok()
                    .filter(|m| m.len() < 16 * 1024 * 1024)
                    .and_then(|_| std::fs::read(p).ok())
            })
            .and_then(|b| serde_json::from_slice::<Index>(&b).ok())
            .filter(|v| {
                v.version == 1
                    && v.hash == hash
                    && v.slots.len() <= 100_000
                    && v.slots.iter().all(|s| {
                        allowed.contains(&s.path)
                            && s.title.len() <= 4096
                            && s.before.iter().chain(&s.after).all(|p| allowed.contains(p))
                    })
            });
        let mut probes = 0;
        let index = if let Some(index) = cached {
            index
        } else {
            let toc_items = collect_toc_items(&epub);
            let toc_paths: Vec<_> = toc_items
                .iter()
                .map(|(_, p)| PreparedPath::new(p))
                .collect();
            let fake_images: HashMap<_, _> =
                image_paths.iter().map(|p| (p.clone(), vec![1])).collect();
            let mut slots: Vec<Slot> = Vec::new();
            let mut toc = Vec::new();
            let mut pending = Vec::new();
            for entry in epub.spine().iter().filter_map(|v| v.manifest_entry()) {
                crate::open_operation::check()?;
                let kind = entry.kind().as_str().to_owned();
                if !kind.starts_with("application/xhtml") && !kind.starts_with("text/html") {
                    continue;
                }
                let Some(raw) = resource_key_to_string(entry.resource().key()) else {
                    continue;
                };
                let path = normalize_resource_path(&raw);
                probes += 1;
                let mut probe = Probe {
                    path: &path,
                    images: &fake_images,
                    bytes: Vec::new(),
                    found: false,
                    next: 1024,
                };
                let result = entry.copy_bytes(&mut probe);
                if !probe.found {
                    result.map_err(|e| format!("Cannot index chapter: {e}"))?;
                    probe.inspect();
                }
                if !probe.found {
                    let text =
                        std::str::from_utf8(&probe.bytes).map_err(|_| "Invalid chapter text")?;
                    let document = scraper::Html::parse_document(text);
                    let mut actual_images = HashMap::new();
                    load_referenced_images(
                        &document,
                        &path,
                        &image_paths,
                        &mut actual_images,
                        |p| read_bounded(&epub, p, 32 * 1024 * 1024).ok(),
                    );
                    probe.found =
                        !html::parse_document_blocks(&document, &path, &actual_images, false)
                            .is_empty();
                }
                if !probe.found {
                    let text =
                        std::str::from_utf8(&probe.bytes).map_err(|_| "Invalid chapter text")?;
                    let document = scraper::Html::parse_document(text);
                    let mut images = HashMap::new();
                    load_referenced_images(&document, &path, &image_paths, &mut images, |p| {
                        read_bounded(&epub, p, 32 * 1024 * 1024).ok()
                    });
                    if !html::supplemental_document_images(&document, &path, &images, &[])
                        .is_empty()
                    {
                        pending.push(path);
                    }
                    continue;
                }
                let prepared = PreparedPath::new(&path);
                let matched = toc_paths.iter().position(|p| prepared.matches(p));
                let title = matched
                    .map(|i| toc_items[i].0.clone())
                    .unwrap_or_else(|| format!("第 {} 章", slots.len() + 1));
                if matched.is_some() {
                    toc.push(TocEntry {
                        title: title.clone(),
                        chapter_index: slots.len(),
                    });
                }
                slots.push(Slot {
                    path,
                    title,
                    before: std::mem::take(&mut pending),
                    after: Vec::new(),
                    illustrated: false,
                });
            }
            if !pending.is_empty() {
                if let Some(last) = slots.last_mut() {
                    last.after = pending;
                } else {
                    let path = pending.remove(0);
                    slots.push(Slot {
                        path,
                        title: title.clone(),
                        before: pending,
                        after: Vec::new(),
                        illustrated: true,
                    });
                }
            }
            if toc.is_empty() {
                toc = slots
                    .iter()
                    .enumerate()
                    .map(|(i, s)| TocEntry {
                        title: s.title.clone(),
                        chapter_index: i,
                    })
                    .collect();
            }
            let value = Index {
                version: 1,
                hash,
                slots,
                toc,
            };
            if let Some(p) = &cache_path {
                crate::open_operation::check()?;
                if let Some(parent) = p.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Ok(bytes) = serde_json::to_vec(&value) {
                    let tmp = p.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
                    if std::fs::write(&tmp, bytes).is_ok() {
                        let _ = std::fs::rename(&tmp, p);
                    }
                }
            }
            value
        };
        let chapters = index
            .slots
            .iter()
            .map(|s| Chapter {
                title: s.title.clone(),
                source_href: if s.illustrated {
                    None
                } else {
                    Some(s.path.clone())
                },
                blocks: Vec::new(),
                native_document: Default::default(),
            })
            .collect();
        let (chapter_reviews, review_chapter_indices) = review_catalogue(&index.slots);
        crate::open_operation::check()?;
        let resources = Arc::new(super::super::native::NativeResources::new(epub.clone()));
        Ok(Self {
            title,
            chapters,
            toc: index.toc,
            cover_data: epub
                .manifest()
                .cover_image()
                .and_then(|v| resource_key_to_string(v.resource().key()))
                .and_then(|p| read_bounded(&epub, &p, 16 * 1024 * 1024).ok()),
            fonts: Vec::new(),
            supplemental_images: HashMap::new(),
            chapter_reviews,
            review_chapter_indices,
            chapter_store: Some(Arc::new(ChapterStore {
                archive: Mutex::new(epub),
                resources,
                slots: index.slots,
                image_paths,
                cache: Default::default(),
                loads: AtomicUsize::new(0),
                probes,
            })),
        })
    }
    pub fn loaded_chapter(&self, index: usize) -> Result<Arc<LoadedChapter>, String> {
        if let Some(store) = &self.chapter_store {
            store.load(index)
        } else {
            Ok(Arc::new(LoadedChapter {
                chapter: Arc::new(
                    self.chapters
                        .get(index)
                        .ok_or("Reading chapter is out of range")?
                        .clone(),
                ),
                images: self
                    .supplemental_images
                    .get(&index)
                    .cloned()
                    .unwrap_or_default(),
                cost: 0,
            }))
        }
    }
    pub fn chapter(&self, index: usize) -> Result<Arc<Chapter>, String> {
        let loaded = self.loaded_chapter(index)?;
        Ok(loaded.chapter.clone())
    }
    pub fn load_embedded_fonts(&mut self) -> Result<(), String> {
        let Some(store) = &self.chapter_store else {
            return Ok(());
        };
        let epub = store.archive.lock().unwrap_or_else(|e| e.into_inner());
        let mut total = 0;
        let mut fonts = Vec::new();
        for entry in epub.manifest().fonts() {
            let key =
                resource_key_to_string(entry.resource().key()).ok_or("Invalid embedded font")?;
            let data = read_bounded(&epub, &key, 64 * 1024 * 1024 - total)?;
            total += data.len();
            if total > 64 * 1024 * 1024 {
                return Err("Embedded fonts exceed memory limit".into());
            }
            let name = resource_key_to_string(entry.resource().key())
                .and_then(|p| {
                    Path::new(&p)
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                })
                .unwrap_or_else(|| "EmbeddedFont".into());
            fonts.push((name, data));
        }
        self.fonts = fonts;
        Ok(())
    }
    pub fn chapter_cache_stats(&self) -> (usize, usize, usize) {
        self.chapter_store
            .as_ref()
            .map_or((self.chapters.len(), 0, 0), |s| {
                (
                    s.cache.lock().unwrap_or_else(|e| e.into_inner()).len(),
                    s.loads.load(Ordering::Relaxed),
                    s.probes,
                )
            })
    }
    pub fn ensure_chapter_window(
        &mut self,
        start: usize,
        end: usize,
    ) -> Result<Vec<usize>, String> {
        if self.chapter_store.is_none() {
            return Ok(Vec::new());
        }
        let mut evicted = Vec::new();
        for index in 0..self.chapters.len() {
            if !(start..end).contains(&index)
                && (!self.chapters[index].blocks.is_empty()
                    || self.supplemental_images.contains_key(&index))
            {
                self.chapters[index].blocks.clear();
                self.chapters[index].native_document = Default::default();
                self.supplemental_images.remove(&index);
                evicted.push(index);
            }
        }
        for index in start..end.min(self.chapters.len()) {
            if self.chapters[index].blocks.is_empty() {
                let loaded = self.loaded_chapter(index)?;
                self.chapters[index] = (*loaded.chapter).clone();
                self.supplemental_images
                    .insert(index, loaded.images.clone());
            }
        }
        Ok(evicted)
    }
}
fn review_catalogue(slots: &[Slot]) -> (HashMap<usize, usize>, HashSet<usize>) {
    let mut links = HashMap::new();
    let mut indices = HashSet::new();
    for (i, s) in slots.iter().enumerate() {
        if let Some(base) = s.title.strip_suffix(" - 段评") {
            indices.insert(i);
            let ordinal = slots[..i].iter().filter(|v| v.title == s.title).count();
            if let Some((main, _)) = slots
                .iter()
                .enumerate()
                .filter(|(_, v)| v.title == base)
                .nth(ordinal)
            {
                links.insert(main, i);
            }
        }
    }
    (links, indices)
}

/// Bound decompression before allocating a whole resource, including ZIP bombs.
pub(crate) fn read_bounded(epub: &RbookEpub, path: &str, limit: usize) -> Result<Vec<u8>, String> {
    struct Bytes {
        data: Vec<u8>,
        limit: usize,
        exceeded: bool,
    }
    impl Write for Bytes {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            crate::open_operation::check_io()?;
            if bytes.len() > self.limit.saturating_sub(self.data.len()) {
                self.exceeded = true;
                return Err(std::io::Error::other(
                    "Resource exceeds chapter memory limit",
                ));
            };
            self.data.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut bytes = Bytes {
        data: Vec::new(),
        limit,
        exceeded: false,
    };
    epub.copy_resource(path, &mut bytes).map_err(|_| {
        if bytes.exceeded {
            "Resource exceeds chapter memory limit"
        } else {
            "Cannot read EPUB resource"
        }
    })?;
    Ok(bytes.data)
}
