//! TXT conversion uses the same validated EPUB admission as ordinary imports.
use super::{dispatch_inner, field};
use reader_core::txt::{self, ConvertOptions, SplitConfig};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
fn options(v: &Value) -> ConvertOptions {
    ConvertOptions {
        title: v["title"].as_str().map(str::to_owned),
        author: v["author"].as_str().map(str::to_owned),
        custom_regex: v["custom_regex"].as_str().map(str::to_owned),
        use_heuristic: v["use_heuristic"].as_bool().unwrap_or(true),
        ..Default::default()
    }
}
fn selected(v: &Value, key: &str) -> Result<Vec<usize>, String> {
    let Some(value) = v.get(key) else {
        return Ok(Vec::new());
    };
    let lines = value
        .as_array()
        .filter(|a| a.len() <= 100_000)
        .ok_or("TXT_INVALID_SELECTION")?;
    lines
        .iter()
        .map(|n| {
            n.as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or("TXT_INVALID_SELECTION".into())
        })
        .collect()
}
pub fn analyze(v: &Value) -> Result<Value, String> {
    let options = options(v);
    let config = SplitConfig {
        min_chapter_chars: 100,
        use_heuristic: options.use_heuristic,
        custom_regex: options.custom_regex,
    };
    let analysis = txt::analyze_txt_selected(
        Path::new(field(v, "path")?),
        &config,
        &selected(v, "excluded_lines")?,
        &selected(v, "included_lines")?,
    )
    .map_err(|e| e.to_string())?;
    serde_json::to_value(analysis).map_err(|e| e.to_string())
}
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub fn convert(root: &str, device: &str, v: &Value) -> Result<Value, String> {
    let scratch =
        Scratch(PathBuf::from(root).join(format!(".txt-import-{}", uuid::Uuid::new_v4())));
    std::fs::create_dir(&scratch.0).map_err(|_| "Cannot create storage for TXT conversion")?;
    let result = txt::convert_txt_to_epub_selected(
        Path::new(field(v, "path")?),
        &scratch.0,
        &options(v),
        &selected(v, "excluded_lines")?,
        &selected(v, "included_lines")?,
    )
    .map_err(|e| e.to_string())?;
    // This branch holds no DATA_LOCK; the normal import operation acquires it.
    let entry =
        dispatch_inner(json!({"op":"import","root":root,"device":device,"path":result.epub_path}))?;
    Ok(json!({"entry":entry,"chapter_count":result.chapter_count}))
}

pub fn lines(v: &Value) -> Result<Value, String> {
    let result = txt::find_txt_lines(
        Path::new(field(v, "path")?),
        v["query"].as_str().unwrap_or(""),
    )
    .map_err(|e| e.to_string())?;
    serde_json::to_value(result).map_err(|e| e.to_string())
}
