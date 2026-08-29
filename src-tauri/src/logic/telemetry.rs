//! Machine-wide load, as arithmetic.
//!
//! Windows reports CPU the same way for the machine as it does for a process:
//! cumulative counters since boot. A percentage therefore needs two samples,
//! which makes this stateful for exactly the same reason `logic::cpu` is —
//! and, like that module, the state and the maths live here where they can be
//! tested, while the syscall lives in `platform::windows::system`.
//!
//! # The counters
//!
//! `GetSystemTimes` and the per-processor query both report idle, kernel and
//! user time. The one thing that catches everybody: **kernel time already
//! includes idle time.** Total elapsed processor time is `kernel + user`, and
//! the busy share is `(kernel + user - idle) / (kernel + user)`. Subtracting
//! idle from the denominator as well gives numbers that look plausible and are
//! wrong.
//!
//! # Memory
//!
//! Memory needs no history — `GlobalMemoryStatusEx` reports a level, not a
//! total — so it passes through untouched apart from the percentage.

/// One reading of the idle/kernel/user counters, in 100 ns units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CpuTimes {
    pub idle_100ns: u64,
    /// Includes idle. See the module docs.
    pub kernel_100ns: u64,
    pub user_100ns: u64,
}

impl CpuTimes {
    fn total(&self) -> u64 {
        self.kernel_100ns.saturating_add(self.user_100ns)
    }
}

/// Previous readings, so the next tick can produce a rate.
#[derive(Debug, Clone, Default)]
pub struct SystemCpuTracker {
    total: Option<CpuTimes>,
    per_core: Option<Vec<CpuTimes>>,
}

impl SystemCpuTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Machine-wide utilisation since the previous call, 0–100.
    ///
    /// `None` on the first call, which has nothing to difference against.
    /// Deliberately not a "lifetime average since boot" fallback the way
    /// `logic::cpu` uses a process's creation time: for the machine that
    /// number would be an average over days, presented beside a live readout,
    /// and it would be believed. "—" for one interval is the honest cost.
    pub fn observe(&mut self, now: CpuTimes) -> Option<f32> {
        let previous = self.total.replace(now)?;
        busy_percent(previous, now)
    }

    /// Per-logical-processor utilisation since the previous call.
    ///
    /// `None` until there is a previous sample, and also when the processor
    /// count changes between ticks — which a hot-plugged or parked core can do.
    /// Pairing up mismatched lists would attribute one core's time to another.
    pub fn observe_per_core(&mut self, now: Vec<CpuTimes>) -> Option<Vec<f32>> {
        let previous = self.per_core.replace(now.clone())?;
        if previous.len() != now.len() || now.is_empty() {
            return None;
        }
        previous
            .iter()
            .zip(now.iter())
            .map(|(p, n)| busy_percent(*p, *n))
            .collect()
    }
}

/// The busy share between two readings, 0–100.
///
/// `None` when no processor time elapsed at all — two samples taken inside the
/// same clock tick. Reporting 0% for that would be a measurement of nothing
/// presented as a measurement of idleness.
fn busy_percent(previous: CpuTimes, now: CpuTimes) -> Option<f32> {
    // Saturating rather than wrapping: the counters are monotonic, so a
    // negative delta means a reading was bad, and 0 is the safe answer.
    let total = now.total().saturating_sub(previous.total());
    if total == 0 {
        return None;
    }
    let idle = now.idle_100ns.saturating_sub(previous.idle_100ns);
    let busy = total.saturating_sub(idle);
    let percent = (busy as f64) / (total as f64) * 100.0;
    // Clamped because idle can exceed kernel+user by a tick or two at very
    // short intervals, which would otherwise print a negative percentage.
    Some(percent.clamp(0.0, 100.0) as f32)
}

/// A physical memory reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryStatus {
    pub total_bytes: u64,
    pub available_bytes: u64,
}

impl MemoryStatus {
    /// Bytes in use: total minus available.
    ///
    /// "Available" rather than "free" on purpose — it is the number Windows
    /// itself reports as usable, and it counts the standby cache as available
    /// because that memory can be reclaimed. Using "free" would show a healthy
    /// machine as nearly full.
    pub fn used_bytes(&self) -> u64 {
        self.total_bytes.saturating_sub(self.available_bytes)
    }

