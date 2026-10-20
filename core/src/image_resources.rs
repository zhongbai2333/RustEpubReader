//! Optional book-scoped binary image transport. Not enabled by native renderers yet.
//! A session owns a bounded/lazy publication, not an unbounded image-ID byte map.
use crate::{
    epub::{ContentBlock, EpubBook},
    open_operation::Cancellation,
};
use std::sync::{Arc, Mutex, OnceLock};
use uuid::Uuid;

const EXPIRED: &str = "Image session expired";
const INVALID: &str = "Invalid image resource";
pub const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageLocation {
    Block(usize),
    Supplemental(usize),
    Rendered(usize),
}
impl ImageLocation {
    fn parts(self) -> (char, usize) {
        match self {
            Self::Block(i) => ('b', i),
            Self::Supplemental(i) => ('s', i),
            Self::Rendered(i) => ('r', i),
        }
    }
}
struct Session {
    owner: String,
    nonce: String,
    book: OnceLock<Arc<EpubBook>>,
    cancel: Cancellation,
}
impl Session {
    fn run<T>(&self, work: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        self.cancel.run(work).map_err(|error| {
            if error == crate::open_operation::CANCELLED {
                EXPIRED.into()
            } else {
                error
            }
        })
    }
}

#[derive(Clone)]
pub struct ImageReferenceFactory {
    nonce: String,
    chapters: usize,
    cancel: Cancellation,
}
impl ImageReferenceFactory {
    /// A reference is a locator, not a claim that its target is an image. Fetch
    /// validates the actual canonical/supplemental/rendered item before transfer.
    pub fn reference(&self, chapter: usize, location: ImageLocation) -> Result<String, String> {
        self.cancel.check().map_err(|_| EXPIRED)?;
        if chapter >= self.chapters {
            return Err(INVALID.into());
        }
        let (kind, index) = location.parts();
        Ok(format!("img1:{}:{chapter}:{kind}:{index}", self.nonce))
    }
}

