use super::*;
use crate::library::{
    archive::{ArchiveStore, BookState},
    Bookmark, Highlight, HighlightColor, Library, Note,
};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

const EPUB: &[u8] = include_bytes!("../../../ios/Tests/fixtures/native-reading-modes.epub");
#[test]
fn classifications_remap_book_identity_and_merge_idempotently() {
    use crate::library::categories::command;
    use serde_json::json;
    let (source, id) = populated();
    let root = source.path().to_str().unwrap();
    let created = command(root, &json!({"action":"create","name":"Fiction"})).unwrap();
    let category = created["categories"][0]["id"].as_str().unwrap();
    command(root, &json!({"action":"assign","id":category,"books":[id]})).unwrap();
    command(root, &json!({"action":"create","name":"Empty category"})).unwrap();
    let output = tempfile::tempdir().unwrap();
    let path = output.path().join("categories.readerbackup");
    export(root, &path).unwrap();
    let target = tempfile::tempdir().unwrap();
    let destination = target.path().to_str().unwrap();
    // Existing identical content has a different installation-local UUID.
    let local = Library::default()
        .add_or_update_from_bytes(destination, "Local".into(), EPUB, 0, None)
        .unwrap();
    let existing = command(destination, &json!({"action":"create","name":"fiction"})).unwrap();
    let local_category = existing["categories"][0]["id"].as_str().unwrap();
    for _ in 0..2 {
        restore(destination, &path).unwrap();
    }
    let library = Library::load_from(destination).unwrap();
    assert_eq!(library.books.len(), 1);
    assert_eq!(library.classification.categories.len(), 2);
    assert!(library
        .classification
        .matches(&local.id, Some(local_category)));
    assert!(!library.classification.assignments.contains_key(&id));
    assert!(std::path::Path::new(&local.path).exists());
}

#[test]
fn legacy_backup_without_categories_keeps_destination_classification() {
    use crate::library::categories::command;
    use serde_json::json;
    let (source, _) = populated();
    let output = tempfile::tempdir().unwrap();
    let path = output.path().join("legacy.readerbackup");
    export(source.path().to_str().unwrap(), &path).unwrap();
    let target = tempfile::tempdir().unwrap();
    let root = target.path().to_str().unwrap();
    let local = Library::default()
        .add_or_update_from_bytes(root, "Local".into(), EPUB, 0, None)
        .unwrap();
    let category = command(root, &json!({"action":"create","name":"Kept"})).unwrap()["categories"]
        [0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        root,
        &json!({"action":"assign","id":category,"books":[local.id]}),
    )
    .unwrap();
    restore(root, &path).unwrap();
    assert!(Library::load_from(root)
        .unwrap()
        .classification
        .matches(&local.id, Some(&category)));
}

