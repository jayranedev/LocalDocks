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
//!
//! # Everything else
//!
//! Network and storage are the same shape as CPU: cumulative counters that
//! only become a rate when differenced against a previous sample. GPU and
//! thermal are levels and need no history, but they do need folding and
//! filtering, which is judgement and therefore belongs here rather than beside
//! the syscall.
//!
//! Nothing in this file calls Windows. Every function takes the numbers the
//! platform layer already read, which is what lets the awkward cases —
//! a counter that reset, an interface that appeared mid-session, a thermal zone
//! reporting absolute zero — be tested rather than waited for.

use std::collections::{BTreeMap, HashMap};

use crate::models::{
    GpuTelemetry, NetworkInterface, NetworkTelemetry, StorageDrive, StorageTelemetry,
};

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

// ---------------------------------------------------------------- rate maths

/// A cumulative counter turned into a per-second rate.
///
/// Every counter in this module is cumulative — bytes since the interface came
/// up, bytes since the disk was enumerated — so the rate is always a difference
/// over an elapsed time, never the counter itself. Reading a lifetime total as
/// a throughput is the single most common way to make a network graph lie.
///
/// `None` rather than zero in three cases, all of which mean "no measurement",
/// not "no activity":
///
/// * **No elapsed time.** Two samples inside the same millisecond divide by
///   zero; a negative elapsed means the clock moved backwards.
/// * **The counter went backwards.** A 64-bit octet counter does not wrap in
///   any human timescale, but an adapter that is disabled and re-enabled, or a
///   drive that is removed and reattached, restarts from zero. The interval
///   spanning that reset has no meaningful rate, and reporting the raw
///   difference would print a negative or an astronomical one.
/// * **No previous sample**, handled by the callers below.
pub fn per_second(previous: u64, now: u64, elapsed_millis: i64) -> Option<f64> {
    if elapsed_millis <= 0 {
        return None;
    }
    // Reset, not a wrap: see above.
    let delta = now.checked_sub(previous)?;
    Some((delta as f64) * 1000.0 / (elapsed_millis as f64))
}

// -------------------------------------------------------------------- network

/// One interface as the platform layer read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawInterface {
    /// `InterfaceLuid`, the stable identity. Deliberately not the interface
    /// index, which Windows reassigns.
    pub luid: u64,
    pub name: String,
    pub description: String,
    pub in_octets: u64,
    pub out_octets: u64,
    pub link_speed_bits_per_sec: Option<u64>,
}

/// Previous octet counters, keyed by interface LUID.
#[derive(Debug, Clone, Default)]
pub struct NetworkTracker {
    previous: HashMap<u64, (u64, u64)>,
    at_millis: Option<i64>,
}

impl NetworkTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Turn this tick's counters into rates.
    ///
    /// The map is replaced rather than merged, so an interface that
    /// disappeared is forgotten and cannot be resurrected by a later
    /// reappearance carrying a reset counter. That also bounds the map by what
    /// exists now rather than by how long the app has been open.
    pub fn observe(&mut self, now_millis: i64, interfaces: &[RawInterface]) -> NetworkTelemetry {
        let elapsed = self.at_millis.map(|then| now_millis - then);
        let mut current = HashMap::with_capacity(interfaces.len());
        let mut rows = Vec::with_capacity(interfaces.len());

        for i in interfaces {
            let rate = |field: fn(&(u64, u64)) -> u64, now: u64| -> Option<f64> {
                let previous = self.previous.get(&i.luid)?;
                per_second(field(previous), now, elapsed?)
            };

            rows.push(NetworkInterface {
                name: i.name.clone(),
                description: i.description.clone(),
                receive_bytes_per_sec: rate(|p| p.0, i.in_octets),
                transmit_bytes_per_sec: rate(|p| p.1, i.out_octets),
                link_speed_bits_per_sec: i.link_speed_bits_per_sec,
            });
            current.insert(i.luid, (i.in_octets, i.out_octets));
        }

        self.previous = current;
        self.at_millis = Some(now_millis);

        NetworkTelemetry {
            receive_bytes_per_sec: sum_measured(rows.iter().map(|r| r.receive_bytes_per_sec)),
            transmit_bytes_per_sec: sum_measured(rows.iter().map(|r| r.transmit_bytes_per_sec)),
            interfaces: rows,
        }
    }
}

