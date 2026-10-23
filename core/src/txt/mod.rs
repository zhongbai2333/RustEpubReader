//! TXT-to-EPUB 转换模块。
//!
//! 提供 TXT 文件的编码检测、文本清洗、章节识别、EPUB 生成全流程。

mod build;
mod clean;
mod detect;
pub mod split;

use std::path::{Path, PathBuf};

pub use build::TxtMeta;
pub use split::{ChapterPreview, RawChapter, SplitConfig};

// ── 错误类型 ────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum TxtError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Encoding error: {0}")]
    Encoding(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Build error: {0}")]
    Build(String),
}

// ── 转换选项与结果 ──────────────────────────────────────────────

/// 转换选项。
#[derive(Debug, Clone)]
pub struct ConvertOptions {
    /// 书名。None = 从文件名推导。
    pub title: Option<String>,
    /// 作者。
    pub author: Option<String>,
    /// 用户自定义正则。
    pub custom_regex: Option<String>,
    /// 识别无编号标题。默认开启，需用户核对预览。
    pub use_heuristic: bool,
    /// 语言代码。默认 "zh"。
    pub language: String,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            title: None,
            author: None,
            custom_regex: None,
            use_heuristic: true,
            language: "zh".to_string(),
        }
    }
}

/// 转换结果。
#[derive(Debug, Clone)]
pub struct ConvertResult {
    /// 生成的 EPUB 文件路径。
    pub epub_path: PathBuf,
    /// 书名。
    pub title: String,
    /// 章节数。
    pub chapter_count: usize,
}

// ── 顶层 API ────────────────────────────────────────────────────

/// 将 TXT 文件转换为 EPUB。
///
/// 流程：编码检测 → 文本清洗 → 章节识别 → EPUB 生成。
/// 输出文件存放在 `output_dir` 下，文件名为 `{uuid}.epub`。
pub fn convert_txt_to_epub(
    txt_path: &Path,
    output_dir: &Path,
    options: &ConvertOptions,
) -> Result<ConvertResult, TxtError> {
    convert_txt_to_epub_excluding(txt_path, output_dir, options, &[])
}

pub fn convert_txt_to_epub_excluding(
    txt_path: &Path,
    output_dir: &Path,
    options: &ConvertOptions,
    excluded: &[usize],
) -> Result<ConvertResult, TxtError> {
    convert_txt_to_epub_selected(txt_path, output_dir, options, excluded, &[])
}
pub fn convert_txt_to_epub_selected(
    txt_path: &Path,
    output_dir: &Path,
    options: &ConvertOptions,
    excluded: &[usize],
    included: &[usize],
) -> Result<ConvertResult, TxtError> {
    let config = SplitConfig {
        min_chapter_chars: 100,
        use_heuristic: options.use_heuristic,
        custom_regex: options.custom_regex.clone(),
    };
    let (chapters, _, _) = prepare(txt_path, &config, excluded, included)?;
    let title = options
        .title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| derive_title_from_path(txt_path));
    let epub_path = output_dir.join(format!("{}.epub", uuid::Uuid::new_v4()));
    let meta = TxtMeta {
        title: title.clone(),
        author: options.author.clone(),
        language: options.language.clone(),
    };
    if let Err(error) = build::build_epub(&chapters, &meta, &epub_path) {
        let _ = std::fs::remove_file(&epub_path);
        return Err(error);
    }
    Ok(ConvertResult {
        epub_path,
        title,
        chapter_count: chapters.len(),
    })
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TxtAnalysis {
    pub encoding: String,
    pub char_count: usize,
    pub chapter_count: usize,
    pub heading_count: usize,
    pub chapters: Vec<ChapterPreview>,
}
fn prepare(
    path: &Path,
    config: &SplitConfig,
    excluded: &[usize],
    included: &[usize],
) -> Result<(Vec<RawChapter>, String, usize), TxtError> {
    let decoded = detect::read_txt_file(path)?;
    let cleaned = clean::clean_text(&decoded.text);
    if cleaned.is_empty() {
        return Err(TxtError::Parse("TXT_EMPTY".into()));
    }
    if let Some(pattern) = config
        .custom_regex
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        if pattern.len() > 2048 {
            return Err(TxtError::Parse("TXT_INVALID_REGEX".into()));
        }
        let re =
            regex::Regex::new(pattern).map_err(|_| TxtError::Parse("TXT_INVALID_REGEX".into()))?;
        if !cleaned
            .lines()
            .any(|line| !line.trim().is_empty() && re.is_match(line.trim()))
        {
            return Err(TxtError::Parse("TXT_PATTERN_NO_MATCH".into()));
        }
    }
    let lines: Vec<_> = cleaned.lines().collect();
    if excluded.len() > 100_000
        || included.len() > 100_000
        || excluded.iter().chain(included).any(|&i| i >= lines.len())
        || included.iter().any(|&i| {
            lines
                .get(i)
                .is_none_or(|l| l.trim().is_empty() || l.chars().count() > 512)
        })
    {
        return Err(TxtError::Parse("TXT_INVALID_SELECTION".into()));
    }
    let chapters = split::split_chapters_selected(&cleaned, config, excluded, included);
    Ok((chapters, decoded.encoding, cleaned.chars().count()))
}
fn preview(ch: &RawChapter) -> ChapterPreview {
    ChapterPreview {
        title: ch.title.clone(),
        line_start: ch.line_start,
        char_count: ch.content.chars().count(),
        is_heading: ch.is_heading,
        excerpt: ch.content.chars().take(240).collect(),
    }
}
/// Bounded bridge response; conversion always processes the complete document.
pub fn analyze_txt(
    path: &Path,
    config: &SplitConfig,
    excluded: &[usize],
) -> Result<TxtAnalysis, TxtError> {
    analyze_txt_selected(path, config, excluded, &[])
}
pub fn analyze_txt_selected(
    path: &Path,
    config: &SplitConfig,
    excluded: &[usize],
    included: &[usize],
) -> Result<TxtAnalysis, TxtError> {
    let (chapters, encoding, char_count) = prepare(path, config, excluded, included)?;
    Ok(TxtAnalysis {
        encoding,
        char_count,
        chapter_count: chapters.len(),
        heading_count: chapters.iter().filter(|c| c.is_heading).count(),
        chapters: chapters.iter().take(500).map(preview).collect(),
    })
}
pub fn preview_chapters(
    path: &Path,
    config: &SplitConfig,
) -> Result<Vec<ChapterPreview>, TxtError> {
    let (chapters, _, _) = prepare(path, config, &[], &[])?;
    Ok(chapters.iter().map(preview).collect())
}

