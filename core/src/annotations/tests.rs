use super::*;
use crate::library::Library;
pub(super) fn book() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let mut lib = Library::load_from(root).unwrap();
    let entry = lib
        .add_or_update_from_bytes(root, "A *book*".into(), b"fixture", 0, None)
        .unwrap();
    (dir, entry.id)
}
fn mark(block: usize) -> Annotation {
    Annotation::Bookmark(Bookmark {
        chapter: 1,
        block,
        created_at: 1,
    })
}
fn state(
    actor: &str,
    now: u64,
    value: Option<Annotation>,
    base: &AnnotationState,
    key: &str,
) -> AnnotationState {
    let mut next = base.clone();
    next.edit(key, value, base.entries.get(key), actor, now, 0)
        .unwrap();
    next
}
#[test]
fn causality_wins_over_device_clock_skew_and_merge_is_idempotent() {
    let a = state(
        "phone",
        u64::MAX,
        Some(mark(3)),
        &AnnotationState::default(),
        "b:1:3",
    );
    let b = state("pad", 1, None, &a, "b:1:3");
    let mut forward = a.clone();
    forward.merge(&b).unwrap();
    let mut reverse = b.clone();
    reverse.merge(&a).unwrap();
    assert_eq!(forward, reverse);
    assert_eq!(forward, b);
    assert!(!forward.merge(&b).unwrap());
    assert!(forward.entries["b:1:3"][0].value.is_none());
}
#[test]
fn concurrent_edits_are_preserved_delete_wins_and_explicit_readd_is_possible() {
    let initial = state(
        "phone",
        100,
        Some(mark(3)),
        &AnnotationState::default(),
        "b:1:3",
    );
    let removed = state("phone", 101, None, &initial, "b:1:3");
    let edited = state("pad", 10000, Some(mark(3)), &initial, "b:1:3");
    let mut merged = removed.clone();
    merged.merge(&edited).unwrap();
    assert_eq!(merged.entries["b:1:3"].len(), 2);
    let (dir, id) = book();
    let mut config = Library::load_book_config(dir.path().to_str().unwrap(), &id)
        .unwrap()
        .unwrap();
    merged.materialize(&mut config);
    assert!(config.bookmarks.is_empty());
    let added = state("phone", 1, Some(mark(3)), &merged, "b:1:3");
    added.materialize(&mut config);
    assert_eq!(config.bookmarks.len(), 1);
}
#[test]
fn independent_offline_marks_union_without_resurrection() {
    let a = state(
        "phone",
        1,
        Some(mark(1)),
        &AnnotationState::default(),
        "b:1:1",
    );
    let b = state(
        "pad",
        2,
        Some(mark(2)),
        &AnnotationState::default(),
        "b:1:2",
    );
    let mut merged = a.clone();
    merged.merge(&b).unwrap();
    assert_eq!(merged.entries.len(), 2);
    let mut deleted = state("phone", 3, None, &merged, "b:1:1");
    deleted.merge(&a).unwrap();
    deleted.merge(&b).unwrap();
    assert!(deleted.entries["b:1:1"][0].value.is_none());
}
#[test]
fn concurrent_note_contents_survive_and_user_edit_resolves_both() {
    let note = |text: &str| {
        Annotation::Note(Note {
            highlight_id: "highlight".into(),
            content: text.into(),
            created_at: 1,
            updated_at: 2,
        })
    };
    let original = state(
        "phone",
        1,
        Some(note("Original")),
        &AnnotationState::default(),
        "n:highlight",
    );
    let mut a = state("phone", 1000, Some(note("Phone")), &original, "n:highlight");
    let b = state("pad", 1, Some(note("Pad")), &original, "n:highlight");
    a.merge(&b).unwrap();
    assert_eq!(a.entries["n:highlight"].len(), 2);
    let resolved = state("phone", 2, Some(note("Combined")), &a, "n:highlight");
    assert_eq!(resolved.entries["n:highlight"].len(), 1);
    assert!(
        matches!(&resolved.entries["n:highlight"][0].value,Some(Annotation::Note(n)) if n.content=="Combined")
    );
}
#[test]
fn stale_ui_saves_preserve_unrelated_annotations_and_new_progress() {
    let (dir, id) = book();
    let root = dir.path().to_str().unwrap();
    let mut a = Library::load_book_config(root, &id).unwrap().unwrap();
    let mut b = a.clone();
    let mut progress = a.clone();
    a.bookmarks.push(Bookmark {
        chapter: 1,
        block: 1,
        created_at: 1,
    });
    a.save(root).unwrap();
    b.bookmarks.push(Bookmark {
        chapter: 1,
        block: 2,
        created_at: 1,
    });
    b.save(root).unwrap();
    progress.last_chapter = 4;
    progress.save(root).unwrap();
    a.settings.theme_override = Some("night".into());
    a.save(root).unwrap();
    let loaded = Library::load_book_config(root, &id).unwrap().unwrap();
    assert_eq!(loaded.bookmarks.len(), 2);
    assert_eq!(loaded.last_chapter, 4);
    assert_eq!(loaded.settings.theme_override.as_deref(), Some("night"));
    let state = loaded.annotation_state.clone();
    a.save(root).unwrap();
    assert_eq!(
        Library::load_book_config(root, &id)
            .unwrap()
            .unwrap()
            .annotation_state,
        state
    );
    a.settings.theme_override = None;
    a.save(root).unwrap();
    assert_eq!(
        Library::load_book_config(root, &id)
            .unwrap()
            .unwrap()
            .settings
            .theme_override,
        None
    );
}
#[test]
fn failed_save_does_not_publish_revision_and_removed_config_cannot_be_recreated() {
    let (dir, id) = book();
    let root = dir.path().to_str().unwrap();
    let mut config = Library::load_book_config(root, &id).unwrap().unwrap();
    let before = config.annotation_state.clone();
    let baseline = config.persisted.clone();
    config.bookmarks.push(Bookmark {
        chapter: 1,
        block: 1,
        created_at: 1,
    });
    std::fs::remove_file(config_path(root, &id).unwrap()).unwrap();
    assert!(config.save(root).is_err());
    assert_eq!(config.annotation_state, before);
    assert_eq!(config.persisted, baseline);
    assert!(!config_path(root, &id).unwrap().exists());
}
#[test]
fn remote_apply_preserves_progress_does_not_echo_and_survives_restore() {
    let (dir, id) = book();
    let root = dir.path().to_str().unwrap();
    let mut config = Library::load_book_config(root, &id).unwrap().unwrap();
    config.last_chapter = 9;
    config.save(root).unwrap();
    let remote = state(
        "pad",
        1,
        Some(mark(2)),
        &AnnotationState::default(),
        "b:1:2",
    );
    assert!(apply_remote(root, &id, &remote).unwrap());
    assert!(!apply_remote(root, &id, &remote).unwrap());
    let mut loaded = Library::load_book_config(root, &id).unwrap().unwrap();
    assert_eq!(loaded.last_chapter, 9);
    assert_eq!(loaded.bookmarks.len(), 1);
    loaded.save(root).unwrap();
    assert_eq!(loaded.annotation_state, remote);
    let mut lib = Library::load_from(root).unwrap();
    let path = lib.books[0].path.clone();
    crate::library::archive::ArchiveStore::archive(root, &mut lib, &[path]).unwrap();
    let hash = loaded.file_hash.clone().unwrap();
    let deleted = state("pad", 0, None, &remote, "b:1:2");
    apply_remote(root, &id, &deleted).unwrap();
    crate::library::archive::ArchiveStore::change(
        root,
        &hash,
        crate::library::archive::BookState::Active,
    )
    .unwrap();
    let restored = Library::load_book_config(root, &id).unwrap().unwrap();
    assert!(restored.bookmarks.is_empty());
}
#[test]
fn untrusted_state_rejects_clock_collisions_future_schema_and_paths_atomically() {
    let original = state(
        "pad",
        1,
        Some(mark(2)),
        &AnnotationState::default(),
        "b:1:2",
    );
    let mut a = original.clone();
    let mut malicious = original.clone();
    malicious.entries.get_mut("b:1:2").unwrap()[0].value = None;
    assert!(a.merge(&malicious).is_err());
    assert_eq!(a, original);
    malicious = original.clone();
    malicious.schema_version = 2;
    assert!(a.merge(&malicious).is_err());
    assert_eq!(a, original);
    malicious = original.clone();
    let v = malicious.entries.remove("b:1:2").unwrap();
    malicious.entries.insert("n:../../x".into(), v);
    assert!(a.merge(&malicious).is_err());
    let (dir, _) = book();
    assert!(load_config(dir.path().to_str().unwrap(), "../private").is_err());
}

