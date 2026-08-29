//! IPC models.
//!
//! These are the Rust half of the contract in `src/types.ts`. A change here
//! without the matching change there is a runtime failure, not a compile error
//! — the boundary is JSON, and JSON has no type checker.
//!
//! Two serde conventions carry the whole mapping:
//!
//!   * `#[serde(rename_all = "camelCase")]` on every struct, because Rust
//!     fields are snake_case and the TypeScript contract is camelCase.
//!   * `Option<T>` for every `| null` in the contract. Serde emits `null` for
//!     `None` by default, and we deliberately do NOT use
//!     `skip_serializing_if` — the frontend expects the key to be present.
//!
//! Scope note: this file models exactly what `Snapshot` needs and nothing else.
//! `ProcessDetail`, `FieldState` and `TerminateResult` are specified in
//! docs/BACKEND.md and will land with the commands that return them, per the
//! rule that types appear when something needs them.

use serde::Serialize;

/// Process identity: `{pid}-{startedAt}`.
///
/// A bare PID is not an identity — Windows recycles them. See
/// docs/ARCHITECTURE.md § 3.
pub type ProcessId = String;

pub fn make_process_id(pid: u32, started_at: &str) -> ProcessId {
    format!("{pid}-{started_at}")
}

/// TS: `type Protocol = 'TCP' | 'UDP'`
///
/// `Hash` because the protocol is one component of an endpoint's identity
/// (docs/ARCHITECTURE.md § 5), and deduplicating sockets means putting that
/// identity in a set.
/// Constructed by port discovery (docs/ROADMAP.md milestone 4). Suppressed
/// narrowly rather than at module level so that dead code anywhere else in
/// this file is a real warning.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Protocol {
    Tcp,
    Udp,
}

/// TS: `interface Endpoint`
/// Constructed by port discovery (docs/ROADMAP.md milestone 4).
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub protocol: Protocol,
    /// Presentation form: "127.0.0.1" or "[::1]".
    pub address: String,
    pub port: u16,
}

/// TS: `Service['status'] = 'running' | 'stopped'`
/// Constructed by service joining (docs/ROADMAP.md milestone 5).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceStatus {
    Running,
    Stopped,
}

/// TS: `ProcessRow['status'] = 'running' | 'sleeping'`
///
/// Deliberately a separate enum from `ServiceStatus`: the contract gives the
/// two rows different variants, and collapsing them into one loose enum would
/// let the backend emit a value the frontend cannot render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProcessStatus {
    Running,
    /// Unreachable today: Windows has no process-level wait state, so nothing
    /// can observe it. Kept because the TypeScript union declares it — the
    /// contract, not the current backend, decides what this enum contains.
    #[allow(dead_code)]
    Sleeping,
}

/// TS: `PortRow['state'] = 'LISTENING'`
/// Constructed by port discovery (docs/ROADMAP.md milestone 4).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PortState {
    Listening,
}

/// TS: `type Relevance = 'developer' | 'system' | 'unknown'`
///
/// What the Developer Registry made of a service. Three outcomes, not two:
/// `Unknown` is the default and it is a real answer, not a shrug. The registry
/// is not exhaustive, so a service it has never seen is reported as
/// unrecognised rather than guessed into one of the other two buckets.
///
/// Developer mode shows `Developer` and nothing else. `System` and `Unknown`
/// both hide, but they are kept apart because they are different claims —
/// "this is Spotify" versus "this has not been classified" — and the
/// difference is what makes a wrong classification reportable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Relevance {
    Developer,
    System,
    Unknown,
}

/// TS: `interface Service` — tier 1, refreshed every sampler tick.
/// Constructed by service joining (docs/ROADMAP.md milestone 5).
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    pub id: ProcessId,
    pub label: String,
    /// `null` until project detection (V2).
    pub framework: Option<String>,
    pub process_name: String,
    pub pid: u32,
    pub parent_pid: u32,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub thread_count: u32,
    /// Process creation time, ISO-8601. Half of the process identity.
    pub started_at: String,
    pub uptime_seconds: f64,
    pub endpoints: Vec<Endpoint>,
    pub status: ServiceStatus,
    /// What the Developer Registry concluded. See `logic::classify`.
    pub relevance: Relevance,
    /// One sentence naming the rule that produced `relevance`, shown in the UI
    /// and printed in the validation report. Never empty: an unexplained
    /// classification is one the user cannot argue with, which is the failure
    /// mode a registry is supposed to avoid.
    pub relevance_reason: String,
}