/// Sum the values that exist, or `None` if none did.
///
/// The machine total must not count an interface that has no rate yet. An
/// interface appearing mid-session — a VPN connecting, a phone tethering —
/// carries a lifetime octet count, and treating its first observation as a
/// delta would print several gigabytes per second exactly once. Nor may the
/// total be zero when nothing could be measured: that is the difference
/// between an idle machine and an unmeasured one.
fn sum_measured(values: impl Iterator<Item = Option<f64>>) -> Option<f64> {
    let mut total = 0.0;
    let mut any = false;
    for v in values.flatten() {
        total += v;
        any = true;
    }
    any.then_some(total)
}

// -------------------------------------------------------------------- storage

/// One physical drive's cumulative counters, as the platform layer read them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawDrive {
    pub number: u32,
    pub model: String,
    pub bytes_read: u64,
    pub bytes_written: u64,
    /// Cumulative idle time in 100 ns units, straight from `DISK_PERFORMANCE`.
    pub idle_time_100ns: u64,
}

/// Previous drive counters, keyed by physical drive number.
#[derive(Debug, Clone, Default)]
pub struct StorageTracker {
    previous: HashMap<u32, (u64, u64, u64)>,
    at_millis: Option<i64>,
}

impl StorageTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe(&mut self, now_millis: i64, drives: &[RawDrive]) -> StorageTelemetry {
        let elapsed = self.at_millis.map(|then| now_millis - then);
        let mut current = HashMap::with_capacity(drives.len());
        let mut rows = Vec::with_capacity(drives.len());

        for d in drives {
            let previous = self.previous.get(&d.number).copied();
            let rate = |before: u64, now: u64| -> Option<f64> { per_second(before, now, elapsed?) };

            rows.push(StorageDrive {
                number: d.number,
                model: d.model.clone(),
                read_bytes_per_sec: previous.and_then(|p| rate(p.0, d.bytes_read)),
                write_bytes_per_sec: previous.and_then(|p| rate(p.1, d.bytes_written)),
                active_percent: previous
                    .zip(elapsed)
                    .and_then(|(p, e)| active_percent(p.2, d.idle_time_100ns, e)),
            });
            current.insert(d.number, (d.bytes_read, d.bytes_written, d.idle_time_100ns));
        }

        self.previous = current;
        self.at_millis = Some(now_millis);

        StorageTelemetry {
            read_bytes_per_sec: sum_measured(rows.iter().map(|r| r.read_bytes_per_sec)),
            write_bytes_per_sec: sum_measured(rows.iter().map(|r| r.write_bytes_per_sec)),
            // The busiest drive, not the sum. Two drives at 50% is not a
            // machine at 100%.
            active_percent: rows
                .iter()
                .filter_map(|r| r.active_percent)
                .fold(None, |best: Option<f32>, v| {
                    Some(best.map_or(v, |b| b.max(v)))
                }),
            drives: rows,
        }
    }
}

/// Share of the interval the drive was not idle, 0–100.
///
/// Derived from idle time because that is the counter Windows maintains, and
/// because the obvious alternative is wrong: `ReadTime + WriteTime` exceeds the
/// elapsed time on any device that services requests concurrently, which is
/// every NVMe drive made this decade. Windows' own `% Disk Time` has exactly
/// that flaw and routinely reports several hundred percent.
fn active_percent(previous_idle: u64, now_idle: u64, elapsed_millis: i64) -> Option<f32> {
    if elapsed_millis <= 0 {
        return None;
    }
    let idle_delta = now_idle.checked_sub(previous_idle)?;
    let elapsed_100ns = (elapsed_millis as f64) * 10_000.0;
    let busy = (elapsed_100ns - idle_delta as f64) / elapsed_100ns * 100.0;
    // Clamped because the idle counter and the wall clock are sampled a moment
    // apart, so a fully idle drive can report marginally more idle time than
    // elapsed time.
    Some(busy.clamp(0.0, 100.0) as f32)
}

