use super::*;
use crate::{
    annotations::AnnotationState,
    library::{Bookmark, Library},
};
const PREFIX: &str = "/repos/tester/EpubReaderInfo/contents";
fn fixture() -> (tempfile::TempDir, String, String, AnnotationState) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let mut lib = Library::load_from(root).unwrap();
    let entry = lib
        .add_or_update_from_bytes(root, "Annotated".into(), b"annotated", 0, None)
        .unwrap();
    let mut config = Library::load_book_config(root, &entry.id).unwrap().unwrap();
    config.bookmarks.push(Bookmark {
        chapter: 1,
        block: 1,
        created_at: 1,
    });
    config.save(root).unwrap();
    (
        dir,
        entry.id,
        config.file_hash.unwrap(),
        config.annotation_state,
    )
}
fn remote() -> AnnotationState {
    serde_json::from_value(json!({"schema_version":1,"entries":{"b:1:2":[{"clock":{"remote":1},"source":"remote","edited_at":1,"value":{"kind":"bookmark","data":{"chapter":1,"block":2,"created_at":1}}}]}})).unwrap()
}
fn index(device: &str, hash: &str, object: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({"schema_version":1,"device":device,"books":{hash:object}})).unwrap()
}
fn listing(devices: &[&str]) -> Step {
    step(
        "GET",
        &format!("{PREFIX}/annotation-devices-v1"),
        200,
        json!(devices
            .iter()
            .map(|d| json!({"name":format!("{d}.json"),"type":"file"}))
            .collect::<Vec<_>>()),
    )
}
#[test]
fn annotations_publish_after_objects_and_repeated_sync_is_read_only_without_echo() {
    let (dir, id, hash, local) = fixture();
    let root = dir.path().to_str().unwrap();
    let remote = remote();
    let remote_bytes = serde_json::to_vec(&remote).unwrap();
    let object = digest(&remote_bytes);
    let mut merged = local.clone();
    merged.merge(&remote).unwrap();
    let merged_object = digest(&serde_json::to_vec(&merged).unwrap());
    let own_path = format!("{PREFIX}/annotation-devices-v1/local.json");
    let remote_path = format!("{PREFIX}/annotation-devices-v1/remote.json");
    let upload = format!("{PREFIX}/annotation-books-v1/local/{hash}/{merged_object}.json");
    let steps = vec![
        listing(&["remote"]),
        step("GET", &own_path, 404, json!({})),
        raw(&remote_path, index("remote", &hash, &object)),
        raw(
            &format!("{PREFIX}/annotation-books-v1/remote/{hash}/{object}.json"),
            remote_bytes,
        ),
        step("PUT", &upload, 201, json!({})),
        step("GET", &own_path, 404, json!({})),
        step("GET", &own_path, 404, json!({})),
        step("PUT", &own_path, 201, json!({})),
    ];
    let (api, server) = mock(steps);
    super::super::annotations::sync(&api, root, "local").unwrap();
    server.join().unwrap();
    assert_eq!(
        Library::load_book_config(root, &id)
            .unwrap()
            .unwrap()
            .annotation_state,
        local,
        "worker must stage before UI generation/account checks"
    );
    assert_eq!(
        apply_cached_annotations(root, "tester").unwrap(),
        vec![id.clone()]
    );
    let saved = Library::load_book_config(root, &id).unwrap().unwrap();
    assert_eq!(saved.annotation_state, merged);
    assert_eq!(saved.bookmarks.len(), 2);
    let steps = vec![
        listing(&["local", "remote"]),
        raw(&own_path, index("local", &hash, &merged_object)),
        raw(&remote_path, index("remote", &hash, &object)),
    ];
    let (api, server) = mock(steps);
    super::super::annotations::sync(&api, root, "local").unwrap();
    server.join().unwrap();
    assert!(apply_cached_annotations(root, "tester").unwrap().is_empty());
    assert_eq!(
        Library::load_book_config(root, &id)
            .unwrap()
            .unwrap()
            .annotation_state,
        merged
    );
}
#[test]
fn invalid_remote_object_never_overwrites_saved_annotations() {
    let (dir, id, hash, local) = fixture();
    let root = dir.path().to_str().unwrap();
    let object = "a".repeat(64);
    let steps = vec![
        listing(&["remote"]),
        step(
            "GET",
            &format!("{PREFIX}/annotation-devices-v1/local.json"),
            404,
            json!({}),
        ),
        raw(
            &format!("{PREFIX}/annotation-devices-v1/remote.json"),
            index("remote", &hash, &object),
        ),
        raw(
            &format!("{PREFIX}/annotation-books-v1/remote/{hash}/{object}.json"),
            b"invalid".to_vec(),
        ),
    ];
    let (api, server) = mock(steps);
    assert!(super::super::annotations::sync(&api, root, "local")
        .unwrap_err()
        .contains("integrity"));
    server.join().unwrap();
    assert_eq!(
        Library::load_book_config(root, &id)
            .unwrap()
            .unwrap()
            .annotation_state,
        local
    );
}
#[test]
fn publication_failure_retains_local_commit_and_never_publishes_a_partial_index() {
    let (dir, id, hash, local) = fixture();
    let root = dir.path().to_str().unwrap();
    let object = digest(&serde_json::to_vec(&local).unwrap());
    let steps = vec![
        step(
            "GET",
            &format!("{PREFIX}/annotation-devices-v1"),
            404,
            json!({}),
        ),
        step(
            "GET",
            &format!("{PREFIX}/annotation-devices-v1/local.json"),
            404,
            json!({}),
        ),
        step(
            "PUT",
            &format!("{PREFIX}/annotation-books-v1/local/{hash}/{object}.json"),
            500,
            json!({}),
        ),
    ];
    let (api, server) = mock(steps);
    assert!(super::super::annotations::sync(&api, root, "local").is_err());
    server.join().unwrap();
    assert_eq!(
        Library::load_book_config(root, &id)
            .unwrap()
            .unwrap()
            .annotation_state,
        local
    );
    assert!(apply_cached_annotations(root, "tester").unwrap().is_empty());
}