/// TS: `interface ProcessRow`
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessRow {
    pub id: ProcessId,
    pub pid: u32,
    pub parent_pid: u32,
    pub name: String,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub thread_count: u32,
    pub started_at: String,
    pub uptime_seconds: f64,
    pub status: ProcessStatus,
    /// True when this process also appears in `services`.
    pub is_service: bool,
}

/// TS: `interface PortRow` — one row per socket, deliberately unmerged.
/// Constructed by port discovery (docs/ROADMAP.md milestone 4).
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortRow {
    pub port: u16,
    pub protocol: Protocol,
    pub address: String,
    pub pid: u32,
    /// `None` when the socket could not be attributed to a process the user
    /// owns. The frontend renders such a row as informational only.
    pub process_id: Option<ProcessId>,
    pub process_name: String,
    pub service_label: Option<String>,
    pub state: PortState,
}

/// TS: `interface SystemTelemetry` — machine-wide load, once per tick.
///
/// # Available versus unavailable
///
/// Every reading is `Option`, and `None` always means **not measured** — never
/// "measured as zero". A zero here is a real zero. That distinction is the
/// whole contract: docs/BACKEND.md forbids inventing a number to fill a slot,
/// and a dashboard that shows 0 °C because a provider failed is worse than one
/// that shows nothing, because the reader cannot tell the difference.
///
/// It applies at two levels. A `None` on a whole section — `network`, `gpus`,
/// `thermal` — means the provider is not present on this machine at all: no
/// WDDM 2.0 driver, no ACPI thermal zones. A `None` on a single rate inside a
/// present section means the value could not be computed *this tick*, almost
/// always because it is the first sample and a rate needs two.
///
/// # Shape
///
/// CPU and memory stay flat because they were already flat and nothing here
/// requires changing them. Network and storage nest one level, and only
/// because each genuinely has per-device detail — interfaces, drives, adapters
/// — that the machine-wide figure is derived from. No level of nesting exists
/// for extensibility alone.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemTelemetry {
    /// Machine-wide CPU utilisation over the last interval, 0–100. `None` on
    /// the first tick, which has no previous sample to difference against.
    pub cpu_percent: Option<f32>,
    /// Per-logical-processor utilisation over the same interval, in the order
    /// Windows enumerates them. `None` when the query is unavailable, which is
    /// a different fact from an empty list.
    pub per_core_percent: Option<Vec<f32>>,
    /// How many logical processors the CPU percentages are divided by.
    pub logical_processors: u32,
    /// Physical memory installed in the machine. Not to be confused with
    /// `ProcessRow.memory_bytes`, which is one process's working set — the two
    /// are never summed or compared.
    pub memory_total_bytes: Option<u64>,
    pub memory_used_bytes: Option<u64>,
    /// Used as a share of total, 0–100. Carried explicitly rather than derived
    /// in the UI so the two numbers can never disagree.
    pub memory_percent: Option<f32>,
    /// `None` if the interface table could not be read at all.
    pub network: Option<NetworkTelemetry>,
    /// `None` if no physical drive could be opened.
    pub storage: Option<StorageTelemetry>,
    /// `None` when this machine exposes no GPU performance counters — a VM
    /// with a basic display adapter, or a pre-WDDM-2.0 driver. An empty list
    /// would claim the provider answered and found no adapters.
    pub gpus: Option<Vec<GpuTelemetry>>,
}

/// TS: `interface NetworkTelemetry`
///
/// Throughput is derived from cumulative octet counters, never read as an
/// instantaneous rate — `MIB_IF_ROW2` reports bytes since the interface came
/// up, so a rate needs two samples and an elapsed time.
///
/// The machine-wide figures are the sum of the per-interface rates that could
/// actually be computed. An interface that appeared this tick contributes
/// nothing rather than its whole lifetime total, which would otherwise show as
/// a multi-gigabyte spike the moment a VPN connects.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkTelemetry {
    pub receive_bytes_per_sec: Option<f64>,
    pub transmit_bytes_per_sec: Option<f64>,
    /// The interfaces the totals were computed from: operational, non-loopback
    /// and not a filter interface. Never every row in the table — this machine
    /// reports 50, of which 47 are tunnels, filters and disconnected adapters.
    pub interfaces: Vec<NetworkInterface>,
}

