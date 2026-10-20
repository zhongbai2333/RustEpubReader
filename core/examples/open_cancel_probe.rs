//! Optimized synchronous-worker cancellation measurements, not UI animation timings.
use reader_core::{
    epub::EpubBook,
    open_operation::{Cancellation, CANCELLED},
};
use serde_json::json;
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};
fn main() {
    if cfg!(debug_assertions) {
        panic!("Use an optimized Cargo profile");
    }
    let path = std::env::args().nth(1).expect("EPUB path");
    let mut samples = Vec::new();
    for repeat in 0..3 {
        for delay_ms in [0, 1, 10] {
            let directory = tempfile::tempdir().unwrap();
            let cache = directory.path().join("index");
            let index = cache.clone();
            let token = Cancellation::default();
            let worker_token = token.clone();
            let source = path.clone();
            let (ready_tx, ready_rx) = mpsc::channel();
            let (done_tx, done_rx) = mpsc::channel();
            // Zero delay deliberately tests cancel-before-dispatch. Positive
            // delays measure a running worker if it has not already completed.
            if delay_ms == 0 {
                token.cancel();
            }
            let worker = std::thread::spawn(move || {
                ready_tx.send(()).unwrap();
                let result = worker_token.run(|| EpubBook::open_cached(&source, &index));
                done_tx
                    .send(result.map(|book| book.chapters.len()))
                    .unwrap();
            });
            ready_rx.recv().unwrap();
            if delay_ms > 0 {
                std::thread::sleep(Duration::from_millis(delay_ms));
            }
            let completed_early = done_rx.try_recv().ok();
            let start = Instant::now();
            token.cancel();
            let result = completed_early.clone().unwrap_or_else(|| {
                done_rx
                    .recv_timeout(Duration::from_secs(30))
                    .expect("Worker failed to quiesce")
            });
            let quiescence_ms = start.elapsed().as_secs_f64() * 1000.;
            worker.join().unwrap();
            let cancelled = result.as_ref().is_err_and(|error| error == CANCELLED);
            if completed_early.is_none() {
                assert!(
                    cancelled,
                    "Active opening did not acknowledge cancellation: {result:?}"
                );
            }
            // No cancelled lifetime leaks into a later normal publication/cache.
            let next = EpubBook::open_cached(&path, &cache).unwrap();
            assert!(!next.chapters.is_empty());
            assert_eq!(next.chapter_cache_stats().1, 0);
            samples.push(json!({"repeat":repeat,"delay_ms":delay_ms,"completed_before_cancel":completed_early.is_some(),"cancelled":cancelled,"cancel_request_to_worker_quiescence_ms":quiescence_ms,"normal_reopen_valid":true}));
        }
    }
    println!("{}",serde_json::to_string_pretty(&json!({"schema":1,"optimized":true,"book_sha256":EpubBook::file_hash(&path).unwrap(),"samples":samples,"passed":true,"measures_ui_cancel_latency":false,"hard_realtime_guarantee":false})).unwrap());
}