/// 从文件路径推导书名（去掉扩展名）。
fn derive_title_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "未命名".to_string())
}

/// Search bounded source lines in the same cleaned coordinate space as the preview.
/// The extra row indicates truncation without returning the full source over the bridge.
pub fn find_txt_lines(path: &Path, query: &str) -> Result<Vec<ChapterPreview>, TxtError> {
    if query.len() > 2048 {
        return Err(TxtError::Parse("TXT_INVALID_SELECTION".into()));
    }
    let decoded = detect::read_txt_file(path)?;
    let cleaned = clean::clean_text(&decoded.text);
    let query = query.trim().to_lowercase();
    let lines: Vec<_> = cleaned.lines().collect();
    Ok(lines
        .iter()
        .enumerate()
        .filter(|(_, l)| {
            !l.trim().is_empty()
                && l.chars().count() <= 512
                && (query.is_empty() || l.to_lowercase().contains(&query))
        })
        .take(81)
        .map(|(i, l)| ChapterPreview {
            title: l.trim().into(),
            line_start: i,
            char_count: 0,
            is_heading: true,
            excerpt: lines
                .iter()
                .skip(i + 1)
                .flat_map(|l| l.chars().chain(std::iter::once('\n')))
                .take(240)
                .collect(),
        })
        .collect())
}

/// Stable error-to-copy contract shared by native clients.
pub fn error_key(raw: &str) -> Option<&'static str> {
    [
        ("TXT_INVALID_REGEX", "txt.error_regex"),
        ("TXT_PATTERN_NO_MATCH", "txt.error_no_match"),
        ("TXT_ENCODING_FAILED", "txt.error_encoding"),
        ("TXT_TOO_LARGE", "txt.error_size"),
        ("TXT_EMPTY", "txt.error_empty"),
        ("TXT_INVALID_SELECTION", "txt.error_selection"),
    ]
    .into_iter()
    .find_map(|(code, key)| raw.contains(code).then_some(key))
}

/// Owned JSON contract used by native manual-correction tools.
pub fn command(v: &serde_json::Value) -> Result<serde_json::Value, String> {
    use serde_json::json;
    let path = Path::new(v["path"].as_str().ok_or("TXT_INVALID_SELECTION")?);
    let lines = |key: &str| -> Result<Vec<usize>, String> {
        let Some(value) = v.get(key) else {
            return Ok(Vec::new());
        };
        value
            .as_array()
            .filter(|a| a.len() <= 100_000)
            .ok_or("TXT_INVALID_SELECTION")?
            .iter()
            .map(|n| {
                n.as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or("TXT_INVALID_SELECTION".into())
            })
            .collect()
    };
    let options = ConvertOptions {
        title: v["title"].as_str().map(str::to_owned),
        author: v["author"].as_str().map(str::to_owned),
        custom_regex: v["custom_regex"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        use_heuristic: v["use_heuristic"].as_bool().unwrap_or(true),
        ..Default::default()
    };
    match v["action"].as_str().unwrap_or("analyze") {
        "lines" => serde_json::to_value(
            find_txt_lines(path, v["query"].as_str().unwrap_or("")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string()),
        "analyze" => serde_json::to_value(
            analyze_txt_selected(
                path,
                &SplitConfig {
                    min_chapter_chars: 100,
                    use_heuristic: options.use_heuristic,
                    custom_regex: options.custom_regex,
                },
                &lines("excluded_lines")?,
                &lines("included_lines")?,
            )
            .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string()),
        "convert" => {
            let result = convert_txt_to_epub_selected(
                path,
                Path::new(v["directory"].as_str().ok_or("Invalid output directory")?),
                &options,
                &lines("excluded_lines")?,
                &lines("included_lines")?,
            )
            .map_err(|e| e.to_string())?;
            Ok(
                json!({"epubPath":result.epub_path,"title":result.title,"chapterCount":result.chapter_count}),
            )
        }
        _ => Err("Invalid TXT operation".into()),
    }
}