/// TS: `interface NetworkInterface`
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkInterface {
    /// The name the user sees in Windows, e.g. "Ethernet".
    pub name: String,
    /// The adapter's own description, e.g. "Realtek Gaming GbE Family
    /// Controller".
    pub description: String,
    pub receive_bytes_per_sec: Option<f64>,
    pub transmit_bytes_per_sec: Option<f64>,
    /// Negotiated receive link speed, bits per second. `None` when the adapter
    /// does not report one.
    pub link_speed_bits_per_sec: Option<u64>,
}

/// TS: `interface StorageTelemetry`
///
/// System-level, deliberately. Per-process disk accounting is not V1 and
/// `GetProcessIoCounters` could not provide it honestly anyway: it counts file,
/// network and device I/O together, so it cannot answer "how much is this
/// touching the disk".
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageTelemetry {
    pub read_bytes_per_sec: Option<f64>,
    pub write_bytes_per_sec: Option<f64>,
    /// The busiest drive's active time, not a sum. Two drives at 50% is not a
    /// machine at 100%, and adding them would say so.
    pub active_percent: Option<f32>,
    pub drives: Vec<StorageDrive>,
}

/// TS: `interface StorageDrive` — one physical drive.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageDrive {
    /// The physical drive number, as in `\\.\PhysicalDrive0`.
    pub number: u32,
    /// Vendor and product string from the device, or the drive number when the
    /// identity query is refused.
    pub model: String,
    pub read_bytes_per_sec: Option<f64>,
    pub write_bytes_per_sec: Option<f64>,
    /// Share of the interval the drive was not idle, 0–100. Derived from idle
    /// time rather than from busy time, because that is the counter Windows
    /// actually maintains, and it is what Task Manager calls "Active time".
    pub active_percent: Option<f32>,
}

/// TS: `interface GpuTelemetry` — one display adapter.
///
/// Utilisation comes from the same performance counters Task Manager uses.
/// Per-engine values are summed across processes and then the **maximum across
/// engine types** is taken, not the sum: 3D, Copy, Video Decode and the rest
/// are separate hardware queues that run concurrently, so adding them reports
/// well over 100% on a machine doing one thing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuTelemetry {
    /// Adapter description from DXGI, e.g. "NVIDIA GeForce RTX 4070 Laptop
    /// GPU". Falls back to the adapter LUID if DXGI did not enumerate it.
    pub name: String,
    /// `None` when the adapter has memory counters but no engine counters,
    /// which some virtual and remote adapters do.
    pub utilization_percent: Option<f32>,
    pub dedicated_memory_used_bytes: Option<u64>,
    /// Installed dedicated video memory, from DXGI. `None` for an adapter the
    /// counters know about but DXGI did not enumerate.
    pub dedicated_memory_total_bytes: Option<u64>,
    pub shared_memory_used_bytes: Option<u64>,
}

/// TS: `interface ScanTiming` — how long this tick took, in milliseconds.
///
/// Published rather than logged because the cost of a monitoring tool is the
/// user's business, and because a telemetry provider that becomes slow on some
/// machine should be visible without a debug build.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanTiming {
    pub total_millis: f64,
    pub processes_millis: f64,
    pub ports_millis: f64,
    pub telemetry_millis: f64,
}

/// TS: `interface Snapshot` — everything one sampler tick produces.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub sequence: u64,
    /// When the scan completed, ISO-8601.
    pub captured_at: String,
    pub services: Vec<Service>,
    pub processes: Vec<ProcessRow>,
    pub ports: Vec<PortRow>,
    /// `None` renders as "—" in the UI, not as a confident 0. Conflict
    /// detection is V2 (docs/ROADMAP.md § 2.3); until the backend computes it,
    /// claiming zero conflicts would be a claim we cannot support.
    pub conflicts: Option<u32>,
    /// Machine-wide load for this tick.
    pub system: SystemTelemetry,
    /// What this tick cost.
    pub timing: ScanTiming,
    /// Which Developer Registry produced the `relevance` on every service in
    /// this snapshot. Shipped so a classification someone disagrees with can
    /// be pinned to a specific version of the tables.
    pub registry_version: u32,
}

