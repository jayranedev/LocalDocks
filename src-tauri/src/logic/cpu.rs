//! CPU percentage, as pure arithmetic.
//!
//! Windows reports *cumulative* kernel and user time per process, so a
//! percentage is not a value that can be read — it is a rate, and a rate needs
//! two points. docs/BACKEND.md states the formula:
//!
//! ```text
//! cpu% = Δ(kernel + user) / Δ(wall clock) / core_count
//! ```
//!
//! Dividing by the logical core count means the scale is "share of the whole
//! machine": a single-threaded process pinning one core of eight reads 12.5%,
//! and only a process saturating every core reads 100%. That is the number
//! Task Manager shows, and it is the one that answers "is this what is making
//! my laptop hot".
//!
//! Nothing in this file calls a syscall or knows what Windows is. It takes the
//! core count as a parameter for exactly that reason.

use std::collections::HashMap;

use crate::models::ProcessId;

/// Percentages are clamped here.
///
/// The maths can exceed 100 legitimately: `GetProcessTimes` is quantised to the
/// ~15.6 ms scheduler tick, so over a short interval a process can appear to
/// have used slightly more CPU time than wall time allows. Reporting 104% would
/// be reporting measurement noise as fact.
const MAX_PERCENT: f32 = 100.0;

/// One process's cumulative CPU time at one instant, with the identity it
/// belongs to.
///
/// The identity — not the PID — is the key. Windows recycles PIDs, and a
/// recycled PID carrying the previous occupant's CPU total forward would
/// produce a spectacular fictional spike on the tick after the swap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuObservation {
    pub id: ProcessId,
    /// Kernel + user, in 100-nanosecond units, since the process started.
    pub cpu_time_100ns: u64,
    /// When the process started, Unix milliseconds. Used as the reference point
    /// for a process seen for the first time.
    pub created_at_millis: i64,
}

/// Remembers the previous scan so the next one can be a rate.
///
/// State is bounded by the number of live processes: `observe` replaces the
/// whole map each scan, so an exited process is forgotten on the tick after it
/// disappears and nothing accumulates. This is not resource history — that is
/// V2 (docs/ROADMAP.md) and deliberately absent here.
#[derive(Debug)]
pub struct CpuTracker {
    previous: HashMap<ProcessId, u64>,
    previous_at_millis: Option<i64>,
    logical_cores: u32,
}

impl CpuTracker {
    /// `logical_cores` is queried once by the sampler and passed in, so this
    /// module stays a pure function of its inputs.
    ///
    /// A zero would turn the division into a divide-by-zero, so it is floored
    /// at one. Over-reporting a busy process is a better failure than a NaN
    /// crossing the IPC boundary as `null`.
    pub fn new(logical_cores: u32) -> Self {
        Self {
            previous: HashMap::new(),
            previous_at_millis: None,
            logical_cores: logical_cores.max(1),
        }
    }

    /// How many processes are being remembered.
    ///
    /// Test-only: nothing in production needs this number, but "the map never
    /// grows without bound" is the invariant most worth asserting about this
    /// type, and it cannot be asserted from outside without a way to look.
    #[cfg(test)]
    pub fn tracked(&self) -> usize {
        self.previous.len()
    }

    /// Fold one scan in, and return this scan's percentages.
    ///
    /// The returned map only contains processes a percentage could be computed
    /// for; the caller decides what an absent entry renders as.
    ///
    /// Four cases, all of them ordinary:
    ///
    /// * **Seen before** — the normal path. Rate over the interval since the
    ///   last scan.
    /// * **First observation** — no previous sample, but the process's creation
    ///   time is a real reference point we already hold, so the window becomes
    ///   "since this process started". It is the same formula with a longer
    ///   window, and it means a freshly-appeared process reports a measured
    ///   lifetime average rather than a fabricated zero.
    /// * **Disappeared** — nothing to do. It is simply not in `current`, so it
    ///   is not carried into the new map.
    /// * **PID reused** — a different creation time makes a different identity,
    ///   so the old entry is not found and the new process is treated as new.
    ///   The stale entry is dropped with every other unseen identity.
    pub fn observe(
        &mut self,
        now_millis: i64,
        current: &[CpuObservation],
    ) -> HashMap<ProcessId, f32> {
        let mut percentages = HashMap::with_capacity(current.len());
        let mut next = HashMap::with_capacity(current.len());

        for obs in current {
            let window = match self.previous.get(&obs.id) {
                // Rate since the previous scan.
                Some(&before) => self
                    .previous_at_millis
                    .map(|then| (obs.cpu_time_100ns.saturating_sub(before), now_millis - then)),
                // Rate since the process started.
                None => Some((obs.cpu_time_100ns, now_millis - obs.created_at_millis)),
            };

            if let Some((cpu_delta_100ns, elapsed_millis)) = window {
                if let Some(pct) = percent(cpu_delta_100ns, elapsed_millis, self.logical_cores) {
                    percentages.insert(obs.id.clone(), pct);
                }
            }

            next.insert(obs.id.clone(), obs.cpu_time_100ns);
        }

        self.previous = next;
        self.previous_at_millis = Some(now_millis);
        percentages
    }
}

