//! Progress is applied by clients before this bounded, independently retryable file batch.
use super::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};
// Cancelled workers may still be completing HTTP requests. Keep their file queues
// serialized, while progress requests remain independent of this lock.
static TRANSFER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SyncPhase {
    Progress,
    Books,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransferFailure {
    pub hash: String,
    pub title: String,
    pub direction: String,
    pub error: String,
    pub attempts: u32,
    pub retry_at: u64,
}

#[derive(Default, Serialize, Deserialize)]
pub(super) struct RetryQueue {
    failures: BTreeMap<String, TransferFailure>,
    #[serde(skip)]
    path: PathBuf,
}
impl RetryQueue {
    pub(super) fn load(root: &str, owner: &str, device: &str) -> Result<Self, String> {
        if owner.is_empty()
            || owner.len() > 80
            || !owner
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            || !crate::cloud_sync::valid_device_id(device)
        {
            return Err("Invalid sync account or device".into());
        }
        let path = Path::new(root).join("github-sync-transfers").join(format!(
            "{}.json",
            digest(format!("{}:{device}", owner.to_ascii_lowercase()).as_bytes())
        ));
        let mut queue: Self = match std::fs::read(&path) {
            Ok(bytes) if bytes.len() <= 4 * 1024 * 1024 => {
                serde_json::from_slice(&bytes).map_err(|_| "Invalid transfer retry queue")?
            }
            Ok(_) => return Err("Transfer retry queue exceeds size limit".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(_) => return Err("Cannot read transfer retry queue".into()),
        };
        if queue.failures.len() > 10000
            || queue.failures.iter().any(|(key, f)| {
                !crate::cloud_sync::valid_hash(&f.hash)
                    || *key != Self::key(&f.direction, &f.hash)
                    || !matches!(f.direction.as_str(), "upload" | "download")
                    || f.title.len() > 4096
                    || f.error.len() > 4096
            })
        {
            return Err("Invalid transfer retry queue".into());
        }
        queue.path = path;
        Ok(queue)
    }
    fn key(direction: &str, hash: &str) -> String {
        format!("{direction}:{hash}")
    }
    fn save(&self) -> Result<(), String> {
        std::fs::create_dir_all(self.path.parent().unwrap())
            .map_err(|_| "Cannot create transfer retry directory")?;
        let temporary = self.path.with_extension("json.tmp");
        let bytes = serde_json::to_vec(self).map_err(|_| "Cannot encode transfer retry queue")?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err("Transfer retry queue exceeds size limit".into());
        }
        std::fs::write(&temporary, bytes)
            .and_then(|_| std::fs::rename(temporary, &self.path))
            .map_err(|_| "Cannot save transfer retry queue".into())
    }
    fn due(&self, direction: &str, hash: &str, manual: bool) -> bool {
        self.failures
            .get(&Self::key(direction, hash))
            .is_none_or(|f| manual || f.retry_at <= crate::now_secs())
    }
    fn failed(&mut self, direction: &str, info: &CloudBook, error: String) -> Result<(), String> {
        let key = Self::key(direction, &info.hash);
        let attempts = self
            .failures
            .get(&key)
            .map_or(1, |f| f.attempts.saturating_add(1));
        let delay = (30u64 * (1u64 << attempts.saturating_sub(1).min(7))).min(3600);
        // Error messages contain only local diagnostic text and HTTP status, never credentials or URLs.
        let error: String = error.chars().take(1000).collect();
        self.failures.insert(
            key,
            TransferFailure {
                hash: info.hash.clone(),
                title: info.title.clone(),
                direction: direction.into(),
                error,
                attempts,
                retry_at: crate::now_secs().saturating_add(delay),
            },
        );
        self.save()
    }
    fn succeeded(&mut self, direction: &str, hash: &str) -> Result<(), String> {
        if self.failures.remove(&Self::key(direction, hash)).is_some() {
            self.save()?;
        }
        Ok(())
    }
}

pub fn record_transfer_failure(
    root: &str,
    owner: &str,
    device: &str,
    direction: &str,
    info: &CloudBook,
    error: String,
) -> Result<Vec<TransferFailure>, String> {
    let _guard = TRANSFER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !matches!(direction, "upload" | "download")
        || !crate::cloud_sync::valid_hash(&info.hash)
        || info.title.len() > 4096
    {
        return Err("Invalid failed transfer".into());
    }
    let mut queue = RetryQueue::load(root, owner, device)?;
    queue.failed(direction, info, error)?;
    Ok(queue.failures.into_values().collect())
}

/// A download is complete only after the client validates and saves it into the library.
pub fn complete_transfer(
    root: &str,
    owner: &str,
    device: &str,
    direction: &str,
    hash: &str,
) -> Result<Vec<TransferFailure>, String> {
    let _guard = TRANSFER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !matches!(direction, "upload" | "download") || !crate::cloud_sync::valid_hash(hash) {
        return Err("Invalid completed transfer".into());
    }
    let mut queue = RetryQueue::load(root, owner, device)?;
    queue.succeeded(direction, hash)?;
    Ok(queue.failures.into_values().collect())
}

// Keep the shared transport entry point explicit for the three platform adapters.
#[allow(clippy::too_many_arguments)]
pub fn sync_phase(
    token: String,
    owner: String,
    mut own: DeviceSnapshot,
    books: Vec<LocalBook>,
    cancel: Arc<AtomicBool>,
    count_contributions: bool,
    phase: SyncPhase,
    data_dir: &str,
    retry_failed: bool,
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
    };
    match phase {
        SyncPhase::Progress => progress(&api, &mut own),
        SyncPhase::Books => {
            let _guard = TRANSFER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let mut retries = RetryQueue::load(data_dir, &api.owner, &own.device_id)?;
            files(&api, &mut own, books, &mut retries, retry_failed)
        }
    }
}