#[test]
fn restoring_old_book_snapshot_never_reuses_a_published_local_revision() {
    let (dir, id) = book();
    let root = dir.path().to_str().unwrap();
    let mut config = load_config(root, &id).unwrap().unwrap();
    config.bookmarks.push(Bookmark {
        chapter: 1,
        block: 1,
        created_at: 1,
    });
    config.save(root).unwrap();
    let old = std::fs::read(config_path(root, &id).unwrap()).unwrap();
    config.bookmarks[0].created_at = 2;
    config.save(root).unwrap();
    let published = config.annotation_state.clone();
    crate::storage::replace(&config_path(root, &id).unwrap(), &old).unwrap();
    let mut restored = load_config(root, &id).unwrap().unwrap();
    restored.bookmarks[0].created_at = 3;
    restored.save(root).unwrap();
    let mut merged = published;
    merged.merge(&restored.annotation_state).unwrap();
    assert_eq!(merged, restored.annotation_state);
    assert!(merged.entries["b:1:1"][0].clock.values().any(|c| *c >= 3));
}
#[test]
fn damaged_device_counter_rotates_identity_and_preserves_recovery_evidence() {
    let (dir, id) = book();
    let root = dir.path().to_str().unwrap();
    let mut config = load_config(root, &id).unwrap().unwrap();
    config.bookmarks.push(Bookmark {
        chapter: 1,
        block: 1,
        created_at: 1,
    });
    config.save(root).unwrap();
    config.bookmarks[0].created_at = 2;
    config.save(root).unwrap();
    let previous = config.annotation_state.entries["b:1:1"][0].source.clone();
    std::fs::write(
        Path::new(root).join("annotation-device.json"),
        b"broken identity",
    )
    .unwrap();
    config.bookmarks[0].created_at = 3;
    config.save(root).unwrap();
    assert_ne!(previous, config.annotation_state.entries["b:1:1"][0].source);
    assert!(std::fs::read_dir(root)
        .unwrap()
        .flatten()
        .any(|e| e.file_name().to_string_lossy().contains("corrupt")));
}