#[test]
fn a_foreign_uuid_collision_never_overwrites_the_existing_original() {
    let (source, foreign_id) = populated();
    let output = tempfile::tempdir().unwrap();
    let package_path = output.path().join("library.readerbackup");
    export(source.path().to_str().unwrap(), &package_path).unwrap();
    let target = tempfile::tempdir().unwrap();
    let root = target.path().to_str().unwrap();
    let books = target.path().join("books");
    fs::create_dir(&books).unwrap();
    let file = books.join(format!("{foreign_id}.epub"));
    let mut other = EPUB.to_vec();
    other.extend_from_slice(b"different original");
    fs::write(&file, &other).unwrap();
    // The owned UUID path is legitimate local state, but its content differs.
    let mut library = Library::load_from(root).unwrap();
    let old = library
        .add_or_update(
            root,
            "Existing original".into(),
            file.to_string_lossy().into(),
            0,
            None,
        )
        .unwrap();
    assert_eq!(old.id, foreign_id);
    restore(root, &package_path).unwrap();
    let restored = Library::load_from(root).unwrap();
    assert_eq!(restored.books.len(), 2);
    assert_eq!(fs::read(&file).unwrap(), other);
    assert!(restored
        .books
        .iter()
        .any(|book| book.id != foreign_id && fs::read(&book.path).unwrap() == EPUB));
}
#[test]
fn an_oversized_local_config_cannot_allocate_a_whole_backup_or_replace_an_existing_export() {
    let (source, id) = populated();
    let path = source.path().join("books").join(format!("{id}.json"));
    fs::OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_len(16 * 1024 * 1024 + 1)
        .unwrap();
    let output = tempfile::tempdir().unwrap();
    let destination = output.path().join("kept.readerbackup");
    fs::write(&destination, b"keep existing export").unwrap();
    assert!(export(source.path().to_str().unwrap(), &destination)
        .unwrap_err()
        .contains("exceeds backup limit"));
    assert_eq!(fs::read(destination).unwrap(), b"keep existing export");
}
#[test]
fn inspection_fingerprint_prevents_replacing_a_confirmed_package() {
    let (source, _) = populated();
    let output = tempfile::tempdir().unwrap();
    let path = output.path().join("library.readerbackup");
    export(source.path().to_str().unwrap(), &path).unwrap();
    let verified = inspect(&path).unwrap();
    let changed = package(
        &path,
        |m| m.books[0].config.title = "Changed after confirmation".into(),
        None,
    );
    let target = tempfile::tempdir().unwrap();
    let request = serde_json::json!({"action":"restore","path":changed.path().join("test.readerbackup"),"expected_manifest":verified.manifest_sha256});
    assert!(command(target.path().to_str().unwrap(), &request)
        .unwrap_err()
        .contains("changed after verification"));
    assert!(!target.path().join("library.json").exists());
}
#[test]
fn reading_activity_is_portable_and_repeated_restore_does_not_double_legacy_time() {
    let (source, id) = populated();
    let root = source.path().to_str().unwrap();
    let mut cfg = Library::load_book_config(root, &id).unwrap().unwrap();
    cfg.reading_stats = Some(crate::library::ReadingStats {
        total_seconds: 123,
        sessions: vec![],
    });
    cfg.save(root).unwrap();
    crate::reading_activity::checkpoint(
        root,
        "phone",
        &id,
        "session-1",
        60,
        1_700_000_000,
        0,
        false,
    )
    .unwrap();
    let output = tempfile::tempdir().unwrap();
    let path = output.path().join("library.readerbackup");
    export(root, &path).unwrap();
    let target = tempfile::tempdir().unwrap();
    let root = target.path().to_str().unwrap();
    for _ in 0..2 {
        restore(root, &path).unwrap();
        let report = crate::reading_activity::report(root, "pad", "2023-11-14", None).unwrap();
        assert_eq!(report.reading, 60);
        assert_eq!(report.undated, 123);
        assert_eq!(report.total, 183);
    }
}
fn populated() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let mut library = Library::load_from(root).unwrap();
    let book = library
        .add_or_update_from_bytes(root, "中文 Book".into(), EPUB, 1, Some("真实章名".into()))
        .unwrap();
    let mut config = Library::load_book_config(root, &book.id).unwrap().unwrap();
    config.bookmarks.push(Bookmark {
        chapter: 1,
        block: 2,
        created_at: 3,
    });
    config.highlights.push(Highlight {
        id: crate::annotations::new_highlight_id(),
        chapter: 1,
        start_block: 2,
        start_offset: 0,
        end_block: 2,
        end_offset: 3,
        color: HighlightColor::Pink,
        created_at: 3,
    });
    config.notes.push(Note {
        highlight_id: config.highlights[0].id.clone(),
        content: "中文😀 portable note".into(),
        created_at: 3,
        updated_at: 4,
    });
    config.last_block = 2;
    config.last_char_offset = 3;
    config.save(root).unwrap();
    fs::write(
        dir.path().join("github-token.json"),
        b"secret token never included",
    )
    .unwrap();
    fs::write(
        dir.path().join("pairing-key.pem"),
        b"private key never included",
    )
    .unwrap();
    (dir, book.id)
}
fn package(
    source: &Path,
    change: impl FnOnce(&mut Manifest),
    extra: Option<&str>,
) -> tempfile::TempDir {
    let out = tempfile::tempdir().unwrap();
    let mut zip = zip::ZipArchive::new(fs::File::open(source).unwrap()).unwrap();
    let mut manifest = package::read_manifest(&mut zip).unwrap();
    change(&mut manifest);
    let mut writer =
        zip::ZipWriter::new(fs::File::create(out.path().join("test.readerbackup")).unwrap());
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    writer.start_file("manifest.json", options).unwrap();
    writer
        .write_all(&serde_json::to_vec(&manifest).unwrap())
        .unwrap();
    for index in 0..zip.len() {
        let mut file = zip.by_index(index).unwrap();
        if file.name() == "manifest.json" {
            continue;
        }
        writer.start_file(file.name(), options).unwrap();
        std::io::copy(&mut file, &mut writer).unwrap();
    }
    if let Some(name) = extra {
        writer.start_file(name, options).unwrap();
        writer.write_all(b"untrusted").unwrap();
    }
    writer.finish().unwrap();
    out
}
#[test]
fn round_trip_keeps_content_and_annotations_without_paths_accounts_or_private_keys() {
    let (source, old_id) = populated();
    let output = tempfile::tempdir().unwrap();
    let archive = output.path().join("library.readerbackup");
    let summary = export(source.path().to_str().unwrap(), &archive).unwrap();
    assert_eq!(summary.books, 1);
    let mut zip = zip::ZipArchive::new(fs::File::open(&archive).unwrap()).unwrap();
    assert_eq!(zip.len(), 2);
    let mut json = String::new();
    zip.by_name("manifest.json")
        .unwrap()
        .read_to_string(&mut json)
        .unwrap();
    assert!(!json.contains(source.path().to_str().unwrap()));
    assert!(!json.contains("secret token"));
    assert!(!json.contains("private key"));
    let target = tempfile::tempdir().unwrap();
    let root = target.path().to_str().unwrap();
    restore(root, &archive).unwrap();
    let library = Library::load_from(root).unwrap();
    assert_eq!(library.books.len(), 1);
    let book = &library.books[0];
    assert_ne!(book.id, old_id);
    assert_eq!(fs::read(&book.path).unwrap(), EPUB);
    let config = Library::load_book_config(root, &book.id).unwrap().unwrap();
    assert_eq!(config.notes[0].content, "中文😀 portable note");
    assert_eq!(config.last_block, 2);
    assert_eq!(config.last_char_offset, 3);
    assert_eq!(config.bookmarks.len(), 1);
    assert_eq!(config.highlights.len(), 1);
    assert_eq!(config.last_chapter_title.as_deref(), Some("真实章名"));
    assert!(!target.path().join("github-token.json").exists());
    assert!(!target.path().join("annotation-device.json").exists());
    let before = fs::read(
        target
            .path()
            .join("books")
            .join(format!("{}.json", book.id)),
    )
    .unwrap();
    restore(root, &archive).unwrap();
    let again = Library::load_from(root).unwrap();
    assert_eq!(again.books.len(), 1);
    assert_eq!(again.books[0].id, book.id);
    assert_eq!(
        before,
        fs::read(
            target
                .path()
                .join("books")
                .join(format!("{}.json", book.id))
        )
        .unwrap()
    );
}
#[test]
fn reopening_an_old_position_does_not_outrank_a_newer_position_edit_during_restore() {
    let (source, id) = populated();
    let source_root = source.path().to_str().unwrap();
    let mut library = Library::load_from(source_root).unwrap();
    let path = library.books[0].path.clone();
    assert!(library
        .apply_position_at(source_root, &path, 1, None, 2, 3, 1000)
        .unwrap());
    library.set_last_opened(source_root, &path, 5000).unwrap();
    let output = tempfile::tempdir().unwrap();
    let first = output.path().join("first.readerbackup");
    export(source_root, &first).unwrap();
    let target = tempfile::tempdir().unwrap();
    let root = target.path().to_str().unwrap();
    restore(root, &first).unwrap();
    let mut local = Library::load_from(root).unwrap();
    let local_entry = local.books[0].clone();
    assert!(local
        .apply_position_at(root, &local_entry.path, 2, None, 3, 4, 2000)
        .unwrap());
    local
        .set_last_opened(root, &local_entry.path, 9000)
        .unwrap();
    library.set_last_opened(source_root, &path, 10000).unwrap();
    let reopened = output.path().join("reopened.readerbackup");
    export(source_root, &reopened).unwrap();
    restore(root, &reopened).unwrap();
    let restored = Library::load_book_config(root, &local_entry.id)
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            restored.last_chapter,
            restored.position_modified_ms,
            restored.last_opened
        ),
        (2, 2000, 10000)
    );
    assert!(library
        .apply_position_at(source_root, &path, 3, None, 3, 5, 3000)
        .unwrap());
    let newer = output.path().join("newer.readerbackup");
    export(source_root, &newer).unwrap();
    restore(root, &newer).unwrap();
    let restored = Library::load_book_config(root, &local_entry.id)
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            restored.last_chapter,
            restored.position_modified_ms,
            restored.last_opened
        ),
        (3, 3000, 10000)
    );
    assert!(Library::load_book_config(source_root, &id)
        .unwrap()
        .is_some());
}

