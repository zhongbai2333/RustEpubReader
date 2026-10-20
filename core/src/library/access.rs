//! Cooperating library/config readers and writers share one reentrant root lease.
//! The persistent file lock also protects annotation counters across processes.
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    marker::PhantomData,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{Arc, Condvar, Mutex, OnceLock, Weak},
    thread::ThreadId,
};
#[derive(Default)]
struct State {
    owner: Option<ThreadId>,
    depth: usize,
    file: Option<File>,
}
#[derive(Default)]
struct Gate {
    state: Mutex<State>,
    ready: Condvar,
}
static GATES: OnceLock<Mutex<HashMap<PathBuf, Weak<Gate>>>> = OnceLock::new();
/// A lease is thread-bound; nested calls on this thread do not reacquire the OS lock.
pub struct Guard {
    gate: Arc<Gate>,
    _thread_bound: PhantomData<Rc<()>>,
}
impl Drop for Guard {
    fn drop(&mut self) {
        let mut state = self.gate.state.lock().unwrap_or_else(|e| e.into_inner());
        debug_assert_eq!(state.owner, Some(std::thread::current().id()));
        state.depth = state.depth.saturating_sub(1);
        if state.depth == 0 {
            // Closing the sole descriptor releases the OS lock, including during unwind.
            state.file.take();
            state.owner = None;
            self.gate.ready.notify_all();
        }
    }
}
pub fn lock(root: &str) -> Result<Guard, String> {
    std::fs::create_dir_all(root).map_err(|_| "Cannot open library storage")?;
    let directory = Path::new(root)
        .canonicalize()
        .map_err(|_| "Cannot resolve library storage")?;
    let gate = {
        let mut gates = GATES
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        gates.retain(|_, gate| gate.strong_count() > 0);
        match gates.get(&directory).and_then(Weak::upgrade) {
            Some(gate) => gate,
            None => {
                let gate = Arc::new(Gate::default());
                gates.insert(directory.clone(), Arc::downgrade(&gate));
                gate
            }
        }
    };
    let owner = std::thread::current().id();
    let mut state = gate.state.lock().unwrap_or_else(|e| e.into_inner());
    while state.owner.is_some_and(|previous| previous != owner) {
        state = gate.ready.wait(state).unwrap_or_else(|e| e.into_inner());
    }
    if state.owner == Some(owner) {
        state.depth = state
            .depth
            .checked_add(1)
            .ok_or("Library access nesting exhausted")?;
        drop(state);
        return Ok(Guard {
            gate,
            _thread_bound: PhantomData,
        });
    }
    state.owner = Some(owner);
    state.depth = 1;
    drop(state);
    let guard = Guard {
        gate: gate.clone(),
        _thread_bound: PhantomData,
    };
    let path = directory.join(".library-access.lock");
    if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("Library access lock cannot be a symbolic link".into());
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let file = options
        .open(&path)
        .map_err(|_| "Cannot open library access lock")?;
    File::lock(&file).map_err(|_| "Cannot acquire library access lock")?;
    gate.state.lock().unwrap_or_else(|e| e.into_inner()).file = Some(file);
    crate::backup::transaction::recover(&directory)?;
    crate::backup::transaction::reclaim_staging(&directory);
    Ok(guard)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aliases_and_nested_calls_release_only_the_outer_lease() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let outer = lock(root).unwrap();
        let inner = lock(&format!("{root}/.")).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let other = root.to_owned();
        let worker = std::thread::spawn(move || {
            let _guard = lock(&other).unwrap();
            tx.send(()).unwrap();
        });
        assert!(rx
            .recv_timeout(std::time::Duration::from_millis(30))
            .is_err());
        drop(inner);
        assert!(rx
            .recv_timeout(std::time::Duration::from_millis(30))
            .is_err());
        drop(outer);
        rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
        worker.join().unwrap();
    }
    #[test]
    fn failure_and_panic_release_ownership_without_truncating_the_lock_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        std::fs::write(dir.path().join(".library-access.lock"), b"retained").unwrap();
        let _ = std::panic::catch_unwind(|| {
            let _guard = lock(root).unwrap();
            panic!("isolated unwind");
        });
        let _guard = lock(root).unwrap();
        assert_eq!(
            std::fs::read(dir.path().join(".library-access.lock")).unwrap(),
            b"retained"
        );
        let other = tempfile::tempdir().unwrap();
        std::fs::create_dir(other.path().join(".library-access.lock")).unwrap();
        assert!(lock(other.path().to_str().unwrap()).is_err());
        std::fs::remove_dir(other.path().join(".library-access.lock")).unwrap();
        assert!(lock(other.path().to_str().unwrap()).is_ok());
    }
    #[test]
    #[ignore = "Subprocess entry; exercised by process_lease_blocks_until_the_holder_exits"]
    fn child_lock_probe() {
        let root = std::env::var("READER_ACCESS_CHILD_ROOT").unwrap();
        std::fs::write(Path::new(&root).join("ready"), b"ready").unwrap();
        let _guard = lock(&root).unwrap();
        std::fs::write(Path::new(&root).join("acquired"), b"acquired").unwrap();
    }
    #[test]
    fn process_lease_blocks_until_the_holder_exits() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let guard = lock(root).unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "library::access::tests::child_lock_probe",
                "--ignored",
            ])
            .env("READER_ACCESS_CHILD_ROOT", root)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let result = || {
            for _ in 0..1000 {
                if dir.path().join("ready").exists() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            assert!(dir.path().join("ready").exists());
            assert!(!dir.path().join("acquired").exists());
            drop(guard);
            for _ in 0..1000 {
                if let Some(status) = child.try_wait().unwrap() {
                    assert!(status.success());
                    assert!(dir.path().join("acquired").exists());
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            panic!("Child did not acquire the released lease");
        };
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(result));
        if child.try_wait().unwrap().is_none() {
            let _ = child.kill();
            let _ = child.wait();
        }
        outcome.unwrap();
    }
}
