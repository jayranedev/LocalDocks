mod models;
mod time;

use std::sync::atomic::{AtomicU64, Ordering};

use models::Snapshot;

/// Tick counter for `Snapshot.sequence`.
///
/// The contract calls this a monotonic tick counter, so it is one — returning a
/// hardcoded 0 would misreport a documented field. Ownership moves to the
/// sampler when that lands (milestone 3); this is the smallest honest
/// implementation until then.
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![set_sample_interval, get_snapshot])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Tell the backend which cadence to sample at.
///
/// The frontend chooses the interval; the backend owns the loop. Nothing to own
/// yet — the sampler is milestone 3 — so this records the request and returns.
#[tauri::command]
fn set_sample_interval(interval_ms: u64) {
    log::info!("sample interval requested: {interval_ms} ms (sampler not implemented yet)");
}

/// Return the current snapshot.
///
/// Structurally complete and truthful: no enumeration has run, so every
/// collection is empty and `conflicts` is unknown rather than zero. Windows
/// process and port discovery are the next milestones.
///
/// Returns `Snapshot` rather than `Result<Snapshot, _>` because there is no
/// failure path yet. When enumeration lands this becomes a `Result`, and the
/// frontend needs no change: a Rust `Err` already arrives as a rejected promise
/// that `describeError` in `src/lib/ipc.ts` handles.
#[tauri::command]
fn get_snapshot() -> Snapshot {
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed) + 1;
    let snapshot = Snapshot::empty(sequence, time::now_iso8601());

    log::info!(
        "get_snapshot -> seq {} at {} (0 services, 0 processes, 0 ports)",
        snapshot.sequence,
        snapshot.captured_at
    );

    snapshot
}
