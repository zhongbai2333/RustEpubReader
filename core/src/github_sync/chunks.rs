//! Content-addressed private draft-release chunks. Metadata is published after the
//! entire manifest exists; partial downloads never become library entries.
use super::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, io::Write, path::Path};

pub(super) const CHUNK_BYTES: u64 = 8 * 1024 * 1024;
const MANIFEST_LIMIT: u64 = 256 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Part {
    index: usize,
    size: u64,
    hash: String,
    asset: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Manifest {
    version: u32,
    book: CloudBook,
    release: u64,
    chunk_bytes: u64,
    parts: Vec<Part>,
}
impl Manifest {
    pub(super) fn validate(&self, expected: &CloudBook) -> Result<(), String> {
        if self.version != 2
            || self.book.hash != expected.hash
            || self.book.size != expected.size
            || !crate::cloud_sync::valid_hash(&self.book.hash)
            || self.book.title.len() > 4096
            || self.book.size <= MAX_BOOK_BYTES
            || self.book.size > MAX_DOCUMENT_BYTES
            || self.release == 0
            || self.chunk_bytes != CHUNK_BYTES
            || self.parts.len() as u64 != self.book.size.div_ceil(CHUNK_BYTES)
        {
            return Err("Invalid chunked book manifest".into());
        }
        let mut assets = BTreeSet::new();
        for (index, part) in self.parts.iter().enumerate() {
            let size = CHUNK_BYTES.min(self.book.size - index as u64 * CHUNK_BYTES);
            if part.index != index
                || part.size != size
                || part.asset == 0
                || !assets.insert(part.asset)
                || !crate::cloud_sync::valid_hash(&part.hash)
            {
                return Err("Invalid chunked book part".into());
            }
        }
        Ok(())
    }
}
fn tag(hash: &str) -> String {
    format!("reader-file-v2-{hash}")
}
fn manifest_path(hash: &str) -> String {
    format!("books-v2/{hash}.json")
}
fn scope(api: &Api, root: &str, hash: &str) -> PathBuf {
    Path::new(root)
        .join("github-sync-chunks")
        .join(digest(api.owner.to_ascii_lowercase().as_bytes()))
        .join(hash)
}
pub(super) fn cleanup(root: &str, owner: &str, hash: &str) {
    if crate::cloud_sync::valid_hash(hash) && !owner.is_empty() {
        let path = Path::new(root)
            .join("github-sync-chunks")
            .join(digest(owner.to_ascii_lowercase().as_bytes()))
            .join(hash);
        let _ = std::fs::remove_dir_all(path);
    }
}
fn manifest(api: &Api, info: &CloudBook) -> Result<Option<Manifest>, String> {
    let Some(bytes) = api.read(&manifest_path(&info.hash), MANIFEST_LIMIT)? else {
        return Ok(None);
    };
    let value: Manifest =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid chunked book manifest")?;
    value.validate(info)?;
    Ok(Some(value))
}
fn file_releases(api: &Api, hash: &str) -> Result<Vec<Value>, String> {
    let mut matches = Vec::new();
    // Authenticated owners can see draft releases. Keep storage unpublished so
    // repository-wide release immutability cannot lock future repairs/deletes.
    for page in 1..=50 {
        let value = api
            .get_json(&format!(
                "{}/releases?per_page=100&page={page}",
                api.repo_path()
            ))?
            .ok_or("Cannot list private file storage")?;
        let values = value.as_array().ok_or("Invalid file storage releases")?;
        if values.len() > 100 {
            return Err("Too many file storage releases".into());
        }
        for value in values.iter().filter(|v| v["tag_name"] == tag(hash)) {
            matches.push(check_release(value.clone(), hash)?);
        }
        if values.len() < 100 {
            matches.sort_by_key(|v| v["id"].as_u64().unwrap());
            return Ok(matches);
        }
    }
    Err("Too many file storage releases".into())
}
fn release(api: &Api, hash: &str) -> Result<Value, String> {
    if let Some(value) = file_releases(api, hash)?.into_iter().next() {
        return Ok(value);
    }
    let response=api.request(reqwest::Method::POST,&format!("{}/releases",api.repo_path()),Some(json!({
        "tag_name":tag(hash),"name":format!("Reader file {}",&hash[..12]),"draft":true,
        "prerelease":true,"make_latest":"false","body":"Private reader file storage (chunk protocol v2)."
    })),false)?;
    if response.status() == StatusCode::UNPROCESSABLE_ENTITY {
        return file_releases(api, hash)?
            .into_iter()
            .next()
            .ok_or_else(|| "Cannot resolve file storage release".into());
    }
    if !response.status().is_success() {
        return Err(error(&response));
    }
    let value = serde_json::from_slice(&bounded(response, STATE_LIMIT)?)
        .map_err(|_| "Invalid file storage release")?;
    check_release(value, hash)
}
fn check_release(value: Value, hash: &str) -> Result<Value, String> {
    if value["id"].as_u64().is_none_or(|id| id == 0) || value["tag_name"] != tag(hash) {
        return Err("Invalid file storage release".into());
    }
    Ok(value)
}
fn assets(api: &Api, release: u64) -> Result<Vec<Value>, String> {
    let mut result = Vec::new();
    // A 1 GiB book has at most 128 chunks; bound even a tampered release.
    for page in 1..=2 {
        let values = api
            .get_json(&format!(
                "{}/releases/{release}/assets?per_page=100&page={page}",
                api.repo_path()
            ))?
            .ok_or("Missing file storage release")?;
        let values = values.as_array().ok_or("Invalid file storage assets")?;
        if values.len() > 100 {
            return Err("Too many file storage assets".into());
        }
        result.extend(values.iter().cloned());
        if values.len() < 100 {
            return Ok(result);
        }
    }
    Err("Too many file storage assets".into())
}
fn asset_path(api: &Api, id: u64) -> String {
    format!("{}/releases/assets/{id}", api.repo_path())
}
fn approved_redirect(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && matches!(
            url.host_str(),
            Some("release-assets.githubusercontent.com" | "objects.githubusercontent.com")
        )
}
fn read_asset(api: &Api, id: u64, limit: u64) -> Result<Vec<u8>, String> {
    if api.cancel.load(Ordering::SeqCst) {
        return Err("Sync cancelled".into());
    }
    let response = api.request_with_accept(
        reqwest::Method::GET,
        &asset_path(api, id),
        None,
        "application/octet-stream",
    )?;
    let response = if response.status() == StatusCode::FOUND {
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .ok_or("Invalid asset redirect")?;
        let url = reqwest::Url::parse(location).map_err(|_| "Invalid asset redirect")?;
        if !approved_redirect(&url) {
            return Err("Untrusted asset redirect".into());
        }
        // Signed asset URLs authenticate themselves. Never forward the GitHub token.
        api.client
            .get(url)
            .header("User-Agent", "RustEpubReader")
            .send()
            .map_err(|_| "Failed to read GitHub response")?
    } else {
        response
    };
    if !response.status().is_success() {
        return Err(error(&response));
    }
    bounded(response, limit)
}
fn checked_asset(api: &Api, value: &Value, hash: &str, size: u64) -> Result<u64, String> {
    let id = value["id"]
        .as_u64()
        .filter(|id| *id > 0)
        .ok_or("Invalid chunk asset")?;
    if value["size"].as_u64() != Some(size) || value["state"] != "uploaded" {
        return Err("Incomplete chunk asset".into());
    }
    if let Some(server_hash) = value["digest"].as_str() {
        if server_hash != format!("sha256:{hash}") {
            return Err("Remote chunk integrity check failed".into());
        }
    } else if digest(&read_asset(api, id, size)?) != hash {
        return Err("Remote chunk integrity check failed".into());
    }
    Ok(id)
}
fn upload_asset(api: &Api, release: u64, name: &str, bytes: Vec<u8>) -> Result<Value, String> {
    api.before_request(true)?;
    let base = if api.base == "https://api.github.com" {
        "https://uploads.github.com"
    } else {
        &api.base
    };
    let url = format!(
        "{base}{}/releases/{release}/assets?name={name}",
        api.repo_path()
    );
    let response = api
        .client
        .post(url)
        .bearer_auth(&api.token)
        .header("User-Agent", "RustEpubReader")
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("Content-Type", "application/octet-stream")
        .body(bytes)
        .send()
        .map_err(|_| "Cannot reach GitHub; local data is retained")?;
    if response.status() == StatusCode::UNPROCESSABLE_ENTITY {
        return Ok(Value::Null);
    }
    if !response.status().is_success() {
        return Err(error(&response));
    }
    serde_json::from_slice(&bounded(response, STATE_LIMIT)?)
        .map_err(|_| "Invalid chunk upload response".into())
}
pub(super) fn upload(
    api: &Api,
    book: &LocalBook,
    budget: &mut u64,
) -> Result<Option<bool>, String> {
    if manifest(api, &book.info)?.is_some() {
        return Ok(Some(false));
    }
    if crate::file_hash(&book.path.to_string_lossy())? != book.info.hash {
        return Err("Local book changed during sync; retry later".into());
    }
    let release = release(api, &book.info.hash)?["id"].as_u64().unwrap();
    let mut existing = assets(api, release)?;
    let mut input = std::fs::File::open(&book.path).map_err(|_| "Cannot read local book")?;
    let mut parts = Vec::new();
    for index in 0..book.info.size.div_ceil(CHUNK_BYTES) as usize {
        if api.cancel.load(Ordering::SeqCst) {
            return Err("Sync cancelled".into());
        }
        let size = CHUNK_BYTES.min(book.info.size - index as u64 * CHUNK_BYTES);
        let mut bytes = Vec::new();
        (&mut input)
            .take(size)
            .read_to_end(&mut bytes)
            .map_err(|_| "Cannot read local book")?;
        if bytes.len() as u64 != size {
            return Err("Local book changed during sync; retry later".into());
        }
        let hash = digest(&bytes);
        let name = format!("{index:04}-{hash}.bin");
        let found = existing.iter().find(|v| v["name"] == name).cloned();
        let verified = found
            .as_ref()
            .filter(|v| v["state"] == "uploaded")
            .map(|v| checked_asset(api, v, &hash, size))
            .transpose();
        let verified = match verified {
            Ok(id) => id,
            Err(error)
                if error == "Incomplete chunk asset"
                    || error == "Remote chunk integrity check failed" =>
            {
                None
            }
            Err(error) => return Err(error),
        };
        let id = if let Some(id) = verified {
            id
        } else {
            if *budget < size {
                return Ok(None);
            }
            // A failed upload can leave a starter; a damaged completed asset
            // must also be replaced rather than retried forever.
            if let Some(stale) = found {
                let id = stale["id"].as_u64().ok_or("Invalid partial chunk asset")?;
                let response =
                    api.request(reqwest::Method::DELETE, &asset_path(api, id), None, false)?;
                if !response.status().is_success() && response.status() != StatusCode::NOT_FOUND {
                    return Err(error(&response));
                }
            }
            let value = upload_asset(api, release, &name, bytes)?;
            *budget -= size;
            if value.is_null() {
                existing = assets(api, release)?;
                let value = existing
                    .iter()
                    .find(|v| v["name"] == name)
                    .ok_or("Chunk upload conflict; retry later")?;
                checked_asset(api, value, &hash, size)?
            } else {
                checked_asset(api, &value, &hash, size)?
            }
        };
        parts.push(Part {
            index,
            size,
            hash,
            asset: id,
        });
        if let Some(meter) = &api.meter {
            meter.update_bytes(
                "uploading",
                ((index + 1) as u64 * CHUNK_BYTES).min(book.info.size) as usize,
                book.info.size as usize,
                &book.info.title,
            );
        }
    }
    // Detect replacement or modification while the chunks were being read.
    if input
        .read(&mut [0u8; 1])
        .map_err(|_| "Cannot read local book")?
        != 0
        || crate::file_hash(&book.path.to_string_lossy())? != book.info.hash
    {
        return Err("Local book changed during sync; retry later".into());
    }
    let value = Manifest {
        version: 2,
        book: book.info.clone(),
        release,
        chunk_bytes: CHUNK_BYTES,
        parts,
    };
    value.validate(&book.info)?;
    let bytes = serde_json::to_vec(&value).map_err(|_| "Cannot encode chunk manifest")?;
    if !api.put(&manifest_path(&book.info.hash), &bytes, None)?
        && manifest(api, &book.info)?.is_none()
    {
        return Err("Chunk manifest conflict; retry later".into());
    }
    Ok(Some(true))
}
pub(super) fn download(
    api: &Api,
    info: &CloudBook,
    root: &str,
    budget: &mut u64,
) -> Result<Option<PathBuf>, String> {
    let manifest = manifest(api, info)?.ok_or("Remote book manifest is missing")?;
    let directory = scope(api, root, &info.hash);
    std::fs::create_dir_all(&directory).map_err(|_| "Cannot create chunk cache")?;
    let complete = directory.join("complete.epub");
    if std::fs::metadata(&complete).is_ok_and(|v| v.len() == info.size)
        && crate::file_hash(&complete.to_string_lossy())? == info.hash
    {
        return Ok(Some(complete));
    }
    let mut done = 0u64;
    for part in &manifest.parts {
        if api.cancel.load(Ordering::SeqCst) {
            return Err("Sync cancelled".into());
        }
        let path = directory.join(format!("{:04}-{}.chunk", part.index, part.hash));
        let valid = std::fs::metadata(&path).is_ok_and(|v| v.len() == part.size)
            && crate::file_hash(&path.to_string_lossy())? == part.hash;
        if !valid {
            if *budget < part.size {
                return Ok(None);
            }
            let bytes = read_asset(api, part.asset, part.size)?;
            *budget -= part.size;
            if bytes.len() as u64 != part.size || digest(&bytes) != part.hash {
                return Err("Remote chunk integrity check failed".into());
            }
            let temporary = path.with_extension("partial");
            std::fs::write(&temporary, &bytes)
                .and_then(|_| {
                    if path.exists() {
                        std::fs::remove_file(&path)?;
                    }
                    std::fs::rename(&temporary, &path)
                })
                .map_err(|_| "Cannot save downloaded chunk")?;
        }
        done += part.size;
        if let Some(meter) = &api.meter {
            meter.update_bytes(
                "downloading",
                done as usize,
                info.size as usize,
                &info.title,
            );
        }
    }
    let temporary = directory.join("assembled.partial");
    let mut output =
        std::fs::File::create(&temporary).map_err(|_| "Cannot assemble downloaded book")?;
    let mut hash = Sha256::new();
    let mut size = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    for part in &manifest.parts {
        if api.cancel.load(Ordering::SeqCst) {
            return Err("Sync cancelled".into());
        }
        let mut file =
            std::fs::File::open(directory.join(format!("{:04}-{}.chunk", part.index, part.hash)))
                .map_err(|_| "Cannot read downloaded chunk")?;
        loop {
            let n = file
                .read(&mut buffer)
                .map_err(|_| "Cannot read downloaded chunk")?;
            if n == 0 {
                break;
            }
            size += n as u64;
            hash.update(&buffer[..n]);
            output
                .write_all(&buffer[..n])
                .map_err(|_| "Cannot assemble downloaded book")?;
        }
    }
    output
        .sync_all()
        .map_err(|_| "Cannot save downloaded book")?;
    drop(output);
    if size != info.size || format!("{:x}", hash.finalize()) != info.hash {
        return Err("Remote book integrity check failed".into());
    }
    if complete.exists() {
        std::fs::remove_file(&complete).map_err(|_| "Cannot replace invalid cached book")?;
    }
    std::fs::rename(&temporary, &complete).map_err(|_| "Cannot finalize downloaded book")?;
    Ok(Some(complete))
}
pub(super) fn delete(api: &Api, hash: &str) -> Result<(), String> {
    // Tombstones are published by the caller first. A stable per-book release
    // owns all its chunks, including interrupted uploads not in a manifest yet.
    for value in file_releases(api, hash)? {
        let id = value["id"].as_u64().unwrap();
        let response = api.request(
            reqwest::Method::DELETE,
            &format!("{}/releases/{id}", api.repo_path()),
            None,
            false,
        )?;
        if !response.status().is_success() && response.status() != StatusCode::NOT_FOUND {
            return Err(error(&response));
        }
    }
    let path = api.contents(&manifest_path(hash));
    if let Some(meta) = api.get_json(&path)? {
        let sha = meta["sha"]
            .as_str()
            .ok_or("Invalid chunk manifest metadata")?;
        let mut body = json!({"message":"Delete archived reader file manifest","sha":sha});
        if !api.count_contributions {
            let identity =
                json!({"name":"RustEpubReader Sync","email":"sync@rustepubreader.local"});
            body["author"] = identity.clone();
            body["committer"] = identity;
        }
        let response = api.request(reqwest::Method::DELETE, &path, Some(body), false)?;
        if !response.status().is_success() && response.status() != StatusCode::NOT_FOUND {
            return Err(error(&response));
        }
    }
    Ok(())
}
