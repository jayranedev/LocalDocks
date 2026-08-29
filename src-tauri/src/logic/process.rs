//! Mapping enumerated processes onto the `ProcessRow` contract.
//!
//! This is where the honesty rules bite. `ProcessRow` has eleven fields;
//! Toolhelp plus `GetProcessTimes` can truthfully fill seven of them. The other
//! four are handled explicitly below rather than quietly filled in, and each
//! carries the reason it holds the value it holds.

use crate::models::{make_process_id, ProcessRow, ProcessStatus};
use crate::platform::windows::process::{CreationTime, RawProcess};
use crate::time;

/// The result of mapping one scan.
///
/// The counts are not diagnostics — they are the reason a caller can trust
/// `rows`. Every enumerated process lands in exactly one of the three, so a
/// process can never disappear without something incrementing.
#[derive(Debug, Clone, Default)]
pub struct ProcessMapping {
    pub rows: Vec<ProcessRow>,
    /// Enumerated, but the app may not open them. See the module note on why
    /// these cannot become rows.
    pub access_denied: u32,
    /// Enumerated, then exited before their creation time could be read.
    pub exited_during_scan: u32,
}

/// Placeholder CPU usage.
///
/// CPU percent is a rate, and a rate needs two samples: `GetProcessTimes`
/// returns cumulative kernel and user time, so a single scan cannot compute it
/// at any level of effort. The sampler (docs/ROADMAP.md milestone 3) is what
/// makes this measurable.
///
/// Zero is not a measurement here, and the UI cannot currently tell the
/// difference — `cpuPercent` is `number`, not `number | null`, so it renders as
/// a confident "0.0%". That is the same problem `Snapshot.conflicts` solved by
/// being nullable, and it wants the same fix.
const CPU_NOT_MEASURED: f32 = 0.0;

/// Placeholder memory usage.
///
/// Unlike CPU this is readable in a single call, but it needs
/// `GetProcessMemoryInfo` from `Win32_System_ProcessStatus` — an API outside
/// this milestone's brief. Left at zero and reported rather than reached for
/// unasked.
const MEMORY_NOT_MEASURED: u64 = 0;

/// Turn enumerated processes into contract rows.
///
/// `captured_at_millis` is the single instant the whole snapshot is measured
/// against, so every `uptimeSeconds` in one tick shares a reference point
/// instead of drifting by however long the scan took.
pub fn map_processes(raw: &[RawProcess], captured_at_millis: i64) -> ProcessMapping {
    let mut mapping = ProcessMapping {
        rows: Vec::with_capacity(raw.len()),
        ..Default::default()
    };

    for p in raw {
        let created_at_millis = match p.created_at {
            CreationTime::Known(ms) => ms,
            // Excluded, not hidden. A process whose creation time is unreadable
            // has no `{pid}-{startedAt}` identity, and the contract has no
            // field for a row without one. Emitting it with a synthesised
            // timestamp would hand the UI an identity that looks safe to
            // terminate and is not — the exact failure the identity model
            // exists to prevent. The count is the honest alternative until
            // `ProcessRow` can express "seen but not readable".
            CreationTime::AccessDenied => {
                mapping.access_denied += 1;
                continue;
            }
            CreationTime::Gone => {
                mapping.exited_during_scan += 1;
                continue;
            }
        };

        let started_at = time::to_iso8601(created_at_millis);

        mapping.rows.push(ProcessRow {
            id: make_process_id(p.pid, &started_at),
            pid: p.pid,
            parent_pid: p.parent_pid,
            name: p.name.clone(),
            cpu_percent: CPU_NOT_MEASURED,
            memory_bytes: MEMORY_NOT_MEASURED,
            thread_count: p.thread_count,
            started_at,
            uptime_seconds: uptime_seconds(created_at_millis, captured_at_millis),
            // Not a placeholder. Windows has no process-level sleep state —
            // waiting is a property of threads, not of processes — and a
            // process that appeared in the snapshot is by definition running.
            // `ProcessStatus::Sleeping` exists in the contract for the same
            // reason the TypeScript union does, and stays unreachable until
            // something can actually observe it.
            status: ProcessStatus::Running,
            // Truthful by construction: the field means "also appears in
            // `services`", and `services` is empty until service joining lands
            // (docs/ROADMAP.md milestone 5).
            is_service: false,
        });
    }

    mapping
}

