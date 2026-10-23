//! Chapter boundaries are evaluated locally; a book may mix heading styles.
use regex::Regex;
use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

#[derive(Debug, Clone, serde::Serialize)]
pub struct RawChapter {
    pub title: String,
    pub content: String,
    pub line_start: usize,
    pub is_heading: bool,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChapterPreview {
    pub title: String,
    pub line_start: usize,
    pub char_count: usize,
    pub is_heading: bool,
    pub excerpt: String,
}
#[derive(Debug, Clone)]
pub struct SplitConfig {
    /// Minimum body evidence for ambiguous numbered/unnumbered headings.
    /// Explicit chapter headings are never merged for being short.
    pub min_chapter_chars: usize,
    pub use_heuristic: bool,
    pub custom_regex: Option<String>,
}
impl Default for SplitConfig {
    fn default() -> Self {
        Self {
            min_chapter_chars: 100,
            use_heuristic: true,
            custom_regex: None,
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Explicit,
    Numbered,
    Plain,
}
struct Candidate {
    line: usize,
    kind: Kind,
    ordinal: Option<u32>,
}
fn rules() -> &'static [Regex; 5] {
    static RULES: OnceLock<[Regex; 5]> = OnceLock::new();
    RULES.get_or_init(|| [
        Regex::new(r"^第\s*([零〇○一二两三四五六七八九十百千万壹贰叁肆伍陆柒捌玖拾佰仟0-9０-９]+)\s*[章节回卷部篇集幕]").unwrap(),
        Regex::new(r"(?i)^(?:chapter|part|book|volume)\s+([0-9]+|[ivxlcdm]+|one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve)\b").unwrap(),
        Regex::new(r"^卷\s*([零〇一二两三四五六七八九十百千万0-9０-９]+)").unwrap(),
        Regex::new(r"(?i)^(?:序章|序言|序|楔子|引子|前言|引言|开篇|尾声|终章|后记|番外[零〇一二两三四五六七八九十百0-9０-９]*|完结感言|完本感言|作品相关|prologue|foreword|preface|introduction|epilogue|afterword|conclusion)(?:$|[\s:：·—\-].*)").unwrap(),
        Regex::new(r"^([0-9０-９]{1,6}(?:[-－.．][0-9０-９]{1,6})*)(?:[.．、:：\-－]\s*\S.*|\s+\S.*|$)").unwrap(),
    ])
}
fn ordinal(s: &str) -> Option<u32> {
    let s: String = s
        .chars()
        .map(|c| {
            if ('０'..='９').contains(&c) {
                char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap()
            } else {
                c
            }
        })
        .collect();
    if let Ok(n) = s.parse() {
        return Some(n);
    }
    // Compound labels such as 1-1 are assessed structurally, not as decimals.
    if s.contains(['-', '－', '.', '．']) {
        return None;
    }
    let mut total = 0u32;
    let mut digit = 0u32;
    for c in s.chars() {
        match c {
            '零' | '〇' | '○' => digit = 0,
            '一' | '壹' => digit = 1,
            '二' | '两' | '贰' => digit = 2,
            '三' | '叁' => digit = 3,
            '四' | '肆' => digit = 4,
            '五' | '伍' => digit = 5,
            '六' | '陆' => digit = 6,
            '七' | '柒' => digit = 7,
            '八' | '捌' => digit = 8,
            '九' | '玖' => digit = 9,
            '十' | '拾' => {
                total = total.checked_add(digit.max(1) * 10)?;
                digit = 0
            }
            '百' | '佰' => {
                total = total.checked_add(digit.max(1) * 100)?;
                digit = 0
            }
            '千' | '仟' => {
                total = total.checked_add(digit.max(1) * 1000)?;
                digit = 0
            }
            '万' => {
                total = total.checked_add(digit)?.max(1).checked_mul(10000)?;
                digit = 0
            }
            _ => return None,
        }
    }
    total.checked_add(digit)
}
fn candidate(line: &str, index: usize, heuristic: bool) -> Option<Candidate> {
    let text = line.trim();
    let len = text.chars().count();
    if len == 0 || len > 96 {
        return None;
    }
    let bare = text.trim_start_matches(['【', '[', '（', '(']).trim();
    for re in &rules()[..4] {
        if let Some(c) = re.captures(bare) {
            let suffix = &bare[c.get(0).unwrap().end()..];
            // A prose sentence referring to a chapter is not its heading.
            if suffix.starts_with(['中', '里', '的']) && suffix.contains(['，', '。', ',']) {
                continue;
            }
            if text.ends_with('。') {
                continue;
            }
            return Some(Candidate {
                line: index,
                kind: Kind::Explicit,
                ordinal: c.get(1).and_then(|n| ordinal(n.as_str())),
            });
        }
    }
    if len <= 64 && !text.ends_with(['。', '，', '；', ',', ';']) {
        if let Some(c) = rules()[4].captures(bare) {
            return Some(Candidate {
                line: index,
                kind: Kind::Numbered,
                ordinal: c.get(1).and_then(|n| ordinal(n.as_str())),
            });
        }
    }
    if heuristic
        && (2..=32).contains(&len)
        && text.chars().any(char::is_alphanumeric)
        && !text.contains([
            '。', '，', '；', '：', '“', '”', '「', '」', ',', ';', ':', '"',
        ])
        && !text.ends_with(['！', '？', '!', '?', '.'])
    {
        return Some(Candidate {
            line: index,
            kind: Kind::Plain,
            ordinal: None,
        });
    }
    None
}

pub fn split_chapters(text: &str, config: &SplitConfig) -> Vec<RawChapter> {
    split_chapters_excluding(text, config, &[])
}
/// Excluding a boundary preserves its heading line in the adjacent body.
pub fn split_chapters_excluding(
    text: &str,
    config: &SplitConfig,
    excluded: &[usize],
) -> Vec<RawChapter> {
    split_chapters_selected(text, config, excluded, &[])
}
pub fn split_chapters_selected(
    text: &str,
    config: &SplitConfig,
    excluded: &[usize],
    included: &[usize],
) -> Vec<RawChapter> {
    let lines: Vec<&str> = text.lines().collect();
    let omitted: HashSet<usize> = excluded.iter().copied().collect();
    let matches: Vec<usize> = if let Some(pattern) = config
        .custom_regex
        .as_ref()
        .filter(|s| !s.trim().is_empty())
    {
        match Regex::new(pattern) {
            Ok(re) => lines
                .iter()
                .enumerate()
                .filter(|(_, l)| !l.trim().is_empty() && re.is_match(l.trim()))
                .map(|(i, _)| i)
                .collect(),
            Err(_) => Vec::new(),
        }
    } else {
        detect(&lines, config)
    };
    let mut selected: Vec<usize> = matches
        .into_iter()
        .chain(included.iter().copied())
        .filter(|i| *i < lines.len() && !lines[*i].trim().is_empty() && !omitted.contains(i))
        .collect();
    selected.sort_unstable();
    selected.dedup();
    if selected.is_empty() {
        return fallback(&lines);
    }
    let mut chapters = Vec::new();
    let front = lines[..selected[0]].join("\n");
    if !front.trim().is_empty() {
        chapters.push(RawChapter {
            title: "前言".into(),
            content: front.trim().into(),
            line_start: 0,
            is_heading: false,
        });
    }
    for (n, &start) in selected.iter().enumerate() {
        let end = selected.get(n + 1).copied().unwrap_or(lines.len());
        chapters.push(RawChapter {
            title: lines[start].trim().into(),
            content: lines[start + 1..end].join("\n").trim().into(),
            line_start: start,
            is_heading: true,
        });
    }
    chapters
}
fn detect(lines: &[&str], config: &SplitConfig) -> Vec<usize> {
    let isolated = |i: usize| {
        (i == 0 || lines[i - 1].trim().is_empty())
            && (i + 1 == lines.len() || lines[i + 1].trim().is_empty())
    };
    let candidates: Vec<_> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, line)| candidate(line, i, config.use_heuristic))
        .filter(|c| c.kind != Kind::Plain || isolated(c.line))
        .collect();
    let mut prefix = vec![0usize];
    for line in lines {
        prefix.push(prefix.last().unwrap() + line.trim().chars().count());
    }
    let bodies: Vec<_> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| {
            prefix[candidates.get(i + 1).map_or(lines.len(), |n| n.line)] - prefix[c.line + 1]
        })
        .collect();
    let threshold = config.min_chapter_chars.max(24);
    let numeric_peers = candidates
        .iter()
        .zip(&bodies)
        .filter(|(c, n)| c.kind == Kind::Numbered && **n >= threshold)
        .count();
    let plain_peers = candidates
        .iter()
        .zip(&bodies)
        .filter(|(c, n)| c.kind == Kind::Plain && **n >= threshold)
        .count();
    let explicit = candidates.iter().any(|c| c.kind == Kind::Explicit);
    let mut selected = Vec::new();
    for (i, c) in candidates.iter().enumerate() {
        let body = bodies[i];
        let ordinal_link = c.ordinal.is_some_and(|n| {
            i > 0
                && candidates[i - 1]
                    .ordinal
                    .is_some_and(|p| p.checked_add(1) == Some(n))
                && candidates[i - 1].kind == Kind::Explicit
                || candidates.get(i + 1).is_some_and(|next| {
                    next.kind == Kind::Explicit && next.ordinal == n.checked_add(1)
                })
        });
        let list_tail = i > 0
            && candidates[i - 1].kind == Kind::Numbered
            && bodies[i - 1] < 24
            && c.line - candidates[i - 1].line <= 3;
        let accept = match c.kind {
            Kind::Explicit => true,
            Kind::Numbered => {
                !list_tail
                    && (body >= threshold && (numeric_peers >= 2 || explicit && isolated(c.line))
                        || body > 0 && ordinal_link)
            }
            Kind::Plain => body >= threshold && (plain_peers >= 2 || explicit) && isolated(c.line),
        };
        if accept {
            selected.push(c.line)
        }
    }
    // Keep an initial printed TOC as text instead of importing duplicate empty chapters.
    let candidate_lines: HashSet<_> = candidates
        .iter()
        .filter(|c| c.kind != Kind::Plain)
        .map(|c| c.line)
        .collect();
    let last_occurrence: HashMap<_, _> = lines
        .iter()
        .enumerate()
        .map(|(i, l)| (l.trim(), i))
        .collect();
    for (i, line) in lines.iter().enumerate() {
        if !matches!(
            line.trim().to_lowercase().as_str(),
            "目录" | "目 录" | "contents" | "table of contents"
        ) {
            continue;
        }
        let mut run = Vec::new();
        let mut seen_titles = HashSet::new();
        for (j, l) in lines.iter().enumerate().skip(i + 1) {
            if l.trim().is_empty() {
                continue;
            }
            if candidate_lines.contains(&j) {
                if !seen_titles.insert(l.trim()) {
                    break;
                }
                run.push(j)
            } else {
                break;
            }
        }
        if run.len() >= 2
            && run
                .iter()
                .filter(|&&j| {
                    last_occurrence
                        .get(lines[j].trim())
                        .is_some_and(|&last| last > j)
                })
                .count()
                >= 2
        {
            let toc: HashSet<_> = run.into_iter().collect();
            selected.retain(|j| !toc.contains(j));
        }
    }
    selected
}
fn fallback(lines: &[&str]) -> Vec<RawChapter> {
    const LIMIT: usize = 8000;
    let mut result = Vec::new();
    let mut content = String::new();
    let mut count = 0;
    let mut start = 0;
    for (i, line) in lines.iter().enumerate() {
        let mut rest = *line;
        while !rest.is_empty() {
            let room = LIMIT - count;
            let end = rest.char_indices().nth(room).map_or(rest.len(), |(n, _)| n);
            if content.is_empty() {
                start = i
            }
            content.push_str(&rest[..end]);
            count += rest[..end].chars().count();
            rest = &rest[end..];
            if count == LIMIT {
                result.push(RawChapter {
                    title: format!("第{}部分", result.len() + 1),
                    content: content.trim().into(),
                    line_start: start,
                    is_heading: false,
                });
                content.clear();
                count = 0;
            }
        }
        if !content.is_empty() {
            content.push('\n')
        }
        // Prefer paragraph boundaries rather than forcing every part to its upper bound.
        if count >= 6000 {
            result.push(RawChapter {
                title: format!("第{}部分", result.len() + 1),
                content: content.trim().into(),
                line_start: start,
                is_heading: false,
            });
            content.clear();
            count = 0;
        }
    }
    if !content.trim().is_empty() || result.is_empty() {
        result.push(RawChapter {
            title: if result.is_empty() {
                "全文".into()
            } else {
                format!("第{}部分", result.len() + 1)
            },
            content: content.trim().into(),
            line_start: start,
            is_heading: false,
        });
    }
    result
}
