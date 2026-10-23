use reader_core::txt::{
    self,
    split::{split_chapters, split_chapters_excluding},
    ConvertOptions, SplitConfig,
};
fn body() -> String {
    "这是完整的正文段落，不能因为章节样式变化而漏掉任何内容。".repeat(8)
}
#[test]
fn mixed_heading_styles_are_recognized_within_one_book() {
    let titles = [
        "第一章 初见",
        "第2章 再会",
        "3. 风起",
        "4- 夜行",
        "5-1 山间",
        "5-2 相见",
        "我的世界",
        "第六章 归来",
    ];
    let text = titles
        .iter()
        .map(|t| format!("{t}\n\n{}\n\n", body()))
        .collect::<String>();
    let chapters = split_chapters(&text, &SplitConfig::default());
    assert_eq!(
        chapters
            .iter()
            .map(|c| c.title.as_str())
            .collect::<Vec<_>>(),
        titles
    );
    assert!(chapters.iter().all(|c| c.content == body()));
}
#[test]
fn short_preamble_and_short_explicit_chapters_are_not_lost_or_merged() {
    let text = "题记：致读者\n第一章 开始\n一。\n第2章 继续\n二。\n第三章 结束\n三。";
    let c = split_chapters(text, &SplitConfig::default());
    assert_eq!(c.len(), 4);
    assert_eq!(c[0].content, "题记：致读者");
    assert_eq!(
        c.iter()
            .skip(1)
            .map(|x| x.content.as_str())
            .collect::<Vec<_>>(),
        ["一。", "二。", "三。"]
    );
}
#[test]
fn prose_references_and_shopping_lists_are_not_chapters() {
    let text = format!(
        "第一章 清单\n第一章中，提到过这些物品。\n1. 苹果\n2. 香蕉\n3. 水\n{}\n第二章 结束\n{}",
        body(),
        body()
    );
    let c = split_chapters(&text, &SplitConfig::default());
    assert_eq!(c.len(), 2);
    assert!(c[0].content.contains("3. 水"));
}
#[test]
fn unnumbered_titles_are_optional_and_not_filtered_by_pronouns() {
    let text = format!("我的世界\n\n{}\n\n他的归途\n\n{}", body(), body());
    assert_eq!(split_chapters(&text, &SplitConfig::default()).len(), 2);
    assert!(split_chapters(
        &text,
        &SplitConfig {
            use_heuristic: false,
            ..Default::default()
        }
    )
    .iter()
    .all(|c| !c.is_heading));
}
#[test]
fn numbered_chapters_work_without_ten_lines_per_chapter() {
    let text = format!(
        "1. 开始\n{}\n2- 继续\n{}\n3-1 新篇\n{}",
        body(),
        body(),
        body()
    );
    assert_eq!(split_chapters(&text, &SplitConfig::default()).len(), 3);
}
#[test]
fn a_printed_toc_is_retained_as_text_without_duplicate_empty_chapters() {
    let text=format!("目录\n第一章 初见\n第二章 再会\n第三章 归来\n\n第一章 初见\n{}\n第二章 再会\n{}\n第三章 归来\n{}",body(),body(),body());
    let c = split_chapters(&text, &SplitConfig::default());
    assert_eq!(c.iter().filter(|c| c.is_heading).count(), 3);
    assert!(c[0]
        .content
        .contains("目录\n第一章 初见\n第二章 再会\n第三章 归来"));
}
#[test]
fn excluding_a_boundary_does_not_remove_its_heading_or_body() {
    let text = "第一章 开始\n甲。\n第二章 中途\n乙。\n第三章 结束\n丙。";
    let c = split_chapters_excluding(text, &SplitConfig::default(), &[2]);
    assert_eq!(c.len(), 2);
    assert_eq!(c[0].content, "甲。\n第二章 中途\n乙。");
}
#[test]
fn unstructured_single_line_text_is_bounded_without_losing_unicode() {
    let text = "中文😀e\u{301}".repeat(6000);
    let c = split_chapters(&text, &SplitConfig::default());
    assert!(c.len() > 1);
    assert!(c
        .iter()
        .all(|c| !c.is_heading && c.content.chars().count() <= 8000));
    assert_eq!(
        c.iter().map(|c| c.content.as_str()).collect::<String>(),
        text
    );
}
#[test]
fn invalid_or_nonmatching_custom_rules_report_errors_without_writing_outputs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.txt");
    std::fs::write(&path, "第一章 开始\n正文。").unwrap();
    for regex in ["[", "^NEVER$"] {
        let options = ConvertOptions {
            custom_regex: Some(regex.into()),
            ..Default::default()
        };
        assert!(txt::convert_txt_to_epub(&path, dir.path(), &options).is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
#[test]
fn utf16_bom_and_legacy_chinese_decode_without_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let text = "第一章 开始\n中文正文。\n第2章 结束\n完整保留。";
    let path = dir.path().join("book.txt");
    for little in [true, false] {
        let mut bytes = if little {
            vec![0xff, 0xfe]
        } else {
            vec![0xfe, 0xff]
        };
        for c in text.encode_utf16() {
            bytes.extend(if little {
                c.to_le_bytes()
            } else {
                c.to_be_bytes()
            });
        }
        std::fs::write(&path, &bytes).unwrap();
        let a = txt::analyze_txt(&path, &Default::default(), &[]).unwrap();
        assert_eq!(a.heading_count, 2);
        assert!(a.encoding.starts_with("UTF-16"));
        assert!(!a.chapters[0].excerpt.contains('\u{fffd}'));
    }
    let (gbk, _, errors) = encoding_rs::GBK.encode(text);
    assert!(!errors);
    std::fs::write(&path, &gbk).unwrap();
    let a = txt::analyze_txt(&path, &Default::default(), &[]).unwrap();
    assert_eq!(a.heading_count, 2);
    assert!(a.chapters[0].excerpt.contains("中文正文"));
}
#[test]
fn preview_is_bounded_but_conversion_keeps_every_chapter_and_source_is_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.txt");
    let text = (1..=503)
        .map(|n| format!("第{n}章 章节\n正文{n}。\n"))
        .collect::<String>();
    std::fs::write(&path, &text).unwrap();
    let a = txt::analyze_txt(&path, &Default::default(), &[]).unwrap();
    assert_eq!(a.chapters.len(), 500);
    assert_eq!(a.chapter_count, 503);
    let output = txt::convert_txt_to_epub(&path, dir.path(), &Default::default()).unwrap();
    assert_eq!(output.chapter_count, 503);
    let book = reader_core::epub::EpubBook::open(&output.epub_path).unwrap();
    assert_eq!(book.chapters.len(), 503);
    assert_eq!(std::fs::read_to_string(path).unwrap(), text);
}
#[test]
fn sparse_oversized_files_are_rejected_before_reading() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large.txt");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(64 * 1024 * 1024 + 1).unwrap();
    assert!(txt::analyze_txt(&path, &Default::default(), &[])
        .unwrap_err()
        .to_string()
        .contains("TXT_TOO_LARGE"));
}