#[test]
fn stale_note_editor_keeps_new_remote_text_and_a_later_edit_resolves_versions() {
    let (dir, id) = book();
    let root = dir.path().to_str().unwrap();
    let mut config = load_config(root, &id).unwrap().unwrap();
    config.highlights.push(Highlight {
        id: "h".into(),
        chapter: 1,
        start_block: 0,
        start_offset: 0,
        end_block: 0,
        end_offset: 3,
        color: Default::default(),
        created_at: 1,
    });
    config.save(root).unwrap();
    let first = save_note(root, &id, "h", "Original", Some(None)).unwrap();
    let remote_note = Annotation::Note(Note {
        highlight_id: "h".into(),
        content: "Remote edit".into(),
        created_at: 1,
        updated_at: 1,
    });
    let remote = state("pad", 1, Some(remote_note), &first.annotation_state, "n:h");
    apply_remote(root, &id, &remote).unwrap();
    let concurrent = save_note(root, &id, "h", "Local draft", Some(Some("Original"))).unwrap();
    assert_eq!(concurrent.annotation_state.entries["n:h"].len(), 2);
    assert_eq!(
        view(&concurrent)["concurrent_notes"],
        serde_json::json!(["h"])
    );
    let combined = &concurrent.notes[0].content;
    assert!(combined.contains("Local draft"));
    assert!(combined.contains("Remote edit"));
    let resolved = save_note(
        root,
        &id,
        "h",
        "Combined deliberately",
        Some(Some(combined)),
    )
    .unwrap();
    assert_eq!(resolved.annotation_state.entries["n:h"].len(), 1);
    assert_eq!(resolved.notes[0].content, "Combined deliberately");
    assert_eq!(view(&resolved)["concurrent_notes"], serde_json::json!([]));
    let deleted = save_note(root, &id, "h", "", Some(Some("Combined deliberately"))).unwrap();
    assert!(deleted.notes.is_empty());
    let mut after = deleted.annotation_state.clone();
    after.merge(&remote).unwrap();
    assert_eq!(after, deleted.annotation_state);
}