/// TS: `type FieldState<T>`
///
/// A tier-2 field is not a string that might be empty — it is a value, a
/// refusal, or an absence, and the three are different facts. docs/BACKEND.md:
/// "AccessDenied is a value, not an error." This is where that value lands.
///
/// Internally tagged so the JSON matches the TypeScript union exactly:
/// `{"kind":"ok","value":"..."}`, `{"kind":"denied"}`, `{"kind":"unavailable"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum FieldState<T> {
    Ok {
        value: T,
    },
    /// The process is owned by another account. Renders as "Requires
    /// elevation" — and LocalDocks does not elevate.
    Denied,
    /// Not readable: the process went away, the identity no longer matches, or
    /// this Windows build does not expose the field. Never used to mean "the
    /// value happens to be empty".
    Unavailable,
}

/// TS: `interface ProcessDetail` — tier 2, fetched when a panel opens.
///
/// Never produced by the sampler. docs/ARCHITECTURE.md § 4 keeps these fields
/// out of the scan loop because they are expensive and awkward on Windows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessDetail {
    pub process_id: ProcessId,
    pub executable: FieldState<String>,
    pub command_line: FieldState<String>,
    pub working_directory: FieldState<String>,
}

impl ProcessDetail {
    /// Every field in the same state.
    ///
    /// Used when the failure is about the process rather than the field: a
    /// refused open denies all three, a stale identity makes all three
    /// unavailable. Filling them in one at a time would imply the fields were
    /// tried individually.
    pub fn all(process_id: ProcessId, state: FieldState<String>) -> Self {
        Self {
            process_id,
            executable: state.clone(),
            command_line: state.clone(),
            working_directory: state,
        }
    }
}