#[test]
fn restore_does_not_rewind_newer_progress_or_revive_deleted_annotations() {
    let (source, id) = populated();
    let output = tempfile::tempdir().unwrap();
    let archive = output.path().join("library.readerbackup");
    let root = source.path().to_str().unwrap();
    export(root, &archive).unwrap();
    let mut config = Library::load_book_config(root, &id).unwrap().unwrap();
    config.last_chapter = 2;
    config.last_block = 7;
    config.last_char_offset = 9;
    config.last_opened += 1000;
    config.bookmarks.clear();
    config.save(root).unwrap();
    restore(root, &archive).unwrap();
    let current = Library::load_book_config(root, &id).unwrap().unwrap();
    assert_eq!(
        (
            current.last_chapter,
            current.last_block,
            current.last_char_offset
        ),
        (2, 7, 9)
    );
    assert!(current.bookmarks.is_empty());
    assert_eq!(Library::load_from(root).unwrap().books[0].last_chapter, 2);
}
#[test]
fn archives_round_trip_and_a_newer_local_delete_is_not_undone() {
    let (source, id) = populated();
    let root = source.path().to_str().unwrap();
    let mut library = Library::load_from(root).unwrap();
    let selected = vec![library.books[0].path.clone()];
    ArchiveStore::archive(root, &mut library, &selected).unwrap();
    let output = tempfile::tempdir().unwrap();
    let archive = output.path().join("library.readerbackup");
    assert_eq!(export(root, &archive).unwrap().archived, 1);
    let target = tempfile::tempdir().unwrap();
    let target_root = target.path().to_str().unwrap();
    restore(target_root, &archive).unwrap();
    assert!(Library::load_from(target_root).unwrap().books.is_empty());
    assert_eq!(ArchiveStore::load(target_root).unwrap().list().len(), 1);
    let hash = Library::load_book_config(root, &id)
        .unwrap()
        .unwrap()
        .file_hash
        .unwrap();
    ArchiveStore::change(root, &hash, BookState::Deleted).unwrap();
    restore(root, &archive).unwrap();
    assert!(Library::load_from(root).unwrap().books.is_empty());
    assert_eq!(
        ArchiveStore::load(root).unwrap().records[&hash]
            .lifecycle
            .state,
        BookState::Deleted
    );
}
#[test]
fn malformed_future_paths_checksums_and_duplicate_identities_leave_live_catalog_unchanged() {
    let (source, _) = populated();
    let output = tempfile::tempdir().unwrap();
    let archive = output.path().join("library.readerbackup");
    export(source.path().to_str().unwrap(), &archive).unwrap();
    let (target, _) = populated();
    let root = target.path().to_str().unwrap();
    let before = fs::read(target.path().join("library.json")).unwrap();
    let cases: Vec<tempfile::TempDir> = vec![
        package(&archive, |m| m.schema_version = 2, None),
        package(
            &archive,
            |m| m.books[0].config.epub_path = "../../escape".into(),
            None,
        ),
        package(&archive, |m| m.books[0].hash = "0".repeat(64), None),
        package(
            &archive,
            |m| {
                let value = serde_json::to_value(&m.books[0]).unwrap();
                m.books.push(serde_json::from_value(value).unwrap());
            },
            None,
        ),
        package(&archive, |_| {}, Some("../escape")),
    ];
    for malformed in cases {
        let path = malformed.path().join("test.readerbackup");
        assert!(restore(root, &path).is_err());
        assert_eq!(
            before,
            fs::read(target.path().join("library.json")).unwrap()
        );
        assert!(!target.path().join(".library-restore").exists());
    }
    let truncated = output.path().join("truncated");
    let bytes = fs::read(&archive).unwrap();
    fs::write(&truncated, &bytes[..bytes.len() / 2]).unwrap();
    assert!(restore(root, &truncated).is_err());
    assert_eq!(
        before,
        fs::read(target.path().join("library.json")).unwrap()
    );
}
#[cfg(unix)]
#[test]
fn a_symlinked_books_directory_cannot_redirect_restore_writes() {
    let (source, _) = populated();
    let output = tempfile::tempdir().unwrap();
    let archive = output.path().join("library.readerbackup");
    export(source.path().to_str().unwrap(), &archive).unwrap();
    let target = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), target.path().join("books")).unwrap();
    assert!(restore(target.path().to_str().unwrap(), &archive).is_err());
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    assert!(!target.path().join("library.json").exists());
}