// ------------------------------------------------------------------------ gpu

/// One engine counter instance, already parsed out of its PDH instance name.
#[derive(Debug, Clone, PartialEq)]
pub struct RawGpuEngine {
    pub adapter: u64,
    /// `3D`, `Copy`, `VideoDecode` and so on. Kept because engines are what
    /// makes summing wrong.
    pub engine_type: String,
    pub utilization_percent: f64,
}

/// One adapter's memory counters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawGpuMemory {
    pub adapter: u64,
    pub dedicated_used_bytes: u64,
    pub shared_used_bytes: u64,
}

/// One adapter as DXGI describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawGpuAdapter {
    pub luid: u64,
    pub name: String,
    pub dedicated_memory_bytes: u64,
}

/// Fold the three GPU sources into one row per adapter.
///
/// Utilisation is the **maximum across engine types**, each engine type first
/// summed across processes. Engines are separate hardware queues that run
/// concurrently — a game rendering while a video decodes has both 3D and Video
/// Decode busy — so adding them reports far over 100% for a machine doing one
/// thing. Maximum is what Task Manager shows and it is the only reading of
/// these counters that stays inside 0–100.
///
/// Adapters are keyed by LUID, which is what lets a counter instance, a memory
/// instance and a DXGI description refer to the same card. An adapter known
/// only to the counters still appears, named by its LUID rather than dropped:
/// the utilisation is real even when the identity is missing.
pub fn fold_gpus(
    adapters: &[RawGpuAdapter],
    engines: &[RawGpuEngine],
    memory: &[RawGpuMemory],
) -> Vec<GpuTelemetry> {
    // BTreeMap rather than HashMap so the order is the adapter LUID order and
    // therefore stable between ticks — a list of cards that reshuffles itself
    // every second is unreadable.
    let mut by_luid: BTreeMap<u64, GpuTelemetry> = BTreeMap::new();

    for m in memory {
        let row = adapter_entry(&mut by_luid, m.adapter);
        row.dedicated_memory_used_bytes = Some(m.dedicated_used_bytes);
        row.shared_memory_used_bytes = Some(m.shared_used_bytes);
    }

    // (adapter, engine type) -> utilisation summed across processes.
    let mut per_engine: BTreeMap<(u64, &str), f64> = BTreeMap::new();
    for e in engines {
        *per_engine
            .entry((e.adapter, e.engine_type.as_str()))
            .or_insert(0.0) += e.utilization_percent;
    }
    // Then the maximum across engine types, per adapter.
    for ((adapter, _), total) in per_engine {
        let row = adapter_entry(&mut by_luid, adapter);
        let best = row.utilization_percent.unwrap_or(0.0).max(total as f32);
        row.utilization_percent = Some(best.clamp(0.0, 100.0));
    }

    for a in adapters {
        let row = adapter_entry(&mut by_luid, a.luid);
        row.name = a.name.clone();
        row.dedicated_memory_total_bytes = Some(a.dedicated_memory_bytes);
    }

    // Windows keeps counter instances for adapters DXGI does not enumerate:
    // the Basic Render Driver, remote-session adapters, and stale entries. On
    // the development machine that is four counter LUIDs against two real
    // cards. Keeping the two extra rows would make the UI say "3 other
    // adapters" about a laptop with two.
    //
    // So an adapter survives if DXGI described it — that is a real card,
    // however idle — or if its counters show something. What is dropped is
    // only the rows that are both anonymous and entirely zero, which by
    // definition carry no measurement.
    let described: std::collections::BTreeSet<u64> = adapters.iter().map(|a| a.luid).collect();
    by_luid
        .into_iter()
        .filter(|(luid, row)| described.contains(luid) || has_activity(row))
        .map(|(_, row)| row)
        .collect()
}