fn authorize(api: &Api, own: &DeviceSnapshot) -> Result<(), String> {
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
    api.ensure_repo()
}

fn publish(api: &Api, own: &mut DeviceSnapshot) -> Result<(), String> {
    let path = format!("devices/{}.json", own.device_id);
    for _ in 0..3 {
        let metadata = api.get_json(&api.contents(&path))?;
        let sha = metadata.as_ref().and_then(|m| m["sha"].as_str());
        let remote = api.read_snapshot(&path)?;
        if let Some(remote) = &remote {
            own.merge_own(remote)?;
        }
        own.validate()?;
        let bytes = serde_json::to_vec_pretty(own).map_err(|_| "Cannot serialize progress")?;
        if bytes.len() as u64 > STATE_LIMIT {
            return Err("Device snapshot exceeds size limit".into());
        }
        if remote
            .as_ref()
            .and_then(|r| serde_json::to_vec_pretty(r).ok())
            .as_deref()
            == Some(bytes.as_slice())
            || api.put(&path, &bytes, sha)?
        {
            return Ok(());
        }
    }
    Err("Progress conflict; local changes retained for next sync".into())
}
pub(super) fn progress(api: &Api, own: &mut DeviceSnapshot) -> Result<SyncResult, String> {
    authorize(api, own)?;
    publish(api, own)?;
    Ok(SyncResult {
        snapshots: api.snapshots()?,
        own: own.clone(),
        downloaded: Vec::new(),
        uploaded: 0,
        failed: Vec::new(),
        next_retry_after: None,
        terminal_error: None,
    })
}
fn global_error(message: &str) -> bool {
    message.starts_with("RETRY_AFTER:")
        || message.starts_with("REAUTHORIZE:")
        || message.contains("login expired")
        || message.contains("account changed")
        || message.contains("Cannot reach GitHub")
        || message.contains("Failed to read GitHub response")
}
fn upload(
    api: &Api,
    book: &LocalBook,
    known: &BTreeMap<String, CloudBook>,
) -> Result<bool, String> {
    if !crate::cloud_sync::valid_hash(&book.info.hash)
        || book.info.size > MAX_BOOK_BYTES
        || book.info.title.len() > 4096
    {
        return Err("Invalid local book".into());
    }
    if let Some(remote) = known.get(&book.info.hash) {
        if remote.size != book.info.size {
            return Err("Remote book size mismatch".into());
        }
        return Ok(false);
    }
    let path = format!("books/{}.epub", book.info.hash);
    if let Some(metadata) = api.get_json(&api.contents(&path))? {
        if metadata["size"].as_u64() != Some(book.info.size) {
            return Err("Remote book size mismatch".into());
        }
        return Ok(false);
    }
    let file = std::fs::File::open(&book.path).map_err(|_| "Cannot read local book")?;
    let mut bytes = Vec::new();
    file.take(MAX_BOOK_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read local book")?;
    if bytes.len() as u64 != book.info.size || digest(&bytes) != book.info.hash {
        return Err("Local book changed during sync; retry later".into());
    }
    if api.put(&path, &bytes, None)? {
        return Ok(true);
    }
    if api
        .get_json(&api.contents(&path))?
        .and_then(|m| m["size"].as_u64())
        != Some(book.info.size)
    {
        return Err("Book upload conflict; retry later".into());
    }
    Ok(false)
}
pub(super) fn files(
    api: &Api,
    own: &mut DeviceSnapshot,
    books: Vec<LocalBook>,
    retries: &mut RetryQueue,
    retry_failed: bool,
) -> Result<SyncResult, String> {
    authorize(api, own)?;
    let snapshots = api.snapshots()?;
    let mut known = BTreeMap::new();
    for snapshot in &snapshots {
        if snapshot.device_id == own.device_id {
            own.merge_own(snapshot)?;
        }
        for (hash, info) in &snapshot.books {
            known.entry(hash.clone()).or_insert_with(|| info.clone());
        }
    }
    let local: BTreeSet<_> = books.iter().map(|b| b.info.hash.clone()).collect();
    let retry_keys: BTreeSet<_> = retries.failures.keys().cloned().collect();
    retries.failures.retain(|_, f| {
        if f.direction == "upload" {
            local.contains(&f.hash)
        } else {
            !local.contains(&f.hash)
                && !own.books.contains_key(&f.hash)
                && known.contains_key(&f.hash)
        }
    });
    retries.save()?;
    let mut uploaded = 0;
    let mut attempted = 0;
    let mut remaining = 0;
    let mut changed = false;
    let mut terminal_error = None;
    for book in books {
        if api.cancel.load(Ordering::SeqCst) {
            return Err("Sync cancelled".into());
        }
        if retry_failed && !retry_keys.contains(&RetryQueue::key("upload", &book.info.hash)) {
            continue;
        }
        if !retries.due("upload", &book.info.hash, retry_failed) {
            continue;
        }
        // Already indexed immutable books need no per-book HTTP request.
        if !known.contains_key(&book.info.hash) {
            if attempted >= 20 || terminal_error.is_some() {
                remaining += 1;
                continue;
            }
            attempted += 1;
        }
        match upload(api, &book, &known) {
            Ok(wrote) => {
                uploaded += usize::from(wrote);
                changed |= !own.books.contains_key(&book.info.hash);
                own.books.insert(book.info.hash.clone(), book.info.clone());
                known.insert(book.info.hash.clone(), book.info.clone());
                retries.succeeded("upload", &book.info.hash)?;
            }
            Err(message) => {
                if message == "Sync cancelled" {
                    return Err(message);
                }
                if global_error(&message) {
                    terminal_error = Some(message.clone());
                }
                retries.failed("upload", &book.info, message)?;
            }
        }
    }
    // Only advertise hashes whose file is already present, even if another book failed.
    if changed && terminal_error.is_none() {
        if let Err(message) = publish(api, own) {
            if message == "Sync cancelled" {
                return Err(message);
            }
            terminal_error = Some(message);
        }
    }
    let mut downloaded = Vec::new();
    let mut total_bytes = 0;
    attempted = 0;
    for (hash, info) in known {
        if local.contains(&hash)
            || own.books.contains_key(&hash)
            || retry_failed && !retry_keys.contains(&RetryQueue::key("download", &hash))
        {
            continue;
        }
        if !retries.due("download", &hash, retry_failed) {
            continue;
        }
        if attempted >= 20
            || total_bytes + info.size > 100 * 1024 * 1024
            || terminal_error.is_some()
        {
            remaining += 1;
            continue;
        }
        if api.cancel.load(Ordering::SeqCst) {
            return Err("Sync cancelled".into());
        }
        attempted += 1;
        let result = (|| {
            let bytes = api
                .read(&format!("books/{hash}.epub"), MAX_BOOK_BYTES)?
                .ok_or("Remote book file is missing")?;
            if bytes.len() as u64 != info.size || digest(&bytes) != hash {
                return Err("Remote book integrity check failed".into());
            }
            Ok(bytes)
        })();
        match result {
            Ok(bytes) => {
                total_bytes += info.size;
                downloaded.push(DownloadedBook { info, bytes });
            }
            Err(message) => {
                if message == "Sync cancelled" {
                    return Err(message);
                }
                if global_error(&message) {
                    terminal_error = Some(message.clone());
                }
                retries.failed("download", &info, message)?;
            }
        }
    }
    let next_retry_after = retries
        .failures
        .values()
        .filter(|f| {
            !(f.direction == "download" && downloaded.iter().any(|b| b.info.hash == f.hash))
        })
        .map(|f| f.retry_at.saturating_sub(crate::now_secs()).max(1))
        .chain((remaining > 0).then_some(15))
        .min();
    Ok(SyncResult {
        own: own.clone(),
        snapshots,
        downloaded,
        uploaded,
        failed: retries.failures.values().cloned().collect(),
        next_retry_after,
        terminal_error,
    })
}
