//! The sampler: scheduling, state and orchestration.
//!
//! docs/ARCHITECTURE.md § 2 — Rust owns the cadence. The frontend subscribes;
//! it never polls, and a React render can never reach a syscall. The argument
//! that settles it is CPU percentage: `GetProcessTimes` is cumulative, so a
//! rate needs the previous sample, so there is a stateful sampler in this
//! design whether or not it is built deliberately.
//!
//! # Shape
//!
//! One dedicated OS thread runs `scan -> publish -> wait -> repeat`. That
//! single choice satisfies several requirements at once and is why there is no
//! machinery here for any of them:
//!
//! * **No overlapping scans.** The loop is sequential. A scan that runs longer
//!   than the interval simply delays the next one; nothing can start a second
//!   scan while one is in flight, because there is nobody to start it.
//! * **No async, so no mutex held across an await.** `std::sync::Mutex` is the
//!   right tool, as docs/ARCHITECTURE.md says. `tokio::Mutex` would buy
//!   nothing: there is nothing to await.
//! * **Bounded state.** One snapshot and one CPU sample per live process.
//!   Resource history is V2 and is deliberately absent.
//!
//! The interval is the *gap between scans*, not a fixed period. A scan taking
//! longer than the interval pushes the next one out rather than stacking up,
//! which is the behaviour that cannot degrade into overlap.
//!
//! # Waiting
//!
//! The wait is a `Condvar::wait_timeout`, not a `thread::sleep`, so an interval
//! change or a shutdown takes effect at once instead of after the current sleep
//! expires. That is also what makes shutdown prompt.

use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::errors::SystemError;
use crate::logic::cpu::CpuTracker;
use crate::logic::ports::map_ports;
use crate::logic::process::map_processes;
use crate::models::Snapshot;
use crate::platform;
use crate::time;

/// Fastest cadence the sampler will accept.
///
/// A full scan of ~400 processes measures around 20 ms on a development
/// machine, so 250 ms keeps the sampler under roughly a 10% duty cycle even if
/// a slower machine is several times worse. Below this the sampler stops being
/// a sampler and starts being a busy loop, and the UI's fastest offered choice
/// is 500 ms — so the floor already leaves headroom.
pub const MIN_INTERVAL_MS: u64 = 250;

/// Slowest cadence the sampler will accept.
///
/// Past a minute the "live" readout is a fiction and the CPU percentage is an
/// average over a window long enough to hide everything interesting. Rejecting
/// it is more honest than accepting a value that makes the display lie.
pub const MAX_INTERVAL_MS: u64 = 60_000;

/// What the sampler hands to whoever is listening.
///
/// A plain callback rather than a Tauri dependency: it keeps this module
/// testable without an app handle, and it is the only outward-facing seam.
/// This is not a `ProcessSource` abstraction — the platform layer is still
/// called directly, exactly as docs/BACKEND.md requires.
pub enum SamplerEvent {
    /// A scan completed. Emitted as `services:update`.
    Update(Snapshot),
    /// A scan failed. Emitted as `services:error`. The previous good snapshot
    /// is still in state, so the UI keeps showing it behind a warning.
    Failure(String),
}

/// Everything the sampler remembers between ticks.
///
/// Deliberately small. Anything that can be recomputed from a scan is not kept.
struct State {
    /// The most recent good snapshot. `get_snapshot` returns a clone of this
    /// and never triggers a scan.
    snapshot: Snapshot,
    /// Previous CPU totals, keyed by process identity. The only reason the
    /// sampler is stateful at all.
    cpu: CpuTracker,
    /// Monotonic tick counter for `Snapshot.sequence`.
    sequence: u64,
    /// Current cadence. Read by the wait, written by `set_sample_interval`.
    interval: Duration,
    /// Cleared on shutdown so the loop exits at its next wake.
    running: bool,
}

struct Shared {
    state: Mutex<State>,
    /// Signalled when the interval changes or the sampler is stopping, so the
    /// waiting thread reacts immediately rather than at the end of its timeout.
    wake: Condvar,
}

