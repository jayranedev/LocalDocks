//! Mapping enumerated processes onto the `ProcessRow` contract.
//!
//! Pure: it takes what the platform layer already gathered and produces IPC
//! shapes. The one piece of state it touches is the `CpuTracker`, which is
//! itself pure — it remembers numbers, not handles.

use crate::logic::cpu::{CpuObservation, CpuTracker};
use crate::models::{make_process_id, ProcessRow, ProcessStatus};
use crate::platform::windows::process::{ProcessProbe, RawProcess};
use crate::time;

/// The result of mapping one scan.
///
/// The counts are not diagnostics — they are the reason a caller can trust
/// `rows`. Every enumerated process lands in exactly one of the three, so a
/// process can never disappear without something incrementing.
#[derive(Debug, Clone, Default)]
pub struct ProcessMapping {
    pub rows: Vec<ProcessRow>,
    /// Enumerated, but the app may not open them. See the note below on why
    /// these cannot become rows.
    pub access_denied: u32,
    /// Enumerated, then exited before they could be read.
    pub exited_during_scan: u32,
}

/// Turn enumerated processes into contract rows, scoring CPU on the way.
///
/// `captured_at_millis` is the single instant the whole snapshot is measured
/// against, so every `uptimeSeconds` in one tick shares a reference point
/// instead of drifting by however long the scan took, and every CPU percentage
/// is a rate over the same window.
pub fn map_processes(
    raw: &[RawProcess],
    captured_at_millis: i64,
    cpu: &mut CpuTracker,
) -> ProcessMapping {
    let mut mapping = ProcessMapping {
        rows: Vec::with_capacity(raw.len()),
        ..Default::default()
    };

    // Identify first, so the CPU tracker sees the whole scan at once. Scoring
    // per-process inside the loop would work, but folding the scan in one go is
    // what lets the tracker retire processes that vanished this tick.
    let mut readable = Vec::with_capacity(raw.len());
    let mut observations = Vec::with_capacity(raw.len());

    for p in raw {
        let (created_at_millis, cpu_time_100ns, working_set_bytes) = match p.probe {
            ProcessProbe::Read {
                created_at_millis,
                cpu_time_100ns,
                working_set_bytes,
            } => (created_at_millis, cpu_time_100ns, working_set_bytes),
            // Excluded, not hidden. A process whose creation time is unreadable
            // has no `{pid}-{startedAt}` identity, and the contract has no
            // field for a row without one. Emitting it with a synthesised
            // timestamp would hand the UI an identity that looks safe to
            // terminate and is not — the exact failure the identity model
            // exists to prevent. The count is the honest alternative until
            // `ProcessRow` can express "seen but not readable".
            ProcessProbe::AccessDenied => {
                mapping.access_denied += 1;
                continue;
            }
            ProcessProbe::Gone => {
                mapping.exited_during_scan += 1;
                continue;
            }
        };

        let started_at = time::to_iso8601(created_at_millis);
        let id = make_process_id(p.pid, &started_at);

        observations.push(CpuObservation {
            id: id.clone(),
            cpu_time_100ns,
            created_at_millis,
        });
        readable.push((p, id, started_at, created_at_millis, working_set_bytes));
    }

    let percentages = cpu.observe(captured_at_millis, &observations);

    for (p, id, started_at, created_at_millis, working_set_bytes) in readable {
        mapping.rows.push(ProcessRow {
            // A process on its very first tick of a run whose creation time
            // landed in this same millisecond has no measurable window yet.
            // That is one tick, and it resolves itself on the next one.
            cpu_percent: percentages.get(&id).copied().unwrap_or(0.0),
            id,
            pid: p.pid,
            parent_pid: p.parent_pid,
            name: p.name.clone(),
            memory_bytes: working_set_bytes,
            thread_count: p.thread_count,
            uptime_seconds: uptime_seconds(created_at_millis, captured_at_millis),
            started_at,
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
    const SECOND_100NS: u64 = 10_000_000;

    fn raw(pid: u32, probe: ProcessProbe) -> RawProcess {
        RawProcess {
            pid,
            parent_pid: 4,
            name: "node.exe".into(),
            thread_count: 18,
            probe,
        }
    }

    fn read(created_at_millis: i64, cpu_time_100ns: u64, working_set_bytes: u64) -> ProcessProbe {
        ProcessProbe::Read {
            created_at_millis,
            cpu_time_100ns,
            working_set_bytes,
        }
    }

    fn tracker() -> CpuTracker {
        CpuTracker::new(4)
    }

    #[test]
    fn every_enumerated_process_is_accounted_for() {
        // The invariant that makes the counts trustworthy: nothing vanishes.
        let input = vec![
            raw(100, read(CAPTURED - 1000, 0, 1024)),
            raw(200, ProcessProbe::AccessDenied),
            raw(300, ProcessProbe::Gone),
            raw(400, read(CAPTURED - 2000, 0, 2048)),
            raw(500, ProcessProbe::AccessDenied),
        ];

        let m = map_processes(&input, CAPTURED, &mut tracker());

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
        let m = map_processes(&[raw(8420, read(CAPTURED, 0, 0))], CAPTURED, &mut tracker());
        let row = &m.rows[0];

        assert_eq!(row.started_at, "2026-08-28T09:00:00.000Z");
        assert_eq!(row.id, "8420-2026-08-28T09:00:00.000Z");
        assert_eq!(row.id, make_process_id(row.pid, &row.started_at));
    }

    #[test]
    fn identities_within_one_snapshot_are_unique() {
        // Two live processes cannot share a PID, so two rows cannot share an
        // identity. A duplicate would make the UI's React keys collide.
        let input: Vec<_> = (1..40u32)
            .map(|pid| raw(pid, read(CAPTURED - (pid as i64) * 10, 0, 0)))
            .collect();
        let m = map_processes(&input, CAPTURED, &mut tracker());

        let mut seen = std::collections::HashSet::new();
        for row in &m.rows {
            assert!(seen.insert(row.id.clone()), "duplicate identity {}", row.id);
        }
        assert_eq!(seen.len(), input.len());
    }

    #[test]
    fn rows_without_a_readable_probe_are_never_emitted() {
        // Not "are emitted with a guess". The absence is the point.
        let m = map_processes(
            &[
                raw(4, ProcessProbe::AccessDenied),
                raw(88, ProcessProbe::Gone),
            ],
            CAPTURED,
            &mut tracker(),
        );
        assert!(m.rows.is_empty());
    }

    #[test]
    fn memory_is_carried_through_as_the_working_set_reported() {
        let m = map_processes(
            &[raw(1, read(CAPTURED - 1000, 0, 148_897_792))],
            CAPTURED,
            &mut tracker(),
        );
        assert_eq!(m.rows[0].memory_bytes, 148_897_792);
    }

    #[test]
    fn cpu_is_a_rate_across_two_scans_not_a_reading_from_one() {
        let mut cpu = CpuTracker::new(1);
        let started = CAPTURED - 60_000;

        // First scan establishes the baseline.
        let first = map_processes(&[raw(1, read(started, 0, 0))], CAPTURED, &mut cpu);
        assert_eq!(first.rows[0].cpu_percent, 0.0, "no CPU burned since start");

        // Half a core-second of work in the next wall second, on one core.
        let second = map_processes(
            &[raw(1, read(started, SECOND_100NS / 2, 0))],
            CAPTURED + 1000,
            &mut cpu,
        );
        assert_eq!(second.rows[0].cpu_percent, 50.0);
    }

    #[test]
    fn a_restarted_pid_is_scored_as_a_new_process() {
        // Same PID, later creation time: a different identity, so the old
        // process's lifetime CPU total must not become the new one's delta.
        let mut cpu = CpuTracker::new(1);
        let first_start = CAPTURED - 600_000;
        map_processes(
            &[raw(8420, read(first_start, 600 * SECOND_100NS, 0))],
            CAPTURED,
            &mut cpu,
        );

        let restarted_at = CAPTURED + 500;
        let m = map_processes(
            &[raw(8420, read(restarted_at, SECOND_100NS / 4, 0))],
            CAPTURED + 1500,
            &mut cpu,
        );

        // 0.25 core-s over the 1 s it has existed, not over ten minutes.
        assert_eq!(m.rows[0].cpu_percent, 25.0);
        assert_ne!(
            m.rows[0].id,
            make_process_id(8420, &time::to_iso8601(first_start))
        );
    }

    #[test]
    fn uptime_is_measured_against_the_capture_instant() {
        let m = map_processes(
            &[raw(1, read(CAPTURED - 4_342_000, 0, 0))],
            CAPTURED,
            &mut tracker(),
        );
        assert_eq!(m.rows[0].uptime_seconds, 4342.0);
    }

    #[test]
    fn uptime_never_goes_negative() {
        // A process created mid-scan is newer than the capture instant.
        let m = map_processes(
            &[raw(1, read(CAPTURED + 500, 0, 0))],
            CAPTURED,
            &mut tracker(),
        );
        assert_eq!(m.rows[0].uptime_seconds, 0.0);
    }

    #[test]
    fn all_rows_share_one_reference_instant() {
        // Two processes of the same age must report the same uptime, whatever
        // the scan cost between them.
        let m = map_processes(
            &[
                raw(1, read(CAPTURED - 60_000, 0, 0)),
                raw(2, read(CAPTURED - 60_000, 0, 0)),
            ],
            CAPTURED,
            &mut tracker(),
        );
        assert_eq!(m.rows[0].uptime_seconds, m.rows[1].uptime_seconds);
    }

    #[test]
    fn platform_facts_are_carried_through_unchanged() {
        let mut p = raw(8420, read(CAPTURED, 0, 0));
        p.parent_pid = 6104;
        p.name = "pwsh.exe".into();
        p.thread_count = 7;

        let m = map_processes(&[p], CAPTURED, &mut tracker());
        let row = &m.rows[0];

        assert_eq!(row.pid, 8420);
        assert_eq!(row.parent_pid, 6104);
        assert_eq!(row.name, "pwsh.exe");
        assert_eq!(row.thread_count, 7);
    }

    #[test]
    fn no_row_claims_to_be_a_service_while_service_joining_is_unimplemented() {
        let m = map_processes(&[raw(1, read(CAPTURED, 0, 0))], CAPTURED, &mut tracker());
        assert!(!m.rows[0].is_service);
    }

    #[test]
    fn an_empty_scan_maps_to_an_empty_result() {
        let m = map_processes(&[], CAPTURED, &mut tracker());
        assert!(m.rows.is_empty());
        assert_eq!(m.access_denied, 0);
        assert_eq!(m.exited_during_scan, 0);
    }

    #[test]
    fn the_cpu_tracker_only_remembers_processes_that_are_still_there() {
        let mut cpu = tracker();
        let both = vec![
            raw(1, read(CAPTURED - 1000, 0, 0)),
            raw(2, read(CAPTURED - 1000, 0, 0)),
        ];
        map_processes(&both, CAPTURED, &mut cpu);
        assert_eq!(cpu.tracked(), 2);

        map_processes(&both[..1], CAPTURED + 1000, &mut cpu);
        assert_eq!(cpu.tracked(), 1, "an exited process must be retired");
    }
}
