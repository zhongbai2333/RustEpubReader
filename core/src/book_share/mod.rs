//! Explicit book sharing, independent of private cloud sync and permanent PIN pairs.
#[cfg(feature = "github-sync")]
mod github;
mod lan;
mod package;
#[cfg(test)]
mod tests;
mod ticket;
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine,
};
use package::Received;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    net::{Shutdown, TcpListener, TcpStream},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, LazyLock, Mutex,
    },
};
pub use ticket::{Location, Ticket};

#[derive(Clone, Default, Serialize)]
pub struct Progress {
    pub phase: String,
    pub done: u64,
    pub total: u64,
    pub error: Option<String>,
}
pub struct Work {
    id: String,
    cancel: AtomicBool,
    progress: Mutex<Progress>,
    stream: Mutex<Option<TcpStream>>,
}
impl Work {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            id: uuid::Uuid::new_v4().to_string(),
            cancel: AtomicBool::new(false),
            progress: Mutex::new(Progress {
                phase: "idle".into(),
                ..Default::default()
            }),
            stream: Mutex::new(None),
        })
    }
    pub fn check(&self) -> Result<(), String> {
        if self.cancel.load(Ordering::SeqCst) {
            Err("Share cancelled".into())
        } else {
            Ok(())
        }
    }
    pub fn phase(&self, phase: &str, total: u64) {
        if self.check().is_ok() {
            *self.progress.lock().unwrap_or_else(|e| e.into_inner()) = Progress {
                phase: phase.into(),
                done: 0,
                total,
                error: None,
            };
        }
    }
    pub fn advance(&self, bytes: u64) {
        let mut p = self.progress.lock().unwrap_or_else(|e| e.into_inner());
        p.done = p.done.saturating_add(bytes);
    }
    pub fn fail(&self, error: String) {
        let mut p = self.progress.lock().unwrap_or_else(|e| e.into_inner());
        p.phase = if self.cancel.load(Ordering::SeqCst) {
            "cancelled"
        } else {
            "error"
        }
        .into();
        p.error = if self.cancel.load(Ordering::SeqCst) {
            None
        } else {
            Some(error)
        };
    }
    fn attach(&self, stream: &TcpStream) -> Result<(), String> {
        self.check()?;
        *self.stream.lock().unwrap_or_else(|e| e.into_inner()) = Some(
            stream
                .try_clone()
                .map_err(|_| "Cannot track share connection")?,
        );
        self.check()
    }
    fn detach(&self) {
        self.stream.lock().unwrap_or_else(|e| e.into_inner()).take();
    }
    fn stop(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Some(stream) = self.stream.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = stream.shutdown(Shutdown::Both);
        }
        let mut p = self.progress.lock().unwrap_or_else(|e| e.into_inner());
        p.phase = "cancelled".into();
        p.error = None;
    }
}
#[derive(Clone, Serialize)]
struct Outgoing {
    kind: String,
    link: String,
    qr: String,
    pin: Option<String>,
    expires: Option<u64>,
    count: usize,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct History {
    pub id: String,
    pub owner: String,
    pub repository: String,
    pub release: u64,
    pub created: u64,
    pub count: usize,
    #[serde(default = "published_by_default")]
    pub ready: bool,
}
fn published_by_default() -> bool {
    true
}
struct Runtime {
    work: Arc<Work>,
    outgoing: Option<Outgoing>,
    pending: Option<Arc<Received>>,
}
static RUNTIMES: LazyLock<Mutex<HashMap<String, Arc<Mutex<Runtime>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static HISTORY_LOCK: Mutex<()> = Mutex::new(());
fn runtime(root: &str) -> Arc<Mutex<Runtime>> {
    RUNTIMES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry(root.into())
        .or_insert_with(|| {
            package::cleanup_abandoned(root);
            Arc::new(Mutex::new(Runtime {
                work: Work::new(),
                outgoing: None,
                pending: None,
            }))
        })
        .clone()
}
fn begin(root: &str) -> Arc<Work> {
    let r = runtime(root);
    let mut r = r.lock().unwrap_or_else(|e| e.into_inner());
    r.work.stop();
    r.outgoing = None;
    r.pending = None;
    r.work = Work::new();
    r.work.phase("preparing", 0);
    r.work.clone()
}
fn operation_work(root: &str, request: &Value) -> Result<Arc<Work>, String> {
    if let Some(id) = request["job_id"].as_str() {
        let r = runtime(root);
        let r = r.lock().unwrap_or_else(|e| e.into_inner());
        if r.work.id != id {
            return Err("Share cancelled".into());
        }
        r.work.check()?;
        Ok(r.work.clone())
    } else {
        Ok(begin(root))
    }
}
fn set_outgoing(root: &str, work: &Arc<Work>, ticket: &Ticket, count: usize) -> Result<(), String> {
    work.check()?;
    let link = ticket.link()?;
    let qr = STANDARD.encode(ticket::qr_png(&link)?);
    let (kind, pin, expires) = match &ticket.location {
        Location::Lan { pin, expires, .. } => ("lan", Some(pin.clone()), Some(*expires)),
        Location::Github { .. } => ("github", None, None),
    };
    let runtime = runtime(root);
    let mut r = runtime.lock().unwrap_or_else(|e| e.into_inner());
    work.check()?;
    if !Arc::ptr_eq(&r.work, work) {
        return Err("Share cancelled".into());
    }
    r.outgoing = Some(Outgoing {
        kind: kind.into(),
        link,
        qr,
        pin,
        expires,
        count,
    });
    Ok(())
}
fn ids(request: &Value) -> Result<Vec<String>, String> {
    request["ids"]
        .as_array()
        .filter(|x| !x.is_empty())
        .ok_or("No books selected")?
        .iter()
        .map(|x| {
            x.as_str()
                .map(str::to_owned)
                .ok_or("Invalid book selection".into())
        })
        .collect()
}
pub fn state(root: &str) -> Value {
    let r = runtime(root);
    let r = r.lock().unwrap_or_else(|e| e.into_inner());
    let p = r
        .work
        .progress
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let preview = r.pending.as_ref().map(|received| &received.manifest);
    json!({"job_id":r.work.id,"progress":p,"outgoing":r.outgoing,"preview":preview,"history":history(root)})
}
pub fn stop(root: &str) {
    let r = runtime(root);
    let mut r = r.lock().unwrap_or_else(|e| e.into_inner());
    r.work.stop();
    r.outgoing = None;
    r.pending = None;
}
pub fn dispatch(root: &str, request: Value) -> Result<Value, String> {
    if !Path::new(root).is_absolute() {
        return Err("Invalid share storage".into());
    }
    let op = request["op"].as_str().ok_or("Missing share operation")?;
    match op {
        "state" => Ok(state(root)),
        "arm" => {
            let work = begin(root);
            match request["operation"].as_str() {
                Some("receive") => work.phase("receiving", 0),
                Some("delete_share") => work.phase("deleting", 0),
                _ => {}
            }
            Ok(json!({"job_id":work.id}))
        }
        "cancel" => {
            let r = runtime(root);
            let mut r = r.lock().unwrap_or_else(|e| e.into_inner());
            if request["job_id"].as_str().is_none_or(|id| r.work.id == id) {
                r.work.stop();
                r.outgoing = None;
                r.pending = None;
            }
            drop(r);
            Ok(state(root))
        }
        "decode_image" => {
            let path = request["path"].as_str().ok_or("Missing QR image")?;
            Ok(json!({"link":ticket::decode_image(Path::new(path))?}))
        }
        "start_lan" => {
            let ids = ids(&request)?;
            let work = operation_work(root, &request)?;
            let result: Result<Value, String> = (|| {
                let package = package::prepare(root, &ids, &work)?;
                let address = crate::sharing::get_all_local_ips()
                    .into_iter()
                    .find(|ip| {
                        ip.parse::<std::net::Ipv4Addr>()
                            .is_ok_and(|ip| ip.is_private() || ip.is_link_local())
                    })
                    .ok_or("No local network address is available")?;
                let listener = TcpListener::bind("0.0.0.0:0")
                    .map_err(|_| "Cannot listen for local book sharing")?;
                let ticket = Ticket {
                    location: Location::Lan {
                        address: format!(
                            "{address}:{}",
                            listener
                                .local_addr()
                                .map_err(|_| "Cannot start local share")?
                                .port()
                        ),
                        session: uuid::Uuid::new_v4().to_string(),
                        pin: format!("{:04}", rand::random::<u32>() % 10000),
                        expires: crate::now_secs() + 600,
                    },
                    key: URL_SAFE_NO_PAD.encode(package.key),
                };
                set_outgoing(root, &work, &ticket, ids.len())?;
                work.phase("waiting", 0);
                let worker = work.clone();
                std::thread::spawn(move || lan::listen(package, ticket, listener, worker));
                Ok(state(root))
            })();
            if let Err(e) = &result {
                work.fail(e.clone());
            }
            result
        }
        "receive" => {
            let work = operation_work(root, &request)?;
            let ticket = match Ticket::parse(request["link"].as_str().ok_or("Missing share link")?)
            {
                Ok(ticket) => ticket,
                Err(error) => {
                    work.fail(error.clone());
                    return Err(error);
                }
            };
            let received = match ticket.location {
                Location::Lan { .. } => lan::receive(root, &ticket, &work),
                Location::Github { .. } => {
                    #[cfg(feature = "github-sync")]
                    {
                        github::receive(root, &ticket, &work)
                    }
                    #[cfg(not(feature = "github-sync"))]
                    {
                        Err("GitHub sharing is unavailable in this build".into())
                    }
                }
            };
            match received {
                Ok(received) => {
                    work.check()?;
                    let r = runtime(root);
                    let mut r = r.lock().unwrap_or_else(|e| e.into_inner());
                    work.check()?;
                    if !Arc::ptr_eq(&r.work, &work) {
                        return Err("Share cancelled".into());
                    }
                    r.pending = Some(Arc::new(received));
                    work.phase("preview", 0);
                    drop(r);
                    Ok(state(root))
                }
                Err(error) => {
                    work.fail(error.clone());
                    Err(error)
                }
            }
        }
        "accept" => {
            let r = runtime(root);
            let (received, work) = {
                let r = r.lock().unwrap_or_else(|e| e.into_inner());
                r.work.check()?;
                (
                    r.pending
                        .clone()
                        .ok_or("No verified share is ready to import")?,
                    r.work.clone(),
                )
            };
            let incoming = received
                .manifest
                .books
                .iter()
                .zip(&received.paths)
                .map(|(b, p)| (b.title.clone(), b.hash.clone(), p.clone()))
                .collect::<Vec<_>>();
            work.phase(
                "importing",
                received.manifest.books.iter().map(|b| b.size).sum(),
            );
            let result = crate::library::Library::read_snapshot(root).and_then(|mut library| {
                library.import_shared_books_checked(
                    root,
                    &incoming,
                    || work.check(),
                    |bytes| work.advance(bytes),
                )
            });
            match result {
                Ok((books, skipped)) => {
                    let mut r = r.lock().unwrap_or_else(|e| e.into_inner());
                    if Arc::ptr_eq(&r.work, &work) {
                        r.pending = None;
                        work.phase("imported", 0);
                    }
                    drop(r);
                    Ok(json!({"imported":books.len(),"skipped":skipped,"state":state(root)}))
                }
                Err(error) => {
                    work.fail(error.clone());
                    Err(error)
                }
            }
        }
        #[cfg(feature = "github-sync")]
        "publish" => {
            let ids = ids(&request)?;
            let token = request["token"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("Sign in to GitHub with repository access to share")?;
            let work = operation_work(root, &request)?;
            let result: Result<Value, String> = (|| {
                let package = package::prepare(root, &ids, &work)?;
                let (ticket, record) = github::publish(root, package, token, &work)?;
                let finalized = (|| -> Result<(), String> {
                    work.check()?;
                    add_history(root, record.clone())?;
                    set_outgoing(root, &work, &ticket, ids.len())
                })();
                if let Err(error) = finalized {
                    if github::delete(&record, token, &Work::new()).is_ok() {
                        let _ = remove_history(root, &record.id);
                    }
                    return Err(error);
                }
                work.phase("published", 0);
                Ok(state(root))
            })();
            if let Err(e) = &result {
                work.fail(e.clone());
            }
            result
        }
        #[cfg(feature = "github-sync")]
        "delete_share" => {
            let id = request["id"].as_str().ok_or("Missing published share")?;
            let token = request["token"]
                .as_str()
                .ok_or("Sign in to GitHub to delete a share")?;
            let entries = history(root);
            let entry = entries
                .iter()
                .find(|h| h.id == id)
                .ok_or("Published share is no longer in local history")?;
            let work = operation_work(root, &request)?;
            let result = github::delete(entry, token, &work);
            if let Err(e) = result {
                work.fail(e.clone());
                return Err(e);
            }
            remove_history(root, id)?;
            work.phase("deleted", 0);
            Ok(state(root))
        }
        _ => Err("Unknown book share operation".into()),
    }
}
fn history(root: &str) -> Vec<History> {
    std::fs::read(Path::new(root).join("book-share-history.json"))
        .ok()
        .filter(|b| b.len() <= 2 * 1024 * 1024)
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or_default()
}
fn add_history(root: &str, entry: History) -> Result<(), String> {
    let _guard = HISTORY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut entries = history(root);
    entries.retain(|previous| previous.id != entry.id);
    entries.push(entry);
    write_history(root, &entries)
}
fn remove_history(root: &str, id: &str) -> Result<(), String> {
    let _guard = HISTORY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    write_history(
        root,
        &history(root)
            .into_iter()
            .filter(|h| h.id != id)
            .collect::<Vec<_>>(),
    )
}
fn write_history(root: &str, entries: &[History]) -> Result<(), String> {
    let path = Path::new(root).join("book-share-history.json");
    let temporary =
        Path::new(root).join(format!(".book-share-history-{}.tmp", uuid::Uuid::new_v4()));
    let mut file = package::create_private(&temporary)?;
    use std::io::Write;
    let result: Result<(), String> = (|| {
        file.write_all(&serde_json::to_vec(entries).map_err(|_| "Cannot save share history")?)
            .and_then(|_| file.sync_all())
            .map_err(|_| "Cannot save share history")?;
        std::fs::rename(&temporary, &path).map_err(|_| "Cannot save share history".to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}