/// Seconds between process creation and the capture instant.
///
/// Clamped at zero: a process created while the scan was walking is younger
/// than the capture instant, and a negative age would render as a process that
/// starts in the future.
fn uptime_seconds(created_at_millis: i64, captured_at_millis: i64) -> f64 {
    let elapsed = captured_at_millis - created_at_millis;
    if elapsed <= 0 {
        return 0.0;
    }
    elapsed as f64 / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPTURED: i64 = 1_787_907_600_000; // 2026-08-28T09:00:00.000Z

    fn raw(pid: u32, created_at: CreationTime) -> RawProcess {
        RawProcess {
            pid,
            parent_pid: 4,
            name: "node.exe".into(),
            thread_count: 18,
            created_at,
        }
    }

    #[test]
    fn every_enumerated_process_is_accounted_for() {
        // The invariant that makes the counts trustworthy: nothing vanishes.
        let input = vec![
            raw(100, CreationTime::Known(CAPTURED - 1000)),
            raw(200, CreationTime::AccessDenied),
            raw(300, CreationTime::Gone),
            raw(400, CreationTime::Known(CAPTURED - 2000)),
            raw(500, CreationTime::AccessDenied),
        ];

        let m = map_processes(&input, CAPTURED);

        assert_eq!(m.rows.len(), 2);
        assert_eq!(m.access_denied, 2);
        assert_eq!(m.exited_during_scan, 1);
        assert_eq!(
            m.rows.len() as u32 + m.access_denied + m.exited_during_scan,
            input.len() as u32
        );
    }

    #[test]
    fn identity_pairs_the_pid_with_the_rendered_start_time() {
        // The `id` must be derivable from the row's own fields, or the frontend
        // and backend can disagree about which process a row refers to.
        let m = map_processes(&[raw(8420, CreationTime::Known(CAPTURED))], CAPTURED);
        let row = &m.rows[0];

        assert_eq!(row.started_at, "2026-08-28T09:00:00.000Z");
        assert_eq!(row.id, "8420-2026-08-28T09:00:00.000Z");
        assert_eq!(row.id, make_process_id(row.pid, &row.started_at));
    }

    #[test]
    fn rows_without_a_readable_start_time_are_never_emitted() {
        // Not "are emitted with a guess". The absence is the point.
        let m = map_processes(
            &[
                raw(4, CreationTime::AccessDenied),
                raw(88, CreationTime::Gone),
            ],
            CAPTURED,
        );
        assert!(m.rows.is_empty());
    }

    #[test]
    fn uptime_is_measured_against_the_capture_instant() {
        let m = map_processes(
            &[raw(1, CreationTime::Known(CAPTURED - 4_342_000))],
            CAPTURED,
        );
        assert_eq!(m.rows[0].uptime_seconds, 4342.0);
    }

    #[test]
    fn uptime_never_goes_negative() {
        // A process created mid-scan is newer than the capture instant.
        let m = map_processes(&[raw(1, CreationTime::Known(CAPTURED + 500))], CAPTURED);
        assert_eq!(m.rows[0].uptime_seconds, 0.0);
    }

    #[test]
    fn all_rows_share_one_reference_instant() {
        // Two processes of the same age must report the same uptime, whatever
        // the scan cost between them.
        let m = map_processes(
            &[
                raw(1, CreationTime::Known(CAPTURED - 60_000)),
                raw(2, CreationTime::Known(CAPTURED - 60_000)),
            ],
            CAPTURED,
        );
        assert_eq!(m.rows[0].uptime_seconds, m.rows[1].uptime_seconds);
    }

    #[test]
    fn platform_facts_are_carried_through_unchanged() {
        let mut p = raw(8420, CreationTime::Known(CAPTURED));
        p.parent_pid = 6104;
        p.name = "pwsh.exe".into();
        p.thread_count = 7;

        let m = map_processes(&[p], CAPTURED);
        let row = &m.rows[0];

        assert_eq!(row.pid, 8420);
        assert_eq!(row.parent_pid, 6104);
        assert_eq!(row.name, "pwsh.exe");
        assert_eq!(row.thread_count, 7);
    }

    #[test]
    fn no_row_claims_to_be_a_service_while_service_joining_is_unimplemented() {
        let m = map_processes(&[raw(1, CreationTime::Known(CAPTURED))], CAPTURED);
        assert!(!m.rows[0].is_service);
    }

    #[test]
    fn an_empty_scan_maps_to_an_empty_result() {
        let m = map_processes(&[], CAPTURED);
        assert!(m.rows.is_empty());
        assert_eq!(m.access_denied, 0);
        assert_eq!(m.exited_during_scan, 0);
    }
}
