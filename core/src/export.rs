//! Logic for exporting books, annotations, and reading data.
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

mod publication;

pub fn export_book(
    source_epub: &str,
    output_path: &str,
    config: &BookConfig,
    mode: ExportMode,
) -> Result<(), String> {
    publication::export(source_epub, output_path, config, mode)
}

/// Export a selected set with the same modes on all platforms. Never overwrites.
pub fn export_selection(
    root: &str,
    books: &[crate::library::BookEntry],
    directory: &std::path::Path,
    mode: ExportMode,
) -> Result<Vec<String>, String> {
    if mode == ExportMode::Original {
        return export_originals(books, directory);
    }
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let mut paths = Vec::new();
    for book in books {
        let config = crate::library::Library::load_book_config(root, &book.id)?
            .ok_or("Book configuration is missing")?;
        let name = book_filename(&book.title);
        let mut path = directory.join(&name);
        let mut suffix = 2;
        while path.exists() {
            path = directory.join(format!(
                "{} ({suffix}).epub",
                name.trim_end_matches(".epub")
            ));
            suffix += 1;
        }
        export_book(&book.path, &path.to_string_lossy(), &config, mode)?;
        paths.push(path.to_string_lossy().into_owned());
    }
    Ok(paths)
}
impl ExportMode {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "original" => Ok(Self::Original),
            "corrections" => Ok(Self::WithCorrections),
            "annotations" => Ok(Self::WithAnnotations),
            "full" => Ok(Self::Full),
            _ => Err("Invalid export mode".into()),
        }
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
