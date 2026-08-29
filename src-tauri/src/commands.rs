//! The IPC surface.
//!
//! Thin by design (docs/BACKEND.md): every handler here is an argument check
//! and a call into the sampler. If one of these ever needs a test of its own,
//! logic has leaked into it and belongs in `logic/` or `sampler`.

use tauri::State;

use crate::errors::SystemError;
use crate::models::Snapshot;
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
