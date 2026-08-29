mod errors;
mod logic;
mod models;
mod platform;
mod time;

use std::sync::atomic::{AtomicU64, Ordering};

use errors::SystemError;
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
/// Enumerates real processes. `services` and `ports` stay empty, and
/// `conflicts` stays `None`, because port discovery and service joining are
/// later milestones and an empty list is the truthful representation of work
/// that has not been done.
///
/// Returns `Result` now that there is a failure path: a Rust `Err` arrives in
/// the frontend as a rejected promise, which `describeError` in
/// `src/lib/ipc.ts` already handles. No frontend change is required.
#[tauri::command]
fn get_snapshot() -> Result<Snapshot, SystemError> {
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed) + 1;

    // Read the clock once. Every `uptimeSeconds` in this snapshot is measured
    // against this instant, and `capturedAt` reports the same one, so the two
    // cannot disagree by however long the scan took.
    let captured_at_millis = time::now_unix_millis();

    let raw = platform::windows::process::enumerate().inspect_err(|e| {
        log::error!("process enumeration failed: {e}");
    })?;

    let mapping = logic::process::map_processes(&raw, captured_at_millis);

    // Excluded processes are logged every tick rather than counted silently.
    // The contract has no field for "seen but unreadable", so the log is the
    // only place this is currently visible — see docs/BACKEND.md.
    if mapping.access_denied > 0 || mapping.exited_during_scan > 0 {
        log::info!(
            "{} of {} processes omitted: {} access denied, {} exited during the scan",
            mapping.access_denied + mapping.exited_during_scan,
            raw.len(),
            mapping.access_denied,
            mapping.exited_during_scan
        );
    }

    let snapshot = Snapshot {
        sequence,
        captured_at: time::to_iso8601(captured_at_millis),
        services: Vec::new(),
        processes: mapping.rows,
        ports: Vec::new(),
        conflicts: None,
    };

    log::info!(
        "get_snapshot -> seq {} at {} ({} services, {} processes, {} ports)",
        snapshot.sequence,
        snapshot.captured_at,
        snapshot.services.len(),
        snapshot.processes.len(),
        snapshot.ports.len()
    );

    Ok(snapshot)
}
