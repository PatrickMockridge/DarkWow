//! Deterministic test output file creation.
//!
//! Every heavyweight test opens its own log file at a known path.
//! Output is written from within Rust — zero shell dependency.
//!
//! File path: `/tmp/hw_<name>_<timestamp>_<counter>.log`
//! The `[test_log]` message printed to stderr tells the operator exactly where.

use std::fs::File;
use std::sync::atomic::{AtomicU64, Ordering};

static LOG_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Create a log file at `/tmp/hw_<test_name>_<timestamp>_<counter>.log`.
/// Prints the path to stderr so the operator knows where output is going.
/// Returns the raw File handle — caller wraps it in a Mutex and attaches
/// it to HeavyweightPipeline.log_file.
pub fn create_log_file(test_name: &str) -> File {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let n = LOG_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = format!("/tmp/hw_{}_{}_{}.log", test_name, ts, n);
    let file = File::create(&path).expect("failed to create test log file");
    eprintln!("[test_log] output -> {}", path);
    file
}