pub struct Sampler {
    shared: Arc<Shared>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Sampler {
    /// Build a stopped sampler holding an empty-but-valid snapshot.
    ///
    /// Seeding with an empty snapshot means `get_snapshot` has something
    /// structurally correct to return even if the frontend asks before the
    /// first scan has landed.
    pub fn new(logical_cores: u32, interval: Duration) -> Self {
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(State {
                    snapshot: empty_snapshot(),
                    cpu: CpuTracker::new(logical_cores),
                    sequence: 0,
                    interval,
                    running: false,
                }),
                wake: Condvar::new(),
            }),
            thread: Mutex::new(None),
        }
    }

    /// Start the sampling thread.
    ///
    /// The first scan runs immediately, before any wait, so the UI is not made
    /// to sit through an interval before it sees anything.
    pub fn start<E>(&self, emit: E)
    where
        E: Fn(SamplerEvent) + Send + 'static,
    {
        let mut slot = lock(&self.thread);
        if slot.is_some() {
            return;
        }

        lock(&self.shared.state).running = true;

        let shared = Arc::clone(&self.shared);
        let handle = std::thread::Builder::new()
            .name("localdocks-sampler".into())
            .spawn(move || run(shared, emit))
            .expect("the OS refused to start the sampler thread");

        *slot = Some(handle);
    }

    /// A clone of the cached snapshot.
    ///
    /// Deliberately cheap: this is a state read, never a scan. The frontend
    /// calls it once to seed itself and then lives on `services:update`.
    pub fn snapshot(&self) -> Snapshot {
        lock(&self.shared.state).snapshot.clone()
    }

    /// Change the cadence, taking effect at the next wake rather than after the
    /// current wait expires.
    pub fn set_interval(&self, interval_ms: u64) -> Result<(), SystemError> {
        let interval = validate_interval(interval_ms)?;
        lock(&self.shared.state).interval = interval;
        self.shared.wake.notify_all();
        log::info!("sample interval set to {interval_ms} ms");
        Ok(())
    }

    /// Stop the thread and wait for it to finish.
    ///
    /// Called from Tauri's `Exit` event. The thread wakes from the condvar at
    /// once; if a scan is in flight it finishes that scan first, which is
    /// bounded by the scan cost rather than by the interval.
    pub fn stop(&self) {
        {
            let mut state = lock(&self.shared.state);
            if !state.running {
                return;
            }
            state.running = false;
        }
        self.shared.wake.notify_all();

        if let Some(handle) = lock(&self.thread).take() {
            // A panicked sampler thread is already logged by the panic hook;
            // failing to join must not take the shutdown path down with it.
            if handle.join().is_err() {
                log::error!("the sampler thread panicked before shutdown");
            }
        }
        log::info!("sampler stopped");
    }
}

/// The sampling loop.
fn run<E>(shared: Arc<Shared>, emit: E)
where
    E: Fn(SamplerEvent) + Send + 'static,
{
    log::info!("sampler started");

    loop {
        // Both scans outside the lock. Enumeration is the expensive part and
        // holding the mutex across it would block `get_snapshot` for its
        // duration.
        //
        // Processes first, then sockets: port attribution reads the process
        // list, and doing it in this order means the process a socket is
        // attributed to was observed no later than the socket itself.
        let scanned = platform::windows::process::enumerate()
            .and_then(|processes| platform::windows::ports::enumerate().map(|p| (processes, p)));
        let captured_at_millis = time::now_unix_millis();

        let event = match scanned {
            Ok((raw, endpoints)) => {
                let mut state = lock(&shared.state);
                let snapshot = advance(&mut state, captured_at_millis, &raw, &endpoints);
                drop(state);
                SamplerEvent::Update(snapshot)
            }
            Err(e) => {
                // Requirement: keep the previous good snapshot. Nothing in
                // state is touched, so `get_snapshot` still returns the last
                // good scan and the UI degrades to a warning over live data
                // rather than blanking.
                log::error!("sampler tick failed: {e}");
                SamplerEvent::Failure(e.to_string())
            }
        };

        // Emit with no lock held: a listener that calls back into the sampler
        // would otherwise deadlock.
        emit(event);

        let state = lock(&shared.state);
        if !state.running {
            break;
        }
        let interval = state.interval;
        // wait_timeout releases the lock while waiting and reacquires it on
        // wake, so `set_sample_interval` and `stop` are never blocked by it.
        let (state, _) = shared
            .wake
            .wait_timeout(state, interval)
            .unwrap_or_else(|e| e.into_inner());
        if !state.running {
            break;
        }
    }

    log::info!("sampler loop exited");
}

