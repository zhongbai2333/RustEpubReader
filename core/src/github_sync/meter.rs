//! Session-scoped telemetry; polling never takes the library or network locks.
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        LazyLock, Mutex,
    },
};
#[derive(Clone, Default, Serialize)]
pub struct SyncMeter {
    pub phase: String,
    pub completed: usize,
    pub total: usize,
    pub title: String,
}
static METERS: LazyLock<Mutex<BTreeMap<String, (u64, SyncMeter)>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));
static SERIAL: AtomicU64 = AtomicU64::new(0);
pub(super) struct Reporter {
    root: String,
    serial: u64,
}
impl Reporter {
    pub fn start(root: &str, phase: &str) -> Self {
        let serial = SERIAL.fetch_add(1, Ordering::SeqCst);
        METERS.lock().unwrap_or_else(|e| e.into_inner()).insert(
            root.into(),
            (
                serial,
                SyncMeter {
                    phase: phase.into(),
                    ..Default::default()
                },
            ),
        );
        Self {
            root: root.into(),
            serial,
        }
    }
    pub fn update(&self, phase: &str, completed: usize, total: usize, title: &str) {
        if let Some((serial, meter)) = METERS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(&self.root)
        {
            if *serial == self.serial {
                *meter = SyncMeter {
                    phase: phase.into(),
                    completed,
                    total,
                    title: title.into(),
                };
            }
        }
    }
}
pub fn sync_meter(root: &str) -> SyncMeter {
    METERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(root)
        .map(|(_, m)| m.clone())
        .unwrap_or_default()
}
