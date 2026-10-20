//! UI polling does not wait on the long-running root/storage lease.
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{LazyLock, Mutex},
};

#[derive(Clone, Serialize)]
pub(super) struct Progress {
    running: bool,
    phase: String,
    completed: usize,
    total: usize,
}
static OPERATIONS: LazyLock<Mutex<BTreeMap<String, Progress>>> = LazyLock::new(Default::default);
pub(super) struct Operation {
    root: String,
    finished: bool,
}
impl Operation {
    pub(super) fn begin(root: &str, phase: &str) -> Result<Self, String> {
        let mut operations = OPERATIONS.lock().unwrap_or_else(|e| e.into_inner());
        if operations.get(root).is_some_and(|p| p.running) {
            return Err("Another backup operation is running".into());
        }
        if operations.len() >= 16 {
            if let Some(key) = operations
                .iter()
                .find(|(_, p)| !p.running)
                .map(|(k, _)| k.clone())
            {
                operations.remove(&key);
            } else {
                return Err("Too many backup operations".into());
            }
        }
        operations.insert(
            root.into(),
            Progress {
                running: true,
                phase: phase.into(),
                completed: 0,
                total: 0,
            },
        );
        Ok(Self {
            root: root.into(),
            finished: false,
        })
    }
    pub(super) fn finish(&mut self) {
        self.finished = true;
        update(&self.root, "complete", 0, 0);
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        if let Some(progress) = OPERATIONS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(&self.root)
        {
            progress.running = false;
            if !self.finished {
                progress.phase = "failed".into();
            }
        }
    }
}
pub(super) fn update(root: &str, phase: &str, completed: usize, total: usize) {
    if let Some(progress) = OPERATIONS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_mut(root)
    {
        progress.phase = phase.into();
        progress.completed = completed;
        progress.total = total;
    }
}
pub(super) fn status(root: &str) -> Result<serde_json::Value, String> {
    let value = OPERATIONS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(root)
        .cloned()
        .unwrap_or(Progress {
            running: false,
            phase: "idle".into(),
            completed: 0,
            total: 0,
        });
    serde_json::to_value(value).map_err(|_| "Cannot encode backup status".into())
}
