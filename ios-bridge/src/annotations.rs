use super::{book, entry, field, number, validate_position};
use reader_core::library::{
    BookConfig, Bookmark, CorrectionRecord, Highlight, HighlightColor, Library, Note,
};
use serde_json::{json, Value};
use std::{io::Write, path::PathBuf};

fn persist(root: &str, config: &mut BookConfig) -> Result<(), String> {
    config.updated_at = reader_core::now_secs();
    let destination = PathBuf::from(root)
        .join("books")
        .join(format!("{}.json", config.id));
    let temporary = destination.with_extension("json.tmp");
    let result = (|| {
        let mut file = std::fs::File::create(&temporary).map_err(|_| "Cannot save annotations")?;
        file.write_all(
            &serde_json::to_vec_pretty(config).map_err(|_| "Cannot encode annotations")?,
        )
        .map_err(|_| "Cannot save annotations")?;
        file.sync_all().map_err(|_| "Cannot save annotations")?;
        std::fs::rename(&temporary, &destination).map_err(|_| "Cannot save annotations")
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    Ok(result?)
}
pub fn dispatch(root: &str, v: &Value) -> Result<Value, String> {
    let e = entry(root, field(v, "id")?)?;
    let mut config = Library::read_book_config(root, &e.id).ok_or("Cannot read annotations")?;
    let action = v["action"].as_str().unwrap_or("list");
    if action != "list" {
        match action {
            "bookmark" => {
                let chapter = number(v, "chapter")?;
                let block = number(v, "block")?;
                validate_position(&book(root, &e)?, chapter, block, 0)?;
                if config
                    .bookmarks
                    .iter()
                    .any(|b| b.chapter == chapter && b.block == block)
                {
                    config
                        .bookmarks
                        .retain(|b| b.chapter != chapter || b.block != block);
                } else {
                    config.bookmarks.push(Bookmark {
                        chapter,
                        block,
                        created_at: reader_core::now_secs(),
                    });
                }
            }
            "highlight" => {
                let chapter = number(v, "chapter")?;
                let (start_block, start_offset, end_block, end_offset) = (
                    number(v, "start_block")?,
                    number(v, "start_offset")?,
                    number(v, "end_block")?,
                    number(v, "end_offset")?,
                );
                let parsed = book(root, &e)?;
                validate_position(&parsed, chapter, start_block, start_offset)?;
                validate_position(&parsed, chapter, end_block, end_offset)?;
                if (start_block, start_offset) >= (end_block, end_offset) {
                    return Err("Invalid highlight range".into());
                }
                let color = match v["color"].as_str().unwrap_or("Yellow") {
                    "Yellow" => HighlightColor::Yellow,
                    "Green" => HighlightColor::Green,
                    "Blue" => HighlightColor::Blue,
                    "Pink" => HighlightColor::Pink,
                    _ => return Err("Invalid highlight color".into()),
                };
                let id = uuid::Uuid::new_v4().to_string();
                let now = reader_core::now_secs();
                config.highlights.push(Highlight {
                    id: id.clone(),
                    chapter,
                    start_block,
                    start_offset,
                    end_block,
                    end_offset,
                    color,
                    created_at: now,
                });
                if let Some(content) = v["note"].as_str().filter(|s| !s.trim().is_empty()) {
                    if content.len() > 32 * 1024 {
                        return Err("Note is too long".into());
                    }
                    config.notes.push(Note {
                        highlight_id: id,
                        content: content.into(),
                        created_at: now,
                        updated_at: now,
                    });
                }
            }
            "correction" => {
                let chapter = number(v, "chapter")?;
                let block = number(v, "block")?;
                let offset = number(v, "char_offset")?;
                let parsed = book(root, &e)?;
                validate_position(&parsed, chapter, block, offset)?;
                let original = field(v, "original")?;
                let corrected = field(v, "corrected")?;
                let text = match &parsed.chapters[chapter].blocks[block] {
                    reader_core::epub::ContentBlock::Paragraph { spans, .. }
                    | reader_core::epub::ContentBlock::Heading { spans, .. } => {
                        spans.iter().map(|s| s.text.as_str()).collect::<String>()
                    }
                    _ => String::new(),
                };
                if original.chars().count() != 1
                    || corrected.chars().count() != 1
                    || text.chars().nth(offset).map(|c| c.to_string()).as_deref() != Some(original)
                {
                    return Err("Invalid correction".into());
                }
                config.corrections.retain(|c| {
                    (c.chapter, c.block_idx, c.char_offset) != (chapter, block, offset)
                });
                config.corrections.push(CorrectionRecord {
                    chapter,
                    block_idx: block,
                    char_offset: offset,
                    original: original.into(),
                    corrected: corrected.into(),
                    status: "accepted".into(),
                });
            }
            "note" => {
                let id = field(v, "highlight_id")?;
                let content = field(v, "note")?;
                if content.len() > 32 * 1024 || !config.highlights.iter().any(|h| h.id == id) {
                    return Err("Invalid note".into());
                }
                if let Some(n) = config.notes.iter_mut().find(|n| n.highlight_id == id) {
                    n.content = content.into();
                    n.updated_at = reader_core::now_secs();
                } else {
                    let now = reader_core::now_secs();
                    config.notes.push(Note {
                        highlight_id: id.into(),
                        content: content.into(),
                        created_at: now,
                        updated_at: now,
                    });
                }
            }
            "delete" => {
                let id = field(v, "highlight_id")?;
                config.highlights.retain(|h| h.id != id);
                config.notes.retain(|n| n.highlight_id != id);
            }
            _ => return Err("Unknown annotation operation".into()),
        }
        persist(root, &mut config)?;
    }
    Ok(
        json!({"highlights":config.highlights,"notes":config.notes,"bookmarks":config.bookmarks,"corrections":config.corrections}),
    )
}
pub fn remove(root: &str, v: &Value) -> Result<Value, String> {
    let ids = v["ids"].as_array().ok_or("Missing book selection")?;
    if ids.is_empty() || ids.len() > 1000 {
        return Err("Invalid book selection".into());
    }
    let mut lib = Library::load_from(root);
    let directory = PathBuf::from(root)
        .join("books")
        .canonicalize()
        .map_err(|_| "Cannot read library storage")?;
    let mut selected = Vec::new();
    for id in ids {
        let id = id.as_str().ok_or("Invalid book selection")?;
        let e = lib
            .books
            .iter()
            .find(|e| e.id == id)
            .ok_or("Book is no longer in the library")?;
        let path = PathBuf::from(&e.path);
        // Only owned UUID files can be deleted. Never follow an externally supplied path/symlink.
        if path.file_name().and_then(|f| f.to_str()) != Some(&format!("{}.epub", e.id))
            || path
                .symlink_metadata()
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false)
            || path.parent().and_then(|p| p.canonicalize().ok()).as_ref() != Some(&directory)
        {
            return Err("Book is outside library storage".into());
        }
        selected.push((e.id.clone(), e.path.clone()));
    }
    // Persist a local deletion marker before file removal, so an in-flight
    // cloud download cannot resurrect a book deleted in this library.
    let mut deleted: std::collections::BTreeSet<String> =
        std::fs::read(PathBuf::from(root).join("deleted-local-books.json"))
            .ok()
            .and_then(|d| serde_json::from_slice(&d).ok())
            .unwrap_or_default();
    for (_, path) in &selected {
        if let Ok(hash) = reader_core::epub::EpubBook::file_hash(path) {
            deleted.insert(hash);
        }
    }
    let marker = PathBuf::from(root).join("deleted-local-books.json");
    let temp = marker.with_extension("json.tmp");
    std::fs::write(
        &temp,
        serde_json::to_vec(&deleted).map_err(|_| "Cannot save library changes")?,
    )
    .map_err(|_| "Cannot save library changes")?;
    std::fs::rename(temp, marker).map_err(|_| "Cannot save library changes")?;
    for (id, path) in &selected {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("Cannot delete local book".into()),
        }
        lib.remove_by_path(root, path);
        let _ = std::fs::remove_file(directory.join(format!("{id}.cover")));
    }
    let stored = Library::load_from(root);
    if stored
        .books
        .iter()
        .any(|e| selected.iter().any(|(id, _)| &e.id == id))
    {
        return Err("Cannot save library changes".into());
    }
    Ok(json!({"removed":selected.len()}))
}
