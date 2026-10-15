use reader_core::epub::native::{CssPolicy, NativeDocument, NativeEvent};
use reader_core::epub::{ContentBlock, EpubBook};
fn fixture() -> EpubBook {
    EpubBook::open(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/original-layout.epub"
    ))
    .unwrap()
}
fn paragraph(text: &str) -> ContentBlock {
    ContentBlock::Paragraph {
        spans: vec![reader_core::epub::TextSpan {
            text: text.into(),
            style: reader_core::epub::InlineStyle::Normal,
            link_url: None,
            correction: None,
        }],
        anchor_id: Some("p".into()),
        native_style: Default::default(),
    }
}
#[test]
fn styles_and_scripts_share_stable_native_blocks() {
    let book = fixture();
    let ch = &book.chapters[0];
    let styled=ch.blocks.iter().position(|b|matches!(b,ContentBlock::Paragraph{anchor_id,..} if anchor_id.as_deref()==Some("styled"))).unwrap();
    let revealed=ch.blocks.iter().position(|b|matches!(b,ContentBlock::Paragraph{anchor_id,..} if anchor_id.as_deref()==Some("revealed"))).unwrap();
    let originals = serde_json::to_string(&ch.blocks).unwrap();
    for policy in [CssPolicy::None, CssPolicy::Mixed, CssPolicy::Publisher] {
        let initial = ch.native_document.present(policy, true, &[]);
        assert!(initial.errors.is_empty(), "{:?}", initial.errors);
        assert_eq!(initial.styles.len(), ch.blocks.len());
        assert!(initial.styles[revealed].hidden);
        assert_eq!(
            initial.styles[styled].align.as_deref(),
            if policy == CssPolicy::None {
                None
            } else {
                Some("right")
            }
        );
        assert_eq!(
            initial.styles[styled].line,
            if policy == CssPolicy::Publisher {
                Some(1.9)
            } else {
                None
            }
        );
        let action = initial.styles.iter().find_map(|s| s.action).unwrap();
        let events = [NativeEvent {
            node: action,
            kind: "click".into(),
        }];
        let changed = ch.native_document.present(policy, true, &events);
        assert!(changed.errors.is_empty(), "{:?}", changed.errors);
        assert!(
            !changed.styles[revealed].hidden,
            "action={action}, changed={:?}",
            changed
        );
        assert!(changed
            .styles
            .iter()
            .any(|s| s.text.as_deref() == Some("收起内容")));
        let disabled = ch.native_document.present(policy, false, &events);
        assert!(disabled.styles[revealed].hidden);
        assert!(disabled
            .styles
            .iter()
            .all(|s| s.action.is_none() && s.text.is_none()));
    }
    assert_eq!(serde_json::to_string(&ch.blocks).unwrap(), originals);
}
#[test]
fn cascade_imports_inline_important_and_class_mutation() {
    let html = r#"<html><head><link rel="stylesheet" href="../book.css"/><script>document.addEventListener('DOMContentLoaded',()=>{document.getElementById('p').addEventListener('click',function(){this.classList.add('active')})})</script></head><body><section><p id="p" style="text-align:left">正文</p></section></body></html>"#;
    let doc = NativeDocument::parse(html, "Text/book.xhtml", &[paragraph("正文")], |p| {
        match p{"book.css"=>Some("@import 'base.css'; #p{text-align:center !important;font-size:2em} .active{display:none}".into()),"base.css"=>Some("section{font-style:italic} p{line-height:2}".into()),_=>None}
    });
    let base = doc.present(CssPolicy::Publisher, true, &[]);
    assert!(base.errors.is_empty());
    let s = &base.styles[0];
    assert_eq!(s.align.as_deref(), Some("center"));
    assert_eq!(s.size_px, Some(32.0));
    assert_eq!(s.line, Some(2.0));
    assert!(s.italic);
    let changed = doc.present(
        CssPolicy::Mixed,
        true,
        &[NativeEvent {
            node: s.action.unwrap(),
            kind: "click".into(),
        }],
    );
    assert!(changed.errors.is_empty());
    assert!(changed.styles[0].hidden);
}
#[test]
fn untrusted_scripts_are_bounded_and_have_no_host_io() {
    for script in ["while(true){}","new Array(10000000).fill('large')","fetch('https://example.org')","document.body.innerHTML = '<iframe src=\\\"file:///etc/passwd\\\"></iframe>'; throw Error('blocked')"] {
  let doc=NativeDocument::parse(&format!("<html><head><script>{script}</script></head><body><p id='p'>正文</p></body></html>"),"book.xhtml",&[paragraph("正文")],|_|None);
  let now=std::time::Instant::now();let value=doc.present(CssPolicy::Mixed,true,&[]);assert!(!value.errors.is_empty(),"{script}");assert!(now.elapsed()<std::time::Duration::from_secs(2));assert_eq!(value.styles.len(),1);
  assert!(doc.present(CssPolicy::Mixed,false,&[]).errors.is_empty());
 }
}
#[test]
fn external_and_traversing_resources_are_not_loaded() {
    let mut reads = Vec::new();
    NativeDocument::parse("<link rel='stylesheet' href='https://evil/book.css'><script src='../../escape.js'></script>","book.xhtml",&[],|p|{reads.push(p.to_owned());None});
    assert!(reads.is_empty());
}
