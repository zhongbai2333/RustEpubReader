//! Client-owned public storage containing only encrypted, random-named Release assets.
//! Anonymous downloads use a separate HTTP client which never receives a login token.
use super::{
    package::{self, Package, Received, Scratch},
    ticket::{identifier, Location, Ticket},
    History, Work,
};
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine,
};
use reqwest::{
    blocking::{Body, Client, Response},
    redirect::Policy,
    Method,
};
use serde_json::{json, Value};
use std::{
    fs::File,
    io::{Read, Write},
    sync::Arc,
    time::Duration,
};
pub const REPOSITORY: &str = "EpubReaderShares";
const MARKER: &str = ".rust-epub-share.json";
const PURPOSE: &str = "RustEpubReader encrypted book shares v1";
const DESCRIPTION: &str =
    "Encrypted EPUB Reader share packages; decryption keys are never stored here.";
struct Api {
    client: Client,
    token: String,
    base: String,
    uploads: String,
    work: Arc<Work>,
}
fn error(response: &Response) -> String {
    match response.status().as_u16() {
        401 => "GitHub login expired; sign in again".into(),
        403 | 429 => "GitHub denied access or rate limited this request; retry later".into(),
        404 => "GitHub share is missing or has been deleted".into(),
        s => format!("GitHub share request failed (HTTP {s})"),
    }
}
fn limited(response: Response) -> Result<Value, String> {
    if !response.status().is_success() {
        return Err(error(&response));
    }
    let mut bytes = Vec::new();
    response
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read GitHub response")?;
    if bytes.len() > 1024 * 1024 {
        return Err("GitHub response is too large".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "Invalid GitHub response".into())
}
impl Api {
    fn new(token: &str, work: Arc<Work>) -> Result<Self, String> {
        if token.trim().is_empty() {
            return Err("Sign in to GitHub with repository access to share".into());
        }
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(30))
            .redirect(Policy::none())
            .build()
            .map_err(|_| "Cannot prepare GitHub connection")?;
        Ok(Self {
            client,
            token: token.into(),
            base: "https://api.github.com".into(),
            uploads: "https://uploads.github.com".into(),
            work,
        })
    }
    fn request(&self, method: Method, path: &str, body: Option<Value>) -> Result<Response, String> {
        self.work.check()?;
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&self.token)
            .header("User-Agent", "RustEpubReader")
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .map_err(|_| "Cannot reach GitHub sharing storage")?;
        Ok(response)
    }
    fn owner(&self) -> Result<String, String> {
        let user = limited(self.request(Method::GET, "/user", None)?)?;
        user["login"]
            .as_str()
            .filter(|s| identifier(s))
            .map(str::to_owned)
            .ok_or("Invalid GitHub account".into())
    }
    fn ensure_repository(&self, owner: &str) -> Result<(), String> {
        let route = format!("/repos/{owner}/{REPOSITORY}");
        let response = self.request(Method::GET, &route, None)?;
        let created = response.status().as_u16() == 404;
        let repo = if created {
            limited(self.request(Method::POST,"/user/repos",Some(json!({"name":REPOSITORY,"description":DESCRIPTION,"private":false,"auto_init":true})))?)?
        } else {
            limited(response)?
        };
        if repo["private"].as_bool() != Some(false)
            || repo["owner"]["login"]
                .as_str()
                .is_none_or(|s| !s.eq_ignore_ascii_case(owner))
        {
            return Err(
                "The dedicated share repository must be public and owned by your account".into(),
            );
        }
        let marker = self.request(Method::GET, &format!("{route}/contents/{MARKER}"), None)?;
        if marker.status().as_u16() == 404
            && (created || repo["description"].as_str() == Some(DESCRIPTION))
        {
            limited(self.request(Method::PUT,&format!("{route}/contents/{MARKER}"),Some(json!({"message":"Initialize encrypted book sharing","content":STANDARD.encode(serde_json::to_vec(&json!({"purpose":PURPOSE})).unwrap())})))?)?;
        } else {
            let marker = limited(marker)?;
            let content = marker["content"]
                .as_str()
                .ok_or("The existing repository is not an EPUB Reader share repository")?;
            let bytes = STANDARD
                .decode(
                    content
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .collect::<String>(),
                )
                .map_err(|_| "Invalid share repository marker")?;
            let marker: Value =
                serde_json::from_slice(&bytes).map_err(|_| "Invalid share repository marker")?;
            if marker["purpose"].as_str() != Some(PURPOSE) {
                return Err(
                    "The existing repository is not an EPUB Reader share repository".into(),
                );
            }
        }
        Ok(())
    }
    fn remove_release(&self, owner: &str, release: u64, id: &str) -> Result<(), String> {
        let route = format!("/repos/{owner}/{REPOSITORY}/releases/{release}");
        let response = self.request(Method::DELETE, &route, None)?;
        if !response.status().is_success() && response.status().as_u16() != 404 {
            return Err(error(&response));
        }
        let tag = self.request(
            Method::DELETE,
            &format!("/repos/{owner}/{REPOSITORY}/git/refs/tags/reader-share-{id}"),
            None,
        )?;
        if !tag.status().is_success() && tag.status().as_u16() != 404 {
            return Err(error(&tag));
        }
        Ok(())
    }
}
struct CancelReader {
    file: File,
    work: Arc<Work>,
}
impl Read for CancelReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.work
            .check()
            .map_err(|_| std::io::Error::other("Share cancelled"))?;
        let n = self.file.read(buf)?;
        self.work.advance(n as u64);
        Ok(n)
    }
}
pub fn publish(
    root: &str,
    package: Package,
    token: &str,
    work: &Arc<Work>,
) -> Result<(Ticket, History), String> {
    let api = Api::new(token, work.clone())?;
    publish_with_api(root, package, &api)
}
fn publish_with_api(root: &str, package: Package, api: &Api) -> Result<(Ticket, History), String> {
    let owner = api.owner()?;
    api.ensure_repository(&owner)?;
    api.work.check()?;
    let id = uuid::Uuid::new_v4().to_string();
    let release=limited(api.request(Method::POST,&format!("/repos/{owner}/{REPOSITORY}/releases"),Some(json!({"tag_name":format!("reader-share-{id}"),"name":"Encrypted book share","body":"Receive this encrypted package with its EPUB Reader share QR code. The decryption key is not stored in this repository.","draft":false,"prerelease":false})))?)?;
    let release_id = release["id"]
        .as_u64()
        .filter(|n| *n > 0)
        .ok_or("Invalid GitHub release response")?;
    let mut record = History {
        id: id.clone(),
        owner: owner.clone(),
        repository: REPOSITORY.into(),
        release: release_id,
        created: crate::now_secs(),
        count: package.manifest.books.len(),
        ready: false,
    };
    // Record before upload: a crash or failed cleanup remains manageable without keys.
    let result = (|| {
        super::add_history(root, record.clone())?;
        api.work.check()?;
        let size = std::fs::metadata(&package.path)
            .map_err(|_| "Cannot read encrypted package")?
            .len();
        api.work.phase("uploading", size);
        let file = File::open(&package.path).map_err(|_| "Cannot read encrypted package")?;
        let response = api
            .client
            .post(format!(
                "{}/repos/{owner}/{REPOSITORY}/releases/{release_id}/assets?name={id}.rershare",
                api.uploads
            ))
            .bearer_auth(&api.token)
            .header("User-Agent", "RustEpubReader")
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("Content-Type", "application/octet-stream")
            .header("Content-Length", size)
            .timeout(Duration::from_secs(600))
            .body(Body::new(CancelReader {
                file,
                work: api.work.clone(),
            }))
            .send()
            .map_err(|_| "Cannot upload encrypted package")?;
        let asset = limited(response)?;
        api.work.check()?;
        let asset_id = asset["id"]
            .as_u64()
            .filter(|n| *n > 0)
            .ok_or("Invalid GitHub asset response")?;
        record.ready = true;
        Ok((
            Ticket {
                location: Location::Github {
                    owner: owner.clone(),
                    repository: REPOSITORY.into(),
                    asset: asset_id,
                    share: Some(id.clone()),
                },
                key: URL_SAFE_NO_PAD.encode(package.key),
            },
            record.clone(),
        ))
    })();
    if result.is_err() {
        // A cancelled upload must not leave a published half-share. Cleanup ignores
        // the cancelled operation's flag and only touches our newly-created IDs.
        if let Ok(cleanup) = Api::new(&api.token, Work::new()) {
            let cleanup = Api {
                base: api.base.clone(),
                uploads: api.uploads.clone(),
                ..cleanup
            };
            if cleanup.remove_release(&owner, release_id, &id).is_ok() {
                let _ = super::remove_history(root, &id);
            }
        }
    }
    result
}
fn anonymous_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(600))
        .redirect(Policy::custom(|attempt| {
            let url = attempt.url();
            let host = url.host_str().unwrap_or("");
            if attempt.previous().len() > 5 {
                attempt.error("Too many redirects")
            } else if url.scheme() == "https"
                && (host == "api.github.com"
                    || host == "github.com"
                    || host == "githubusercontent.com"
                    || host.ends_with(".githubusercontent.com"))
            {
                attempt.follow()
            } else {
                attempt.error("Invalid GitHub download redirect")
            }
        }))
        .build()
        .map_err(|_| "Cannot prepare anonymous GitHub download".into())
}
pub fn receive(root: &str, ticket: &Ticket, work: &Arc<Work>) -> Result<Received, String> {
    let (owner, repository, asset) = match &ticket.location {
        Location::Github {
            owner,
            repository,
            asset,
            ..
        } => (owner, repository, asset),
        _ => return Err("Invalid GitHub share link".into()),
    };
    work.check()?;
    let url = match &ticket.location { Location::Github{share:Some(id),..}=>format!("https://github.com/{owner}/{repository}/releases/download/reader-share-{id}/{id}.rershare"), _=>format!("https://api.github.com/repos/{owner}/{repository}/releases/assets/{asset}") };
    let response = anonymous_client()?
        .get(url)
        .header("User-Agent", "RustEpubReader")
        .header("Accept", "application/octet-stream")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .map_err(|_| "Cannot download shared package from GitHub")?;
    download_response(root, ticket, response, work)
}
fn download_response(
    root: &str,
    ticket: &Ticket,
    mut response: Response,
    work: &Arc<Work>,
) -> Result<Received, String> {
    if !response.status().is_success() {
        return Err(error(&response));
    }
    if response
        .content_length()
        .is_some_and(|s| s > package::MAX_PACKAGE)
    {
        return Err("Share package is too large".into());
    }
    let scratch = Scratch::new(root)?;
    let path = scratch.0.join("download.rershare");
    let mut output = package::create_private(&path)?;
    let mut buf = [0; 65536];
    let mut total = 0;
    let advertised = response.content_length();
    let mut prefix = Vec::new();
    let mut expected = None;
    work.phase("receiving", response.content_length().unwrap_or(0));
    loop {
        work.check()?;
        let n = response
            .read(&mut buf)
            .map_err(|_| "GitHub share download interrupted")?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if expected.is_none() {
            let keep = n.min((512 * 1024 + 29usize).saturating_sub(prefix.len()));
            prefix.extend_from_slice(&buf[..keep]);
            expected = package::inspect_prefix(&prefix, ticket)?;
            if expected.is_some_and(|length| advertised.is_some_and(|size| size != length)) {
                return Err(
                    "Shared package length does not match its authenticated book list".into(),
                );
            }
        }
        if expected.is_some_and(|length| total > length) {
            return Err("Unexpected data after share package".into());
        }
        if total > package::MAX_PACKAGE {
            return Err("Share package is too large".into());
        }
        output
            .write_all(&buf[..n])
            .map_err(|_| "Cannot save encrypted download")?;
        work.advance(n as u64);
    }
    output
        .sync_all()
        .map_err(|_| "Cannot save encrypted download")?;
    if expected != Some(total) {
        return Err("Incomplete encrypted package".into());
    }
    package::decode(root, &path, ticket, work)
}
pub fn delete(history: &History, token: &str, work: &Arc<Work>) -> Result<(), String> {
    if !identifier(&history.owner)
        || history.repository != REPOSITORY
        || uuid::Uuid::parse_str(&history.id).is_err()
        || history.release == 0
    {
        return Err("Invalid published share record".into());
    }
    let api = Api::new(token, work.clone())?;
    let owner = api.owner()?;
    if !owner.eq_ignore_ascii_case(&history.owner) {
        return Err("Sign in with the account that created this share".into());
    }
    let response = api.request(
        Method::GET,
        &format!("/repos/{owner}/{REPOSITORY}/releases/{}", history.release),
        None,
    )?;
    if response.status().as_u16() != 404 {
        let release = limited(response)?;
        if release["tag_name"].as_str() != Some(&format!("reader-share-{}", history.id)) {
            return Err("Release does not match this published share".into());
        }
    }
    api.remove_release(&owner, history.release, &history.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::BufRead, net::TcpListener, sync::Mutex, thread};
    type Captured = Arc<Mutex<Vec<(String, Vec<u8>)>>>;
    fn mock(replies: Vec<(u16, Vec<u8>)>) -> (String, Captured, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let records = Arc::new(Mutex::new(Vec::new()));
        let captured = records.clone();
        let handle = thread::spawn(move || {
            for (status, body) in replies {
                let deadline = std::time::Instant::now() + Duration::from_secs(15);
                let mut stream = loop {
                    if let Ok((stream, _)) = listener.accept() {
                        break stream;
                    }
                    assert!(
                        std::time::Instant::now() < deadline,
                        "Mock GitHub request did not arrive"
                    );
                    thread::sleep(Duration::from_millis(10));
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
                let mut headers = String::new();
                let mut len = 0usize;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    assert!(!line.is_empty());
                    if line == "\r\n" {
                        break;
                    }
                    if line.to_ascii_lowercase().starts_with("content-length:") {
                        len = line.split(':').nth(1).unwrap().trim().parse().unwrap();
                    }
                    headers.push_str(&line);
                }
                let mut data = vec![0; len];
                reader.read_exact(&mut data).unwrap();
                captured.lock().unwrap().push((headers, data));
                write!(stream,"HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\nContent-Type: application/octet-stream\r\n\r\n",body.len()).unwrap();
                stream.write_all(&body).unwrap();
            }
        });
        (base, records, handle)
    }
    fn json_body(value: Value) -> Vec<u8> {
        serde_json::to_vec(&value).unwrap()
    }
    fn setup() -> Vec<(u16, Vec<u8>)> {
        vec![
            (200, json_body(json!({"login":"sender"}))),
            (
                200,
                json_body(json!({"private":false,"owner":{"login":"sender"}})),
            ),
            (
                200,
                json_body(
                    json!({"content":STANDARD.encode(json_body(json!({"purpose":PURPOSE})))}),
                ),
            ),
            (201, json_body(json!({"id":41}))),
        ]
    }
    fn fixture(root: &str) -> Package {
        let source = std::path::Path::new(root).join("sample.txt");
        std::fs::write(
            &source,
            "第一章\nPrivate book title, Chinese 中文 test content.\n第二章\nMore text.",
        )
        .unwrap();
        let epub = crate::txt::convert_txt_to_epub(
            &source,
            std::path::Path::new(root),
            &Default::default(),
        )
        .unwrap()
        .epub_path;
        let entry = crate::library::Library::load_from(root)
            .unwrap()
            .add_or_update(
                root,
                "Private book title".into(),
                epub.to_string_lossy().into_owned(),
                0,
                None,
            )
            .unwrap();
        package::prepare(root, &[entry.id], &Work::new()).unwrap()
    }
    #[test]
    fn publisher_uploads_only_ciphertext_to_dedicated_public_repository() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let package = fixture(root);
        let key = URL_SAFE_NO_PAD.encode(package.key);
        let ciphertext = std::fs::read(&package.path).unwrap();
        let mut replies = setup();
        replies.push((201, json_body(json!({"id":77}))));
        let (base, records, handle) = mock(replies);
        let mut api = Api::new("test-only-token", Work::new()).unwrap();
        api.base = base.clone();
        api.uploads = base;
        let (ticket, record) = publish_with_api(root, package, &api).unwrap();
        handle.join().unwrap();
        let requests = records.lock().unwrap();
        assert_eq!(requests.len(), 5);
        for (header, _) in requests.iter() {
            assert!(header
                .to_lowercase()
                .contains("authorization: bearer test-only-token"));
            assert!(!header.contains("EpubReaderInfo"));
        }
        assert_eq!(requests[4].1, ciphertext);
        assert!(!requests[4]
            .1
            .windows(key.len())
            .any(|w| w == key.as_bytes()));
        assert!(!requests[4]
            .1
            .windows(18)
            .any(|w| w == b"Private book title"));
        let release: Value = serde_json::from_slice(&requests[3].1).unwrap();
        assert_eq!(release["name"], "Encrypted book share");
        assert!(!String::from_utf8(requests[3].1.clone())
            .unwrap()
            .contains("Private book title"));
        assert_eq!(ticket.key, key);
        assert_eq!(record.owner, "sender");
        let history = serde_json::to_value(record).unwrap();
        assert!(history.get("key").is_none());
        assert!(history.get("title").is_none());
    }
    #[test]
    fn anonymous_recipient_download_has_no_authorization_and_does_not_import() {
        let sender = tempfile::tempdir().unwrap();
        let receiver = tempfile::tempdir().unwrap();
        let package = fixture(sender.path().to_str().unwrap());
        let ticket = Ticket {
            location: Location::Github {
                owner: "sender".into(),
                repository: REPOSITORY.into(),
                asset: 77,
                share: None,
            },
            key: URL_SAFE_NO_PAD.encode(package.key),
        };
        let (base, records, handle) = mock(vec![(200, std::fs::read(&package.path).unwrap())]);
        let response = anonymous_client()
            .unwrap()
            .get(format!("{base}/asset"))
            .header("User-Agent", "RustEpubReader")
            .send()
            .unwrap();
        let received = download_response(
            receiver.path().to_str().unwrap(),
            &ticket,
            response,
            &Work::new(),
        )
        .unwrap();
        handle.join().unwrap();
        assert_eq!(received.manifest.books[0].title, "Private book title");
        assert!(!records.lock().unwrap()[0]
            .0
            .to_lowercase()
            .contains("authorization"));
        assert!(!receiver.path().join("library.json").exists());
    }
    #[test]
    fn failed_upload_deletes_only_the_new_release_and_its_tag() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let package = fixture(root);
        let mut replies = setup();
        replies.extend([(500, Vec::new()), (204, Vec::new()), (204, Vec::new())]);
        let (base, records, handle) = mock(replies);
        let mut api = Api::new("test-only-token", Work::new()).unwrap();
        api.base = base.clone();
        api.uploads = base;
        assert!(publish_with_api(root, package, &api).is_err());
        handle.join().unwrap();
        let requests = records.lock().unwrap();
        assert!(requests[5]
            .0
            .starts_with("DELETE /repos/sender/EpubReaderShares/releases/41 "));
        assert!(requests[6]
            .0
            .starts_with("DELETE /repos/sender/EpubReaderShares/git/refs/tags/reader-share-"));
    }
    #[test]
    fn a_private_repository_is_rejected_before_any_upload_or_permission_change() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let package = fixture(root);
        let (base, records, handle) = mock(vec![
            (200, json_body(json!({"login":"sender"}))),
            (
                200,
                json_body(json!({"private":true,"owner":{"login":"sender"}})),
            ),
        ]);
        let mut api = Api::new("test-only-token", Work::new()).unwrap();
        api.base = base.clone();
        api.uploads = base;
        assert!(publish_with_api(root, package, &api).is_err());
        handle.join().unwrap();
        let requests = records.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|(h, _)| h.starts_with("GET ")));
    }
    #[test]
    fn failed_remote_cleanup_retains_a_deletable_record_without_keys() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let package = fixture(root);
        let mut replies = setup();
        replies.extend([(500, Vec::new()), (500, Vec::new())]);
        let (base, records, handle) = mock(replies);
        let mut api = Api::new("test-only-token", Work::new()).unwrap();
        api.base = base.clone();
        api.uploads = base;
        assert!(publish_with_api(root, package, &api).is_err());
        handle.join().unwrap();
        assert_eq!(records.lock().unwrap().len(), 6);
        let history: Value = serde_json::from_slice(
            &std::fs::read(dir.path().join("book-share-history.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(history[0]["release"], 41);
        assert_eq!(history[0]["ready"], false);
        assert!(history[0].get("key").is_none());
        assert!(history[0].get("token").is_none());
    }
}
