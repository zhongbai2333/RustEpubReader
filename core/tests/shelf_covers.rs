use reader_core::{epub::EpubBook, library::covers};
use std::{
    io::{Cursor, Write},
    path::Path,
};

fn epub(path: &Path, color: Option<[u8; 3]>, body: Option<&str>) {
    let bytes = color.map(|color| {
        let image = image::RgbImage::from_pixel(960, 1440, image::Rgb(color));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        bytes.into_inner()
    });
    epub_bytes(path, bytes.as_deref(), body);
}

fn epub_bytes(path: &Path, data: Option<&[u8]>, body: Option<&str>) {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    let mut put = |name: &str, bytes: &[u8]| {
        zip.start_file(name, options).unwrap();
        zip.write_all(bytes).unwrap();
    };
    put("mimetype", b"application/epub+zip");
    put("META-INF/container.xml", br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#);
    let cover = if data.is_some() {
        r#"<item id="cover" href="cover.png" media-type="image/png" properties="cover-image"/>"#
    } else {
        ""
    };
    put("book.opf", format!(r#"<?xml version="1.0"?><package version="3.0" xmlns="http://www.idpf.org/2007/opf" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">fixture</dc:identifier><dc:title>Shelf fixture</dc:title><dc:language>zh</dc:language></metadata><manifest>{cover}<item id="text" href="text.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="text"/></spine></package>"#).as_bytes());
    if let Some(bytes) = data {
        put("cover.png", bytes);
    }
    if let Some(body) = body {
        put("text.xhtml", body.as_bytes());
    }
    zip.finish().unwrap();
}

#[test]
fn shelf_cover_does_not_read_chapters_and_is_bounded_cached_and_invalidated() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("book.epub");
    let cache = dir.path().join("cache");
    // The spine deliberately points to a missing file: a full book parse fails.
    epub(&source, Some([220, 30, 10]), None);
    assert!(EpubBook::open(&source).is_err());
    assert!(EpubBook::read_cover(&source).unwrap().is_some());
    let first = covers::thumbnail(&source, &cache).unwrap();
    let image = image::load_from_memory(&first).unwrap();
    assert_eq!((image.width(), image.height()), (480, 720));
    let file = std::fs::read_dir(&cache)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let modified = file.metadata().unwrap().modified().unwrap();
    assert_eq!(covers::thumbnail(&source, &cache).unwrap(), first);
    assert_eq!(
        file.metadata().unwrap().modified().unwrap(),
        modified,
        "warm lookup rewrote the thumbnail"
    );
    // Source changes invalidate positive and negative entries without a full EPUB hash.
    epub(&source, None, Some("different source length"));
    assert!(covers::thumbnail(&source, &cache).is_none());
    assert!(std::fs::read_dir(&cache)
        .unwrap()
        .any(|f| f.unwrap().metadata().unwrap().len() == 0));
    epub(
        &source,
        Some([10, 40, 230]),
        Some("new source length, new cover"),
    );
    assert_ne!(covers::thumbnail(&source, &cache).unwrap(), first);
}

#[test]
fn invalid_epub_is_not_persisted_as_a_missing_cover() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("book.epub");
    let cache = dir.path().join("cache");
    std::fs::write(&source, b"partial download").unwrap();
    assert!(covers::thumbnail(&source, &cache).is_none());
    assert!(!cache.exists());
    epub(&source, Some([30, 180, 20]), None);
    let bytes = covers::thumbnail(&source, &cache).unwrap();
    let file = std::fs::read_dir(&cache)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::write(file, b"partial cache").unwrap();
    assert_eq!(covers::thumbnail(&source, &cache).unwrap(), bytes);
}

#[test]
fn chapter_heavy_book_cold_and_warm_shelf_work() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("large.epub");
    let cache = dir.path().join("cache");
    let body = format!(
        "<html xmlns=\"http://www.w3.org/1999/xhtml\"><body>{}</body></html>",
        "<p>很多书籍的书架只需读取封面，不应解析正文和样式。</p>".repeat(2000)
    );
    epub(&source, Some([100, 80, 60]), Some(&body));
    let start = std::time::Instant::now();
    let book = EpubBook::open(&source).unwrap();
    let full = start.elapsed();
    assert!(book.cover_data.is_some());
    let start = std::time::Instant::now();
    let bytes = covers::thumbnail(&source, &cache).unwrap();
    let cold = start.elapsed();
    let start = std::time::Instant::now();
    for _ in 0..100 {
        assert_eq!(covers::thumbnail(&source, &cache).unwrap(), bytes);
    }
    println!("Shelf fixture (2000 paragraphs): full parse {full:?}, cold thumbnail {cold:?}, 100 warm thumbnails {:?}", start.elapsed());
}

#[test]
fn oversized_cover_decompression_is_bounded_and_gif_bmp_remain_supported() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("book.epub");
    epub_bytes(&source, Some(&vec![0; 16 * 1024 * 1024 + 1]), None);
    assert!(EpubBook::read_cover(&source).is_err());
    assert!(covers::thumbnail(&source, &dir.path().join("oversize-cache")).is_none());
    assert!(!dir.path().join("oversize-cache").exists());
    for format in [image::ImageFormat::Gif, image::ImageFormat::Bmp] {
        let image = image::RgbImage::from_pixel(20, 30, image::Rgb([15, 180, 90]));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, format).unwrap();
        epub_bytes(&source, Some(bytes.get_ref()), None);
        let thumbnail =
            covers::thumbnail(&source, &dir.path().join(format!("{format:?}"))).unwrap();
        let image = image::load_from_memory(&thumbnail).unwrap();
        assert_eq!((image.width(), image.height()), (20, 30));
    }
}
