//! Logic for exporting books, annotations, and reading data.
use crate::epub::{ContentBlock, CorrectionStatus, EpubBook, InlineStyle};
use crate::library::BookConfig;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ExportMode {
    Original,
    WithCorrections,
    WithAnnotations,
    Full,
}

pub fn export_book(
    source_epub: &str,
    output_path: &str,
    config: &BookConfig,
    mode: ExportMode,
) -> Result<(), String> {
    match mode {
        ExportMode::Original => {
            std::fs::copy(source_epub, output_path).map_err(|e| format!("复制文件失败: {e}"))?;
            Ok(())
        }
        _ => {
            let book = EpubBook::open(source_epub)?;
            build_modified_epub(&book, output_path, config, &mode)
        }
    }
}

fn build_modified_epub(
    book: &EpubBook,
    output_path: &str,
    config: &BookConfig,
    mode: &ExportMode,
) -> Result<(), String> {
    use epub_builder::{EpubBuilder, EpubContent, ZipLibrary};

    let file = std::fs::File::create(output_path).map_err(|e| format!("创建文件失败: {e}"))?;
    let mut builder = EpubBuilder::new(ZipLibrary::new().map_err(|e| format!("{e}"))?)
        .map_err(|e| format!("{e}"))?;

    builder
        .metadata("title", &config.title)
        .map_err(|e| format!("{e}"))?;
    builder
        .metadata("generator", "RustEpubReader")
        .map_err(|e| format!("{e}"))?;

    let css = generate_export_css(mode);
    builder
        .stylesheet(css.as_bytes())
        .map_err(|e| format!("{e}"))?;

    for (ch_idx, chapter) in book.chapters.iter().enumerate() {
        let xhtml = build_chapter_xhtml(chapter, ch_idx, config, mode);
        let filename = format!("chapter_{ch_idx:04}.xhtml");
        let content = EpubContent::new(&filename, xhtml.as_bytes()).title(&chapter.title);
        builder.add_content(content).map_err(|e| format!("{e}"))?;
    }

    builder
        .generate(file)
        .map_err(|e| format!("生成 EPUB 失败: {e}"))?;
    Ok(())
}

fn generate_export_css(mode: &ExportMode) -> String {
    let mut css = String::from(
        "body { font-family: serif; line-height: 1.6; margin: 1em; }\n\
         h1, h2, h3 { margin: 1em 0 0.5em; }\n\
         p { margin: 0.5em 0; text-indent: 2em; }\n",
    );

    if matches!(mode, ExportMode::WithAnnotations | ExportMode::Full) {
        css.push_str(
            ".highlight-yellow { background-color: #fff3cd; }\n\
             .highlight-green { background-color: #d4edda; }\n\
             .highlight-blue { background-color: #cce5ff; }\n\
             .highlight-pink { background-color: #f8d7da; }\n\
             .reader-note { margin: 0.5em 0 0.5em 1em; padding: 0.5em; \
             border-left: 3px solid #6c757d; font-size: 0.85em; color: #6c757d; }\n\
             .note-label { font-weight: bold; margin-bottom: 0.2em; }\n",
        );
    }

    if matches!(mode, ExportMode::WithCorrections | ExportMode::Full) {
        css.push_str(".corrected { background-color: #d4edda; text-decoration: underline; }\n");
    }

    css
}

fn build_chapter_xhtml(
    chapter: &crate::epub::Chapter,
    ch_idx: usize,
    config: &BookConfig,
    mode: &ExportMode,
) -> String {
    let mut html = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE html>\n\
         <html xmlns=\"http://www.w3.org/1999/xhtml\">\n\
         <head><title></title>\
         <link rel=\"stylesheet\" type=\"text/css\" href=\"stylesheet.css\"/>\
         </head>\n<body>\n",
    );

    // Bookmark anchors
    for (i, bm) in config.bookmarks.iter().enumerate() {
        if bm.chapter == ch_idx {
            html.push_str(&format!("<a id=\"bookmark-{i}\"/>\n"));
        }
    }

    for (blk_idx, block) in chapter.blocks.iter().enumerate() {
        let highlight = if matches!(mode, ExportMode::WithAnnotations | ExportMode::Full) {
            config
                .highlights
                .iter()
                .find(|h| h.chapter == ch_idx && h.start_block <= blk_idx && h.end_block >= blk_idx)
        } else {
            None
        };

        match block {
            ContentBlock::Heading { level, spans, anchor_id } => {
                let tag = format!("h{}", level.min(&6));
                if let Some(id) = anchor_id {
                    html.push_str(&format!(
                        "<{tag} id=\"{}\">",
                        crate::escape_html(id)
                    ));
                } else {
                    html.push_str(&format!("<{tag}>"));
                }
                for span in spans {
                    html.push_str(&crate::escape_html(&span.text));
                }
                html.push_str(&format!("</{tag}>\n"));
            }
            ContentBlock::Paragraph { spans, anchor_id } => {
                if let Some(id) = anchor_id {
                    html.push_str(&format!(
                        "<p id=\"{}\">",
                        crate::escape_html(id)
                    ));
                } else {
                    html.push_str("<p>");
                }
                if let Some(hl) = highlight {
                    html.push_str(&format!("<span class=\"{}\">", hl.color.css_class()));
                }
                for span in spans {
                    if matches!(mode, ExportMode::WithCorrections | ExportMode::Full) {
                        if let Some(corr) = &span.correction {
                            if corr.status == CorrectionStatus::Accepted {
                                html.push_str(&format!(
                                    "<span class=\"corrected\">{}</span>",
                                    crate::escape_html(&corr.corrected)
                                ));
                                continue;
                            }
                        }
                    }
                    render_span_xhtml(&mut html, span);
                }
                if let Some(hl) = highlight {
                    html.push_str("</span>");
                    // Append notes for this highlight
                    for note in &config.notes {
                        if note.highlight_id == hl.id {
                            html.push_str(&format!(
                                "<aside class=\"reader-note\">\
                                 <p class=\"note-label\">\u{1f4dd}</p>\
                                 <p>{}</p></aside>",
                                crate::escape_html(&note.content)
                            ));
                        }
                    }
                }
                html.push_str("</p>\n");
            }
            ContentBlock::Image { alt, .. } => {
                let alt_text = alt.as_deref().unwrap_or("image");
                html.push_str(&format!("<p>[{}]</p>\n", crate::escape_html(alt_text)));
            }
            ContentBlock::Separator => {
                html.push_str("<hr/>\n");
            }
            ContentBlock::BlankLine => {
                html.push_str("<br/>\n");
            }
        }
    }

    html.push_str("</body>\n</html>");
    html
}

fn render_span_xhtml(html: &mut String, span: &crate::epub::TextSpan) {
    let text = crate::escape_html(&span.text);
    match span.style {
        InlineStyle::Bold => html.push_str(&format!("<strong>{text}</strong>")),
        InlineStyle::Italic => html.push_str(&format!("<em>{text}</em>")),
        InlineStyle::BoldItalic => html.push_str(&format!("<strong><em>{text}</em></strong>")),
        InlineStyle::Normal => html.push_str(&text),
    }
}
