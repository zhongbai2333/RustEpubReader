//! Logic for exporting books, annotations, and reading data.
use crate::epub::{ContentBlock, CorrectionStatus, EpubBook, InlineStyle};
use crate::library::BookConfig;

/// A portable display-name filename; storage UUIDs never appear in exports.
pub fn book_filename(title: &str) -> String {
    let mut name = String::new();
    for c in title.trim().chars() {
        let c = if c.is_control() || "<>:\"/\\|?*".contains(c) {
            '_'
        } else {
            c
        };
        if name.len() + c.len_utf8() > 180 {
            break;
        }
        name.push(c);
    }
    let name = name.trim_matches(['.', ' ']);
    let stem = if name.is_empty() { "Book" } else { name };
    let reserved = stem.split('.').next().unwrap_or("").to_ascii_uppercase();
    let stem = if matches!(reserved.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (reserved.len() == 4
            && (reserved.starts_with("COM") || reserved.starts_with("LPT"))
            && matches!(reserved.as_bytes()[3], b'1'..=b'9'))
    {
        format!("_{stem}")
    } else {
        stem.to_owned()
    };
    format!("{stem}.epub")
}

/// Copy selected originals without overwriting an existing export or source.
pub fn export_originals(
    books: &[crate::library::BookEntry],
    directory: &std::path::Path,
) -> Result<Vec<String>, String> {
    use std::io::ErrorKind;
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let mut paths = Vec::new();
    for book in books {
        let filename = book_filename(&book.title);
        let stem = filename.strip_suffix(".epub").unwrap();
        let mut source = std::fs::File::open(&book.path).map_err(|e| e.to_string())?;
        let mut suffix = 1usize;
        loop {
            let path = directory.join(if suffix == 1 {
                filename.clone()
            } else {
                format!("{stem} ({suffix}).epub")
            });
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut output) => {
                    if let Err(error) =
                        std::io::copy(&mut source, &mut output).and_then(|_| output.sync_all())
                    {
                        let _ = std::fs::remove_file(&path);
                        return Err(error.to_string());
                    }
                    paths.push(path.to_string_lossy().into_owned());
                    break;
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => suffix += 1,
                Err(error) => return Err(error.to_string()),
            }
        }
    }
    Ok(paths)
}

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
            let book = EpubBook::open_lazy(source_epub)?;
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

    for ch_idx in 0..book.chapters.len() {
        let chapter = book.chapter(ch_idx)?;
        let xhtml = build_chapter_xhtml(&chapter, ch_idx, config, mode);
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
            ContentBlock::Heading { level, spans, .. } => {
                let tag = format!("h{}", level.min(&6));
                html.push_str(&format!("<{tag}>"));
                for span in spans {
                    html.push_str(&crate::escape_html(&span.text));
                }
                html.push_str(&format!("</{tag}>\n"));
            }
            ContentBlock::Paragraph { spans, .. } => {
                html.push_str("<p>");
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

#[cfg(test)]
mod filename_tests {
    use super::*;
    #[test]
    fn portable_unicode_names_have_bounded_bytes_and_no_path_components() {
        assert_eq!(book_filename("中文 Book 📚"), "中文 Book 📚.epub");
        assert_eq!(book_filename("../../A:B?"), "_.._A_B_.epub");
        assert_eq!(book_filename("CON"), "_CON.epub");
        assert_eq!(book_filename("LPT9.txt"), "_LPT9.txt.epub");
        assert_eq!(book_filename("...  "), "Book.epub");
        assert!(book_filename(&"书".repeat(200)).len() <= 185);
    }
    #[test]
    fn duplicate_exports_keep_names_and_never_overwrite_sources_or_previous_files() {
        let dir = std::env::temp_dir().join(format!("reader_exports_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("中文 Book.epub");
        std::fs::write(&source, b"original epub bytes").unwrap();
        let entry = crate::library::BookEntry {
            id: uuid::Uuid::new_v4().to_string(),
            title: "中文 Book".into(),
            path: source.to_string_lossy().into(),
            last_chapter: 0,
            last_block: 0,
            last_char_offset: 0,
            last_chapter_title: None,
            last_opened: 0,
        };
        let paths = export_originals(&[entry.clone(), entry], &dir).unwrap();
        assert_eq!(
            std::path::Path::new(&paths[0]).file_name().unwrap(),
            "中文 Book (2).epub"
        );
        assert_eq!(
            std::path::Path::new(&paths[1]).file_name().unwrap(),
            "中文 Book (3).epub"
        );
        for path in paths {
            assert_eq!(std::fs::read(path).unwrap(), b"original epub bytes");
        }
        assert_eq!(std::fs::read(source).unwrap(), b"original epub bytes");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