#[test]
fn unknown_heading_can_be_selected_without_rewriting_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unknown.txt");
    let text = "前面的原文。\n\n§ 风从海上来 §\n\n正文仍然完整。\n\n下一段原文。";
    std::fs::write(&path, text).unwrap();
    let matches = reader_core::txt::find_txt_lines(&path, "风从").unwrap();
    assert_eq!(matches.len(), 1);
    let line = matches[0].line_start;
    let config = reader_core::txt::SplitConfig {
        use_heuristic: false,
        ..Default::default()
    };
    let result =
        reader_core::txt::analyze_txt_selected(&path, &config, &[], &[line, line]).unwrap();
    assert_eq!(result.heading_count, 1);
    assert_eq!(result.chapter_count, 2);
    assert_eq!(result.chapters[1].title, "§ 风从海上来 §");
    assert!(result.chapters[1].excerpt.contains("正文仍然完整"));
    let restored =
        reader_core::txt::analyze_txt_selected(&path, &config, &[line], &[line]).unwrap();
    assert_eq!(restored.heading_count, 0);
    assert!(restored.chapters[0].excerpt.contains("§ 风从海上来 §"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    assert!(reader_core::txt::analyze_txt_selected(&path, &config, &[], &[99999]).is_err());
    assert!(reader_core::txt::analyze_txt_selected(&path, &config, &[], &[1]).is_err());
}
#[test]
fn source_search_response_is_bounded_and_case_insensitive() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("search.txt");
    std::fs::write(
        &path,
        (0..2000)
            .map(|i| format!("Unusual section {i}\n"))
            .collect::<String>(),
    )
    .unwrap();
    let result = reader_core::txt::find_txt_lines(&path, "UNUSUAL").unwrap();
    assert_eq!(result.len(), 81);
    assert_eq!(result[80].line_start, 80);
    assert!(result.iter().all(|r| r.excerpt.chars().count() <= 240));
}

#[test]
fn invalid_explicit_encoding_is_not_replaced_with_guessed_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken.txt");
    for bytes in [vec![0xEF, 0xBB, 0xBF, 0xFF], vec![0xFF, 0xFE, 0x61]] {
        std::fs::write(&path, &bytes).unwrap();
        let error = reader_core::txt::analyze_txt(&path, &Default::default(), &[]).unwrap_err();
        assert_eq!(
            reader_core::txt::error_key(&error.to_string()),
            Some("txt.error_encoding")
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn unusual_oversized_chinese_ordinals_never_overflow() {
    let title = format!("第{}章", "万".repeat(40));
    let chapters =
        reader_core::txt::split::split_chapters(&format!("{title}\n正文。"), &Default::default());
    assert_eq!(chapters[0].title, title);
    assert_eq!(chapters[0].content, "正文。");
}
