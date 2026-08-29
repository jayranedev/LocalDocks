//! The IPC surface.
//!
//! Thin by design (docs/BACKEND.md): every handler here is an argument check
//! and a call into the sampler. If one of these ever needs a test of its own,
//! logic has leaked into it and belongs in `logic/` or `sampler`.

use tauri::State;

use crate::errors::SystemError;
use crate::logic::identity;
use crate::models::{FieldState, ProcessDetail, ProcessId, Snapshot, TerminateResult};
use crate::platform::windows::control;
use crate::sampler::Sampler;

/// Return the current snapshot.
///
/// A cached state read, never a scan. The sampler owns the cadence
/// (docs/ARCHITECTURE.md § 2), so this exists only to seed a frontend that has
/// just subscribed; every update after that arrives on `services:update`. If
/// this triggered a scan, a React render could drive Windows syscalls, which is
/// the thing the architecture is built to prevent.
///
/// Infallible: reading state cannot fail, and a scan that did fail leaves the
/// previous good snapshot in place rather than surfacing here.
#[tauri::command]
pub fn get_snapshot(sampler: State<'_, Sampler>) -> Snapshot {
    sampler.snapshot()
}

/// Choose the cadence the sampler runs at.
///
/// The UI chooses; the backend owns. Rejects a value outside
/// `sampler::MIN_INTERVAL_MS..=MAX_INTERVAL_MS` rather than clamping, so a
/// caller with a bug hears about it. The error crosses to JavaScript as a
/// rejected promise, which `describeError` in `src/lib/ipc.ts` already handles.
#[tauri::command]
pub fn set_sample_interval(
    interval_ms: u64,
    sampler: State<'_, Sampler>,
) -> Result<(), SystemError> {
    sampler.set_interval(interval_ms)
}

/// Tier-2 fields for one process, fetched when a detail panel opens.
///
/// Infallible: every way this can go wrong is a state of a field rather than an
/// error, so the panel always has something honest to render. A malformed
/// identity — which the app never generates — yields `unavailable` without
/// touching the process at all.
#[tauri::command]
pub fn get_process_detail(process_id: ProcessId) -> ProcessDetail {
    let Some(parsed) = identity::parse(&process_id) else {
        log::warn!("get_process_detail refused a malformed identity: {process_id:?}");
        return ProcessDetail::all(process_id, FieldState::Unavailable);
    };
    control::process_detail(&process_id, parsed.pid, &parsed.started_at)
}

/// Force-terminate a process, after proving it is still the one the caller meant.
///
/// Takes the PID and the creation time separately because that is the shape the
/// frontend already sends. Both halves are required: a PID alone is not an
/// identity (docs/ARCHITECTURE.md § 3), and a mismatch returns `stale` rather
/// than killing whatever now holds the number.
#[tauri::command]
pub fn terminate_process(pid: u32, started_at: String) -> TerminateResult {
    if pid == 0 || started_at.is_empty() {
        return TerminateResult::Stale {
            message: "Incomplete process identity. Nothing was terminated.".to_string(),
        };
    }
    control::terminate(pid, &started_at)
}

/// Open a URL in the user's browser.
///
/// The URL is validated against an allowlist of `http` and `https` before it
/// goes anywhere near the OS — see `logic::url`. No shell is invoked and no
/// argument string is built.
#[tauri::command]
pub fn open_external(url: String) -> Result<(), SystemError> {
    control::open_external(&url)
}
