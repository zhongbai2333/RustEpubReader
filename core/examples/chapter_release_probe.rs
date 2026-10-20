//! Offline production-core timings; does not measure platform drawing or GPU presentation.
use reader_core::diagnostics::{peak_rss, rss};
use reader_core::epub::{native::CssPolicy, EpubBook};
use serde_json::json;
use std::time::Instant;
fn main() {
    if cfg!(debug_assertions) {
        panic!("Run the performance probe with --release");
    }
    let path = std::env::args().nth(1).expect("EPUB path");
    let dir = tempfile::tempdir().unwrap();
    let baseline = rss();
    let start = Instant::now();
    let book = EpubBook::open_cached(&path, dir.path()).unwrap();
    let open_ms = start.elapsed().as_secs_f64() * 1000.;
    let metadata_rss = rss();
    let cold_stats = book.chapter_cache_stats();
    assert!(!book.chapters.is_empty(), "EPUB has no readable chapters");
    let first = 3.min(book.chapters.len() - 1);
    let mut samples = Vec::new();
    for index in [first, book.chapters.len() / 2, book.chapters.len() - 1] {
        let t = Instant::now();
        let chapter = book.chapter(index).unwrap();
        let body_ms = t.elapsed().as_secs_f64() * 1000.;
        let style = chapter
            .native_document
            .present(CssPolicy::Mixed, false, &[]);
        assert!(style.errors.is_empty());
        samples.push(json!({"chapter":index,"body_ms":body_ms,"body_and_style_ms":t.elapsed().as_secs_f64()*1000.,"blocks":chapter.blocks.len()}));
    }
    let after_samples = rss();
    let mut walk = Vec::new();
    for index in first..(first + 200).min(book.chapters.len()) {
        let t = Instant::now();
        let chapter = book.chapter(index).unwrap();
        let _ = chapter
            .native_document
            .present(CssPolicy::Mixed, false, &[]);
        walk.push(t.elapsed().as_secs_f64() * 1000.);
    }
    walk.sort_by(f64::total_cmp);
    let walk_rss = rss();
    let stats = book.chapter_cache_stats();
    let chapters = book.chapters.len();
    drop(book);
    let released_rss = rss();
    let t = Instant::now();
    let cached = EpubBook::open_cached(&path, dir.path()).unwrap();
    let cached_open_ms = t.elapsed().as_secs_f64() * 1000.;
    assert_eq!(cached.chapter_cache_stats(), (0, 0, 0));
    println!("{}",serde_json::to_string_pretty(&json!({"profile":"release","book_sha256":EpubBook::file_hash(&path).unwrap(),"chapter_count":chapters,"cold_catalogue_ms":open_ms,"cold_stats":cold_stats,"cached_catalogue_ms":cached_open_ms,"samples":samples,"sequential_200_chapters":{"median_body_style_ms":walk[walk.len()/2],"p95_body_style_ms":walk[walk.len()*95/100],"maximum_body_style_ms":walk.last(),"cache_stats":stats},"rss_bytes":{"baseline":baseline,"catalogue":metadata_rss,"after_samples":after_samples,"after_200_chapters":walk_rss,"after_close":released_rss,"peak":peak_rss()},"platform_first_paint_measured":false})).unwrap());
}