#[test]
fn annotation_ids_are_unique_even_within_the_same_second_and_reused_position() {
    let ids: std::collections::BTreeSet<_> = (0..1000).map(|_| new_highlight_id()).collect();
    assert_eq!(ids.len(), 1000);
    assert!(ids.iter().all(|id| uuid::Uuid::parse_str(id).is_ok()));
}

#[test]
fn stale_editor_after_note_deletion_fails_without_resurrecting_the_note() {
    let (dir, id) = book();
    let root = dir.path().to_str().unwrap();
    let mut config = load_config(root, &id).unwrap().unwrap();
    config.highlights.push(Highlight {
        id: "h".into(),
        chapter: 0,
        start_block: 0,
        start_offset: 0,
        end_block: 0,
        end_offset: 1,
        color: Default::default(),
        created_at: 1,
    });
    config.save(root).unwrap();
    save_note(root, &id, "h", "Original", Some(None)).unwrap();
    let deleted = save_note(root, &id, "h", "", Some(Some("Original"))).unwrap();
    assert!(save_note(root, &id, "h", "Unseen draft", Some(Some("Original"))).is_err());
    let stored = load_config(root, &id).unwrap().unwrap();
    assert!(stored.notes.is_empty());
    assert_eq!(stored.annotation_state, deleted.annotation_state);
}

#[test]
fn legacy_colliding_highlight_ids_upgrade_deterministically_without_dropping_content() {
    let (dir, id) = book();
    let root = dir.path().to_str().unwrap();
    let mut config = load_config(root, &id).unwrap().unwrap();
    let first = Highlight {
        id: "same-second-0".into(),
        chapter: 1,
        start_block: 0,
        start_offset: 0,
        end_block: 0,
        end_offset: 1,
        color: Default::default(),
        created_at: 1,
    };
    let mut second = first.clone();
    second.start_offset = 1;
    second.end_offset = 3;
    config.highlights = vec![first, second];
    config.notes = vec![
        Note {
            highlight_id: "same-second-0".into(),
            content: "First note".into(),
            created_at: 1,
            updated_at: 1,
        },
        Note {
            highlight_id: "same-second-0".into(),
            content: "Second note".into(),
            created_at: 2,
            updated_at: 2,
        },
    ];
    // Emulate an older client; normal new-client saves reject duplicate identities.
    crate::storage::save(&config_path(root, &id).unwrap(), &config).unwrap();
    let mut a = load_config(root, &id).unwrap().unwrap();
    let b = load_config(root, &id).unwrap().unwrap();
    assert_eq!(a.highlights.len(), 2);
    assert_ne!(a.highlights[0].id, a.highlights[1].id);
    assert_eq!(a.annotation_state, b.annotation_state);
    assert!(
        a.notes[0].content.contains("First note") && a.notes[0].content.contains("Second note")
    );
    a.save(root).unwrap();
    assert_eq!(
        load_config(root, &id).unwrap().unwrap().annotation_state,
        a.annotation_state
    );
}
