//! Explicit, repeatable cold-open measurements. No timing assertions in normal CI.
use epub_builder::{EpubBuilder, EpubContent, ZipLibrary};
use reader_core::epub::EpubBook;
use std::{io::Cursor, time::Instant};

#[test]
#[ignore = "cold-open profiling; run explicitly with --ignored --nocapture"]
fn long_books_cold_open() {
    let directory = tempfile::tempdir().unwrap();
    let mut measurements = Vec::new();
    for (chapters, paragraphs) in [(1, 1000), (1, 4000), (1, 8000), (200, 40)] {
        let path = directory
            .path()
            .join(format!("{chapters}-{paragraphs}.epub"));
        let mut builder = EpubBuilder::new(ZipLibrary::new().unwrap()).unwrap();
        builder.metadata("title", "Long book benchmark").unwrap();
        for chapter in 0..chapters {
            let mut html = String::from("<html xmlns='http://www.w3.org/1999/xhtml'><head><title>Chapter</title></head><body>");
            for paragraph in 0..paragraphs {
                html.push_str(&format!("<p>第{chapter}章第{paragraph}段：正文不能漏，阅读进度坐标必须稳定。A paragraph with Unicode 😀 and enough words to measure a long book.</p>"));
            }
            html.push_str("</body></html>");
            builder
                .add_content(EpubContent::new(
                    format!("ch{chapter}.xhtml"),
                    Cursor::new(html),
                ))
                .unwrap();
        }
        builder
            .generate(std::fs::File::create(&path).unwrap())
            .unwrap();
        if let Ok(destination) = std::env::var("EPUB_OPEN_FIXTURES") {
            std::fs::create_dir_all(&destination).unwrap();
            std::fs::copy(
                &path,
                std::path::Path::new(&destination).join(path.file_name().unwrap()),
            )
            .unwrap();
        }
        let start = Instant::now();
        let book = EpubBook::open(&path).unwrap();
        let open_ms = start.elapsed().as_secs_f64() * 1000.;
        assert_eq!(book.chapters.len(), chapters);
        assert!(book.chapters.iter().all(|ch| ch.blocks.len() == paragraphs));
        let start = Instant::now();
        let clone = book.clone();
        let clone_ms = start.elapsed().as_secs_f64() * 1000.;
        drop(clone);
        let measurement = serde_json::json!({"chapters":chapters,"paragraphs_per_chapter":paragraphs,"open_ms":open_ms,"clone_ms":clone_ms});
        println!("{measurement}");
        measurements.push(measurement);
    }
    if let Ok(path) = std::env::var("EPUB_OPEN_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&measurements).unwrap()).unwrap();
    }
}
