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

// TEMPORARY — technical debt, remove in `feature/process-discovery`.
//
// The types below are the IPC contract, but only `Snapshot::empty` is
// constructed so far, so the build emits five "never constructed" warnings
// that would drown out real ones.
//
// TODO(process-discovery): delete this attribute once enumeration actually
// constructs Service / ProcessRow / PortRow. From that point a dead-code
// warning here is a genuine signal, and leaving the suppression in place
// would hide real dead code rather than expected absence.
#![allow(dead_code)]

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Protocol {
    Tcp,
    Udp,
}

/// TS: `interface Endpoint`
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub protocol: Protocol,
    /// Presentation form: "127.0.0.1" or "[::1]".
    pub address: String,
    pub port: u16,
}

/// TS: `Service['status'] = 'running' | 'stopped'`
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
    Sleeping,
}

/// TS: `PortRow['state'] = 'LISTENING'`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PortState {
    Listening,
}

/// TS: `interface Service` — tier 1, refreshed every sampler tick.
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
}

impl Snapshot {
    /// An empty snapshot for the given tick.
    ///
    /// Structurally valid and truthful: nothing has been enumerated yet, so
    /// every collection is empty and `conflicts` is unknown.
    pub fn empty(sequence: u64, captured_at: String) -> Self {
        Self {
            sequence,
            captured_at,
            services: Vec::new(),
            processes: Vec::new(),
            ports: Vec::new(),
            conflicts: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The contract is JSON, so the tests assert on JSON — not on Rust values.
    /// These are the assertions that would actually catch a drift from
    /// `src/types.ts`.
    #[test]
    fn empty_snapshot_matches_the_typescript_shape() {
        let s = Snapshot::empty(1, "2026-08-28T09:00:00.000Z".into());
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
        assert_eq!(serde_json::to_value(PortState::Listening).unwrap(), "LISTENING");
        assert_eq!(serde_json::to_value(ServiceStatus::Running).unwrap(), "running");
        assert_eq!(serde_json::to_value(ProcessStatus::Sleeping).unwrap(), "sleeping");
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
