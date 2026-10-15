use reader_core::epub::{ContentBlock, EpubBook};
fn fixture() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/illustrations.epub")
}
#[test]
fn illustrations_use_native_image_bytes_without_moving_canonical_text() {
    let book = EpubBook::open(fixture()).unwrap();
    let rendered = book.render_blocks(fixture()).unwrap();
    assert_eq!(book.chapters.len(), 9);
    for (chapter, rendered_chapter) in rendered.iter().enumerate().take(8) {
        let images: Vec<_> = rendered_chapter
            .iter()
            .filter_map(|(_, _, block)| match block {
                ContentBlock::Image { data, .. } => Some(data),
                _ => None,
            })
            .collect();
        assert_eq!(images.len(), 1, "chapter {chapter}");
        let data = images[0];
        assert!(
            data.starts_with(b"\x89PNG")
                || data.starts_with(b"\xff\xd8")
                || data.starts_with(b"RIFF")
        );
        let canonical_text: Vec<_> = book.chapters[chapter]
            .blocks
            .iter()
            .filter_map(text)
            .collect();
        let rendered_text: Vec<_> = rendered_chapter
            .iter()
            .filter_map(|(index, offset, block)| {
                text(block).inspect(|value| {
                    assert_eq!(*offset, 0);
                    assert_eq!(
                        text(&book.chapters[chapter].blocks[*index]),
                        Some(value.clone())
                    );
                })
            })
            .collect();
        assert_eq!(canonical_text, rendered_text);
        let canonical_images = book.chapters[chapter]
            .blocks
            .iter()
            .filter(|block| matches!(block, ContentBlock::Image { .. }))
            .count();
        assert_eq!(
            canonical_images + book.supplemental_images.get(&chapter).map_or(0, Vec::len),
            1
        );
    }
    assert!(matches!(
        &book.chapters[1].blocks[1],
        ContentBlock::Image { .. }
    ));
    assert!(!book.supplemental_images.contains_key(&1));
}
#[test]
fn javascript_is_not_executed_or_exposed_as_reading_text_and_static_fallback_survives() {
    let book = EpubBook::open(fixture()).unwrap();
    let text = book.chapters[8]
        .blocks
        .iter()
        .filter_map(text)
        .collect::<String>();
    assert!(text.contains("静态回退内容"));
    assert!(text.contains("脚本按钮"));
    assert!(!text.contains("SCRIPT_EXECUTED_SENTINEL"));
    assert!(!text.contains("textContent"));
    let links: Vec<_> = book.chapters[8]
        .blocks
        .iter()
        .flat_map(|block| match block {
            ContentBlock::Paragraph { spans, .. } | ContentBlock::Heading { spans, .. } => spans
                .iter()
                .filter_map(|s| s.link_url.as_deref())
                .collect::<Vec<_>>(),
            _ => vec![],
        })
        .collect();
    assert!(links.contains(&"ch0.xhtml#png-end"));
    assert!(!links.iter().any(|link| link.starts_with("javascript:")));
}
fn text(block: &ContentBlock) -> Option<String> {
    match block {
        ContentBlock::Paragraph { spans, .. } | ContentBlock::Heading { spans, .. } => {
            Some(spans.iter().map(|s| s.text.as_str()).collect())
        }
        _ => None,
    }
}

#[test]
fn image_only_spine_items_are_visible_without_renumbering_existing_text_chapters() {
    use epub_builder::{EpubBuilder, EpubContent, ZipLibrary};
    use std::io::Cursor;
    let source = EpubBook::open(fixture()).unwrap();
    let png = &source.supplemental_images[&0][0].data;
    for only_images in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("image-pages.epub");
        let mut builder = EpubBuilder::new(ZipLibrary::new().unwrap()).unwrap();
        builder.metadata("title", "Image pages").unwrap();
        builder
            .add_resource("image.png", Cursor::new(png.as_slice()), "image/png")
            .unwrap();
        let mut pages = vec!["<p><img src='image.png' alt='intro'/></p>"];
        if !only_images {
            pages.push("<h1>One</h1><p>first paragraph</p>");
        }
        pages.push("<p><img src='image.png' alt='middle'/></p>");
        if !only_images {
            pages.push("<h1>Two</h1><p>second paragraph</p>");
        }
        pages.push("<p><img src='image.png' alt='end'/></p>");
        for (i, body) in pages.iter().enumerate() {
            let html = format!("<html xmlns='http://www.w3.org/1999/xhtml'><head><title>Page {i}</title></head><body>{body}</body></html>");
            builder
                .add_content(EpubContent::new(format!("ch{i}.xhtml"), Cursor::new(html)))
                .unwrap();
        }
        builder
            .generate(std::fs::File::create(&path).unwrap())
            .unwrap();
        let book = EpubBook::open(&path).unwrap();
        let rendered = book.render_blocks(&path).unwrap();
        assert_eq!(
            rendered
                .iter()
                .flatten()
                .filter(|(_, _, block)| matches!(block, ContentBlock::Image { .. }))
                .count(),
            3
        );
        if only_images {
            assert_eq!(book.chapters.len(), 1);
            assert_eq!(book.chapters[0].blocks.len(), 3);
        } else {
            assert_eq!(book.chapters.len(), 2);
            assert_eq!(book.chapters[0].blocks.len(), 2);
            assert_eq!(book.chapters[1].blocks.len(), 2);
            assert_eq!(
                book.chapters[0].source_href.as_deref(),
                Some("/OEBPS/ch1.xhtml")
            );
            assert_eq!(
                book.chapters[1].source_href.as_deref(),
                Some("/OEBPS/ch3.xhtml")
            );
            assert!(!book.supplemental_images[&0][0].after);
            assert!(book.supplemental_images[&1].last().unwrap().after);
        }
    }
}