#[derive(Default)]
pub struct ImageResources {
    current: Mutex<Option<Arc<Session>>>,
}
impl ImageResources {
    /// Reserve on the native reading request before asynchronous preparation.
    /// A later attach cannot publish over a replacement or a closed request.
    pub fn reserve(&self, owner: &str) -> Result<String, String> {
        if owner.is_empty() {
            return Err(INVALID.into());
        }
        let nonce = Uuid::new_v4().to_string();
        let session = Arc::new(Session {
            owner: owner.into(),
            nonce: nonce.clone(),
            book: OnceLock::new(),
            cancel: Cancellation::default(),
        });
        let previous = {
            let mut current = self
                .current
                .lock()
                .map_err(|_| "Image registry unavailable")?;
            if let Some(previous) = current.as_ref() {
                previous.cancel.cancel();
            }
            current.replace(session)
        };
        drop(previous);
        Ok(nonce)
    }
    pub fn attach(&self, owner: &str, nonce: &str, book: Arc<EpubBook>) -> Result<(), String> {
        if book.chapters.is_empty() {
            return Err(INVALID.into());
        }
        let current = self
            .current
            .lock()
            .map_err(|_| "Image registry unavailable")?;
        let session = current
            .as_ref()
            .filter(|s| s.owner == owner && s.nonce == nonce)
            .ok_or(EXPIRED)?;
        session
            .book
            .set(book)
            .map_err(|_| "Image session already prepared".into())
    }
    /// Convenience for an already prepared book. UI adapters preparing off-thread
    /// should reserve at the reading event and attach using that exact nonce.
    pub fn open(&self, owner: &str, book: Arc<EpubBook>) -> Result<String, String> {
        if book.chapters.is_empty() {
            return Err(INVALID.into());
        }
        let nonce = self.reserve(owner)?;
        self.attach(owner, &nonce, book)?;
        Ok(nonce)
    }
    fn session(&self, owner: &str, nonce: &str) -> Result<Arc<Session>, String> {
        self.current
            .lock()
            .map_err(|_| "Image registry unavailable")?
            .as_ref()
            .filter(|s| s.owner == owner && s.nonce == nonce)
            .cloned()
            .ok_or_else(|| EXPIRED.into())
    }
    /// Admission is short. IO/encoding runs outside the registry mutex and is
    /// cooperatively cancelled on close/replacement, including before publication.
    pub fn with_session<T>(
        &self,
        owner: &str,
        nonce: &str,
        work: impl FnOnce(&EpubBook, ImageReferenceFactory) -> Result<T, String>,
    ) -> Result<T, String> {
        let session = self.session(owner, nonce)?;
        session.run(|| {
            let book = session.book.get().ok_or("Image session not prepared")?;
            work(
                book,
                ImageReferenceFactory {
                    nonce: session.nonce.clone(),
                    chapters: book.chapters.len(),
                    cancel: session.cancel.clone(),
                },
            )
        })
    }
    pub fn close(&self, owner: &str, nonce: &str) -> Result<bool, String> {
        let previous = {
            let mut current = self
                .current
                .lock()
                .map_err(|_| "Image registry unavailable")?;
            if current
                .as_ref()
                .is_none_or(|s| s.owner != owner || s.nonce != nonce)
            {
                return Ok(false);
            }
            let previous = current.take();
            if let Some(previous) = &previous {
                previous.cancel.cancel();
            }
            previous
        };
        drop(previous);
        Ok(true)
    }
    /// Returned bytes have ordinary Arc ownership. Close invalidates subsequent
    /// requests; it does not revoke bytes/bitmaps already returned to a caller.
    pub fn bytes(&self, owner: &str, reference: &str) -> Result<Arc<Vec<u8>>, String> {
        let parsed = ParsedReference::parse(reference)?;
        let session = self.session(owner, parsed.nonce)?;
        session.run(|| {
            let book = session.book.get().ok_or("Image session not prepared")?;
            if parsed.chapter >= book.chapters.len() {
                return Err(INVALID.into());
            }
            let bytes = match parsed.location {
                ImageLocation::Block(index) => {
                    let chapter = book.loaded_chapter(parsed.chapter)?;
                    match chapter.chapter.blocks.get(index) {
                        Some(ContentBlock::Image { data, .. }) => data.clone(),
                        _ => return Err(INVALID.into()),
                    }
                }
                ImageLocation::Supplemental(index) => book
                    .loaded_chapter(parsed.chapter)?
                    .images
                    .get(index)
                    .ok_or(INVALID)?
                    .data
                    .clone(),
                ImageLocation::Rendered(index) => {
                    match book.render_chapter(parsed.chapter)?.get(index) {
                        Some((_, _, ContentBlock::Image { data, .. })) => data.clone(),
                        _ => return Err(INVALID.into()),
                    }
                }
            };
            if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
                return Err("Image exceeds transfer limits".into());
            }
            Ok(bytes)
        })
    }
}
impl Drop for ImageResources {
    fn drop(&mut self) {
        let current = self.current.get_mut().unwrap_or_else(|e| e.into_inner());
        if let Some(session) = current {
            session.cancel.cancel();
        }
    }
}
struct ParsedReference<'a> {
    nonce: &'a str,
    chapter: usize,
    location: ImageLocation,
}
impl<'a> ParsedReference<'a> {
    fn parse(value: &'a str) -> Result<Self, String> {
        if value.len() > 128 {
            return Err(INVALID.into());
        }
        let mut parts = value.split(':');
        if parts.next() != Some("img1") {
            return Err(INVALID.into());
        }
        let nonce = parts.next().ok_or(INVALID)?;
        if nonce.len() != 36
            || Uuid::parse_str(nonce)
                .map(|id| id.to_string())
                .ok()
                .as_deref()
                != Some(nonce)
        {
            return Err(INVALID.into());
        }
        fn index(value: Option<&str>) -> Result<usize, String> {
            let value = value.ok_or(INVALID)?;
            let number = value.parse::<usize>().map_err(|_| INVALID)?;
            if number.to_string() != value {
                return Err(INVALID.into());
            }
            Ok(number)
        }
        let chapter = index(parts.next())?;
        let kind = parts.next().ok_or(INVALID)?;
        let value = index(parts.next())?;
        let location = match kind {
            "b" => ImageLocation::Block(value),
            "s" => ImageLocation::Supplemental(value),
            "r" => ImageLocation::Rendered(value),
            _ => return Err(INVALID.into()),
        };
        if parts.next().is_some() {
            return Err(INVALID.into());
        }
        Ok(Self {
            nonce,
            chapter,
            location,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::epub::{parser::SupplementalImage, Chapter};
    fn fixture() -> Arc<EpubBook> {
        Arc::new(EpubBook {
            chapter_store: None,
            title: "Book".into(),
            chapters: vec![Chapter {
                native_document: Default::default(),
                title: "Chapter".into(),
                source_href: None,
                blocks: vec![
                    ContentBlock::BlankLine,
                    ContentBlock::Image {
                        native_style: Default::default(),
                        data: Arc::new(vec![1, 2, 3]),
                        alt: Some("Inline".into()),
                    },
                ],
            }],
            toc: vec![],
            cover_data: None,
            fonts: vec![],
            supplemental_images: std::collections::HashMap::from([(
                0,
                vec![SupplementalImage {
                    block: 0,
                    after: true,
                    data: Arc::new(vec![4, 5, 6]),
                    alt: None,
                }],
            )]),
            chapter_reviews: Default::default(),
            review_chapter_indices: Default::default(),
        })
    }
    fn reference(
        store: &ImageResources,
        owner: &str,
        token: &str,
        location: ImageLocation,
    ) -> String {
        store
            .with_session(owner, token, |_, factory| factory.reference(0, location))
            .unwrap()
    }
    #[test]
    fn binary_references_preserve_canonical_supplemental_and_rendered_images_without_byte_copies() {
        let book = fixture();
        let store = ImageResources::default();
        let token = store.open("root/book", book.clone()).unwrap();
        let block = reference(&store, "root/book", &token, ImageLocation::Block(1));
        let supplemental = reference(&store, "root/book", &token, ImageLocation::Supplemental(0));
        assert_eq!(*store.bytes("root/book", &block).unwrap(), [1, 2, 3]);
        assert_eq!(*store.bytes("root/book", &supplemental).unwrap(), [4, 5, 6]);
        assert!(Arc::ptr_eq(
            &store.bytes("root/book", &block).unwrap(),
            &store.bytes("root/book", &block).unwrap()
        ));
        for (index, (_, _, value)) in book.render_chapter(0).unwrap().iter().enumerate() {
            if let ContentBlock::Image { data, .. } = value {
                let id = reference(&store, "root/book", &token, ImageLocation::Rendered(index));
                assert!(Arc::ptr_eq(&store.bytes("root/book", &id).unwrap(), data));
            }
        }
    }
    #[test]
    fn malformed_foreign_replaced_closed_and_non_image_references_are_rejected() {
        let store = ImageResources::default();
        let token = store.open("A", fixture()).unwrap();
        let old = reference(&store, "A", &token, ImageLocation::Block(1));
        assert!(store.bytes("B", &old).is_err());
        for id in [
            String::new(),
            "../secret".into(),
            old.replace(":0:b:", ":00:b:"),
            old.replace(":b:", ":x:"),
            format!("{old}:tail"),
            "文".repeat(129),
        ] {
            assert!(store.bytes("A", &id).is_err());
        }
        for location in [
            ImageLocation::Block(0),
            ImageLocation::Block(usize::MAX),
            ImageLocation::Supplemental(2),
        ] {
            let id = reference(&store, "A", &token, location);
            assert!(store.bytes("A", &id).is_err());
        }
        let new_token = store.open("A", fixture()).unwrap();
        assert_ne!(token, new_token);
        assert!(!store.close("A", &token).unwrap());
        assert!(store.bytes("A", &old).is_err());
        let new = reference(&store, "A", &new_token, ImageLocation::Block(1));
        let owned = store.bytes("A", &new).unwrap();
        assert!(store.close("A", &new_token).unwrap());
        assert!(store.bytes("A", &new).is_err());
        assert_eq!(*owned, [1, 2, 3]);
    }
    #[test]
    fn an_old_preparation_or_close_cannot_replace_a_new_reading_reservation() {
        let store = ImageResources::default();
        let old = store.reserve("A").unwrap();
        assert!(store.with_session("A", &old, |_, _| Ok(())).is_err());
        let next = store.reserve("B").unwrap();
        assert!(store.attach("A", &old, fixture()).is_err());
        assert!(!store.close("A", &old).unwrap());
        store.attach("B", &next, fixture()).unwrap();
        assert!(store.attach("B", &next, fixture()).is_err());
        let id = reference(&store, "B", &next, ImageLocation::Block(1));
        assert_eq!(*store.bytes("B", &id).unwrap(), [1, 2, 3]);
        assert!(store.close("B", &next).unwrap());
        assert!(store.attach("B", &next, fixture()).is_err());
    }

    #[test]
    fn closing_does_not_wait_for_a_worker_or_poison_a_reused_publication() {
        let store = Arc::new(ImageResources::default());
        let book = fixture();
        let token = store.open("A", book.clone()).unwrap();
        let (entered, wait) = std::sync::mpsc::channel();
        let (resume, paused) = std::sync::mpsc::channel();
        let worker = store.clone();
        let old = token.clone();
        let thread = std::thread::spawn(move || {
            worker.with_session("A", &old, |_, factory| {
                entered.send(()).unwrap();
                paused.recv().unwrap();
                factory.reference(0, ImageLocation::Block(1))
            })
        });
        wait.recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let closer = store.clone();
        let closing_token = token.clone();
        let (closed, result) = std::sync::mpsc::channel();
        let closing = std::thread::spawn(move || {
            closed.send(closer.close("A", &closing_token)).unwrap();
        });
        let completed = result.recv_timeout(std::time::Duration::from_secs(5));
        // Always release the worker, even if a future regression holds the registry
        // mutex across IO. The test fails instead of deadlocking the entire suite.
        resume.send(()).unwrap();
        let worker_result = thread.join().unwrap();
        closing.join().unwrap();
        assert!(completed.unwrap().unwrap());
        assert_eq!(worker_result.unwrap_err(), EXPIRED);
        let next = store.open("A", book).unwrap();
        let id = reference(&store, "A", &next, ImageLocation::Block(1));
        assert_eq!(*store.bytes("A", &id).unwrap(), [1, 2, 3]);
        assert!(crate::open_operation::check().is_ok());
    }
    #[test]
    fn empty_and_oversized_binary_transfers_fail_before_publication() {
        for size in [0, MAX_IMAGE_BYTES + 1] {
            let mut book = (*fixture()).clone();
            if let ContentBlock::Image { data, .. } = &mut book.chapters[0].blocks[1] {
                *data = Arc::new(vec![0; size]);
            }
            let store = ImageResources::default();
            let token = store.open("A", Arc::new(book)).unwrap();
            let id = reference(&store, "A", &token, ImageLocation::Block(1));
            assert!(store.bytes("A", &id).unwrap_err().contains("limits"));
        }
    }
}
