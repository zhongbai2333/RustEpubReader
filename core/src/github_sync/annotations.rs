//! Annotation protocol is separate from v1 progress snapshots, so old clients cannot erase it.
//! Immutable, hash-verified book objects are published before an atomically replaced device index.
use super::*;
use crate::annotations::AnnotationState;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const INDEX_LIMIT: usize = 5000;
const CACHE_LIMIT: u64 = 64 * 1024 * 1024;
static ANNOTATION_SYNC_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
#[derive(Serialize, Deserialize)]
struct Cache {
    schema_version: u32,
    #[serde(default)]
    books: BTreeMap<String, AnnotationState>,
    #[serde(default)]
    seen: BTreeSet<String>,
}
impl Default for Cache {
    fn default() -> Self {
        Self {
            schema_version: 1,
            books: BTreeMap::new(),
            seen: BTreeSet::new(),
        }
    }
}
#[derive(Serialize, Deserialize, Clone)]
struct Index {
    schema_version: u32,
    device: String,
    books: BTreeMap<String, String>,
}
impl Index {
    fn validate(&self, device: &str) -> Result<(), String> {
        if self.schema_version != 1
            || self.device != device
            || !crate::cloud_sync::valid_device_id(device)
            || self.books.len() > INDEX_LIMIT
            || self.books.iter().any(|(hash, object)| {
                !crate::cloud_sync::valid_hash(hash) || !crate::cloud_sync::valid_hash(object)
            })
        {
            return Err("Invalid annotation device index".into());
        }
        Ok(())
    }
}
fn object_path(device: &str, hash: &str, object: &str) -> String {
    format!("annotation-books-v1/{device}/{hash}/{object}.json")
}
fn index_path(device: &str) -> String {
    format!("annotation-devices-v1/{device}.json")
}
fn read_index(api: &Api, device: &str) -> Result<Index, String> {
    let Some(bytes) = api.read(&index_path(device), STATE_LIMIT)? else {
        return Ok(Index {
            schema_version: 1,
            device: device.into(),
            books: BTreeMap::new(),
        });
    };
    let index: Index =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid annotation device index")?;
    index.validate(device)?;
    Ok(index)
}
fn local_configs(root: &str) -> Result<Vec<(String, String)>, String> {
    let entries = match std::fs::read_dir(PathBuf::from(root).join("books")) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err("Cannot read annotation library".into()),
    };
    let mut result = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| "Cannot read annotation library")?;
        if !entry
            .file_type()
            .map_err(|_| "Cannot read annotation file")?
            .is_file()
        {
            continue;
        }
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let Some(id) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .filter(|id| uuid::Uuid::parse_str(id).is_ok())
        else {
            continue;
        };
        if path
            .metadata()
            .map_err(|_| "Cannot read annotation file")?
            .len()
            > 16 * 1024 * 1024
        {
            return Err("Book configuration exceeds annotation sync limit".into());
        }
        if let Some(config) = crate::library::Library::load_book_config(root, id)? {
            if let Some(hash) = config
                .file_hash
                .filter(|hash| crate::cloud_sync::valid_hash(hash))
            {
                result.push((id.into(), hash));
            }
        }
        if result.len() > INDEX_LIMIT {
            return Err("Annotation library exceeds sync limit".into());
        }
    }
    Ok(result)
}
fn save_cache(path: &std::path::Path, cache: &Cache) -> Result<(), String> {
    let bytes = serde_json::to_vec(cache).map_err(|_| "Cannot encode annotation sync cache")?;
    if bytes.len() as u64 > CACHE_LIMIT {
        return Err("Annotation sync cache exceeds size limit".into());
    }
    crate::storage::save(path, cache)
}
fn merge_cached(
    cache: &mut Cache,
    used: &mut usize,
    hash: &str,
    incoming: &AnnotationState,
) -> Result<(), String> {
    let state = cache.books.entry(hash.into()).or_default();
    let old = serde_json::to_vec(state)
        .map_err(|_| "Cannot encode cached annotations")?
        .len();
    state.merge(incoming)?;
    let new = serde_json::to_vec(state)
        .map_err(|_| "Cannot encode cached annotations")?
        .len();
    *used = used.saturating_sub(old).saturating_add(new);
    if *used > 60 * 1024 * 1024 {
        return Err("Annotation sync batch exceeds memory limit".into());
    }
    Ok(())
}
/// Invoked only after the platform has applied the lightweight progress phase.
/// Errors are reported separately; book transfers and the already applied progress remain usable.
pub(super) fn sync(api: &Api, root: &str, device: &str) -> Result<usize, String> {
    let _guard = ANNOTATION_SYNC_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if !crate::cloud_sync::valid_device_id(device) {
        return Err("Invalid annotation device".into());
    }
    let path = PathBuf::from(root).join("github-annotations").join(format!(
        "{}.json",
        digest(api.owner.to_ascii_lowercase().as_bytes())
    ));
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > CACHE_LIMIT {
            return Err("Annotation sync cache exceeds size limit".into());
        }
    }
    let mut cache: Cache = crate::storage::load(&path)?.unwrap_or_default();
    if cache.schema_version != 1 || cache.books.len() > INDEX_LIMIT || cache.seen.len() > 20000 {
        return Err("Invalid annotation sync cache".into());
    }
    for (hash, state) in &cache.books {
        if !crate::cloud_sync::valid_hash(hash) {
            return Err("Invalid cached annotation book".into());
        }
        state.validate()?;
    }
    let mut used = cache
        .books
        .values()
        .map(|state| {
            serde_json::to_vec(state)
                .map(|b| b.len())
                .map_err(|_| "Cannot encode cached annotations")
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .sum::<usize>();
    let mut received_bytes = 0usize;
    let mut devices = BTreeSet::from([device.to_string()]);
    if let Some(list) = api.get_json(&api.contents("annotation-devices-v1"))? {
        let list = list.as_array().ok_or("Invalid annotation device listing")?;
        if list.len() > 128 {
            return Err("Too many annotation devices".into());
        }
        for item in list {
            let Some(id) = item["name"].as_str().and_then(|s| s.strip_suffix(".json")) else {
                return Err("Invalid annotation device listing".into());
            };
            if item["type"] != "file" || !crate::cloud_sync::valid_device_id(id) {
                return Err("Invalid annotation device listing".into());
            }
            devices.insert(id.into());
        }
    }
    let mut referenced = BTreeSet::new();
    let mut own = Index {
        schema_version: 1,
        device: device.into(),
        books: BTreeMap::new(),
    };
    for id in devices {
        let index = read_index(api, &id)?;
        for (hash, object) in &index.books {
            let reference = format!("{hash}:{object}");
            referenced.insert(reference.clone());
            if !cache.seen.contains(&reference) || !cache.books.contains_key(hash) {
                let bytes = api
                    .read(
                        &object_path(&id, hash, object),
                        crate::annotations::MAX_BYTES as u64,
                    )?
                    .ok_or("Annotation object missing; retry sync")?;
                received_bytes = received_bytes.saturating_add(bytes.len());
                if received_bytes > 64 * 1024 * 1024 {
                    return Err("Annotation download batch exceeds size limit".into());
                }
                if digest(&bytes) != *object {
                    return Err("Annotation integrity check failed".into());
                }
                let state: AnnotationState =
                    serde_json::from_slice(&bytes).map_err(|_| "Invalid remote annotations")?;
                state.validate()?;
                merge_cached(&mut cache, &mut used, hash, &state)?;
                cache.seen.insert(reference);
            }
        }
        if id == device {
            own = index;
        }
        if cache.books.len() > INDEX_LIMIT {
            return Err("Too many annotated books".into());
        }
    }
    let configs = local_configs(root)?;
    for (id, hash) in &configs {
        if let Some(config) = crate::library::Library::load_book_config(root, id)? {
            merge_cached(&mut cache, &mut used, hash, &config.annotation_state)?;
        }
    }
    cache.seen.retain(|hash| referenced.contains(hash));
    save_cache(&path, &cache)?; // Remote edits survive a later file-transfer/download failure.
                                // UI adapters apply the cache only after their generation/account guards succeed.
                                // Cancelled or signed-out workers must never mutate visible book configurations.
    let previous = serde_json::to_vec(&own).map_err(|_| "Cannot encode annotation index")?;
    for (hash, state) in &cache.books {
        if state.is_empty() {
            continue;
        }
        let bytes = serde_json::to_vec(state).map_err(|_| "Cannot encode annotations")?;
        let object = digest(&bytes);
        if own.books.get(hash) == Some(&object) {
            continue;
        }
        let object_path = object_path(device, hash, &object);
        // Retrying an interrupted publication is idempotent; never overwrite a hash-addressed object.
        if !api.put(&object_path, &bytes, None)? {
            let remote = api
                .read(&object_path, crate::annotations::MAX_BYTES as u64)?
                .ok_or("Annotation publication conflict")?;
            if digest(&remote) != object {
                return Err("Annotation object identity mismatch".into());
            }
        }
        own.books.insert(hash.clone(), object.clone());
        cache.seen.insert(format!("{hash}:{object}"));
    }
    own.validate(device)?;
    let bytes = serde_json::to_vec(&own).map_err(|_| "Cannot encode annotation index")?;
    if bytes.len() as u64 > STATE_LIMIT {
        return Err("Annotation index exceeds size limit".into());
    }
    if bytes != previous {
        let index_path = index_path(device);
        let metadata = api.get_json(&api.contents(&index_path))?;
        // Another process using the same installation identity must not lose its index entries.
        let current = read_index(api, device)?;
        if serde_json::to_vec(&current).map_err(|_| "Cannot encode annotation index")? != previous {
            return Err("Annotation index changed; local edits retained for retry".into());
        }
        if !api.put(
            &index_path,
            &bytes,
            metadata.as_ref().and_then(|m| m["sha"].as_str()),
        )? {
            return Err("Annotation index conflict; retry sync".into());
        }
    }
    cache.seen.retain(|hash| {
        referenced.contains(hash)
            || own
                .books
                .iter()
                .any(|(book, object)| format!("{book}:{object}") == *hash)
    });
    save_cache(&path, &cache)?;
    Ok(0)
}

/// Refresh books installed after metadata sync, without waiting for another network transfer.
pub fn apply_cached_annotations(root: &str, owner: &str) -> Result<Vec<String>, String> {
    if owner.is_empty()
        || owner.len() > 80
        || !owner
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        return Err("Invalid annotation account".into());
    }
    let path = PathBuf::from(root).join("github-annotations").join(format!(
        "{}.json",
        digest(owner.to_ascii_lowercase().as_bytes())
    ));
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > CACHE_LIMIT {
            return Err("Annotation cache exceeds size limit".into());
        }
    }
    let Some(cache) = crate::storage::load::<Cache>(&path)? else {
        return Ok(Vec::new());
    };
    if cache.schema_version != 1 || cache.books.len() > INDEX_LIMIT {
        return Err("Too many cached annotated books".into());
    }
    let mut applied = Vec::new();
    for (id, hash) in local_configs(root)? {
        if let Some(state) = cache.books.get(&hash) {
            if crate::annotations::apply_remote(root, &id, state)? {
                applied.push(id);
            }
        }
    }
    Ok(applied)
}
