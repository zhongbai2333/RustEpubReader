//! GitHub API transport. No git checkout, public repositories, or arbitrary download URLs.
use crate::cloud_sync::{CloudBook, DeviceSnapshot, MAX_BOOK_BYTES};
use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::{
    blocking::{Client, Response},
    StatusCode,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Read,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
mod meter;
mod staged;
pub use meter::{sync_meter, SyncMeter};
pub use staged::{
    complete_transfer, record_transfer_failure, sync_phase, SyncPhase, TransferFailure,
};

pub const REPOSITORY: &str = "EpubReaderInfo";
const MARKER: &str = "epub-reader.json";
const STATE_LIMIT: u64 = 1024 * 1024;

pub struct LocalBook {
    pub info: CloudBook,
    pub path: PathBuf,
}
pub struct DownloadedBook {
    pub info: CloudBook,
    pub bytes: Vec<u8>,
}
pub struct SyncResult {
    pub snapshots: Vec<DeviceSnapshot>,
    pub confirmed_archives: std::collections::BTreeSet<String>,
    pub own: DeviceSnapshot,
    pub downloaded: Vec<DownloadedBook>,
    pub uploaded: usize,
    pub failed: Vec<TransferFailure>,
    pub next_retry_after: Option<u64>,
    pub terminal_error: Option<String>,
}

struct Api {
    client: Client,
    token: String,
    owner: String,
    base: String,
    cancel: Arc<AtomicBool>,
    last_write: std::sync::Mutex<Option<std::time::Instant>>,
    count_contributions: bool,
    meter: Option<meter::Reporter>,
}

pub fn has_repo_scope(scopes: &str) -> bool {
    scopes.split(',').any(|s| s.trim() == "repo")
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn error(response: &Response) -> String {
    match response.status() {
        StatusCode::UNAUTHORIZED => "GitHub login expired; sign in again".into(),
        StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS => {
            let wait = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .or_else(|| {
                    response
                        .headers()
                        .get("x-ratelimit-reset")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<u64>().ok())
                        .map(|reset| reset.saturating_sub(crate::now_secs()))
                })
                .unwrap_or(900)
                .max(300);
            format!(
                "RETRY_AFTER:{wait}:GitHub access denied or rate limited (HTTP {})",
                response.status().as_u16()
            )
        }
        _ => format!(
            "GitHub request failed (HTTP {})",
            response.status().as_u16()
        ),
    }
}
fn bounded(response: Response, limit: u64) -> Result<Vec<u8>, String> {
    if response.content_length().is_some_and(|n| n > limit) {
        return Err("Remote file exceeds size limit".into());
    }
    let mut bytes = Vec::new();
    response
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Failed to read GitHub response")?;
    if bytes.len() as u64 > limit {
        return Err("Remote file exceeds size limit".into());
    }
    Ok(bytes)
}
impl Api {
    fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
        raw: bool,
    ) -> Result<Response, String> {
        if method != reqwest::Method::GET {
            let mut previous = self.last_write.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(time) = *previous {
                if let Some(wait) = Duration::from_secs(1).checked_sub(time.elapsed()) {
                    std::thread::sleep(wait);
                }
            }
            *previous = Some(std::time::Instant::now());
        }
        if self.cancel.load(Ordering::SeqCst) {
            return Err("Sync cancelled".into());
        }
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&self.token)
            .header("User-Agent", "RustEpubReader")
            .header(
                "Accept",
                if raw {
                    "application/vnd.github.raw+json"
                } else {
                    "application/vnd.github+json"
                },
            )
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(body) = body {
            request = request.json(&body);
        }
        request
            .send()
            .map_err(|_| "Cannot reach GitHub; local data is retained".into())
    }
    fn repo_path(&self) -> String {
        format!("/repos/{}/{REPOSITORY}", self.owner)
    }
    fn contents(&self, path: &str) -> String {
        format!("{}/contents/{path}", self.repo_path())
    }
    fn get_json(&self, path: &str) -> Result<Option<Value>, String> {
        let response = self.request(reqwest::Method::GET, path, None, false)?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(error(&response));
        }
        serde_json::from_slice(&bounded(response, STATE_LIMIT * 4)?)
            .map(Some)
            .map_err(|_| "Invalid GitHub response".into())
    }
    fn read(&self, path: &str, limit: u64) -> Result<Option<Vec<u8>>, String> {
        let response = self.request(reqwest::Method::GET, &self.contents(path), None, true)?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(error(&response));
        }
        bounded(response, limit).map(Some)
    }
    fn put(&self, path: &str, bytes: &[u8], sha: Option<&str>) -> Result<bool, String> {
        let mut body =
            json!({"message": "Update reader sync data", "content": STANDARD.encode(bytes)});
        if !self.count_contributions {
            // Generic .local addresses cannot be linked to GitHub accounts. Override both
            // roles so GitHub does not substitute the authenticated user for either.
            let identity =
                json!({"name": "RustEpubReader Sync", "email": "sync@rustepubreader.local"});
            body["author"] = identity.clone();
            body["committer"] = identity;
        }
        if let Some(sha) = sha {
            body["sha"] = json!(sha);
        }
        let response = self.request(
            reqwest::Method::PUT,
            &self.contents(path),
            Some(body),
            false,
        )?;
        if matches!(
            response.status(),
            StatusCode::CONFLICT | StatusCode::UNPROCESSABLE_ENTITY
        ) {
            return Ok(false);
        }
        if !response.status().is_success() {
            return Err(error(&response));
        }
        Ok(true)
    }
    fn delete_book(&self, hash: &str) -> Result<(), String> {
        if !crate::cloud_sync::valid_hash(hash) {
            return Err("Invalid book hash".into());
        }
        let path = self.contents(&format!("books/{hash}.epub"));
        for _ in 0..3 {
            let Some(meta) = self.get_json(&path)? else {
                return Ok(());
            };
            let sha = meta["sha"].as_str().ok_or("Invalid remote book metadata")?;
            let mut body = json!({"message":"Delete archived reader book", "sha":sha});
            if !self.count_contributions {
                let identity =
                    json!({"name":"RustEpubReader Sync","email":"sync@rustepubreader.local"});
                body["author"] = identity.clone();
                body["committer"] = identity;
            }
            let response = self.request(reqwest::Method::DELETE, &path, Some(body), false)?;
            if response.status().is_success() || response.status() == StatusCode::NOT_FOUND {
                return Ok(());
            }
            if !matches!(
                response.status(),
                StatusCode::CONFLICT | StatusCode::UNPROCESSABLE_ENTITY
            ) {
                return Err(error(&response));
            }
        }
        Err("Book deletion conflict; retry later".into())
    }
    fn ensure_repo(&self) -> Result<(), String> {
        let mut created = false;
        let repo = match self.get_json(&self.repo_path())? {
            Some(repo) => repo,
            None => {
                let response = self.request(reqwest::Method::POST, "/user/repos", Some(json!({"name": REPOSITORY, "private": true, "auto_init": true, "description": "RustEpubReader private book and reading progress sync"})), false)?;
                if response.status() == StatusCode::CREATED {
                    created = true;
                    serde_json::from_slice(&bounded(response, STATE_LIMIT)?)
                        .map_err(|_| "Invalid repository response")?
                } else if response.status() == StatusCode::UNPROCESSABLE_ENTITY {
                    self.get_json(&self.repo_path())?
                        .ok_or("Repository creation conflict")?
                } else {
                    return Err(error(&response));
                }
            }
        };
        // Never put user data into a public or unrelated repository with the same name.
        if repo["private"] != true
            || !repo["owner"]["login"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(&self.owner))
        {
            return Err(
                "EpubReaderInfo must be a private repository owned by the signed-in account".into(),
            );
        }
        let expected = json!({"application": "RustEpubReader", "schema_version": 1});
        match self.read(MARKER, STATE_LIMIT)? {
            Some(bytes) => {
                let marker: Value = serde_json::from_slice(&bytes).map_err(|_| "Invalid sync repository marker")?;
                if marker != expected { return Err("Unsupported or unrelated EpubReaderInfo repository".into()) }
            }
            None if created => {
                if !self.put(MARKER, &serde_json::to_vec_pretty(&expected).unwrap(), None)? {
                    return Err("Sync repository initialization conflict; retry later".into())
                }
            }
            None => return Err("Existing EpubReaderInfo is not a reader sync repository; rename it before enabling sync".into()),
        }
        Ok(())
    }
    fn read_snapshot(&self, path: &str) -> Result<Option<DeviceSnapshot>, String> {
        let Some(bytes) = self.read(path, STATE_LIMIT)? else {
            return Ok(None);
        };
        let snapshot: DeviceSnapshot =
            serde_json::from_slice(&bytes).map_err(|_| "Invalid remote device snapshot")?;
        snapshot.validate()?;
        if path != format!("devices/{}.json", snapshot.device_id) {
            return Err("Remote device identity mismatch".into());
        }
        Ok(Some(snapshot))
    }
    fn snapshots(&self) -> Result<Vec<DeviceSnapshot>, String> {
        let Some(value) = self.get_json(&self.contents("devices"))? else {
            return Ok(Vec::new());
        };
        let entries = value.as_array().ok_or("Invalid remote device directory")?;
        if entries.len() >= 1000 {
            return Err("Too many sync devices".into());
        }
        let mut snapshots = Vec::new();
        for entry in entries {
            let name = entry["name"].as_str().ok_or("Invalid device filename")?;
            let Some(id) = name.strip_suffix(".json") else {
                continue;
            };
            if !crate::cloud_sync::valid_device_id(id) || entry["type"] != "file" {
                return Err("Invalid remote device file".into());
            }
            if let Some(snapshot) = self.read_snapshot(&format!("devices/{name}"))? {
                snapshots.push(snapshot);
            }
        }
        Ok(snapshots)
    }
}