/// TS: `type TerminateResult`
///
/// `Stale` is a success path, not a failure: it means the identity check
/// refused to kill a recycled PID (docs/BACKEND.md, "IdentityMismatch is a
/// success path for the safety model working").
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum TerminateResult {
    Terminated,
    Stale { message: String },
    Denied,
    Failed { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The contract is JSON, so the tests assert on JSON — not on Rust values.
    /// These are the assertions that would actually catch a drift from
    /// `src/types.ts`.
    #[test]
    fn an_empty_snapshot_matches_the_typescript_shape() {
        let s = Snapshot {
            sequence: 1,
            captured_at: "2026-08-28T09:00:00.000Z".into(),
            services: Vec::new(),
            processes: Vec::new(),
            ports: Vec::new(),
            conflicts: None,
            system: SystemTelemetry::default(),
            timing: ScanTiming::default(),
            registry_version: 1,
        };
        let v = serde_json::to_value(&s).unwrap();

        assert_eq!(v["sequence"], 1);
        assert_eq!(v["capturedAt"], "2026-08-28T09:00:00.000Z");
        assert!(v["services"].as_array().unwrap().is_empty());
        assert!(v["processes"].as_array().unwrap().is_empty());
        assert!(v["ports"].as_array().unwrap().is_empty());

        // The key must be PRESENT and null — absent would deserialise to
        // `undefined` in TS and break the `conflicts === null` check.
        assert!(v.get("conflicts").is_some());
        assert!(v["conflicts"].is_null());

        assert_eq!(v["registryVersion"], 1);
        assert!(v.get("timing").is_some(), "timing is part of the contract");
        assert_eq!(v["timing"]["totalMillis"], 0.0);
        // Unmeasured telemetry is null, never a confident zero.
        for key in [
            "cpuPercent",
            "perCorePercent",
            "memoryTotalBytes",
            "memoryPercent",
        ] {
            assert!(v["system"].get(key).is_some(), "missing system.{key}");
            assert!(v["system"][key].is_null(), "system.{key} should be null");
        }
        assert_eq!(v["system"]["logicalProcessors"], 0);
    }

    /// The contract's whole promise about telemetry: a key that is not
    /// measured is present and null, never absent and never zero. A missing
    /// key deserialises to `undefined` in TypeScript and silently skips every
    /// `=== null` check the UI uses to decide what to render.
    #[test]
    fn unmeasured_telemetry_is_present_and_null_at_every_level() {
        let v = serde_json::to_value(SystemTelemetry::default()).unwrap();

        for key in [
            "cpuPercent",
            "perCorePercent",
            "logicalProcessors",
            "memoryTotalBytes",
            "memoryUsedBytes",
            "memoryPercent",
            "network",
            "storage",
            "gpus",
        ] {
            assert!(
                v.get(key).is_some(),
                "system.{key} is missing from the contract"
            );
        }
        for key in [
            "cpuPercent",
            "perCorePercent",
            "memoryTotalBytes",
            "memoryUsedBytes",
            "memoryPercent",
            "network",
            "storage",
            "gpus",
        ] {
            assert!(
                v[key].is_null(),
                "system.{key} should be null, got {}",
                v[key]
            );
        }
        // The one field that is a count rather than a measurement.
        assert_eq!(v["logicalProcessors"], 0);
    }

    #[test]
    fn a_populated_telemetry_snapshot_serialises_in_camel_case_throughout() {
        let telemetry = SystemTelemetry {
            cpu_percent: Some(12.5),
            per_core_percent: Some(vec![10.0, 15.0]),
            logical_processors: 2,
            memory_total_bytes: Some(16_000_000_000),
            memory_used_bytes: Some(8_000_000_000),
            memory_percent: Some(50.0),
            network: Some(NetworkTelemetry {
                receive_bytes_per_sec: Some(1_024.0),
                transmit_bytes_per_sec: None,
                interfaces: vec![NetworkInterface {
                    name: "Ethernet".into(),
                    description: "Realtek Gaming GbE".into(),
                    receive_bytes_per_sec: Some(1_024.0),
                    transmit_bytes_per_sec: None,
                    link_speed_bits_per_sec: Some(1_000_000_000),
                }],
            }),
            storage: Some(StorageTelemetry {
                read_bytes_per_sec: Some(2_048.0),
                write_bytes_per_sec: Some(512.0),
                active_percent: Some(3.5),
                drives: vec![StorageDrive {
                    number: 0,
                    model: "PhysicalDrive0".into(),
                    read_bytes_per_sec: Some(2_048.0),
                    write_bytes_per_sec: Some(512.0),
                    active_percent: Some(3.5),
                }],
            }),
            gpus: Some(vec![GpuTelemetry {
                name: "Test GPU".into(),
                utilization_percent: Some(41.0),
                dedicated_memory_used_bytes: Some(1_796_993_024),
                dedicated_memory_total_bytes: Some(8_589_934_592),
                shared_memory_used_bytes: Some(72_081_408),
            }]),
        };
        let v = serde_json::to_value(&telemetry).unwrap();

        assert_eq!(v["network"]["receiveBytesPerSec"], 1_024.0);
        // A rate that could not be computed is null inside a section that is
        // otherwise present — a different fact from the whole section missing.
        assert!(v["network"]["transmitBytesPerSec"].is_null());
        assert_eq!(
            v["network"]["interfaces"][0]["linkSpeedBitsPerSec"],
            1_000_000_000u64
        );
        assert_eq!(v["storage"]["activePercent"].as_f64().unwrap(), 3.5);
        assert_eq!(v["storage"]["drives"][0]["readBytesPerSec"], 2_048.0);

        assert_eq!(v["gpus"][0]["utilizationPercent"].as_f64().unwrap(), 41.0);
        assert_eq!(v["gpus"][0]["dedicatedMemoryUsedBytes"], 1_796_993_024u64);

        // Nothing snake_case anywhere in the tree.
        let text = serde_json::to_string(&telemetry).unwrap();
        for leaked in [
            "receive_bytes_per_sec",
            "per_core_percent",
            "active_percent",
            "utilization_percent",
            "dedicated_memory_used_bytes",
            "link_speed_bits_per_sec",
        ] {
            assert!(
                !text.contains(leaked),
                "snake_case leaked into JSON: {leaked}"
            );
        }
    }

    #[test]
    fn scan_timing_serialises_as_four_millisecond_figures() {
        let v = serde_json::to_value(ScanTiming {
            total_millis: 21.5,
            processes_millis: 18.0,
            ports_millis: 1.8,
            telemetry_millis: 1.7,
        })
        .unwrap();
        assert_eq!(v["totalMillis"], 21.5);
        assert_eq!(v["processesMillis"], 18.0);
        assert_eq!(v["portsMillis"], 1.8);
        assert_eq!(v["telemetryMillis"], 1.7);
    }

    #[test]
    fn relevance_serialises_as_the_three_lowercase_variants() {
        assert_eq!(
            serde_json::to_value(Relevance::Developer).unwrap(),
            "developer"
        );
        assert_eq!(serde_json::to_value(Relevance::System).unwrap(), "system");
        assert_eq!(serde_json::to_value(Relevance::Unknown).unwrap(), "unknown");
    }

    #[test]
    fn a_service_carries_its_classification_in_camel_case() {
        let service = Service {
            id: make_process_id(8420, "2026-08-28T09:00:00.000Z"),
            label: "node:5173".into(),
            framework: None,
            process_name: "node.exe".into(),
            pid: 8420,
            parent_pid: 6104,
            cpu_percent: 2.3,
            memory_bytes: 148_897_792,
            thread_count: 18,
            started_at: "2026-08-28T09:00:00.000Z".into(),
            uptime_seconds: 4342.0,
            endpoints: Vec::new(),
            status: ServiceStatus::Running,
            relevance: Relevance::Developer,
            relevance_reason: "Node.js launched with the Vite signature.".into(),
        };
        let v = serde_json::to_value(&service).unwrap();
        assert_eq!(v["relevance"], "developer");
        assert_eq!(
            v["relevanceReason"],
            "Node.js launched with the Vite signature."
        );
        assert!(v.get("relevance_reason").is_none());
    }

    #[test]
    fn fields_serialise_as_camel_case() {
        let row = ProcessRow {
            id: make_process_id(8420, "2026-08-28T09:00:00.000Z"),
            pid: 8420,
            parent_pid: 6104,
            name: "node.exe".into(),
            cpu_percent: 2.3,
            memory_bytes: 148_897_792,
            thread_count: 18,
            started_at: "2026-08-28T09:00:00.000Z".into(),
            uptime_seconds: 4342.0,
            status: ProcessStatus::Running,
            is_service: true,
        };
        let v = serde_json::to_value(&row).unwrap();

        for key in [
            "parentPid",
            "cpuPercent",
            "memoryBytes",
            "threadCount",
            "startedAt",
            "uptimeSeconds",
            "isService",
        ] {
            assert!(v.get(key).is_some(), "missing camelCase key {key}");
        }
        assert!(v.get("parent_pid").is_none(), "snake_case leaked into JSON");
    }

    #[test]
    fn enums_serialise_with_the_casing_the_contract_expects() {
        assert_eq!(serde_json::to_value(Protocol::Tcp).unwrap(), "TCP");
        assert_eq!(serde_json::to_value(Protocol::Udp).unwrap(), "UDP");
        assert_eq!(
            serde_json::to_value(PortState::Listening).unwrap(),
            "LISTENING"
        );
        assert_eq!(
            serde_json::to_value(ServiceStatus::Running).unwrap(),
            "running"
        );
        assert_eq!(
            serde_json::to_value(ProcessStatus::Sleeping).unwrap(),
            "sleeping"
        );
    }

    #[test]
    fn nullable_fields_emit_null_rather_than_being_omitted() {
        let row = PortRow {
            port: 5173,
            protocol: Protocol::Tcp,
            address: "127.0.0.1".into(),
            pid: 8420,
            process_id: None,
            process_name: "node.exe".into(),
            service_label: None,
            state: PortState::Listening,
        };
        let v = serde_json::to_value(&row).unwrap();
        assert!(v["processId"].is_null());
        assert!(v["serviceLabel"].is_null());
        assert!(v.get("processId").is_some());
        assert!(v.get("serviceLabel").is_some());
    }

    #[test]
    fn process_id_pairs_pid_with_creation_time() {
        assert_eq!(
            make_process_id(8420, "2026-08-28T09:00:00.000Z"),
            "8420-2026-08-28T09:00:00.000Z"
        );
    }
}
