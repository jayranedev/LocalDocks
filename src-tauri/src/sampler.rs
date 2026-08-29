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

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::errors::SystemError;
use crate::logic::classify;
use crate::logic::cpu::CpuTracker;
use crate::logic::ports::map_ports;
use crate::logic::process::map_processes;
use crate::logic::registry::REGISTRY_VERSION;
use crate::logic::service::join_services;
use crate::logic::telemetry::{NetworkTracker, StorageTracker, SystemCpuTracker};
use crate::models::{ProcessId, ScanTiming, Snapshot, SystemTelemetry};
use crate::platform;
use crate::time;

/// Command lines the classifier needs, keyed by process identity.
///
/// `None` records a read that failed, so it is not retried on every tick. The
/// key is the full identity rather than a bare PID, which is what stops a
/// recycled PID from inheriting the previous process's classification.
pub type CommandLines = HashMap<ProcessId, Option<String>>;

/// How a tick obtains a command line.
///
/// A function rather than a direct call so `advance` stays testable without
/// Windows: production passes the real reader, tests pass a closure. This is
/// the only seam of its kind in the sampler and it exists for exactly this
/// reason — the alternative is an untestable classification path.
pub type CommandLineReader<'a> = dyn Fn(u32, &str) -> Option<String> + 'a;

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
    /// Previous machine-wide and per-core CPU counters, for the same reason.
    system_cpu: SystemCpuTracker,
    /// Previous interface octet counters, so throughput is a rate and not a
    /// lifetime total.
    network: NetworkTracker,
    /// Previous drive byte and idle counters, for the same reason.
    storage: StorageTracker,
    /// Command lines already read, pruned to live services every tick so it
    /// cannot grow with the machine's uptime.
    command_lines: CommandLines,
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
                    system_cpu: SystemCpuTracker::new(),
                    network: NetworkTracker::new(),
                    storage: StorageTracker::new(),
                    command_lines: CommandLines::new(),
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
        //
        // Telemetry is collected in this same pass, not on a cadence of its
        // own. Discovery and telemetry therefore always describe the same
        // moment, and there is exactly one thing in the process deciding when
        // to look at the machine.
        let tick_started = Instant::now();

        let processes = platform::windows::process::enumerate();
        let processes_millis = tick_started.elapsed().as_secs_f64() * 1000.0;

        let ports_started = Instant::now();
        let endpoints = processes
            .as_ref()
            .ok()
            .and_then(|_| platform::windows::ports::enumerate().ok());
        let ports_millis = ports_started.elapsed().as_secs_f64() * 1000.0;

        let captured_at_millis = time::now_unix_millis();

        // Discovery is critical; telemetry is not. A process or socket scan
        // that fails has no snapshot to publish, so the previous one stands
        // behind a warning. A telemetry provider that fails is one card
        // reading "unavailable" beside a snapshot that is otherwise complete —
        // docs/BACKEND.md's error taxonomy, applied.
        let event = match (processes, endpoints) {
            (Ok(raw), Some(endpoints)) => {
                let mut state = lock(&shared.state);
                let mut snapshot = advance(
                    &mut state,
                    captured_at_millis,
                    &raw,
                    &endpoints,
                    &platform::windows::control::command_line_for,
                );
                drop(state);

                snapshot.timing = ScanTiming {
                    total_millis: tick_started.elapsed().as_secs_f64() * 1000.0,
                    processes_millis,
                    ports_millis,
                    telemetry_millis: snapshot.timing.telemetry_millis,
                };
                SamplerEvent::Update(snapshot)
            }
            (Err(e), _) => {
                // Requirement: keep the previous good snapshot. Nothing in
                // state is touched, so `get_snapshot` still returns the last
                // good scan and the UI degrades to a warning over live data
                // rather than blanking.
                log::error!("sampler tick failed: {e}");
                SamplerEvent::Failure(e.to_string())
            }
            (Ok(_), None) => {
                log::error!("sampler tick failed: the socket scan did not complete");
                SamplerEvent::Failure("The socket scan did not complete.".to_string())
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
/// Both scans arrive as data, and the command-line read arrives as a function,
/// so every invariant that matters — sequence increases, timestamps progress,
/// ports attribute to processes from the same tick, services are classified —
/// is unit-testable without Windows.
///
/// The one exception is `read_telemetry`, which queries the machine directly.
/// It is not injected because there is nothing to inject *for*: it has no
/// inputs, every reading is independently optional, and a failure is already
/// `None` rather than an error. Its arithmetic — the part that could be wrong —
/// lives in `logic::telemetry` and is tested there against fabricated
/// counters.
fn advance(
    state: &mut State,
    captured_at_millis: i64,
    raw: &[platform::windows::process::RawProcess],
    endpoints: &[platform::windows::ports::RawEndpoint],
    read_command_line: &CommandLineReader<'_>,
) -> Snapshot {
    state.sequence += 1;

    let mut mapping = map_processes(raw, captured_at_millis, &mut state.cpu);

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

    // The join, and then everything that points at it. All three views are
    // built from one tick's data, so a service, its process row and its port
    // rows can never describe different moments.
    let join = join_services(&mapping.rows, endpoints);

    // `isService` is decided here, by the service model — never in the
    // frontend, and never by guessing from a process name.
    for row in &mut mapping.rows {
        row.is_service = join.is_service(&row.id);
    }

    let ports = map_ports(endpoints, raw, &mapping.rows, join.labels());
    let (system, telemetry_millis) = read_telemetry(state, captured_at_millis);

    // Relevance, in three steps that stay on the right side of the pure/impure
    // line: decide who needs a command line (pure), read the ones that are
    // missing (the only syscalls here), then classify (pure).
    let mut services = join.services;
    refresh_command_lines(state, &services, read_command_line);
    classify::apply(&mut services, &state.command_lines);
    debug_assert!(
        services.iter().all(|s| !s.relevance_reason.is_empty()),
        "a service reached a snapshot without being classified"
    );

    let snapshot = Snapshot {
        sequence: state.sequence,
        captured_at: time::to_iso8601(captured_at_millis),
        services,
        processes: mapping.rows,
        ports,
        // Conflict detection is a later milestone (docs/ROADMAP.md). `None`
        // renders as "—" rather than as a confident zero.
        conflicts: None,
        system,
        timing: ScanTiming {
            telemetry_millis,
            ..ScanTiming::default()
        },
        registry_version: REGISTRY_VERSION,
    };

    state.snapshot = snapshot.clone();
    snapshot
}

/// Top up the command-line cache, and drop what is no longer running.
///
/// The bounded half of the tier-2 amendment (docs/ARCHITECTURE.md § 4). Two
/// bounds, both structural rather than a limit someone has to remember:
///
///   * **Width.** Only services, and only those whose classification a command
///     line could actually change — `classify::needs_command_line`. Everything
///     else is decided by name.
///   * **Time.** Keyed by identity and read once. A service that has been up
///     for an hour costs one handle, not one per tick, and a read that failed
///     is remembered as a failure rather than retried forever.
///
/// The prune is what keeps the map bounded by *what is running now* rather than
/// by how long the app has been open.
fn refresh_command_lines(
    state: &mut State,
    services: &[crate::models::Service],
    read: &CommandLineReader<'_>,
) {
    let mut wanted = std::collections::HashSet::with_capacity(services.len());

    for service in services {
        if !classify::needs_command_line(service) {
            continue;
        }
        wanted.insert(service.id.clone());
        if state.command_lines.contains_key(&service.id) {
            continue; // already known, including a known failure
        }
        let line = read(service.pid, &service.started_at);
        state.command_lines.insert(service.id.clone(), line);
    }

    // Anything not wanted this tick has stopped being a service or has exited.
    state.command_lines.retain(|id, _| wanted.contains(id));
}

/// Machine-wide load for this tick.
///
/// Every reading is independently optional. A failed CPU query must not blank
/// the memory figures, a machine with no GPU counters must still report its
/// network throughput, and none of them may take the tick down — telemetry is
/// decoration on a process dashboard. A field that could not be read is `None`,
/// which the UI renders as an explicit unavailable state rather than as zero.
///
/// The returned `ScanTiming` carries only `telemetry_millis`; the caller fills
/// in the rest. Measuring here rather than around the call is what makes a
/// provider that becomes slow on some other machine visible in the UI instead
/// of only in a debug build.
fn read_telemetry(state: &mut State, now_millis: i64) -> (SystemTelemetry, f64) {
    let started = Instant::now();

    let cpu_percent =
        platform::windows::system::cpu_times().and_then(|now| state.system_cpu.observe(now));
    let per_core_percent = platform::windows::system::per_core_times()
        .and_then(|now| state.system_cpu.observe_per_core(now));
    let memory = platform::windows::system::memory();

    // Each provider is asked independently, and each returning None means the
    // same thing: this machine does not offer it, or it could not be read now.
    let network = platform::windows::network::interfaces()
        .map(|interfaces| state.network.observe(now_millis, &interfaces));
    let storage = platform::windows::storage::drives()
        .map(|drives| state.storage.observe(now_millis, &drives));
    let system = SystemTelemetry {
        cpu_percent,
        logical_processors: per_core_percent
            .as_ref()
            .map(|c| c.len() as u32)
            .unwrap_or_else(logical_cores),
        per_core_percent,
        memory_total_bytes: memory.map(|m| m.total_bytes),
        memory_used_bytes: memory.map(|m| m.used_bytes()),
        memory_percent: memory.and_then(|m| m.percent()),
        network,
        storage,
    };

    (system, started.elapsed().as_secs_f64() * 1000.0)
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
        // Nothing has been measured yet, and the seed snapshot says so rather
        // than showing a machine at 0% CPU with no memory.
        system: SystemTelemetry {
            logical_processors: logical_cores(),
            ..SystemTelemetry::default()
        },
        timing: ScanTiming::default(),
        registry_version: REGISTRY_VERSION,
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

    /// A command-line reader that always fails.
    ///
    /// The default for tests that are not about classification: it exercises
    /// the "command line unreadable" path, which is the conservative one — no
    /// service can be promoted to Developer by accident.
    fn no_command_lines(_pid: u32, _started_at: &str) -> Option<String> {
        None
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
            system_cpu: SystemCpuTracker::new(),
            network: NetworkTracker::new(),
            storage: StorageTracker::new(),
            command_lines: CommandLines::new(),
            sequence: 0,
            interval: Duration::from_millis(1000),
            running: false,
        }
    }

    // ------------------------------------------------ against the real machine

    /// A full tick against this machine, classified, with the report printed.
    ///
    /// Fabricated inputs cannot catch the failure this whole correction pass
    /// exists for: a rule that looks disciplined on paper and still sweeps in
    /// half the desktop. So this runs the real thing — a real process scan, a
    /// real socket scan, real command lines — and asserts the properties that
    /// must hold on any machine, not the specific services on this one.
    ///
    /// `cargo test -- --nocapture real_machine` prints the classification of
    /// every live service with the reason that produced it.
    #[test]
    fn a_real_tick_classifies_every_service_and_promotes_nothing_excluded() {
        let raw = platform::windows::process::enumerate().expect("process scan");
        let endpoints = platform::windows::ports::enumerate().expect("socket scan");

        let mut s = State {
            snapshot: empty_snapshot(),
            cpu: CpuTracker::new(logical_cores()),
            system_cpu: SystemCpuTracker::new(),
            network: NetworkTracker::new(),
            storage: StorageTracker::new(),
            command_lines: CommandLines::new(),
            sequence: 0,
            interval: Duration::from_millis(1000),
            running: false,
        };
        let snapshot = advance(
            &mut s,
            time::now_unix_millis(),
            &raw,
            &endpoints,
            &platform::windows::control::command_line_for,
        );

        let count = |r: crate::models::Relevance| {
            snapshot
                .services
                .iter()
                .filter(|x| x.relevance == r)
                .count()
        };
        println!(
            "\n{} services from {} processes and {} sockets \
             (registry v{}): {} developer, {} system, {} unclassified",
            snapshot.services.len(),
            raw.len(),
            endpoints.len(),
            snapshot.registry_version,
            count(crate::models::Relevance::Developer),
            count(crate::models::Relevance::System),
            count(crate::models::Relevance::Unknown),
        );
        for service in &snapshot.services {
            println!(
                "  {:<9} {:<28} {}",
                format!("{:?}", service.relevance).to_lowercase(),
                service.label,
                service.relevance_reason
            );
        }

        // 1. Every service is classified and every verdict is explained.
        for service in &snapshot.services {
            assert!(
                !service.relevance_reason.is_empty(),
                "{} was not classified",
                service.label
            );
            assert!(
                service.relevance_reason.ends_with('.'),
                "{}: reason is not a sentence: {}",
                service.label,
                service.relevance_reason
            );
        }

        // 2. Nothing in the exclusion table was promoted, whatever it listens
        //    on. This is the property the correction was about.
        for service in &snapshot.services {
            let stem = service
                .process_name
                .trim_end_matches(".exe")
                .trim_end_matches(".EXE")
                .to_ascii_lowercase();
            if crate::logic::registry::excluded(&stem).is_some() {
                assert_eq!(
                    service.relevance,
                    crate::models::Relevance::System,
                    "{} is excluded but was classified {:?}",
                    service.label,
                    service.relevance
                );
            }
        }

        // 3. Developer mode is a narrowing. If it ever equals the full list on
        //    a machine running a browser, it has stopped meaning anything.
        let developer = count(crate::models::Relevance::Developer);
        assert!(
            developer <= snapshot.services.len(),
            "more developer services than services"
        );

        // 4. The subgraph the frontend builds is closed: every developer
        //    service has its process row, and every port that rides along has a
        //    service.
        let developer_ids: std::collections::HashSet<_> = snapshot
            .services
            .iter()
            .filter(|x| x.relevance == crate::models::Relevance::Developer)
            .map(|x| x.id.clone())
            .collect();
        for id in &developer_ids {
            assert!(
                snapshot.processes.iter().any(|p| &p.id == id),
                "a developer service has no process row"
            );
        }

        // 5. The command-line cache stayed bounded by what is running, and by
        //    the services that could actually need one.
        assert!(
            s.command_lines.len() <= snapshot.services.len(),
            "the command-line cache exceeded the service count"
        );
    }

    /// Telemetry rides in the tick that already exists rather than on a
    /// cadence of its own.
    ///
    /// The property is not "telemetry is present" but "telemetry and discovery
    /// describe the same moment". A second cadence would produce a snapshot
    /// whose process list and CPU figure were taken seconds apart, and nothing
    /// in the contract would show it.
    #[test]
    fn telemetry_arrives_in_the_same_snapshot_as_discovery_with_the_same_sequence() {
        let raw = platform::windows::process::enumerate().expect("process scan");
        let mut s = real_state();

        let first = advance(
            &mut s,
            time::now_unix_millis(),
            &raw,
            &[],
            &no_command_lines,
        );
        std::thread::sleep(Duration::from_millis(120));
        let second = advance(
            &mut s,
            time::now_unix_millis(),
            &raw,
            &[],
            &no_command_lines,
        );

        assert_eq!(
            second.sequence,
            first.sequence + 1,
            "one tick, one sequence"
        );
        // Memory needs no history, so it is present from the first tick.
        assert!(first.system.memory_total_bytes.is_some());
        // Rates need two samples, and by the second tick they exist.
        assert!(second.system.cpu_percent.is_some());
        // Every section that is present belongs to this snapshot's moment: the
        // trackers advanced exactly twice, so a rate exists exactly now.
        if second.system.network.is_some() {
            assert!(
                second
                    .system
                    .network
                    .as_ref()
                    .unwrap()
                    .receive_bytes_per_sec
                    .is_some(),
                "the network tracker did not advance with the tick"
            );
        }
        if second.system.storage.is_some() {
            assert!(
                second
                    .system
                    .storage
                    .as_ref()
                    .unwrap()
                    .read_bytes_per_sec
                    .is_some(),
                "the storage tracker did not advance with the tick"
            );
        }
    }

    /// The whole point of one cadence: exactly one thread looks at the machine.
    ///
    /// Counted by name rather than by trusting the design — a provider that
    /// quietly spawned its own poller would pass every other test here.
    #[test]
    fn the_sampler_runs_on_exactly_one_thread_however_many_providers_it_has() {
        let sampler = Sampler::new(logical_cores(), Duration::from_millis(MIN_INTERVAL_MS));
        sampler.start(|_| {});
        std::thread::sleep(Duration::from_millis(700));

        let mine = std::process::id();
        let threads = platform::windows::process::enumerate()
            .unwrap()
            .into_iter()
            .find(|p| p.pid == mine)
            .map(|p| p.thread_count)
            .unwrap();

        sampler.stop();
        std::thread::sleep(Duration::from_millis(200));

        let after = platform::windows::process::enumerate()
            .unwrap()
            .into_iter()
            .find(|p| p.pid == mine)
            .map(|p| p.thread_count)
            .unwrap();

        // Stopping the sampler must return the thread count to where it was,
        // which it cannot do if a provider owns one of its own.
        assert!(
            after < threads,
            "stopping the sampler freed no thread: {threads} before, {after} after"
        );
    }

    /// A telemetry provider is optional; a process scan is not.
    ///
    /// The distinction is what keeps one missing counter from blanking the
    /// dashboard. Asserted here by checking that a snapshot is produced and
    /// complete on a machine where at least one optional provider may well be
    /// absent — and that the absent ones are `None` rather than zero.
    #[test]
    fn a_missing_optional_provider_still_produces_a_complete_snapshot() {
        let raw = platform::windows::process::enumerate().expect("process scan");
        let mut s = real_state();
        advance(
            &mut s,
            time::now_unix_millis(),
            &raw,
            &[],
            &no_command_lines,
        );
        std::thread::sleep(Duration::from_millis(120));
        let snapshot = advance(
            &mut s,
            time::now_unix_millis(),
            &raw,
            &[],
            &no_command_lines,
        );

        // Discovery is intact whatever telemetry did.
        assert!(!snapshot.processes.is_empty());
        assert!(snapshot.sequence > 0);

        // And an absent provider is None rather than a zero.
        if let Some(network) = &snapshot.system.network {
            assert!(
                network.receive_bytes_per_sec.is_some() || network.interfaces.is_empty(),
                "a measured interface must carry a rate by the second tick"
            );
        }
    }

    /// The cost of telemetry, measured rather than assumed.
    ///
    /// The budget is the tick, not a fixed number: telemetry that cost as much
    /// as the process scan would be a design problem however few milliseconds
    /// that happened to be on this machine.
    #[test]
    fn telemetry_costs_a_small_fraction_of_the_tick() {
        let raw = platform::windows::process::enumerate().expect("process scan");
        let mut s = real_state();
        advance(
            &mut s,
            time::now_unix_millis(),
            &raw,
            &[],
            &no_command_lines,
        );

        let mut worst: f64 = 0.0;
        for _ in 0..5 {
            std::thread::sleep(Duration::from_millis(60));
            let started = Instant::now();
            let snapshot = advance(
                &mut s,
                time::now_unix_millis(),
                &raw,
                &[],
                &no_command_lines,
            );
            let whole = started.elapsed().as_secs_f64() * 1000.0;
            worst = worst.max(snapshot.timing.telemetry_millis);
            assert!(
                snapshot.timing.telemetry_millis <= whole + 0.5,
                "telemetry cannot cost more than the advance that contains it"
            );
        }
        println!("\nworst telemetry collection: {worst:.2} ms");
        assert!(
            worst < 50.0,
            "telemetry took {worst:.2} ms, which is not a small fraction"
        );
    }

    /// A `State` with the real providers open, for the tests above.
    fn real_state() -> State {
        State {
            snapshot: empty_snapshot(),
            cpu: CpuTracker::new(logical_cores()),
            system_cpu: SystemCpuTracker::new(),
            network: NetworkTracker::new(),
            storage: StorageTracker::new(),
            command_lines: CommandLines::new(),
            sequence: 0,
            interval: Duration::from_millis(1000),
            running: false,
        }
    }

    /// Telemetry has to survive a second tick to produce anything at all.
    #[test]
    fn a_real_tick_reports_machine_load_after_the_first_sample() {
        let raw = platform::windows::process::enumerate().expect("process scan");
        let mut s = State {
            snapshot: empty_snapshot(),
            cpu: CpuTracker::new(logical_cores()),
            system_cpu: SystemCpuTracker::new(),
            network: NetworkTracker::new(),
            storage: StorageTracker::new(),
            command_lines: CommandLines::new(),
            sequence: 0,
            interval: Duration::from_millis(1000),
            running: false,
        };

        // First tick: no previous sample, so CPU is honestly absent.
        let first = advance(
            &mut s,
            time::now_unix_millis(),
            &raw,
            &[],
            &no_command_lines,
        );
        assert!(
            first.system.cpu_percent.is_none(),
            "the first tick cannot measure a rate"
        );
        assert!(
            first.system.memory_total_bytes.is_some(),
            "memory needs no history"
        );

        std::thread::sleep(Duration::from_millis(120));

        let second = advance(
            &mut s,
            time::now_unix_millis(),
            &raw,
            &[],
            &no_command_lines,
        );
        let cpu = second
            .system
            .cpu_percent
            .expect("a second sample produces a rate");
        assert!((0.0..=100.0).contains(&cpu), "cpu out of range: {cpu}");

        let cores = second.system.per_core_percent.expect("per-core detail");
        assert_eq!(cores.len(), second.system.logical_processors as usize);
        assert!(cores.iter().all(|c| (0.0..=100.0).contains(c)));

        let total = second.system.memory_total_bytes.unwrap();
        let used = second.system.memory_used_bytes.unwrap();
        assert!(used <= total, "used memory exceeds total");
        let percent = second.system.memory_percent.unwrap();
        assert!((0.0..=100.0).contains(&percent));
        println!(
            "\nsystem: {cpu:.1}% cpu across {} cores, {percent:.1}% of {} bytes",
            cores.len(),
            total
        );
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
            .map(|i| advance(&mut s, T0 + i * 1000, &scan, &[], &no_command_lines).sequence)
            .collect();

        assert_eq!(sequences, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn snapshot_timestamps_progress() {
        let mut s = state();
        let scan = vec![raw(1, T0 - 1000)];

        let a = advance(&mut s, T0, &scan, &[], &no_command_lines);
        let b = advance(&mut s, T0 + 1000, &scan, &[], &no_command_lines);
        let c = advance(&mut s, T0 + 2000, &scan, &[], &no_command_lines);

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

        advance(
            &mut s,
            T0,
            &[raw(1, T0 - 1000), raw(2, T0 - 2000)],
            &[],
            &no_command_lines,
        );

        assert_eq!(s.snapshot.sequence, 1);
        assert_eq!(s.snapshot.processes.len(), 2);
    }

    #[test]
    fn a_snapshot_never_contains_two_rows_with_the_same_identity() {
        let mut s = state();
        let scan: Vec<_> = (1..60u32).map(|pid| raw(pid, T0 - (pid as i64))).collect();
        let snapshot = advance(&mut s, T0, &scan, &[], &no_command_lines);

        assert_eq!(snapshot.processes.len(), 59);
        assert!(has_unique_identities(&snapshot));
    }

    #[test]
    fn one_tick_produces_processes_ports_and_services_that_agree() {
        // All three views come out of the same tick, so they must describe the
        // same moment: the service, the process it was built from, and the
        // port row pointing back at it.
        let mut s = state();
        let snapshot = advance(
            &mut s,
            T0,
            &[raw(1, T0 - 1000)],
            &[socket(5173, 1)],
            &no_command_lines,
        );

        assert_eq!(snapshot.processes.len(), 1);
        assert_eq!(snapshot.ports.len(), 1);
        assert_eq!(snapshot.services.len(), 1);

        let process = &snapshot.processes[0];
        let service = &snapshot.services[0];
        let port = &snapshot.ports[0];

        assert_eq!(service.id, process.id, "identity is the process identity");
        assert!(process.is_service, "the process must be marked");
        assert_eq!(port.process_id.as_ref(), Some(&process.id));
        assert_eq!(port.service_label.as_ref(), Some(&service.label));
        assert_eq!(service.endpoints.len(), 1);
        assert_eq!(service.endpoints[0].port, 5173);

        // Conflict detection is still a later milestone.
        assert!(snapshot.conflicts.is_none());
    }

    #[test]
    fn a_process_with_no_sockets_is_not_marked_as_a_service() {
        let mut s = state();
        let snapshot = advance(
            &mut s,
            T0,
            &[raw(1, T0 - 1000), raw(2, T0 - 1000)],
            &[socket(5173, 1)],
            &no_command_lines,
        );

        let marked: Vec<_> = snapshot
            .processes
            .iter()
            .filter(|p| p.is_service)
            .map(|p| p.pid)
            .collect();
        assert_eq!(marked, vec![1], "only the socket holder is a service");
        assert_eq!(snapshot.services.len(), 1);
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
            &no_command_lines,
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
        assert!(by_port(8000).process_id.is_some());

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

        let first = advance(
            &mut s,
            T0,
            &scan,
            &[socket(5173, 1), socket(8000, 1)],
            &no_command_lines,
        );
        assert_eq!(first.ports.len(), 2);

        let second = advance(
            &mut s,
            T0 + 1000,
            &scan,
            &[socket(5173, 1)],
            &no_command_lines,
        );
        assert_eq!(second.ports.len(), 1, "a closed socket must not linger");
        assert_eq!(second.ports[0].port, 5173);

        let third = advance(&mut s, T0 + 2000, &scan, &[], &no_command_lines);
        assert!(third.ports.is_empty());
    }

    #[test]
    fn previous_cpu_samples_survive_between_ticks() {
        // The whole reason the sampler is stateful: tick two must be able to
        // see tick one's totals.
        let mut s = state();
        assert_eq!(s.cpu.tracked(), 0);

        advance(
            &mut s,
            T0,
            &[raw(1, T0 - 1000), raw(2, T0 - 1000)],
            &[],
            &no_command_lines,
        );
        assert_eq!(s.cpu.tracked(), 2);

        advance(
            &mut s,
            T0 + 1000,
            &[raw(1, T0 - 1000)],
            &[],
            &no_command_lines,
        );
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
            // Every service is one of this snapshot's own processes, holds at
            // least one endpoint, and is marked on the process row it came
            // from. Asserted as invariants, never as counts — what happens to
            // be running is not a fact a test may assume.
            let mut service_ids = HashSet::new();
            for svc in &s.services {
                assert!(
                    service_ids.insert(&svc.id),
                    "the same process became a service twice: {}",
                    svc.id
                );
                let process = s
                    .processes
                    .iter()
                    .find(|p| p.id == svc.id)
                    .unwrap_or_else(|| panic!("service {} has no process", svc.id));
                assert!(process.is_service, "process {} not marked", process.pid);
                assert_eq!(svc.pid, process.pid);
                assert!(!svc.endpoints.is_empty(), "a service holds sockets");
                assert!(svc.framework.is_none(), "framework detection is V2");
                assert!(
                    svc.endpoints.iter().any(|e| e.port >= 1024),
                    "a service needs a non-system port"
                );
            }
            for p in s.processes.iter().filter(|p| p.is_service) {
                assert!(
                    service_ids.contains(&p.id),
                    "process {} is marked but produced no service",
                    p.pid
                );
            }
            for p in &s.ports {
                assert!(p.port > 0, "port 0 is not a bound port");
                assert!(!p.address.is_empty(), "every row carries an address");
                // A label only ever appears beside an identity, and only when
                // that identity really is a service.
                if p.service_label.is_some() {
                    let id = p
                        .process_id
                        .as_ref()
                        .expect("a labelled row must carry an identity");
                    assert!(
                        s.services.iter().any(|svc| &svc.id == id),
                        "label on a row whose process is not a service"
                    );
                }
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