pub fn sync(
    token: String,
    owner: String,
    mut own: DeviceSnapshot,
    books: Vec<LocalBook>,
    cancel: Arc<AtomicBool>,
    count_contributions: bool,
) -> Result<SyncResult, String> {
    let api = Api {
        client: Client::builder()
            .timeout(Duration::from_secs(45))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "Cannot initialize HTTP client")?,
        token,
        owner,
        base: "https://api.github.com".into(),
        cancel,
        last_write: Default::default(),
        count_contributions,
        meter: None,
    };
    run(&api, &mut own, books)
}
fn run(api: &Api, own: &mut DeviceSnapshot, books: Vec<LocalBook>) -> Result<SyncResult, String> {
    own.validate()?;
    let response = api.request(reqwest::Method::GET, "/user", None, false)?;
    if !response.status().is_success() {
        return Err(error(&response));
    }
    if !has_repo_scope(
        response
            .headers()
            .get("x-oauth-scopes")
            .and_then(|s| s.to_str().ok())
            .unwrap_or(""),
    ) {
        return Err("REAUTHORIZE: GitHub repo permission is required for private sync".into());
    }
    let user: Value = serde_json::from_slice(&bounded(response, STATE_LIMIT)?)
        .map_err(|_| "Invalid GitHub account response")?;
    if !user["login"]
        .as_str()
        .is_some_and(|s| s.eq_ignore_ascii_case(&api.owner))
    {
        return Err("GitHub account changed; sign in again".into());
    }
    api.ensure_repo()?;
    let mut uploaded = 0;
    let local_hashes: std::collections::HashSet<_> =
        books.iter().map(|b| b.info.hash.clone()).collect();
    for book in books {
        if !crate::cloud_sync::valid_hash(&book.info.hash) || book.info.size > MAX_BOOK_BYTES {
            return Err("Invalid local book".into());
        }
        let path = format!("books/{}.epub", book.info.hash);
        if let Some(metadata) = api.get_json(&api.contents(&path))? {
            if metadata["size"].as_u64() != Some(book.info.size) {
                return Err("Remote book size mismatch".into());
            }
        } else {
            let file = std::fs::File::open(&book.path).map_err(|_| "Cannot read local book")?;
            let mut bytes = Vec::new();
            file.take(MAX_BOOK_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "Cannot read local book")?;
            if bytes.len() as u64 != book.info.size || digest(&bytes) != book.info.hash {
                return Err("Local book changed during sync; retry later".into());
            }
            if api.put(&path, &bytes, None)? {
                uploaded += 1;
            } else if api.get_json(&api.contents(&path))?.is_none() {
                return Err("Book upload conflict; retry later".into());
            }
        }
        own.books.insert(book.info.hash.clone(), book.info);
    }
    let path = format!("devices/{}.json", own.device_id);
    let mut saved = false;
    for _ in 0..3 {
        let metadata = api.get_json(&api.contents(&path))?;
        let sha = metadata.as_ref().and_then(|m| m["sha"].as_str());
        if let Some(remote) = api.read_snapshot(&path)? {
            own.merge_own(&remote)?;
        }
        own.validate()?;
        let bytes = serde_json::to_vec_pretty(own).map_err(|_| "Cannot serialize progress")?;
        if bytes.len() as u64 > STATE_LIMIT {
            return Err("Device snapshot exceeds size limit".into());
        }
        if api.read(&path, STATE_LIMIT)?.as_deref() == Some(bytes.as_slice())
            || api.put(&path, &bytes, sha)?
        {
            saved = true;
            break;
        }
    }
    if !saved {
        return Err("Progress conflict; local changes retained for next sync".into());
    }
    let snapshots = api.snapshots()?;
    let mut remote_books = BTreeMap::new();
    for snapshot in &snapshots {
        for (hash, book) in &snapshot.books {
            remote_books
                .entry(hash.clone())
                .or_insert_with(|| book.clone());
        }
    }
    let mut downloaded = Vec::new();
    let mut total_bytes = 0u64;
    for (hash, info) in remote_books {
        if local_hashes.contains(&hash) || own.books.contains_key(&hash) {
            continue;
        }
        // Bound the pending UI batch rather than loading an entire remote library into RAM.
        if downloaded.len() >= 20 || total_bytes + info.size > 100 * 1024 * 1024 {
            break;
        }
        let bytes = api
            .read(&format!("books/{hash}.epub"), MAX_BOOK_BYTES)?
            .ok_or("Remote book file is missing")?;
        if bytes.len() as u64 != info.size || digest(&bytes) != hash {
            return Err("Remote book integrity check failed".into());
        }
        total_bytes += info.size;
        downloaded.push(DownloadedBook { info, bytes });
    }
    Ok(SyncResult {
        confirmed_archives: Default::default(),
        snapshots,
        own: own.clone(),
        downloaded,
        uploaded,
        failed: Vec::new(),
        next_retry_after: None,
        terminal_error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    mod stages;
    #[test]
    fn scope_requires_full_repo_not_public_repo_or_repo_status() {
        assert!(!has_repo_scope("public_repo, repo:status"));
        assert!(has_repo_scope("read:user, repo"));
    }
    struct Step {
        method: &'static str,
        path: String,
        status: u16,
        body: Vec<u8>,
        scope: Option<&'static str>,
    }
    fn step(method: &'static str, path: &str, status: u16, value: Value) -> Step {
        Step {
            method,
            path: path.into(),
            status,
            body: serde_json::to_vec(&value).unwrap(),
            scope: None,
        }
    }
    fn raw(path: &str, bytes: Vec<u8>) -> Step {
        Step {
            method: "GET",
            path: path.into(),
            status: 200,
            body: bytes,
            scope: None,
        }
    }
    fn prefix(private: bool, scope: &'static str) -> Vec<Step> {
        let mut user = step("GET", "/user", 200, json!({"login":"tester"}));
        user.scope = Some(scope);
        let mut steps = vec![
            user,
            step(
                "GET",
                "/repos/tester/EpubReaderInfo",
                200,
                json!({"private": private, "owner":{"login":"tester"}}),
            ),
        ];
        if private {
            steps.push(raw(
                "/repos/tester/EpubReaderInfo/contents/epub-reader.json",
                serde_json::to_vec(&json!({"application":"RustEpubReader","schema_version":1}))
                    .unwrap(),
            ));
        }
        steps
    }
    fn mock(steps: Vec<Step>) -> (Api, std::thread::JoinHandle<()>) {
        mock_with_contributions(steps, false)
    }
    fn mock_with_contributions(
        steps: Vec<Step>,
        count_contributions: bool,
    ) -> (Api, std::thread::JoinHandle<()>) {
        use std::{io::Write, net::TcpListener};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let server = std::thread::spawn(move || {
            for step in steps {
                let start = std::time::Instant::now();
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(e)
                            if e.kind() == std::io::ErrorKind::WouldBlock
                                && start.elapsed().as_secs() < 10 =>
                        {
                            std::thread::sleep(Duration::from_millis(5))
                        }
                        Err(e) => panic!("mock request missing: {e}"),
                    }
                };
                // Darwin may inherit O_NONBLOCK from the listening socket.
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0; 1];
                while !request.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                let headers = String::from_utf8(request).unwrap();
                assert_eq!(
                    headers.lines().next().unwrap(),
                    format!("{} {} HTTP/1.1", step.method, step.path)
                );
                assert!(headers
                    .to_lowercase()
                    .contains("authorization: bearer test-token"));
                let length: usize = headers
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|s| s.trim().parse().unwrap())
                    })
                    .unwrap_or(0);
                let mut body = vec![0; length];
                stream.read_exact(&mut body).unwrap();
                if step.method == "POST" {
                    let value: Value = serde_json::from_slice(&body).unwrap();
                    assert_eq!(value["private"], true);
                }
                if step.method == "PUT" {
                    let value: Value = serde_json::from_slice(&body).unwrap();
                    assert!(value.get("force").is_none());
                    if count_contributions {
                        assert!(value.get("author").is_none());
                        assert!(value.get("committer").is_none());
                    } else {
                        let identity = json!({"name":"RustEpubReader Sync","email":"sync@rustepubreader.local"});
                        assert_eq!(value["author"], identity);
                        assert_eq!(value["committer"], identity);
                    }
                    if step.status == 409 {
                        assert_eq!(value["sha"], "old");
                    }
                    let content = STANDARD.decode(value["content"].as_str().unwrap()).unwrap();
                    assert!(!String::from_utf8_lossy(&content).contains("test-token"));
                    if step.path.contains("devices/") {
                        assert!(!String::from_utf8_lossy(&content).contains("epub_path"));
                    }
                }
                let scopes = step
                    .scope
                    .map(|s| format!("X-OAuth-Scopes: {s}\r\n"))
                    .unwrap_or_default();
                write!(
                    stream,
                    "HTTP/1.1 {} OK\r\nContent-Length: {}\r\n{}Connection: close\r\n\r\n",
                    step.status,
                    step.body.len(),
                    scopes
                )
                .unwrap();
                stream.write_all(&step.body).unwrap();
            }
        });
        (
            Api {
                client: Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                    .unwrap(),
                token: "test-token".into(),
                owner: "tester".into(),
                base,
                cancel: Arc::new(AtomicBool::new(false)),
                last_write: Default::default(),
                count_contributions,
                meter: None,
            },
            server,
        )
    }
    #[test]
    fn contribution_choice_applies_to_all_sync_file_types() {
        for count in [false, true] {
            let paths = [
                MARKER.to_string(),
                format!("books/{}.epub", "a".repeat(64)),
                "devices/local.json".into(),
            ];
            let steps = paths
                .iter()
                .map(|path| {
                    step(
                        "PUT",
                        &format!("/repos/tester/EpubReaderInfo/contents/{path}"),
                        201,
                        json!({}),
                    )
                })
                .collect();
            let (api, server) = mock_with_contributions(steps, count);
            for path in paths {
                assert!(api.put(&path, b"payload", None).unwrap());
            }
            server.join().unwrap();
        }
    }
    #[test]
    fn insufficient_permission_stops_before_repository_access() {
        let mut steps = prefix(true, "public_repo");
        steps.truncate(1);
        let (api, server) = mock(steps);
        let error = run(&api, &mut DeviceSnapshot::new("local".into()), vec![])
            .err()
            .unwrap();
        assert!(error.starts_with("REAUTHORIZE:"));
        server.join().unwrap();
    }
    #[test]
    fn public_repository_is_rejected_before_reading_or_writing_books() {
        let (api, server) = mock(prefix(false, "repo"));
        assert!(run(&api, &mut DeviceSnapshot::new("local".into()), vec![])
            .err()
            .unwrap()
            .contains("private repository"));
        server.join().unwrap();
    }
    #[test]
    fn unrelated_private_repository_is_not_overwritten() {
        let mut steps = prefix(true, "repo");
        steps.last_mut().unwrap().body = b"{}".to_vec();
        let (api, server) = mock(steps);
        assert!(run(&api, &mut DeviceSnapshot::new("local".into()), vec![])
            .err()
            .unwrap()
            .contains("unrelated"));
        server.join().unwrap();
    }
    #[test]
    fn uploads_local_book_and_downloads_verified_remote_book() {
        use std::io::Write;
        let mut local = tempfile::NamedTempFile::new().unwrap();
        local.write_all(b"local book").unwrap();
        let local_info = CloudBook {
            hash: digest(b"local book"),
            title: "Local".into(),
            size: 10,
        };
        let remote_bytes = b"remote book".to_vec();
        let remote_hash = digest(&remote_bytes);
        let mut remote = DeviceSnapshot::new("remote".into());
        remote.books.insert(
            remote_hash.clone(),
            CloudBook {
                hash: remote_hash.clone(),
                title: "Remote".into(),
                size: remote_bytes.len() as u64,
            },
        );
        let own_path = "/repos/tester/EpubReaderInfo/contents/devices/local.json";
        let mut steps = prefix(true, "repo");
        let local_path = format!(
            "/repos/tester/EpubReaderInfo/contents/books/{}.epub",
            local_info.hash
        );
        steps.extend([
            step("GET", &local_path, 404, json!({})),
            step("PUT", &local_path, 201, json!({})),
            step("GET", own_path, 404, json!({})),
            step("GET", own_path, 404, json!({})),
            step("GET", own_path, 404, json!({})),
            step("PUT", own_path, 201, json!({})),
            step(
                "GET",
                "/repos/tester/EpubReaderInfo/contents/devices",
                200,
                json!([{"name":"remote.json","type":"file"}]),
            ),
            raw(
                "/repos/tester/EpubReaderInfo/contents/devices/remote.json",
                serde_json::to_vec(&remote).unwrap(),
            ),
            raw(
                &format!("/repos/tester/EpubReaderInfo/contents/books/{remote_hash}.epub"),
                remote_bytes.clone(),
            ),
        ]);
        let (api, server) = mock(steps);
        let result = run(
            &api,
            &mut DeviceSnapshot::new("local".into()),
            vec![LocalBook {
                info: local_info,
                path: local.path().to_owned(),
            }],
        )
        .unwrap();
        assert_eq!(result.uploaded, 1);
        assert_eq!(result.downloaded[0].bytes, remote_bytes);
        server.join().unwrap();
    }
    #[test]
    fn cancellation_prevents_network_requests() {
        let (api, server) = mock(vec![]);
        api.cancel.store(true, Ordering::SeqCst);
        assert_eq!(
            run(&api, &mut DeviceSnapshot::new("local".into()), vec![])
                .err()
                .unwrap(),
            "Sync cancelled"
        );
        server.join().unwrap();
    }
    #[test]
    fn retries_progress_conflict_without_force_or_cross_device_overwrite() {
        let own_path = "/repos/tester/EpubReaderInfo/contents/devices/local.json";
        let remote = DeviceSnapshot::new("local".into());
        let remote_bytes = serde_json::to_vec_pretty(&remote).unwrap();
        let mut own = remote.clone();
        let hash = "a".repeat(64);
        own.progress.insert(
            hash.clone(),
            crate::sharing::ProgressEntry {
                book_hash: hash,
                title: "Book".into(),
                chapter: 2,
                block: 3,
                char_offset: 0,
                chapter_title: None,
                timestamp: 5,
            },
        );
        let mut steps = prefix(true, "repo");
        for (sha, status) in [("old", 409), ("new", 200)] {
            steps.extend([
                step("GET", own_path, 200, json!({"sha":sha})),
                raw(own_path, remote_bytes.clone()),
                raw(own_path, remote_bytes.clone()),
                step("PUT", own_path, status, json!({})),
            ]);
        }
        steps.push(step(
            "GET",
            "/repos/tester/EpubReaderInfo/contents/devices",
            200,
            json!([]),
        ));
        let (api, server) = mock(steps);
        assert_eq!(
            run(&api, &mut own, vec![])
                .unwrap()
                .own
                .progress
                .values()
                .next()
                .unwrap()
                .chapter,
            2
        );
        server.join().unwrap();
    }
    #[test]
    fn corrupt_download_is_rejected() {
        let own_path = "/repos/tester/EpubReaderInfo/contents/devices/local.json";
        let own = DeviceSnapshot::new("local".into());
        let own_bytes = serde_json::to_vec_pretty(&own).unwrap();
        let hash = digest(b"good");
        let mut remote = DeviceSnapshot::new("remote".into());
        remote.books.insert(
            hash.clone(),
            CloudBook {
                hash: hash.clone(),
                title: "Book".into(),
                size: 4,
            },
        );
        let mut steps = prefix(true, "repo");
        steps.extend([
            step("GET", own_path, 200, json!({"sha":"same"})),
            raw(own_path, own_bytes.clone()),
            raw(own_path, own_bytes),
            step(
                "GET",
                "/repos/tester/EpubReaderInfo/contents/devices",
                200,
                json!([{"name":"remote.json","type":"file"}]),
            ),
            raw(
                "/repos/tester/EpubReaderInfo/contents/devices/remote.json",
                serde_json::to_vec(&remote).unwrap(),
            ),
            raw(
                &format!("/repos/tester/EpubReaderInfo/contents/books/{hash}.epub"),
                b"evil".to_vec(),
            ),
        ]);
        let (api, server) = mock(steps);
        assert!(run(&api, &mut own.clone(), vec![])
            .err()
            .unwrap()
            .contains("integrity"));
        server.join().unwrap();
    }
    #[test]
    fn creates_private_repository_only_after_scope_check() {
        let mut user = step("GET", "/user", 200, json!({"login":"tester"}));
        user.scope = Some("repo");
        let own_path = "/repos/tester/EpubReaderInfo/contents/devices/local.json";
        let steps = vec![
            user,
            step("GET", "/repos/tester/EpubReaderInfo", 404, json!({})),
            step(
                "POST",
                "/user/repos",
                201,
                json!({"private":true,"owner":{"login":"tester"}}),
            ),
            step(
                "GET",
                "/repos/tester/EpubReaderInfo/contents/epub-reader.json",
                404,
                json!({}),
            ),
            step(
                "PUT",
                "/repos/tester/EpubReaderInfo/contents/epub-reader.json",
                201,
                json!({}),
            ),
            step("GET", own_path, 404, json!({})),
            step("GET", own_path, 404, json!({})),
            step("GET", own_path, 404, json!({})),
            step("PUT", own_path, 201, json!({})),
            step(
                "GET",
                "/repos/tester/EpubReaderInfo/contents/devices",
                404,
                json!({}),
            ),
        ];
        let (api, server) = mock(steps);
        run(&api, &mut DeviceSnapshot::new("local".into()), vec![]).unwrap();
        server.join().unwrap();
    }
}