/// Fold one successful scan into state and produce the snapshot for it.
///
/// Pure with respect to the OS — it takes both scans as data — so the
/// invariants that matter (sequence increases, timestamps progress, ports
/// attribute to processes from the same tick) are unit-testable.
fn advance(
    state: &mut State,
    captured_at_millis: i64,
    raw: &[platform::windows::process::RawProcess],
    endpoints: &[platform::windows::ports::RawEndpoint],
) -> Snapshot {
    state.sequence += 1;

    let mapping = map_processes(raw, captured_at_millis, &mut state.cpu);

    if mapping.access_denied > 0 || mapping.exited_during_scan > 0 {
        // Excluded processes are reported rather than counted silently. The
        // contract has no field for "seen but unreadable", so the log is the
        // only place this is currently visible — see docs/BACKEND.md.
        log::debug!(
            "{} of {} processes omitted: {} access denied, {} exited during the scan",
            mapping.access_denied + mapping.exited_during_scan,
            raw.len(),
            mapping.access_denied,
            mapping.exited_during_scan
        );
    }

    // Attribution uses this tick's own process data, so a socket is never
    // matched against a process list from a different moment.
    let ports = map_ports(endpoints, raw, &mapping.rows);

    let snapshot = Snapshot {
        sequence: state.sequence,
        captured_at: time::to_iso8601(captured_at_millis),
        // Joining processes and endpoints into Services is the next milestone
        // (docs/ROADMAP.md § 2.2). Empty is the truthful representation of work
        // that has not been done, and `conflicts` stays unknown rather than a
        // confident zero.
        services: Vec::new(),
        processes: mapping.rows,
        ports,
        conflicts: None,
    };

    state.snapshot = snapshot.clone();
    snapshot
}

/// Reject a cadence that would make the sampler misbehave.
///
/// Rejecting rather than clamping: a caller asking for 0 ms has a bug, and
/// silently running at 250 ms would hide it. The bounds are constants above so
/// the error can name them.
pub fn validate_interval(interval_ms: u64) -> Result<Duration, SystemError> {
    if !(MIN_INTERVAL_MS..=MAX_INTERVAL_MS).contains(&interval_ms) {
        return Err(SystemError::invalid_interval(
            interval_ms,
            MIN_INTERVAL_MS,
            MAX_INTERVAL_MS,
        ));
    }
    Ok(Duration::from_millis(interval_ms))
}

/// How many logical CPUs to divide by.
///
/// Falls back to 1 rather than failing: over-reporting a busy process is a
/// better failure than dividing by zero, and this cannot legitimately be zero.
pub fn logical_cores() -> u32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or_else(|e| {
            log::warn!("could not read the logical core count ({e}); assuming 1");
            1
        })
}

fn empty_snapshot() -> Snapshot {
    Snapshot {
        sequence: 0,
        captured_at: time::to_iso8601(time::now_unix_millis()),
        services: Vec::new(),
        processes: Vec::new(),
        ports: Vec::new(),
        conflicts: None,
    }
}