    /// Used as a share of total, 0–100. `None` if the total is zero, which
    /// would mean the reading itself failed.
    pub fn percent(&self) -> Option<f32> {
        if self.total_bytes == 0 {
            return None;
        }
        let share = (self.used_bytes() as f64) / (self.total_bytes as f64) * 100.0;
        Some(share.clamp(0.0, 100.0) as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn times(idle: u64, kernel: u64, user: u64) -> CpuTimes {
        CpuTimes {
            idle_100ns: idle,
            kernel_100ns: kernel,
            user_100ns: user,
        }
    }

    #[test]
    fn the_first_observation_has_nothing_to_difference_and_reports_nothing() {
        let mut t = SystemCpuTracker::new();
        assert_eq!(t.observe(times(0, 0, 0)), None);
        assert_eq!(t.observe_per_core(vec![times(0, 0, 0)]), None);
    }

    #[test]
    fn a_fully_idle_interval_is_zero_percent() {
        let mut t = SystemCpuTracker::new();
        t.observe(times(1_000, 1_000, 0));
        // All of the elapsed kernel time was idle.
        assert_eq!(t.observe(times(2_000, 2_000, 0)), Some(0.0));
    }

    #[test]
    fn a_fully_busy_interval_is_one_hundred_percent() {
        let mut t = SystemCpuTracker::new();
        t.observe(times(1_000, 1_000, 0));
        assert_eq!(t.observe(times(1_000, 1_500, 500)), Some(100.0));
    }

    /// The mistake the module docs warn about: kernel time already contains
    /// idle time, so the denominator is kernel + user, not kernel + user - idle.
    #[test]
    fn idle_is_subtracted_from_the_numerator_only() {
        let mut t = SystemCpuTracker::new();
        t.observe(times(0, 0, 0));
        // 800 idle inside 1000 kernel, plus 0 user: 20% busy.
        let percent = t.observe(times(800, 1_000, 0)).unwrap();
        assert!((percent - 20.0).abs() < 0.001, "got {percent}");
    }

    #[test]
    fn two_samples_inside_one_clock_tick_report_nothing_rather_than_zero() {
        let mut t = SystemCpuTracker::new();
        t.observe(times(500, 1_000, 200));
        assert_eq!(t.observe(times(500, 1_000, 200)), None);
    }

    #[test]
    fn a_counter_that_went_backwards_cannot_produce_a_negative_percentage() {
        let mut t = SystemCpuTracker::new();
        t.observe(times(5_000, 9_000, 1_000));
        let percent = t.observe(times(4_000, 9_500, 1_000));
        assert!(
            percent.map_or(true, |p| (0.0..=100.0).contains(&p)),
            "{percent:?}"
        );
    }

    #[test]
    fn per_core_percentages_are_computed_independently_and_in_order() {
        let mut t = SystemCpuTracker::new();
        t.observe_per_core(vec![times(0, 0, 0), times(0, 0, 0)]);
        let cores = t
            .observe_per_core(vec![times(0, 1_000, 0), times(1_000, 1_000, 0)])
            .unwrap();
        assert_eq!(cores.len(), 2);
        assert_eq!(cores[0], 100.0, "core 0 was fully busy");
        assert_eq!(cores[1], 0.0, "core 1 was fully idle");
    }

    #[test]
    fn a_changed_processor_count_reports_nothing_rather_than_mispairing_cores() {
        let mut t = SystemCpuTracker::new();
        t.observe_per_core(vec![times(0, 0, 0), times(0, 0, 0)]);
        assert_eq!(t.observe_per_core(vec![times(0, 1_000, 0)]), None);
        // And it recovers on the next matching pair.
        assert!(t.observe_per_core(vec![times(0, 2_000, 0)]).is_some());
    }

    #[test]
    fn an_empty_processor_list_reports_nothing() {
        let mut t = SystemCpuTracker::new();
        t.observe_per_core(Vec::new());
        assert_eq!(t.observe_per_core(Vec::new()), None);
    }

    #[test]
    fn every_percentage_stays_inside_zero_to_one_hundred() {
        let mut t = SystemCpuTracker::new();
        let mut previous = times(0, 0, 0);
        t.observe(previous);
        for step in 1..200u64 {
            previous = times(
                previous.idle_100ns + step % 7,
                previous.kernel_100ns + step % 5,
                previous.user_100ns + step % 3,
            );
            if let Some(p) = t.observe(previous) {
                assert!((0.0..=100.0).contains(&p), "{p} out of range");
            }
        }
    }

    #[test]
    fn memory_used_is_total_minus_available() {
        let m = MemoryStatus {
            total_bytes: 16_000_000_000,
            available_bytes: 4_000_000_000,
        };
        assert_eq!(m.used_bytes(), 12_000_000_000);
        let percent = m.percent().unwrap();
        assert!((percent - 75.0).abs() < 0.001, "got {percent}");
    }

    #[test]
    fn a_zero_total_is_a_failed_reading_rather_than_zero_percent_used() {
        let m = MemoryStatus {
            total_bytes: 0,
            available_bytes: 0,
        };
        assert_eq!(m.percent(), None);
    }

    #[test]
    fn available_exceeding_total_cannot_underflow() {
        let m = MemoryStatus {
            total_bytes: 1_000,
            available_bytes: 2_000,
        };
        assert_eq!(m.used_bytes(), 0);
        assert_eq!(m.percent(), Some(0.0));
    }
}
