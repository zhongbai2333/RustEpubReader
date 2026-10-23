//! Patch only owned text ranges; every other ZIP member and XHTML byte survives.
use super::ExportMode;
use crate::{
    epub::{ContentBlock, EpubBook},
    library::BookConfig,
};
use quick_xml::{events::Event, Reader};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::Path,
};

#[derive(Clone, Debug)]
struct Atom {
    ch: char,
    start: usize,
    end: usize,
}
fn atoms(html: &str) -> Result<Vec<Atom>, String> {
    let mut reader = Reader::from_str(html);
    reader.config_mut().check_end_names = false;
    let mut result = Vec::new();
    let mut hidden = 0usize;
    let mut body = false;
    loop {
        let start = reader.buffer_position() as usize;
        match reader
            .read_event()
            .map_err(|e| format!("Cannot map publication markup: {e}"))?
        {
            Event::Start(e) => {
                let name = e.local_name();
                if name.as_ref() == b"body" {
                    body = true;
                }
                if matches!(name.as_ref(), b"head" | b"script" | b"style" | b"rt") {
                    hidden += 1;
                }
            }
            Event::End(e) => {
                if matches!(
                    e.local_name().as_ref(),
                    b"head" | b"script" | b"style" | b"rt"
                ) {
                    hidden = hidden.saturating_sub(1);
                }
                if e.local_name().as_ref() == b"body" {
                    body = false;
                }
            }
            Event::Text(_) | Event::GeneralRef(_) if body && hidden == 0 => {
                let end = reader.buffer_position() as usize;
                let raw = &html[start..end];
                let mut at = 0;
                while at < raw.len() {
                    if raw.as_bytes()[at] == b'&' {
                        let len = raw[at..]
                            .find(';')
                            .ok_or("Unterminated publication entity")?
                            + 1;
                        let entity = &raw[at..at + len];
                        let decoded = if entity == "&nbsp;" {
                            "\u{a0}".into()
                        } else {
                            quick_xml::escape::unescape(entity)
                                .map_err(|_| "Unsupported publication entity")?
                                .into_owned()
                        };
                        if decoded.chars().count() != 1 {
                            return Err("Ambiguous publication entity".into());
                        }
                        result.push(Atom {
                            ch: decoded.chars().next().unwrap(),
                            start: start + at,
                            end: start + at + len,
                        });
                        at += len;
                    } else {
                        let ch = raw[at..].chars().next().unwrap();
                        let len = ch.len_utf8();
                        result.push(Atom {
                            ch,
                            start: start + at,
                            end: start + at + len,
                        });
                        at += len;
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(result)
}
fn text(block: &ContentBlock) -> String {
    match block {
        ContentBlock::Paragraph { spans, .. } | ContentBlock::Heading { spans, .. } => {
            spans.iter().map(|s| s.text.as_str()).collect()
        }
        _ => String::new(),
    }
}
fn patch(
    html: &str,
    chapter: &crate::epub::Chapter,
    index: usize,
    cfg: &BookConfig,
    mode: ExportMode,
) -> Result<String, String> {
    let atoms = atoms(html)?;
    let visible: Vec<usize> = atoms
        .iter()
        .enumerate()
        .filter(|(_, a)| !a.ch.is_whitespace())
        .map(|(i, _)| i)
        .collect();
    let compact: Vec<char> = visible.iter().map(|&i| atoms[i].ch).collect();
    let mut mapped: Vec<Option<Vec<Option<usize>>>> = Vec::new();
    let mut cursor = 0;
    for block in &chapter.blocks {
        let source = text(block);
        let chars: Vec<char> = source.chars().collect();
        let needle: Vec<char> = chars
            .iter()
            .copied()
            .filter(|c| !c.is_whitespace())
            .collect();
        let found = (!needle.is_empty())
            .then(|| {
                compact[cursor..]
                    .windows(needle.len())
                    .position(|s| s == needle)
            })
            .flatten();
        if let Some(at) = found {
            let start = cursor + at;
            let mut n = 0;
            mapped.push(Some(
                chars
                    .iter()
                    .map(|ch| {
                        if ch.is_whitespace() {
                            None
                        } else {
                            let atom = visible[start + n];
                            n += 1;
                            Some(atom)
                        }
                    })
                    .collect(),
            ));
            cursor = start + needle.len();
        } else {
            mapped.push(None);
        }
    }
    let mut edits: BTreeMap<usize, (usize, String)> = BTreeMap::new();
    let mut decorations: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    let mut corrections = std::collections::HashSet::new();
    if matches!(mode, ExportMode::WithCorrections | ExportMode::Full) {
        for c in cfg
            .corrections
            .iter()
            .filter(|c| c.chapter == index && c.status.eq_ignore_ascii_case("accepted"))
        {
            let source = chapter
                .blocks
                .get(c.block_idx)
                .map(text)
                .ok_or("Correction block no longer exists")?;
            if source
                .chars()
                .skip(c.char_offset)
                .take(c.original.chars().count())
                .collect::<String>()
                != c.original
                || c.original.trim().is_empty()
            {
                return Err("Correction no longer matches the original text".into());
            }
            let mapping = mapped
                .get(c.block_idx)
                .and_then(Option::as_ref)
                .ok_or("Cannot map correction to original markup")?;
            let selected: Vec<usize> = mapping
                .iter()
                .skip(c.char_offset)
                .take(c.original.chars().count())
                .flatten()
                .copied()
                .collect();
            if selected.is_empty() {
                return Err("Cannot map correction range".into());
            }
            let first = *selected.first().unwrap();
            let last = *selected.last().unwrap();
            for (a, atom) in atoms.iter().enumerate().take(last + 1).skip(first) {
                if !corrections.insert(a) {
                    return Err("Overlapping accepted corrections".into());
                }
                edits.insert(
                    atom.start,
                    (
                        atom.end,
                        if a == first {
                            crate::escape_html(&c.corrected)
                        } else {
                            String::new()
                        },
                    ),
                );
            }
        }
    }
    if matches!(mode, ExportMode::WithAnnotations | ExportMode::Full) {
        for h in cfg.highlights.iter().filter(|h| h.chapter == index) {
            if h.start_block > h.end_block || h.end_block >= chapter.blocks.len() {
                return Err("Invalid highlight range".into());
            }
            for (b, mapping) in mapped
                .iter()
                .enumerate()
                .take(h.end_block + 1)
                .skip(h.start_block)
            {
                let start = if b == h.start_block {
                    h.start_offset
                } else {
                    0
                };
                let end = if b == h.end_block {
                    h.end_offset
                } else {
                    text(&chapter.blocks[b]).chars().count()
                };
                if start == end {
                    continue;
                }
                let map = mapping
                    .as_ref()
                    .ok_or("Cannot map highlight to original markup")?;
                if end > map.len() || start > end {
                    return Err("Highlight no longer matches original text".into());
                }
                let selected: Vec<usize> = map[start..end].iter().flatten().copied().collect();
                if let (Some(&first), Some(&last)) = (selected.first(), selected.last()) {
                    let color = match h.color {
                        crate::library::HighlightColor::Yellow => "#fff3cd",
                        crate::library::HighlightColor::Green => "#d4edda",
                        crate::library::HighlightColor::Blue => "#cce5ff",
                        crate::library::HighlightColor::Pink => "#f8d7da",
                    };
                    for atom in atoms.iter().take(last + 1).skip(first) {
                        let entry = edits.entry(atom.start).or_insert_with(|| {
                            (atom.end, crate::escape_html(&atom.ch.to_string()))
                        });
                        if !entry.1.is_empty() {
                            entry.1 = format!(
                                "<span style=\"background-color:{color}\">{}</span>",
                                entry.1
                            );
                        }
                    }
                }
            }
            if let Some(atom) = mapped
                .get(h.end_block)
                .and_then(Option::as_ref)
                .and_then(|m| m.iter().take(h.end_offset).rev().flatten().next())
                .map(|&a| &atoms[a])
            {
                for note in cfg.notes.iter().filter(|n| n.highlight_id == h.id) {
                    decorations.entry(atom.end).or_default().push(format!(
                        "<span class=\"reader-note\" style=\"font-size:0.85em\"> [{}]</span>",
                        crate::escape_html(&note.content)
                    ));
                }
            }
        }
        for (i, bm) in cfg
            .bookmarks
            .iter()
            .enumerate()
            .filter(|(_, b)| b.chapter == index)
        {
            let atom = mapped
                .get(bm.block)
                .and_then(Option::as_ref)
                .and_then(|m| m.iter().flatten().next())
                .ok_or("Cannot map bookmark to original markup")?;
            decorations
                .entry(atoms[*atom].start)
                .or_default()
                .push(format!("<a id=\"reader-bookmark-{i}\"></a>"));
        }
    }
    for (at, values) in decorations {
        let e = edits.entry(at).or_insert((at, String::new()));
        e.1 = format!("{}{}", values.join(""), e.1);
    }
    let mut result = String::new();
    let mut at = 0;
    for (start, (end, value)) in edits {
        if start < at {
            return Err("Conflicting publication edits".into());
        }
        result.push_str(&html[at..start]);
        result.push_str(&value);
        at = end;
    }
    result.push_str(&html[at..]);
    Ok(result)
}

pub(super) fn export(
    source: &str,
    output: &str,
    cfg: &BookConfig,
    mode: ExportMode,
) -> Result<(), String> {
    let source = Path::new(source);
    let output = Path::new(output);
    if output.exists() {
        return Err("Export destination already exists".into());
    }
    let parent = output.parent().ok_or("Invalid export destination")?;
    let stage = parent.join(format!(".reader-export-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stage)
            .map_err(|e| e.to_string())?;
        if mode == ExportMode::Original {
            let mut input = std::fs::File::open(source).map_err(|e| e.to_string())?;
            let mut file = file;
            std::io::copy(&mut input, &mut file).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
        } else {
            let book = EpubBook::open_lazy(source)?;
            let count = book.chapters.len();
            if (matches!(mode, ExportMode::WithCorrections | ExportMode::Full)
                && cfg
                    .corrections
                    .iter()
                    .any(|c| c.status.eq_ignore_ascii_case("accepted") && c.chapter >= count))
                || (matches!(mode, ExportMode::WithAnnotations | ExportMode::Full)
                    && (cfg.highlights.iter().any(|h| h.chapter >= count)
                        || cfg.bookmarks.iter().any(|b| b.chapter >= count)
                        || cfg
                            .notes
                            .iter()
                            .any(|n| !cfg.highlights.iter().any(|h| h.id == n.highlight_id))))
            {
                return Err(
                    "Export contains annotations that no longer match publication chapters".into(),
                );
            }

            let mut zip =
                zip::ZipArchive::new(std::fs::File::open(source).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            let mut changes = std::collections::HashMap::new();
            for i in 0..book.chapters.len() {
                let needed = (matches!(mode, ExportMode::WithCorrections | ExportMode::Full)
                    && cfg
                        .corrections
                        .iter()
                        .any(|c| c.chapter == i && c.status.eq_ignore_ascii_case("accepted")))
                    || (matches!(mode, ExportMode::WithAnnotations | ExportMode::Full)
                        && (cfg.highlights.iter().any(|h| h.chapter == i)
                            || cfg.bookmarks.iter().any(|h| h.chapter == i)));
                if !needed {
                    continue;
                }
                let ch = book.chapter(i)?;
                let href = ch
                    .source_href
                    .as_deref()
                    .ok_or("Cannot annotate an image-only chapter")?
                    .trim_start_matches('/');
                if changes.contains_key(href) {
                    return Err(
                        "Multiple annotated chapters map to the same publication resource".into(),
                    );
                }
                let mut member = zip
                    .by_name(href)
                    .map_err(|_| "Original chapter resource is missing")?;
                if member.size() > 32 * 1024 * 1024 {
                    return Err("Publication chapter is too large to rebuild safely".into());
                }
                let mut html = String::new();
                member
                    .read_to_string(&mut html)
                    .map_err(|_| "Publication chapter is not UTF-8")?;
                changes.insert(href.to_owned(), patch(&html, &ch, i, cfg, mode)?);
            }
            let mut writer = zip::ZipWriter::new(file);
            for i in 0..zip.len() {
                let member = zip.by_index(i).map_err(|e| e.to_string())?;
                if let Some(html) = changes.get(member.name()) {
                    let options = zip::write::SimpleFileOptions::default()
                        .compression_method(member.compression());
                    writer
                        .start_file(member.name(), options)
                        .map_err(|e| e.to_string())?;
                    writer
                        .write_all(html.as_bytes())
                        .map_err(|e| e.to_string())?;
                } else {
                    writer.raw_copy_file(member).map_err(|e| e.to_string())?;
                }
            }
            writer
                .finish()
                .map_err(|e| e.to_string())?
                .sync_all()
                .map_err(|e| e.to_string())?;
        }
        #[cfg(target_os = "android")]
        {
            // Android app SELinux forbids hard links, including app-owned files.
            // renameat2 keeps atomic publication and refuses an existing target.
            use std::os::unix::ffi::OsStrExt;
            let from = std::ffi::CString::new(stage.as_os_str().as_bytes())
                .map_err(|_| "Invalid export path")?;
            let to = std::ffi::CString::new(output.as_os_str().as_bytes())
                .map_err(|_| "Invalid export path")?;
            let status = unsafe {
                libc::syscall(
                    libc::SYS_renameat2,
                    libc::AT_FDCWD,
                    from.as_ptr(),
                    libc::AT_FDCWD,
                    to.as_ptr(),
                    libc::RENAME_NOREPLACE,
                )
            };
            if status != 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
        }
        #[cfg(not(target_os = "android"))]
        std::fs::hard_link(&stage, output).map_err(|e| e.to_string())?;
        Ok(())
    })();
    let _ = std::fs::remove_file(stage);
    result
}
