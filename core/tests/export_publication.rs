use reader_core::{
    epub::{ContentBlock, EpubBook},
    export::{export_book, ExportMode},
    library::{BookConfig, CorrectionRecord},
    txt,
};
use serde_json::json;
fn content(book: &EpubBook) -> String {
    book.chapters
        .iter()
        .flat_map(|c| c.blocks.iter())
        .filter_map(|b| match b {
            ContentBlock::Paragraph { spans, .. } | ContentBlock::Heading { spans, .. } => {
                Some(spans.iter().map(|s| s.text.as_str()).collect::<String>())
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn config(path: &str) -> BookConfig {
    serde_json::from_value(json!({"id":"audit","title":"Export audit","epub_path":path,"last_chapter":0,"last_opened":0,"created_at":0,"updated_at":0})).unwrap()
}
#[test]
fn rebuilt_export_applies_accepted_corrections_and_preserves_all_media() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.txt");
    std::fs::write(&path, "第一章 开始\n原文需要校正。").unwrap();
    let original = txt::convert_txt_to_epub(&path, dir.path(), &Default::default()).unwrap();
    let original_bytes = std::fs::read(&original.epub_path).unwrap();
    let book = EpubBook::open(&original.epub_path).unwrap();
    let block=book.chapters[0].blocks.iter().position(|b|matches!(b,ContentBlock::Paragraph{spans,..} if spans.iter().any(|s|s.text.contains("原文")))).unwrap();
    let mut cfg = config(original.epub_path.to_str().unwrap());
    cfg.corrections.push(CorrectionRecord {
        chapter: 0,
        block_idx: block,
        char_offset: 0,
        original: "原".into(),
        corrected: "新".into(),
        status: "accepted".into(),
    });
    let corrected = dir.path().join("corrected.epub");
    export_book(
        original.epub_path.to_str().unwrap(),
        corrected.to_str().unwrap(),
        &cfg,
        ExportMode::WithCorrections,
    )
    .unwrap();
    let actual = content(&EpubBook::open(&corrected).unwrap());
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/illustrations.epub");
    let fixture_bytes = std::fs::read(&fixture).unwrap();
    let source_book = EpubBook::open(&fixture).unwrap();
    let count_images = |b: &EpubBook, p: &std::path::Path| {
        b.render_blocks(p)
            .unwrap()
            .iter()
            .flat_map(|c| c.iter())
            .filter(|(_, _, b)| matches!(b, ContentBlock::Image { .. }))
            .count()
    };
    let images_before = count_images(&source_book, &fixture);
    let rebuilt = dir.path().join("rebuilt.epub");
    export_book(
        fixture.to_str().unwrap(),
        rebuilt.to_str().unwrap(),
        &config(fixture.to_str().unwrap()),
        ExportMode::WithAnnotations,
    )
    .unwrap();
    let images_after = count_images(&EpubBook::open(&rebuilt).unwrap(), &rebuilt);
    let source_unmodified = std::fs::read(&original.epub_path).unwrap() == original_bytes
        && std::fs::read(&fixture).unwrap() == fixture_bytes;
    assert!(source_unmodified);
    assert!(actual.contains("新文需要校正"), "{actual}");
    assert_eq!(images_before, images_after);
    let report = json!({"source_unmodified":source_unmodified,"diagnostic_execution":"completed","accepted_correction_written":actual.contains("新文需要校正"),"uncorrected_text_remains":actual.contains("原文需要校正"),"source_image_blocks":images_before,"rebuilt_image_blocks":images_after,"validation_scope":"synthetic_core_regression"});
    println!("{report}");
}

fn resources(path: &std::path::Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    (0..zip.len())
        .map(|i| {
            let mut member = zip.by_index(i).unwrap();
            let name = member.name().to_owned();
            let mut bytes = Vec::new();
            member.read_to_end(&mut bytes).unwrap();
            (name, bytes)
        })
        .collect()
}
#[test]
fn full_export_preserves_inline_markup_resources_and_unicode_annotations() {
    use epub_builder::{EpubBuilder, EpubContent, ZipLibrary};
    use reader_core::library::{Bookmark, Highlight, HighlightColor, Note};
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.epub");
    let mut builder = EpubBuilder::new(ZipLibrary::new().unwrap()).unwrap();
    builder.metadata("title", "Export fixture").unwrap();
    builder
        .stylesheet(b"body { color: navy }".as_slice())
        .unwrap();
    builder
        .add_resource("image.png", b"image bytes".as_slice(), "image/png")
        .unwrap();
    let markup = r#"<?xml version="1.0" encoding="UTF-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>Chapter</title><link rel="stylesheet" href="stylesheet.css"/></head><body><h1>Chapter</h1><p id="target">原<em>文</em> 😀 &amp; 继续阅读。</p><table><tr><td>表格内容</td></tr></table><img src="image.png" alt="Illustration"/><script>const x = '原文';</script></body></html>"#;
    builder
        .add_content(EpubContent::new("chapter.xhtml", markup.as_bytes()).title("Chapter"))
        .unwrap();
    builder
        .generate(std::fs::File::create(&source).unwrap())
        .unwrap();
    let book = EpubBook::open(&source).unwrap();
    let block=book.chapters[0].blocks.iter().position(|b|matches!(b,ContentBlock::Paragraph{spans,..} if spans.iter().map(|s|s.text.as_str()).collect::<String>().starts_with("原文"))).unwrap();
    let mut cfg = config(source.to_str().unwrap());
    cfg.corrections.push(CorrectionRecord {
        chapter: 0,
        block_idx: block,
        char_offset: 0,
        original: "原文".into(),
        corrected: "新文".into(),
        status: "accepted".into(),
    });
    cfg.highlights.push(Highlight {
        id: "h1".into(),
        chapter: 0,
        start_block: block,
        start_offset: 0,
        end_block: block,
        end_offset: 4,
        color: HighlightColor::Blue,
        created_at: 1,
    });
    cfg.notes.push(Note {
        highlight_id: "h1".into(),
        content: "笔记 <script>evil</script> & 😀".into(),
        created_at: 1,
        updated_at: 1,
    });
    cfg.bookmarks.push(Bookmark {
        chapter: 0,
        block,
        created_at: 1,
    });
    let before = resources(&source);
    let destination = dir.path().join("full.epub");
    export_book(
        source.to_str().unwrap(),
        destination.to_str().unwrap(),
        &cfg,
        ExportMode::Full,
    )
    .unwrap();
    let after = resources(&destination);
    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        after.keys().collect::<Vec<_>>()
    );
    for (name, bytes) in &before {
        if !name.ends_with("chapter.xhtml") {
            assert_eq!(Some(bytes), after.get(name), "resource {name}");
        }
    }
    let html = after
        .iter()
        .find(|(name, _)| name.ends_with("chapter.xhtml"))
        .map(|(_, v)| String::from_utf8(v.clone()).unwrap())
        .unwrap();
    assert!(html.contains("<em>"));
    assert!(html.contains("<table>"));
    assert!(html.contains("<script>const x = '原文';</script>"));
    assert!(html.contains("reader-bookmark-0"));
    assert!(html.contains("&lt;script&gt;evil&lt;/script&gt;"));
    assert!(html.contains("background-color:#cce5ff"));
    let result = content(&EpubBook::open(&destination).unwrap());
    assert!(result.contains("新文"), "{result}");
    assert!(result.contains("😀"));
    assert_eq!(before, resources(&source));
}
#[test]
fn stale_corrections_never_create_partial_output_or_overwrite_sources() {
    let dir = tempfile::tempdir().unwrap();
    let text = dir.path().join("book.txt");
    std::fs::write(&text, "第一章\n原文保留。").unwrap();
    let epub = txt::convert_txt_to_epub(&text, dir.path(), &Default::default()).unwrap();
    let before = std::fs::read(&epub.epub_path).unwrap();
    let mut cfg = config(epub.epub_path.to_str().unwrap());
    cfg.corrections.push(CorrectionRecord {
        chapter: 0,
        block_idx: 999,
        char_offset: 0,
        original: "原".into(),
        corrected: "新".into(),
        status: "accepted".into(),
    });
    let destination = dir.path().join("bad.epub");
    assert!(export_book(
        epub.epub_path.to_str().unwrap(),
        destination.to_str().unwrap(),
        &cfg,
        ExportMode::Full
    )
    .is_err());
    assert!(!destination.exists());
    assert!(export_book(
        epub.epub_path.to_str().unwrap(),
        epub.epub_path.to_str().unwrap(),
        &cfg,
        ExportMode::Original
    )
    .is_err());
    assert_eq!(before, std::fs::read(&epub.epub_path).unwrap());
    assert!(!std::fs::read_dir(dir.path()).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".reader-export")));
}