/// Whether a counter-only adapter measured anything at all.
fn has_activity(row: &GpuTelemetry) -> bool {
    row.utilization_percent.is_some_and(|u| u > 0.0)
        || row.dedicated_memory_used_bytes.is_some_and(|b| b > 0)
        || row.shared_memory_used_bytes.is_some_and(|b| b > 0)
}

/// The row for one adapter, created named by its LUID if nothing has described
/// it yet.
///
/// An adapter known only to the counters still appears rather than being
/// dropped: its utilisation is a real measurement even when DXGI did not
/// enumerate it, and a nameless card is more useful than a missing one.
fn adapter_entry(map: &mut BTreeMap<u64, GpuTelemetry>, luid: u64) -> &mut GpuTelemetry {
    map.entry(luid).or_insert_with(|| GpuTelemetry {
        name: format!("Adapter {luid:#018x}"),
        utilization_percent: None,
        dedicated_memory_used_bytes: None,
        dedicated_memory_total_bytes: None,
        shared_memory_used_bytes: None,
    })
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

    // ------------------------------------------------------------ rate maths

    #[test]
    fn a_rate_is_the_difference_over_the_elapsed_time() {
        // 1000 bytes in 1000 ms is 1000 B/s.
        assert_eq!(per_second(0, 1_000, 1_000), Some(1_000.0));
        // The same difference in half the time is twice the rate.
        assert_eq!(per_second(0, 1_000, 500), Some(2_000.0));
        // A counter that did not move is a real zero, not an absence.
        assert_eq!(per_second(5_000, 5_000, 1_000), Some(0.0));
    }

    #[test]
    fn zero_or_negative_elapsed_time_reports_nothing_rather_than_dividing() {
        assert_eq!(per_second(0, 1_000, 0), None);
        assert_eq!(per_second(0, 1_000, -50), None);
    }

    #[test]
    fn a_counter_that_went_backwards_reports_nothing_rather_than_a_wrong_rate() {
        // An adapter disabled and re-enabled restarts from zero. The interval
        // spanning that has no meaningful rate.
        assert_eq!(per_second(1_000_000, 5, 1_000), None);
    }

    // --------------------------------------------------------------- network

    fn iface(luid: u64, name: &str, r#in: u64, out: u64) -> RawInterface {
        RawInterface {
            luid,
            name: name.into(),
            description: format!("{name} adapter"),
            in_octets: r#in,
            out_octets: out,
            link_speed_bits_per_sec: Some(1_000_000_000),
        }
    }

    #[test]
    fn the_first_network_sample_has_no_rate_and_does_not_report_zero() {
        let mut t = NetworkTracker::new();
        // A lifetime total of 52 GB must not become 52 GB per second.
        let n = t.observe(
            1_000,
            &[iface(1, "Ethernet", 52_000_000_000, 1_000_000_000)],
        );

        assert_eq!(
            n.receive_bytes_per_sec, None,
            "no previous sample means no rate"
        );
        assert_eq!(n.transmit_bytes_per_sec, None);
        assert_eq!(n.interfaces.len(), 1, "the interface is still listed");
        assert_eq!(n.interfaces[0].receive_bytes_per_sec, None);
        assert_eq!(n.interfaces[0].name, "Ethernet");
    }

    #[test]
    fn a_second_network_sample_produces_a_rate() {
        let mut t = NetworkTracker::new();
        t.observe(1_000, &[iface(1, "Ethernet", 1_000, 500)]);
        let n = t.observe(2_000, &[iface(1, "Ethernet", 3_000, 1_500)]);

        assert_eq!(n.receive_bytes_per_sec, Some(2_000.0));
        assert_eq!(n.transmit_bytes_per_sec, Some(1_000.0));
    }

    #[test]
    fn a_network_counter_reset_reports_nothing_for_that_interval_and_recovers() {
        let mut t = NetworkTracker::new();
        t.observe(1_000, &[iface(1, "Ethernet", 900_000, 900_000)]);

        // The adapter was disabled and re-enabled: counters restart.
        let reset = t.observe(2_000, &[iface(1, "Ethernet", 100, 50)]);
        assert_eq!(reset.receive_bytes_per_sec, None);
        assert_eq!(reset.interfaces[0].receive_bytes_per_sec, None);

        // The next interval is measured against the new baseline.
        let after = t.observe(3_000, &[iface(1, "Ethernet", 1_100, 550)]);
        assert_eq!(after.receive_bytes_per_sec, Some(1_000.0));
        assert_eq!(after.transmit_bytes_per_sec, Some(500.0));
    }

    #[test]
    fn an_interface_appearing_contributes_nothing_rather_than_its_lifetime_total() {
        // The bug this guards: a VPN connecting mid-session brings a counter
        // with several gigabytes on it, and treating that as one interval's
        // traffic prints a multi-gigabyte spike exactly once.
        let mut t = NetworkTracker::new();
        t.observe(1_000, &[iface(1, "Ethernet", 1_000, 1_000)]);

        let n = t.observe(
            2_000,
            &[
                iface(1, "Ethernet", 2_000, 2_000),
                iface(2, "VPN", 9_000_000_000, 9_000_000_000),
            ],
        );

        // Only the interface that had a previous sample counts.
        assert_eq!(n.receive_bytes_per_sec, Some(1_000.0));
        assert_eq!(n.interfaces.len(), 2, "the new interface is still listed");
        let vpn = n.interfaces.iter().find(|i| i.name == "VPN").unwrap();
        assert_eq!(vpn.receive_bytes_per_sec, None);

        // And it does count from its second observation.
        let n = t.observe(
            3_000,
            &[
                iface(1, "Ethernet", 3_000, 3_000),
                iface(2, "VPN", 9_000_002_000, 9_000_002_000),
            ],
        );
        assert_eq!(n.receive_bytes_per_sec, Some(3_000.0));
    }

    #[test]
    fn an_interface_disappearing_is_forgotten_and_cannot_be_resurrected() {
        let mut t = NetworkTracker::new();
        t.observe(
            1_000,
            &[iface(1, "Ethernet", 1_000, 0), iface(2, "VPN", 5_000, 0)],
        );
        t.observe(
            2_000,
            &[iface(1, "Ethernet", 2_000, 0), iface(2, "VPN", 6_000, 0)],
        );

        // The VPN drops.
        let gone = t.observe(3_000, &[iface(1, "Ethernet", 3_000, 0)]);
        assert_eq!(gone.interfaces.len(), 1);
        assert_eq!(gone.receive_bytes_per_sec, Some(1_000.0));

        // It comes back with a reset counter. Its stale sample must not have
        // been kept, or this would report a negative or absurd rate.
        let back = t.observe(
            4_000,
            &[iface(1, "Ethernet", 4_000, 0), iface(2, "VPN", 10, 0)],
        );
        let vpn = back.interfaces.iter().find(|i| i.name == "VPN").unwrap();
        assert_eq!(vpn.receive_bytes_per_sec, None);
        assert_eq!(back.receive_bytes_per_sec, Some(1_000.0));
    }

    #[test]
    fn zero_elapsed_time_between_network_samples_reports_nothing() {
        let mut t = NetworkTracker::new();
        t.observe(1_000, &[iface(1, "Ethernet", 1_000, 0)]);
        let n = t.observe(1_000, &[iface(1, "Ethernet", 9_000, 0)]);
        assert_eq!(n.receive_bytes_per_sec, None);
    }

    #[test]
    fn multiple_interfaces_sum_into_the_machine_total() {
        let mut t = NetworkTracker::new();
        t.observe(
            1_000,
            &[
                iface(1, "Ethernet", 0, 0),
                iface(2, "Wi-Fi", 0, 0),
                iface(3, "VPN", 0, 0),
            ],
        );
        let n = t.observe(
            2_000,
            &[
                iface(1, "Ethernet", 1_000, 100),
                iface(2, "Wi-Fi", 2_000, 200),
                iface(3, "VPN", 3_000, 300),
            ],
        );

        assert_eq!(n.receive_bytes_per_sec, Some(6_000.0));
        assert_eq!(n.transmit_bytes_per_sec, Some(600.0));
        assert_eq!(n.interfaces.len(), 3);
        // And each interface keeps its own figure.
        assert_eq!(n.interfaces[1].receive_bytes_per_sec, Some(2_000.0));
    }

    #[test]
    fn no_interfaces_at_all_is_not_a_machine_at_zero() {
        let mut t = NetworkTracker::new();
        t.observe(1_000, &[]);
        let n = t.observe(2_000, &[]);
        assert_eq!(n.receive_bytes_per_sec, None, "unmeasured is not idle");
        assert!(n.interfaces.is_empty());
    }

    #[test]
    fn interfaces_are_keyed_by_luid_not_by_name() {
        // An adapter renamed between ticks must keep its rate.
        let mut t = NetworkTracker::new();
        t.observe(1_000, &[iface(7, "Ethernet", 1_000, 0)]);
        let n = t.observe(2_000, &[iface(7, "Ethernet 2", 2_000, 0)]);
        assert_eq!(n.receive_bytes_per_sec, Some(1_000.0));
    }

    // --------------------------------------------------------------- storage

    fn drive(number: u32, read: u64, write: u64, idle_100ns: u64) -> RawDrive {
        RawDrive {
            number,
            model: format!("PhysicalDrive{number}"),
            bytes_read: read,
            bytes_written: write,
            idle_time_100ns: idle_100ns,
        }
    }

    #[test]
    fn the_first_storage_sample_has_no_rate() {
        let mut t = StorageTracker::new();
        let s = t.observe(
            1_000,
            &[drive(
                0,
                563_000_000_000,
                390_000_000_000,
                1_400_000_000_000,
            )],
        );

        assert_eq!(s.read_bytes_per_sec, None);
        assert_eq!(s.write_bytes_per_sec, None);
        assert_eq!(s.active_percent, None);
        assert_eq!(s.drives.len(), 1, "the drive is still listed");
        assert_eq!(s.drives[0].model, "PhysicalDrive0");
    }

    #[test]
    fn a_second_storage_sample_produces_rates_and_active_time() {
        let mut t = StorageTracker::new();
        t.observe(1_000, &[drive(0, 0, 0, 0)]);
        // One second later: 2 MB read, 1 MB written, idle for 900 ms of it.
        let s = t.observe(2_000, &[drive(0, 2_000_000, 1_000_000, 9_000_000)]);

        assert_eq!(s.read_bytes_per_sec, Some(2_000_000.0));
        assert_eq!(s.write_bytes_per_sec, Some(1_000_000.0));
        let active = s.active_percent.unwrap();
        assert!(
            (active - 10.0).abs() < 0.01,
            "expected 10% active, got {active}"
        );
    }

    #[test]
    fn a_fully_idle_drive_is_zero_percent_and_a_fully_busy_one_is_one_hundred() {
        let mut t = StorageTracker::new();
        t.observe(1_000, &[drive(0, 0, 0, 0)]);
        // Idle for the whole 1000 ms interval.
        assert_eq!(
            t.observe(2_000, &[drive(0, 0, 0, 10_000_000)])
                .active_percent,
            Some(0.0)
        );
        // Idle for none of the next one.
        assert_eq!(
            t.observe(3_000, &[drive(0, 0, 0, 10_000_000)])
                .active_percent,
            Some(100.0)
        );
    }

    #[test]
    fn a_drive_reporting_more_idle_time_than_elapsed_is_clamped_not_negative() {
        // The idle counter and the wall clock are sampled a moment apart.
        let mut t = StorageTracker::new();
        t.observe(1_000, &[drive(0, 0, 0, 0)]);
        let s = t.observe(2_000, &[drive(0, 0, 0, 10_500_000)]);
        assert_eq!(s.active_percent, Some(0.0));
    }

    #[test]
    fn a_storage_counter_reset_reports_nothing_for_that_interval() {
        let mut t = StorageTracker::new();
        t.observe(1_000, &[drive(0, 900_000, 900_000, 900_000)]);
        let s = t.observe(2_000, &[drive(0, 10, 10, 10)]);

        assert_eq!(s.read_bytes_per_sec, None);
        assert_eq!(s.write_bytes_per_sec, None);
        assert_eq!(s.active_percent, None);
    }

    #[test]
    fn a_drive_appearing_or_disappearing_is_handled_like_an_interface() {
        let mut t = StorageTracker::new();
        t.observe(1_000, &[drive(0, 0, 0, 0)]);

        // A USB drive is plugged in carrying a lifetime byte count.
        let s = t.observe(
            2_000,
            &[
                drive(0, 1_000, 0, 10_000_000),
                drive(3, 5_000_000_000, 0, 0),
            ],
        );
        assert_eq!(
            s.read_bytes_per_sec,
            Some(1_000.0),
            "the new drive contributes nothing yet"
        );
        assert_eq!(s.drives.len(), 2);

        // And unplugged again.
        let s = t.observe(3_000, &[drive(0, 2_000, 0, 20_000_000)]);
        assert_eq!(s.drives.len(), 1);
        assert_eq!(s.read_bytes_per_sec, Some(1_000.0));
    }

    #[test]
    fn machine_active_time_is_the_busiest_drive_rather_than_the_sum() {
        // Two drives at 60% is not a machine at 120%.
        let mut t = StorageTracker::new();
        t.observe(1_000, &[drive(0, 0, 0, 0), drive(1, 0, 0, 0)]);
        let s = t.observe(
            2_000,
            &[drive(0, 0, 0, 4_000_000), drive(1, 0, 0, 9_000_000)],
        );

        let active = s.active_percent.unwrap();
        assert!(
            (active - 60.0).abs() < 0.01,
            "expected the busiest drive, got {active}"
        );
        // But the byte rates do sum: two drives reading really is more I/O.
        assert_eq!(s.read_bytes_per_sec, Some(0.0));
    }

    #[test]
    fn zero_elapsed_time_between_storage_samples_reports_nothing() {
        let mut t = StorageTracker::new();
        t.observe(1_000, &[drive(0, 0, 0, 0)]);
        let s = t.observe(1_000, &[drive(0, 9_000, 0, 0)]);
        assert_eq!(s.read_bytes_per_sec, None);
        assert_eq!(s.active_percent, None);
    }

    // ------------------------------------------------------------------- gpu

    fn engine(adapter: u64, kind: &str, percent: f64) -> RawGpuEngine {
        RawGpuEngine {
            adapter,
            engine_type: kind.into(),
            utilization_percent: percent,
        }
    }

    #[test]
    fn gpu_utilisation_sums_within_an_engine_type_and_takes_the_max_across_them() {
        // Two processes both rendering: 30 + 45 on 3D. A video decoding at 20
        // on a different queue. The adapter is 75% busy, not 95%.
        let gpus = fold_gpus(
            &[RawGpuAdapter {
                luid: 1,
                name: "Test GPU".into(),
                dedicated_memory_bytes: 8 << 30,
            }],
            &[
                engine(1, "3D", 30.0),
                engine(1, "3D", 45.0),
                engine(1, "VideoDecode", 20.0),
            ],
            &[],
        );

        assert_eq!(gpus.len(), 1);
        assert_eq!(gpus[0].utilization_percent, Some(75.0));
        assert_eq!(gpus[0].name, "Test GPU");
        assert_eq!(gpus[0].dedicated_memory_total_bytes, Some(8 << 30));
    }

    #[test]
    fn gpu_utilisation_cannot_exceed_one_hundred_percent() {
        let gpus = fold_gpus(&[], &[engine(1, "3D", 80.0), engine(1, "3D", 80.0)], &[]);
        assert_eq!(gpus[0].utilization_percent, Some(100.0));
    }

    #[test]
    fn two_adapters_are_folded_separately_and_ordered_stably() {
        let gpus = fold_gpus(
            &[
                RawGpuAdapter {
                    luid: 20,
                    name: "Discrete".into(),
                    dedicated_memory_bytes: 8 << 30,
                },
                RawGpuAdapter {
                    luid: 10,
                    name: "Integrated".into(),
                    dedicated_memory_bytes: 512 << 20,
                },
            ],
            &[engine(10, "3D", 5.0), engine(20, "3D", 90.0)],
            &[
                RawGpuMemory {
                    adapter: 10,
                    dedicated_used_bytes: 100,
                    shared_used_bytes: 200,
                },
                RawGpuMemory {
                    adapter: 20,
                    dedicated_used_bytes: 300,
                    shared_used_bytes: 400,
                },
            ],
        );

        assert_eq!(gpus.len(), 2);
        // Ordered by LUID, so the list does not reshuffle between ticks.
        assert_eq!(gpus[0].name, "Integrated");
        assert_eq!(gpus[1].name, "Discrete");
        assert_eq!(gpus[0].utilization_percent, Some(5.0));
        assert_eq!(gpus[1].utilization_percent, Some(90.0));
        assert_eq!(gpus[0].dedicated_memory_used_bytes, Some(100));
        assert_eq!(gpus[1].shared_memory_used_bytes, Some(400));
    }

    #[test]
    fn an_adapter_the_counters_know_but_dxgi_did_not_still_appears() {
        let gpus = fold_gpus(&[], &[engine(0x13c64, "3D", 12.0)], &[]);
        assert_eq!(gpus.len(), 1);
        assert_eq!(gpus[0].utilization_percent, Some(12.0));
        // Named by its LUID rather than dropped: the measurement is real even
        // when the identity is missing.
        assert!(gpus[0].name.contains("13c64"), "got {}", gpus[0].name);
        assert_eq!(gpus[0].dedicated_memory_total_bytes, None);
    }

    #[test]
    fn an_anonymous_adapter_with_nothing_on_its_counters_is_dropped() {
        // Windows keeps counter instances for adapters DXGI does not
        // enumerate. One that is both nameless and entirely zero carries no
        // measurement, and keeping it would inflate the adapter count.
        let gpus = fold_gpus(
            &[],
            &[engine(9, "3D", 0.0)],
            &[RawGpuMemory {
                adapter: 9,
                dedicated_used_bytes: 0,
                shared_used_bytes: 0,
            }],
        );
        assert!(gpus.is_empty());
    }

    #[test]
    fn an_adapter_dxgi_described_survives_however_idle_it_is() {
        let gpus = fold_gpus(
            &[RawGpuAdapter {
                luid: 9,
                name: "Idle GPU".into(),
                dedicated_memory_bytes: 1 << 30,
            }],
            &[engine(9, "3D", 0.0)],
            &[RawGpuMemory {
                adapter: 9,
                dedicated_used_bytes: 0,
                shared_used_bytes: 0,
            }],
        );
        assert_eq!(gpus.len(), 1, "a real card is not dropped for being idle");
        assert_eq!(gpus[0].utilization_percent, Some(0.0));
    }

    #[test]
    fn an_adapter_with_memory_but_no_engine_counters_reports_unmeasured_not_idle() {
        let gpus = fold_gpus(
            &[RawGpuAdapter {
                luid: 1,
                name: "Remote".into(),
                dedicated_memory_bytes: 0,
            }],
            &[],
            &[RawGpuMemory {
                adapter: 1,
                dedicated_used_bytes: 5_000,
                shared_used_bytes: 0,
            }],
        );
        assert_eq!(
            gpus[0].utilization_percent, None,
            "no engine counter is not 0%"
        );
        assert_eq!(gpus[0].dedicated_memory_used_bytes, Some(5_000));
    }

    #[test]
    fn no_gpu_sources_at_all_folds_to_an_empty_list() {
        assert!(fold_gpus(&[], &[], &[]).is_empty());
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
