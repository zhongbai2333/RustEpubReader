//! Cooperative cancellation for synchronous publication preparation.
//!
//! A scope belongs to one synchronous worker, never to a cached publication.
//! Cancelling UI work therefore cannot poison later chapter reads. ZIP metadata
//! parsing and HTML parsing are checked on both sides; streaming IO is checked
//! per chunk. Cancellation is cooperative, not a hard real-time deadline.
use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

pub const CANCELLED: &str = "Book opening cancelled";
thread_local! { static CURRENT: RefCell<Option<Cancellation>> = const { RefCell::new(None) }; }

#[derive(Clone, Debug, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn check(&self) -> Result<(), String> {
        if self.0.load(Ordering::Acquire) {
            Err(CANCELLED.into())
        } else {
            Ok(())
        }
    }
    pub fn run<T>(&self, work: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        self.check()?;
        struct Restore(Option<Cancellation>);
        impl Drop for Restore {
            fn drop(&mut self) {
                CURRENT.with(|c| {
                    c.replace(self.0.take());
                });
            }
        }
        let _restore = Restore(CURRENT.with(|c| c.replace(Some(self.clone()))));
        let value = work();
        self.check()?;
        value
    }
}
/// Cheap outside an opening scope; no global book/cache lock is acquired.
pub fn check() -> Result<(), String> {
    CURRENT.with(|c| match c.borrow().as_ref() {
        Some(token) => token.check(),
        None => Ok(()),
    })
}
pub fn check_io() -> std::io::Result<()> {
    check().map_err(std::io::Error::other)
}

/// Register before dispatching a worker, so a cancellation arriving before that
/// worker starts cannot be lost. Unknown/finished IDs are rejected, not created.
#[derive(Default)]
pub struct Registry {
    next: AtomicU64,
    active: Mutex<HashMap<u64, Cancellation>>,
}
impl Registry {
    pub fn begin(&self) -> Result<u64, String> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| "Opening registry unavailable")?;
        if active.len() >= 64 {
            return Err("Too many pending book openings".into());
        }
        // The active-registry mutex serializes every allocation. Keep the
        // counter operation compatible with the project's supported Rust toolchains.
        let id = self
            .next
            .load(Ordering::Relaxed)
            .checked_add(1)
            .ok_or("Opening identifier exhausted")?;
        self.next.store(id, Ordering::Relaxed);
        active.insert(id, Cancellation::default());
        Ok(id)
    }
    pub fn token(&self, id: u64) -> Result<Cancellation, String> {
        self.active
            .lock()
            .map_err(|_| "Opening registry unavailable")?
            .get(&id)
            .cloned()
            .ok_or_else(|| "Book opening expired".into())
    }
    pub fn cancel(&self, id: u64) {
        if let Ok(token) = self.token(id) {
            token.cancel();
        }
    }
    pub fn finish(&self, id: u64) {
        if let Ok(mut active) = self.active.lock() {
            active.remove(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concurrent_allocations_are_unique_and_finished_ids_are_not_reused() {
        let registry = std::sync::Arc::new(Registry::default());
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(32));
        let workers: Vec<_> = (0..32)
            .map(|_| {
                let registry = registry.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    registry.begin().unwrap()
                })
            })
            .collect();
        let mut ids: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
        ids.sort_unstable();
        assert_eq!(ids, (1..=32).collect::<Vec<_>>());
        for id in ids {
            registry.finish(id);
        }
        assert_eq!(registry.begin().unwrap(), 33);
    }
    #[test]
    fn exhausted_ids_fail_without_wrapping_or_publishing_an_operation() {
        let registry = Registry::default();
        registry.next.store(u64::MAX, Ordering::Relaxed);
        assert!(registry.begin().unwrap_err().contains("exhausted"));
        assert!(registry.active.lock().unwrap().is_empty());
        assert_eq!(registry.next.load(Ordering::Relaxed), u64::MAX);
    }
    #[test]
    fn cancellation_before_dispatch_is_not_lost_or_retained_after_finish() {
        let registry = Registry::default();
        let id = registry.begin().unwrap();
        registry.cancel(id);
        assert_eq!(
            registry.token(id).unwrap().run(|| Ok(1)).unwrap_err(),
            CANCELLED
        );
        registry.finish(id);
        assert!(registry.token(id).is_err());
        assert_eq!(
            registry
                .token(registry.begin().unwrap())
                .unwrap()
                .run(|| Ok(2))
                .unwrap(),
            2
        );
    }
    #[test]
    fn scope_restores_on_error_and_does_not_poison_reusable_resources() {
        let token = Cancellation::default();
        let result: Result<(), String> = token.run(|| {
            token.cancel();
            check()
        });
        assert!(result.is_err());
        assert!(check().is_ok());
        let healthy = Cancellation::default();
        assert!(healthy.run(check).is_ok());
    }
    #[test]
    fn cancellation_stops_a_running_worker_without_its_data_lock() {
        let token = Cancellation::default();
        let worker = token.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            worker.run(|| {
                tx.send(()).unwrap();
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
                loop {
                    check()?;
                    if std::time::Instant::now() > deadline {
                        return Err("Cancellation was not observed".into());
                    }
                    std::thread::yield_now();
                }
            })
        });
        rx.recv().unwrap();
        token.cancel();
        let result: Result<(), String> = thread.join().unwrap();
        assert_eq!(result.unwrap_err(), CANCELLED);
        assert!(check().is_ok());
    }
    #[test]
    fn nested_scope_and_unwind_restore_previous_context() {
        let outer = Cancellation::default();
        let inner = Cancellation::default();
        assert!(outer
            .run(|| {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _: Result<(), String> = inner.run(|| panic!("scope unwind"));
                }));
                outer.cancel();
                check()
            })
            .is_err());
        assert!(check().is_ok());
    }
}
