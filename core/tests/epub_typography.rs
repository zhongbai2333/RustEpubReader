use reader_core::epub::native::{CssPolicy, Dimension, NativeDocument};
use reader_core::epub::{ContentBlock, EpubBook};
fn fixture() -> EpubBook {
    EpubBook::open(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/native-typography.epub"
    ))
    .unwrap()
}
fn text(block: &ContentBlock) -> String {
    match block {
        ContentBlock::Paragraph { spans, .. } | ContentBlock::Heading { spans, .. } => {
            spans.iter().map(|s| s.text.as_str()).collect()
        }
        _ => String::new(),
    }
}
#[test]
fn native_metadata_preserves_source_and_supports_inline_ruby_structures() {
    let book = fixture();
    let ch = &book.chapters[0];
    let original = serde_json::to_string(&ch.blocks).unwrap();
    for policy in [CssPolicy::None, CssPolicy::Mixed, CssPolicy::Publisher] {
        let view = ch.native_document.present(policy, false, &[]);
        assert!(view.errors.is_empty());
        assert_eq!(view.styles.len(), ch.blocks.len());
        let inline = ch
            .blocks
            .iter()
            .position(|b| text(b).contains("文字 😀 中文"))
            .unwrap();
        let ruby = ch
            .blocks
            .iter()
            .position(|b| text(b).starts_with("注音"))
            .unwrap();
        let style = &view.styles[inline];
        let chars: Vec<_> = text(&ch.blocks[inline]).chars().collect();
        let big = style
            .inline
            .iter()
            .find(|r| chars[r.start..r.end].iter().collect::<String>() == "大字斜体")
            .unwrap();
        if policy != CssPolicy::None {
            assert_eq!(big.size, Some(1.5));
            assert_eq!(big.bold, Some(true));
            assert!(style.padding_left.is_some());
        }
        assert!(style.inline.iter().any(|r| r.baseline > 0.));
        assert!(style.inline.iter().any(|r| r.baseline < 0.));
        assert!(style
            .inline
            .iter()
            .any(|r| r.note.as_deref() == Some("notes.xhtml#n%E4%B8%80")));
        let ruby_style = &view.styles[ruby];
        assert_eq!(ruby_style.ruby.len(), 3);
        assert!(ruby_style.inline.iter().any(|r| r.hidden));
        let source: Vec<_> = text(&ch.blocks[ruby]).chars().collect();
        assert_eq!(
            ruby_style
                .ruby
                .iter()
                .map(|r| source[r.start..r.end].iter().collect::<String>())
                .collect::<Vec<_>>(),
            ["漢", "字", "中文"]
        );
        assert!(view
            .styles
            .iter()
            .any(|s| s.quote && s.inset_left.is_some()));
        assert!(view
            .styles
            .iter()
            .any(|s| s.list.iter().any(|r| r.depth == 2)));
        let tables: Vec<_> = view
            .styles
            .iter()
            .filter_map(|s| s.table.as_ref())
            .collect();
        assert_eq!(tables.len(), 3);
        assert!(tables.iter().all(|t| t.columns == 3));
        assert_eq!(tables[2].cells[0].span, 2);
        assert_eq!(
            tables[1].cells[2].align.as_deref(),
            if policy == CssPolicy::None {
                None
            } else {
                Some("right")
            }
        );
    }
    assert_eq!(serde_json::to_string(&ch.blocks).unwrap(), original);
    let note = book.native_note(0, "notes.xhtml#n%E4%B8%80").unwrap();
    assert!(note.text.contains("这是跨章节的注释原文"));
    assert!(book.native_note(0, "https://evil/note#n一").is_none());
    assert!(book.native_note(0, "../../escape#n一").is_none());
}
#[test]
fn css_units_cascade_and_image_bounds() {
    for (value, expected) in [
        ("12pt", 16.),
        ("1rem", 20.),
        ("50%", 100.),
        ("calc(1em + 4px)", 22.),
    ] {
        assert!(
            (Dimension::parse(value, 20.).unwrap().resolve(18., 200.) - expected).abs() < 0.001,
            "{value}"
        );
    }
    assert!(Dimension::parse("calc(1em * 2)", 16.).is_none());
    assert!(Dimension::parse("1banana", 16.).is_none());
    let b = ContentBlock::Paragraph {
        native_style: Default::default(),
        anchor_id: Some("p".into()),
        spans: vec![reader_core::epub::TextSpan {
            text: "正文".into(),
            style: reader_core::epub::InlineStyle::Normal,
            link_url: None,
            correction: None,
        }],
    };
    let doc = NativeDocument::parse(
        r#"<html><head><style>html{font-size:20px} body{font-size:1.2em;--line:2} #p{font-size:1rem;color:#123456;margin:2px calc(0.1em + 2px);line-height:var(--line)} body p{font-size:50px;color:#ffffff} p{color:#ff0000 !important}</style></head><body><p id='p' style='color:#000000'>正文</p></body></html>"#,
        "book.xhtml",
        &[b],
        |_| None,
    );
    let view = doc.present(CssPolicy::Publisher, false, &[]);
    let style = &view.styles[0];
    assert_eq!(style.size_px, Some(20.));
    assert_eq!(style.line, Some(2.));
    assert_eq!(style.color.as_deref(), Some("#ff0000"));
    assert_eq!(
        style
            .inset_left
            .as_ref()
            .unwrap()
            .resolve(style.size_at(18.), 300.),
        4.
    );
    let book = fixture();
    let ch = &book.chapters[0];
    let view = ch.native_document.present(CssPolicy::Mixed, false, &[]);
    let image = ch
        .blocks
        .iter()
        .position(|b| matches!(b, ContentBlock::Image { .. }))
        .unwrap();
    let style = &view.styles[image];
    assert_eq!(style.image_width.as_ref().unwrap().percent, 0.5);
    let (w, h) = style.image_size(600., 300., 18., 300., 600.);
    assert_eq!((w, h), (150., 75.));
    let (w, h) = style.image_size(600., 300., 18., 300., f32::INFINITY);
    assert!((w - 150.).abs() < 0.001 && (h - 75.).abs() < 0.001);
    let (w, h) = style.image_size(600., 300., 18., 300., 40.);
    assert!((w - 80.).abs() < 0.001 && (h - 40.).abs() < 0.001);
}