/// The formula itself, isolated so the edge cases are visible in one place.
///
/// Returns `None` when there is no measurable interval — a process created in
/// the same millisecond as the scan, or two scans that landed on the same
/// clock reading. Zero would be a claim; absence is the truth.
fn percent(cpu_delta_100ns: u64, elapsed_millis: i64, logical_cores: u32) -> Option<f32> {
    if elapsed_millis <= 0 {
        return None;
    }

    // 1 ms = 10_000 * 100 ns.
    let elapsed_100ns = (elapsed_millis as f64) * 10_000.0;
    let share = (cpu_delta_100ns as f64) / elapsed_100ns / (logical_cores as f64);

    // Two decimals: the UI renders one, and rounding here keeps float noise
    // like 0.30000000000000004 out of the JSON.
    let pct = ((share * 100.0 * 100.0).round() / 100.0) as f32;
    Some(pct.clamp(0.0, MAX_PERCENT))
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_787_907_600_000; // 2026-08-28T09:00:00.000Z
    const SECOND_100NS: u64 = 10_000_000;

    fn obs(id: &str, cpu_100ns: u64, created: i64) -> CpuObservation {
        CpuObservation {
            id: id.into(),
            cpu_time_100ns: cpu_100ns,
            created_at_millis: created,
        }
    }

    /// Establish a previous sample without asserting on the first tick, so each
    /// test below starts from the steady state it is actually about.
    fn primed(cores: u32, id: &str, cpu_100ns: u64) -> CpuTracker {
        let mut t = CpuTracker::new(cores);
        t.observe(T0, &[obs(id, cpu_100ns, T0)]);
        t
    }

    #[test]
    fn half_a_core_second_over_one_second_on_one_core_is_fifty_percent() {
        let mut t = primed(1, "8420-x", 0);
        let pct = t.observe(T0 + 1000, &[obs("8420-x", SECOND_100NS / 2, T0)]);
        assert_eq!(pct["8420-x"], 50.0);
    }

    #[test]
    fn the_scale_is_the_whole_machine_not_one_core() {
        // Two full core-seconds of work in one wall second. On four cores that
        // is half the machine; on two cores it is all of it.
        let mut four = primed(4, "p", 0);
        assert_eq!(
            four.observe(T0 + 1000, &[obs("p", 2 * SECOND_100NS, T0)])["p"],
            50.0
        );

        let mut two = primed(2, "p", 0);
        assert_eq!(
            two.observe(T0 + 1000, &[obs("p", 2 * SECOND_100NS, T0)])["p"],
            100.0
        );
    }

    #[test]
    fn a_multithreaded_process_cannot_report_more_than_the_whole_machine() {
        // Eight core-seconds on four cores is 200% before clamping. Scheduler
        // quantisation produces smaller versions of this routinely.
        let mut t = primed(4, "p", 0);
        assert_eq!(
            t.observe(T0 + 1000, &[obs("p", 8 * SECOND_100NS, T0)])["p"],
            100.0
        );
    }

    #[test]
    fn an_idle_process_reports_zero_rather_than_being_omitted() {
        // Zero is a measurement here: the process was seen twice and burned
        // nothing. That is different from having no measurement at all.
        let mut t = primed(4, "p", 0);
        let pct = t.observe(T0 + 1000, &[obs("p", 0, T0)]);
        assert_eq!(pct["p"], 0.0);
    }

    #[test]
    fn a_process_seen_for_the_first_time_is_measured_over_its_lifetime() {
        // No previous sample, but the creation time is a real reference point.
        // Started 2 s ago, burned 1 core-second, on 1 core -> 50%.
        let mut t = CpuTracker::new(1);
        let pct = t.observe(T0 + 2000, &[obs("new", SECOND_100NS, T0)]);
        assert_eq!(pct["new"], 50.0);
    }

    #[test]
    fn a_process_appearing_mid_run_does_not_inherit_the_scan_interval() {
        // The tracker has a previous scan, but not for this process. The window
        // must be since the process started (500 ms), not since the last scan
        // (5000 ms) — those differ by 10x.
        let mut t = primed(1, "old", 0);
        let started = T0 + 4500;
        let pct = t.observe(T0 + 5000, &[obs("new", SECOND_100NS / 4, started)]);
        assert_eq!(pct["new"], 50.0); // 0.25 core-s over 0.5 s on 1 core
    }

    #[test]
    fn a_process_that_disappears_is_forgotten_and_never_reported() {
        let mut t = CpuTracker::new(1);
        t.observe(T0, &[obs("a", 0, T0), obs("b", 0, T0)]);
        assert_eq!(t.tracked(), 2);

        let pct = t.observe(T0 + 1000, &[obs("a", SECOND_100NS, T0)]);
        assert!(pct.contains_key("a"));
        assert!(
            !pct.contains_key("b"),
            "an exited process must not be scored"
        );
        assert_eq!(t.tracked(), 1, "state must shrink when processes exit");
    }

    #[test]
    fn a_reused_pid_does_not_inherit_the_previous_occupants_cpu_time() {
        // The identity carries the creation time, so PID 8420 restarting is a
        // different key. Keyed by PID alone, the new process would appear to
        // have burned the old one's entire lifetime total in one interval.
        let old = "8420-2026-08-28T09:00:00.000Z";
        let new = "8420-2026-08-28T09:00:05.000Z";

        let mut t = CpuTracker::new(1);
        t.observe(T0, &[obs(old, 600 * SECOND_100NS, T0)]);

        let restarted_at = T0 + 5000;
        let pct = t.observe(
            T0 + 6000,
            &[CpuObservation {
                id: new.into(),
                cpu_time_100ns: SECOND_100NS / 4,
                created_at_millis: restarted_at,
            }],
        );

        assert!(!pct.contains_key(old), "the old identity is gone");
        // 0.25 core-s over the 1 s it has been alive, not over 600 s of history.
        assert_eq!(pct[new], 25.0);
        assert_eq!(t.tracked(), 1);
    }

    #[test]
    fn two_scans_at_the_same_instant_produce_no_measurement() {
        // Not zero — zero would claim the process was idle. There is simply no
        // interval to divide by.
        let mut t = primed(1, "p", 0);
        let pct = t.observe(T0, &[obs("p", SECOND_100NS, T0)]);
        assert!(pct.is_empty(), "a zero-length window is not a measurement");
    }

    #[test]
    fn a_process_created_this_millisecond_produces_no_measurement() {
        let mut t = CpuTracker::new(1);
        let pct = t.observe(T0, &[obs("brand-new", 0, T0)]);
        assert!(pct.is_empty());
    }

    #[test]
    fn a_clock_that_goes_backwards_produces_no_measurement_rather_than_a_panic() {
        let mut t = primed(1, "p", 0);
        let pct = t.observe(T0 - 1000, &[obs("p", SECOND_100NS, T0)]);
        assert!(pct.is_empty());
    }

    #[test]
    fn cumulative_time_that_decreases_is_treated_as_no_work() {
        // Should not happen — the counter is monotonic — but an unsigned
        // subtraction underflowing would report a preposterous spike.
        let mut t = primed(1, "p", 10 * SECOND_100NS);
        let pct = t.observe(T0 + 1000, &[obs("p", SECOND_100NS, T0)]);
        assert_eq!(pct["p"], 0.0);
    }

    #[test]
    fn processes_in_one_scan_are_scored_independently() {
        let mut t = CpuTracker::new(4);
        t.observe(T0, &[obs("a", 0, T0), obs("b", 0, T0), obs("c", 0, T0)]);

        let pct = t.observe(
            T0 + 1000,
            &[
                obs("a", 4 * SECOND_100NS, T0), // all four cores -> 100%
                obs("b", SECOND_100NS, T0),     // one core       ->  25%
                obs("c", 0, T0),                // idle           ->   0%
            ],
        );

        assert_eq!(pct["a"], 100.0);
        assert_eq!(pct["b"], 25.0);
        assert_eq!(pct["c"], 0.0);
    }

    #[test]
    fn zero_logical_cores_is_floored_rather_than_dividing_by_zero() {
        let mut t = CpuTracker::new(0);
        t.observe(T0, &[obs("p", 0, T0)]);
        let pct = t.observe(T0 + 1000, &[obs("p", SECOND_100NS / 2, T0)]);
        assert_eq!(pct["p"], 50.0);
        assert!(pct["p"].is_finite());
    }

    #[test]
    fn state_stays_bounded_across_many_scans() {
        let mut t = CpuTracker::new(4);
        for tick in 0..200i64 {
            // A rolling window of 10 identities: each scan retires one and
            // introduces one.
            let live: Vec<_> = (tick..tick + 10)
                .map(|n| obs(&format!("pid-{n}"), (n as u64) * 1000, T0))
                .collect();
            t.observe(T0 + tick * 1000, &live);
        }
        assert_eq!(t.tracked(), 10, "only live processes may be remembered");
    }
}