/// Take a lock, recovering from poisoning.
///
/// A panic in one tick must not permanently disable sampling. Nothing here is
/// left half-written under the lock — every field is replaced whole — so the
/// data behind a poisoned mutex is still coherent, and `unwrap()` on a path
/// reachable from a command is exactly what docs/BACKEND.md forbids.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Protocol;
    use crate::platform::windows::ports::RawEndpoint;
    use crate::platform::windows::process::{ProcessProbe, RawProcess};
    use std::collections::HashSet;
    use std::net::{IpAddr, Ipv4Addr};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    /// Identities in a snapshot must be unique.
    fn has_unique_identities(snapshot: &Snapshot) -> bool {
        let mut seen = HashSet::with_capacity(snapshot.processes.len());
        snapshot.processes.iter().all(|p| seen.insert(&p.id))
    }

    const T0: i64 = 1_787_907_600_000;

    fn raw(pid: u32, created_at_millis: i64) -> RawProcess {
        RawProcess {
            pid,
            parent_pid: 4,
            name: "node.exe".into(),
            thread_count: 3,
            probe: ProcessProbe::Read {
                created_at_millis,
                cpu_time_100ns: 0,
                working_set_bytes: 4096,
            },
        }
    }

    fn socket(port: u16, pid: u32) -> RawEndpoint {
        RawEndpoint {
            protocol: Protocol::Tcp,
            address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            scope_id: 0,
            port,
            pid,
        }
    }

    fn state() -> State {
        State {
            snapshot: empty_snapshot(),
            cpu: CpuTracker::new(4),
            sequence: 0,
            interval: Duration::from_millis(1000),
            running: false,
        }
    }

    // ---------------------------------------------------------------- interval

    #[test]
    fn a_sane_interval_is_accepted() {
        for ms in [MIN_INTERVAL_MS, 500, 1000, 2000, 5000, MAX_INTERVAL_MS] {
            assert!(validate_interval(ms).is_ok(), "{ms} ms should be accepted");
        }
    }

    #[test]
    fn every_interval_the_ui_offers_is_accepted() {
        // src/lib/settings.ts INTERVALS. If these ever diverge the settings
        // screen would offer a value the backend refuses.
        for ms in [500, 1000, 2000, 5000] {
            assert!(validate_interval(ms).is_ok(), "the UI offers {ms} ms");
        }
    }

    #[test]
    fn an_interval_that_would_busy_loop_is_rejected() {
        for ms in [0, 1, 10, MIN_INTERVAL_MS - 1] {
            assert!(validate_interval(ms).is_err(), "{ms} ms should be rejected");
        }
    }

    #[test]
    fn an_absurdly_slow_interval_is_rejected() {
        assert!(validate_interval(MAX_INTERVAL_MS + 1).is_err());
        assert!(validate_interval(u64::MAX).is_err());
    }

    #[test]
    fn the_rejection_names_the_bounds_it_enforced() {
        let e = validate_interval(0).unwrap_err();
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "invalidInterval");
        assert_eq!(v["requestedMs"], 0);
        assert_eq!(v["minMs"], MIN_INTERVAL_MS);
        assert_eq!(v["maxMs"], MAX_INTERVAL_MS);
    }

    // ------------------------------------------------------------------ advance

    #[test]
    fn the_sequence_increases_by_one_per_tick_and_never_repeats() {
        let mut s = state();
        let scan = vec![raw(1, T0 - 1000)];

        let sequences: Vec<u64> = (0..5)
            .map(|i| advance(&mut s, T0 + i * 1000, &scan, &[]).sequence)
            .collect();

        assert_eq!(sequences, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn snapshot_timestamps_progress() {
        let mut s = state();
        let scan = vec![raw(1, T0 - 1000)];

        let a = advance(&mut s, T0, &scan, &[]);
        let b = advance(&mut s, T0 + 1000, &scan, &[]);
        let c = advance(&mut s, T0 + 2000, &scan, &[]);

        assert!(a.captured_at < b.captured_at);
        assert!(b.captured_at < c.captured_at);
        // ISO-8601 sorts lexicographically, which is why the contract uses it.
        assert_eq!(a.captured_at, "2026-08-28T09:00:00.000Z");
    }

    #[test]
    fn advancing_replaces_the_cached_snapshot() {
        let mut s = state();
        assert_eq!(s.snapshot.sequence, 0);
        assert!(s.snapshot.processes.is_empty());

        advance(&mut s, T0, &[raw(1, T0 - 1000), raw(2, T0 - 2000)], &[]);

        assert_eq!(s.snapshot.sequence, 1);
        assert_eq!(s.snapshot.processes.len(), 2);
    }

    #[test]
    fn a_snapshot_never_contains_two_rows_with_the_same_identity() {
        let mut s = state();
        let scan: Vec<_> = (1..60u32).map(|pid| raw(pid, T0 - (pid as i64))).collect();
        let snapshot = advance(&mut s, T0, &scan, &[]);

        assert_eq!(snapshot.processes.len(), 59);
        assert!(has_unique_identities(&snapshot));
    }

    #[test]
    fn this_milestone_leaves_services_and_conflicts_alone() {
        // Service joining is the next milestone. Empty is the truthful
        // representation of work that has not been done, and `conflicts` stays
        // unknown rather than a confident zero. Ports are now real.
        let mut s = state();
        let snapshot = advance(&mut s, T0, &[raw(1, T0 - 1000)], &[socket(5173, 1)]);

        assert!(snapshot.services.is_empty());
        assert!(snapshot.conflicts.is_none());
        assert!(!snapshot.processes.is_empty());
        assert_eq!(snapshot.ports.len(), 1);
        assert!(snapshot.ports.iter().all(|p| p.service_label.is_none()));
    }

    #[test]
    fn one_tick_attributes_sockets_to_that_same_tick_s_processes() {
        // The whole point of scanning both inside one tick: the identity on a
        // port row must come from the process list captured beside it.
        let mut s = state();
        let snapshot = advance(
            &mut s,
            T0,
            &[raw(1, T0 - 1000), raw(2, T0 - 2000)],
            &[socket(5173, 1), socket(8000, 2), socket(9999, 4242)],
        );

        let by_port = |port: u16| {
            snapshot
                .ports
                .iter()
                .find(|p| p.port == port)
                .unwrap_or_else(|| panic!("no row for port {port}"))
        };

        let owned = by_port(5173);
        let process = snapshot.processes.iter().find(|p| p.pid == 1).unwrap();
        assert_eq!(owned.process_id.as_ref(), Some(&process.id));
        assert_eq!(by_port(8000).process_id.is_some(), true);

        // A PID that was not in this tick's process scan stays informational.
        let orphan = by_port(9999);
        assert_eq!(orphan.process_id, None);
        assert_eq!(orphan.pid, 4242);
    }

    #[test]
    fn ports_are_rebuilt_each_tick_rather_than_accumulating() {
        // No port history in V1: a socket that closes is gone from the next
        // snapshot, and nothing grows without bound.
        let mut s = state();
        let scan = [raw(1, T0 - 1000)];

        let first = advance(&mut s, T0, &scan, &[socket(5173, 1), socket(8000, 1)]);
        assert_eq!(first.ports.len(), 2);

        let second = advance(&mut s, T0 + 1000, &scan, &[socket(5173, 1)]);
        assert_eq!(second.ports.len(), 1, "a closed socket must not linger");
        assert_eq!(second.ports[0].port, 5173);

        let third = advance(&mut s, T0 + 2000, &scan, &[]);
        assert!(third.ports.is_empty());
    }

    #[test]
    fn previous_cpu_samples_survive_between_ticks() {
        // The whole reason the sampler is stateful: tick two must be able to
        // see tick one's totals.
        let mut s = state();
        assert_eq!(s.cpu.tracked(), 0);

        advance(&mut s, T0, &[raw(1, T0 - 1000), raw(2, T0 - 1000)], &[]);
        assert_eq!(s.cpu.tracked(), 2);

        advance(&mut s, T0 + 1000, &[raw(1, T0 - 1000)], &[]);
        assert_eq!(s.cpu.tracked(), 1, "an exited process must be retired");
    }

    // ------------------------------------------------------------------ sampler

    #[test]
    fn a_stopped_sampler_still_answers_get_snapshot() {
        let s = Sampler::new(4, Duration::from_millis(1000));
        let snapshot = s.snapshot();
        assert_eq!(snapshot.sequence, 0);
        assert!(snapshot.processes.is_empty());
        assert!(snapshot.conflicts.is_none());
    }

    #[test]
    fn setting_a_bad_interval_leaves_the_cadence_alone() {
        let s = Sampler::new(4, Duration::from_millis(1000));
        assert!(s.set_interval(0).is_err());
        assert_eq!(lock(&s.shared.state).interval, Duration::from_millis(1000));

        assert!(s.set_interval(2000).is_ok());
        assert_eq!(lock(&s.shared.state).interval, Duration::from_millis(2000));
    }

    #[test]
    fn stopping_a_sampler_that_never_started_is_harmless() {
        let s = Sampler::new(4, Duration::from_millis(1000));
        s.stop();
        s.stop();
    }

    /// Runs the real sampler against real processes. Asserts invariants, never
    /// counts or CPU values — those depend on the machine.
    #[test]
    fn the_running_sampler_holds_its_invariants() {
        let interval = Duration::from_millis(MIN_INTERVAL_MS);
        let sampler = Sampler::new(logical_cores(), interval);

        let seen: Arc<Mutex<Vec<(Instant, Snapshot)>>> = Arc::new(Mutex::new(Vec::new()));
        let failures = Arc::new(AtomicUsize::new(0));
        let concurrent = Arc::new(AtomicUsize::new(0));
        let max_concurrent = Arc::new(AtomicUsize::new(0));

        {
            let seen = Arc::clone(&seen);
            let failures = Arc::clone(&failures);
            let concurrent = Arc::clone(&concurrent);
            let max_concurrent = Arc::clone(&max_concurrent);
            sampler.start(move |event| {
                // Emission happens inside the loop, so counting how many
                // emissions are in flight counts how many scans are in flight.
                let n = concurrent.fetch_add(1, Ordering::SeqCst) + 1;
                max_concurrent.fetch_max(n, Ordering::SeqCst);
                match event {
                    SamplerEvent::Update(s) => lock(&seen).push((Instant::now(), s)),
                    SamplerEvent::Failure(_) => {
                        failures.fetch_add(1, Ordering::SeqCst);
                    }
                }
                concurrent.fetch_sub(1, Ordering::SeqCst);
            });
        }

        std::thread::sleep(Duration::from_millis(1400));
        sampler.stop();

        let snapshots = lock(&seen).clone();
        assert_eq!(
            failures.load(Ordering::SeqCst),
            0,
            "enumeration must not fail on a healthy machine"
        );
        assert!(
            snapshots.len() >= 3,
            "expected several ticks in 1.4 s at {MIN_INTERVAL_MS} ms, got {}",
            snapshots.len()
        );
        assert_eq!(
            max_concurrent.load(Ordering::SeqCst),
            1,
            "scans must never overlap"
        );

        for pair in snapshots.windows(2) {
            let ((at_a, a), (at_b, b)) = (&pair[0], &pair[1]);
            assert_eq!(b.sequence, a.sequence + 1, "sequence must not skip");
            assert!(b.captured_at > a.captured_at, "timestamps must progress");
            // The real no-overlap evidence: consecutive scans are separated by
            // at least the interval. A concurrent scan would land sooner.
            assert!(
                at_b.duration_since(*at_a) >= interval,
                "ticks {} ms apart, closer than the {} ms interval",
                at_b.duration_since(*at_a).as_millis(),
                interval.as_millis()
            );
        }

        for (_, s) in &snapshots {
            assert!(!s.processes.is_empty(), "a live machine has processes");
            assert!(has_unique_identities(s), "duplicate identity in a snapshot");
            assert!(
                s.services.is_empty(),
                "service joining is the next milestone"
            );
            for p in &s.ports {
                assert!(p.port > 0, "port 0 is not a bound port");
                assert!(!p.address.is_empty(), "every row carries an address");
                assert!(p.service_label.is_none(), "labels arrive with Services");
            }
            let mut sockets = HashSet::new();
            for p in &s.ports {
                assert!(
                    sockets.insert((p.protocol, p.address.as_str(), p.port, p.pid)),
                    "duplicate socket row would collide the frontend key: {p:?}"
                );
            }
            for p in &s.processes {
                assert!((0.0..=100.0).contains(&p.cpu_percent), "cpu out of range");
                assert_eq!(p.id, crate::models::make_process_id(p.pid, &p.started_at));
            }
        }

        // The cached snapshot is the last one published, and reading it again
        // does not advance anything.
        let cached = sampler.snapshot();
        assert_eq!(cached.sequence, snapshots.last().unwrap().1.sequence);
        assert_eq!(sampler.snapshot().sequence, cached.sequence);
    }

    /// A live machine always has processes starting and exiting; over a second
    /// of real sampling at least one CPU reading should be non-zero, which is
    /// what proves the delta path is wired rather than returning constants.
    #[test]
    fn cpu_percentages_come_from_deltas_on_a_live_machine() {
        let sampler = Sampler::new(logical_cores(), Duration::from_millis(MIN_INTERVAL_MS));
        let seen: Arc<Mutex<Vec<Snapshot>>> = Arc::new(Mutex::new(Vec::new()));
        {
            let seen = Arc::clone(&seen);
            sampler.start(move |event| {
                if let SamplerEvent::Update(s) = event {
                    lock(&seen).push(s);
                }
            });
        }

        // Give the machine something to measure.
        let spin = std::thread::spawn(|| {
            let end = std::time::Instant::now() + Duration::from_millis(900);
            let mut n: u64 = 0;
            while std::time::Instant::now() < end {
                n = n.wrapping_mul(6364136223846793005).wrapping_add(1);
            }
            n
        });

        std::thread::sleep(Duration::from_millis(1200));
        sampler.stop();
        let _ = spin.join();

        let snapshots = lock(&seen).clone();
        // Skip the first: it is measured over process lifetimes, not a delta.
        let from_deltas: Vec<f32> = snapshots
            .iter()
            .skip(1)
            .flat_map(|s| s.processes.iter().map(|p| p.cpu_percent))
            .collect();

        assert!(
            !from_deltas.is_empty(),
            "no delta-derived samples collected"
        );
        assert!(
            from_deltas.iter().any(|&p| p > 0.0),
            "every process reported exactly 0% across every tick, which means \
             the delta calculation is not wired up"
        );
    }
}
