use reader_core::epub::{native::CssPolicy, EpubBook};
fn body(chapter: &reader_core::epub::Chapter) -> serde_json::Value {
    serde_json::to_value(chapter).unwrap()
}
#[test]
fn lazy_catalogue_and_bodies_match_existing_coordinates_styles_and_media() {
    for name in [
        "illustrations",
        "native-typography",
        "original-layout",
        "pagination-stress",
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(format!("{name}.epub"));
        let old = EpubBook::open(&path).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let lazy = EpubBook::open_cached(&path, dir.path()).unwrap();
        assert_eq!(old.chapters.len(), lazy.chapters.len(), "{name}");
        assert_eq!(
            serde_json::to_value(&old.toc).unwrap(),
            serde_json::to_value(&lazy.toc).unwrap(),
            "{name}"
        );
        assert!(lazy.chapters.iter().all(|ch| ch.blocks.is_empty()));
        assert_eq!(lazy.chapter_cache_stats().1, 0);
        for index in 0..old.chapters.len() {
            let actual = lazy.chapter(index).unwrap();
            assert_eq!(body(&old.chapters[index]), body(&actual), "{name}/{index}");
            assert_eq!(
                serde_json::to_value(old.render_chapter(index).unwrap()).unwrap(),
                serde_json::to_value(lazy.render_chapter(index).unwrap()).unwrap(),
                "render {name}/{index}"
            );
            for policy in [CssPolicy::None, CssPolicy::Mixed, CssPolicy::Publisher] {
                assert_eq!(
                    serde_json::to_value(old.chapters[index].native_document.present(
                        policy,
                        false,
                        &[]
                    ))
                    .unwrap(),
                    serde_json::to_value(actual.native_document.present(policy, false, &[]))
                        .unwrap(),
                    "styles {name}/{index}/{policy:?}"
                );
            }
            assert!(lazy.chapter_cache_stats().0 <= 8);
        }
        drop(lazy);
        let cached = EpubBook::open_cached(&path, dir.path()).unwrap();
        assert_eq!(cached.chapter_cache_stats(), (0, 0, 0));
        for index in (0..cached.chapters.len()).rev() {
            assert_eq!(
                body(&old.chapters[index]),
                body(&cached.chapter(index).unwrap())
            );
        }
    }
}
#[test]
fn moving_archive_preserves_unloaded_resources_and_eviction_reloads_correctly() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../ios/Tests/fixtures/native-reading-modes.epub");
    let dir = tempfile::tempdir().unwrap();
    let before = dir.path().join("before.epub");
    let after = dir.path().join("after.epub");
    std::fs::copy(&source, &before).unwrap();
    let book = EpubBook::open_lazy(&before).unwrap();
    let first = body(&book.chapter(0).unwrap());
    std::fs::copy(&before, &after).unwrap();
    book.relocate_source(&after).unwrap();
    std::fs::remove_file(&before).unwrap();
    for index in 0..book.chapters.len() {
        book.chapter(index).unwrap();
        assert!(book.chapter_cache_stats().0 <= 8);
    }
    assert_eq!(first, body(&book.chapter(0).unwrap()));
    assert!(book.chapter_cache_stats().1 > book.chapters.len());
}

#[test]
#[ignore = "explicit source-compatibility audit of a caller-supplied local EPUB"]
fn entire_real_book_keeps_legacy_chapter_block_and_scalar_coordinates() {
    use sha2::{Digest, Sha256};
    let path = std::env::var("READER_LARGE_BOOK").unwrap();
    let old = EpubBook::open(&path).unwrap();
    let lazy = EpubBook::open_lazy(&path).unwrap();
    assert_eq!(old.chapters.len(), lazy.chapters.len());
    let mut blocks = 0;
    let mut characters = 0;
    for index in 0..old.chapters.len() {
        let actual = lazy.chapter(index).unwrap();
        let reference = Sha256::digest(serde_json::to_vec(&old.chapters[index]).unwrap());
        let observed = Sha256::digest(serde_json::to_vec(&*actual).unwrap());
        assert_eq!(reference, observed, "canonical chapter {index}");
        blocks += actual.blocks.len();
        for block in &actual.blocks {
            if let reader_core::epub::ContentBlock::Paragraph { spans, .. }
            | reader_core::epub::ContentBlock::Heading { spans, .. } = block
            {
                characters += spans.iter().map(|s| s.text.chars().count()).sum::<usize>();
            }
        }
        assert!(lazy.chapter_cache_stats().0 <= 8);
    }
    let report = serde_json::json!({"book_sha256":EpubBook::file_hash(&path).unwrap(),"chapters":old.chapters.len(),"blocks":blocks,"characters":characters,"all_canonical_body_digests_match":true,"resident_cache_chapters":lazy.chapter_cache_stats().0});
    std::fs::write(
        std::env::var("READER_COMPATIBILITY_REPORT").unwrap(),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}

#[test]
fn empty_chapters_entities_and_illustrated_fallback_keep_the_same_catalogue() {
    use epub_builder::{EpubBuilder, EpubContent, ZipLibrary};
    use std::io::Cursor;
    let documents = [
        vec!["<body>   <br/> </body>".to_owned()],
        vec![
            format!("<body><p>{}&nbsp;</p></body>", " ".repeat(997)),
            "<body><p id='anchor'></p></body>".into(),
            "<body><h1>第一章</h1><p>正文😀</p></body>".into(),
        ],
        vec![
            "<body><svg xmlns='http://www.w3.org/2000/svg' width='20' height='20'><rect width='20' height='20' fill='red'/></svg></body>".into(),
            "<body><svg xmlns='http://www.w3.org/2000/svg' width='20' height='20'><rect width='20' height='20' fill='blue'/></svg></body>".into(),
        ],
    ];
    for documents in documents {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.epub");
        let mut builder = EpubBuilder::new(ZipLibrary::new().unwrap()).unwrap();
        builder.metadata("title", "Index compatibility").unwrap();
        for (index, html) in documents.iter().enumerate() {
            builder
                .add_content(EpubContent::new(
                    format!("chapter{index}.xhtml"),
                    Cursor::new(format!(
                        "<html xmlns='http://www.w3.org/1999/xhtml'>{html}</html>"
                    )),
                ))
                .unwrap();
        }
        builder
            .generate(std::fs::File::create(&path).unwrap())
            .unwrap();
        let old = EpubBook::open(&path).unwrap();
        let lazy = EpubBook::open_lazy(&path).unwrap();
        assert_eq!(old.chapters.len(), lazy.chapters.len());
        for index in 0..old.chapters.len() {
            assert_eq!(
                body(&old.chapters[index]),
                body(&lazy.chapter(index).unwrap())
            );
            assert_eq!(
                serde_json::to_value(old.render_chapter(index).unwrap()).unwrap(),
                serde_json::to_value(lazy.render_chapter(index).unwrap()).unwrap(),
            );
        }
    }
}