#[test]
fn absolute_and_percentage_line_heights_inherit_computed_lengths() {
    for (height, expected) in [("24px", 1.0), ("150%", 1.0), ("1.5", 1.5)] {
        let html=format!("<html><head><style>body{{font-size:16px;line-height:{height}}} p{{font-size:24px}}</style></head><body><p>正文</p></body></html>");
        let block = ContentBlock::Paragraph {
            spans: vec![reader_core::epub::TextSpan {
                text: "正文".into(),
                style: reader_core::epub::InlineStyle::Normal,
                link_url: None,
                correction: None,
            }],
            anchor_id: None,
            native_style: Default::default(),
        };
        let doc = NativeDocument::parse(&html, "test.xhtml", &[block], |_| None);
        let result = doc.present(CssPolicy::Publisher, false, &[]);
        assert_eq!(result.styles[0].size_px, Some(24.));
        assert_eq!(result.styles[0].line, Some(expected), "{height}");
    }
}

#[test]
fn root_text_decorations_variables_and_explicit_font_inheritance() {
    let source = "普通强调代码";
    let block = ContentBlock::Paragraph {
        spans: vec![reader_core::epub::TextSpan {
            text: source.into(),
            style: reader_core::epub::InlineStyle::Normal,
            link_url: None,
            correction: None,
        }],
        anchor_id: Some("p".into()),
        native_style: Default::default(),
    };
    let html = r#"<html><head><style>body{--ink:#224466;font-weight:bold;font-style:italic;font-family:'Unavailable Font',serif;color:var(--ink)} p{text-decoration:underline;font-variant:small-caps;font-weight:inherit} strong{font-weight:bolder} code{font-style:normal}</style></head><body><p id='p'>普通<strong>强调</strong><code>代码</code></p></body></html>"#;
    let doc = NativeDocument::parse(html, "test.xhtml", &[block], |_| None);
    let result = doc.present(CssPolicy::Publisher, false, &[]);
    let style = &result.styles[0];
    assert_eq!(style.color.as_deref(), Some("#224466"));
    assert_eq!(style.family.as_deref(), Some("serif"));
    assert!(style.inline[0].underline && style.inline[0].small_caps);
    assert_eq!(style.inline[0].bold, Some(true));
    assert!(style
        .inline
        .iter()
        .any(|s| s.start == 2 && s.end == 4 && s.bold == Some(true)));
    assert!(style.inline.iter().any(|s| s.start == 4
        && s.end == 6
        && s.family.as_deref() == Some("monospace")
        && s.italic == Some(false)));
    let bare = doc.present(CssPolicy::None, false, &[]);
    assert!(!bare.styles[0]
        .inline
        .iter()
        .any(|s| s.small_caps || s.underline));
}
