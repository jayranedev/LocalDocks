//! The join: `Process + Endpoint[] -> Service`.
//!
//! docs/ARCHITECTURE.md § 1 gives the definition, and it is observable rather
//! than heuristic:
//!
//! > A *Service* is a process holding at least one listening socket on a
//! > non-system port, owned by the current user.
//!
//! The alternative — an allowlist of process names — is "permanently wrong in
//! both directions: it misses new runtimes and it floods the list with Electron
//! apps that happen to look like `node`." Nothing in this file looks at what a
//! program is called in order to decide what it is.
//!
//! Pure and syscall-free. It takes the process rows and the sockets the sampler
//! already collected in this same tick and returns services; it never opens a
//! handle, never queries a table, and never needs Windows to be tested.

use std::collections::HashMap;

use crate::logic::ports::format_address;
use crate::models::{Endpoint, ProcessId, ProcessRow, Protocol, Relevance, Service, ServiceStatus};
use crate::platform::windows::ports::RawEndpoint;

/// The first port outside the IANA system range.
///
/// 0–1023 are the well-known ports: they belong to the machine's own
/// infrastructure — RPC, SMB, DNS, mDNS — not to something a developer started.
/// Including them would bury four dev servers under sixty rows of Windows.
///
/// It is a convention rather than a law, so a service that genuinely binds a
/// low port is missed. That is the documented V1 trade, and the Ports screen
/// still shows the socket.
pub const FIRST_NON_SYSTEM_PORT: u16 = 1024;

/// Services, plus the lookups the rest of the tick needs to point at them.
///
/// The labels are handed out separately so a `ProcessRow` and a `PortRow` can
/// refer to a service without carrying a copy of one.
#[derive(Debug, Clone, Default)]
pub struct ServiceJoin {
    pub services: Vec<Service>,
    labels: HashMap<ProcessId, String>,
}

impl ServiceJoin {
    /// Identity -> label, for `PortRow.serviceLabel`.
    pub fn labels(&self) -> &HashMap<ProcessId, String> {
        &self.labels
    }

    /// Whether this process produced a service, for `ProcessRow.isService`.
    ///
    /// Derived here rather than set anywhere else: the frontend must never be
    /// the thing that decides what counts as a service.
    pub fn is_service(&self, id: &ProcessId) -> bool {
        self.labels.contains_key(id)
    }
}

/// Join this tick's processes to this tick's sockets.
///
/// The ownership half of the predicate is already satisfied by the input.
/// `rows` contains exactly the processes whose creation time could be read,
/// which unelevated means exactly the processes the current user owns —
/// measured on a development machine as 215 openable processes, 214 owned by
/// the user and one that exited mid-check, with none belonging to another
/// account. docs/ARCHITECTURE.md § 1 notes that this definition "happens to be
/// the privilege boundary"; this is that observation used rather than
/// re-queried. A process the app cannot open has no row, so it can never
/// become a service, which is also why no token lookup is needed here.
pub fn join_services(rows: &[ProcessRow], endpoints: &[RawEndpoint]) -> ServiceJoin {
    // Sockets carry a PID and nothing else, so the join key is the PID — but
    // only against processes from this same tick, and the identity always
    // comes from the row rather than being rebuilt from the PID.
    let mut by_pid: HashMap<u32, Vec<&RawEndpoint>> = HashMap::new();
    for e in endpoints {
        by_pid.entry(e.pid).or_default().push(e);
    }

    let mut services = Vec::new();
    let mut labels = HashMap::new();

    for row in rows {
        let Some(owned) = by_pid.get(&row.pid) else {
            continue; // no sockets: not a service
        };
        if !owned.iter().any(|e| is_non_system(e.port)) {
            continue; // only system ports: not a service
        }

        // Every socket the process owns, not only the qualifying ones. The
        // predicate decides membership; the endpoint list describes the
        // service, and omitting a socket it really holds would contradict the
        // Ports screen showing that same socket.
        let endpoints = collect_endpoints(owned);
        let label = label_for(&row.name, &endpoints);

        labels.insert(row.id.clone(), label.clone());
        services.push(Service {
            id: row.id.clone(),
            label,
            // Framework and project detection are V2 (docs/ROADMAP.md).
            // The UI falls back to the process name for this line, so `None`
            // costs nothing and claims nothing.
            framework: None,
            process_name: row.name.clone(),
            pid: row.pid,
            parent_pid: row.parent_pid,
            cpu_percent: row.cpu_percent,
            memory_bytes: row.memory_bytes,
            thread_count: row.thread_count,
            started_at: row.started_at.clone(),
            uptime_seconds: row.uptime_seconds,
            endpoints,
            // A service built from a process in this snapshot is running by
            // definition. `Stopped` exists for the V2 lifecycle work and stays
            // unreachable until something can actually observe it — the same
            // reasoning as `ProcessStatus::Sleeping`.
            status: ServiceStatus::Running,
            // The join observes; it does not judge. Relevance is decided by
            // `logic::classify` in a second pass, because it needs the command
            // line, which needs a syscall this pure module must not make.
            // `Unknown` with no reason is the un-classified state, and the
            // sampler asserts it never survives to a snapshot.
            relevance: Relevance::Unknown,
            relevance_reason: String::new(),
        });
    }

    // Services follow their processes' order, which is Windows' enumeration
    // order and not meaningful. Sorting by identity makes the list stable
    // between ticks; the UI sorts it again for display.
    services.sort_by(|a, b| a.id.cmp(&b.id));

    ServiceJoin { services, labels }
}

/// A port outside the range reserved for system services.
fn is_non_system(port: u16) -> bool {
    port >= FIRST_NON_SYSTEM_PORT
}

/// One socket per `Endpoint`, in a deterministic order.
///
/// Ordering matches the Ports screen: by port, then IPv4 before IPv6, then
/// address, then protocol. That makes `endpoints[0]` the lowest port, which is
/// what `primaryPort()` in the frontend picks, so the label and the port the UI
/// shows always agree.
///
/// Dual-stack sockets stay separate objects. `127.0.0.1:5173` and `[::1]:5173`
/// are two endpoints of one service (docs/ARCHITECTURE.md § 5); collapsing them
/// by port is the bug the whole endpoint model exists to avoid.
fn collect_endpoints(owned: &[&RawEndpoint]) -> Vec<Endpoint> {
    let mut endpoints: Vec<Endpoint> = owned
        .iter()
        .map(|e| Endpoint {
            protocol: e.protocol,
            address: format_address(e.address, e.scope_id),
            port: e.port,
        })
        .collect();

    endpoints.sort_by(|a, b| {
        a.port
            .cmp(&b.port)
            .then_with(|| a.address.starts_with('[').cmp(&b.address.starts_with('[')))
            .then_with(|| a.address.cmp(&b.address))
            .then_with(|| protocol_order(a.protocol).cmp(&protocol_order(b.protocol)))
    });
    endpoints
        .dedup_by(|a, b| a.protocol == b.protocol && a.address == b.address && a.port == b.port);
    endpoints
}

fn protocol_order(p: Protocol) -> u8 {
    match p {
        Protocol::Tcp => 0,
        Protocol::Udp => 1,
    }
}

/// The V1 service label: `{executable stem}:{primary port}`.
///
/// Chosen against three constraints. It must not be framework or project
/// detection, which is V2. It must not classify by executable name, which
/// docs/ARCHITECTURE.md § 1 rules out. And it must be deterministic.
///
/// So it is derived from only two things the service already carries: its own
/// executable name and its own lowest port. That makes it stable across ticks
/// and independent of everything else running — a label never changes because
/// an unrelated service started, which a "disambiguate only when names collide"
/// rule could not promise.
///
/// It also reads the way developers actually refer to a dev server, and the
/// port is what distinguishes two instances of the same runtime. The UI shows
/// `framework ?? processName` on the line beneath, so the full executable name
/// is never lost.
///
/// This is a placeholder for a real name, and deliberately looks like one. V2
/// project detection replaces it with something a human chose.
fn label_for(process_name: &str, endpoints: &[Endpoint]) -> String {
    let stem = executable_stem(process_name);
    match endpoints.first() {
        Some(first) => format!("{stem}:{}", first.port),
        // Unreachable through `join`, which only builds a service once it has
        // found a qualifying endpoint. Handled rather than unwrapped because a
        // panic on a command-reachable path is what docs/BACKEND.md forbids.
        None => stem,
    }
}

/// `node.exe` -> `node`. Nothing more: no title-casing, no mapping table, no
/// interpretation of what the program is.
///
/// Falls back to the raw name rather than to an empty string, so a process with
/// an unusual or unreadable name still gets a label it can be found by.
fn executable_stem(process_name: &str) -> String {
    let trimmed = process_name.trim();
    if trimmed.is_empty() {
        return "unknown".to_string();
    }
    match trimmed.len().checked_sub(4) {
        Some(cut) if trimmed[cut..].eq_ignore_ascii_case(".exe") && cut > 0 => {
            trimmed[..cut].to_string()
        }
        _ => trimmed.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{make_process_id, ProcessStatus};
    use std::collections::HashSet;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    const T: &str = "2026-08-28T09:00:00.000Z";

    fn row(pid: u32, name: &str) -> ProcessRow {
        ProcessRow {
            id: make_process_id(pid, T),
            pid,
            parent_pid: 4,
            name: name.into(),
            cpu_percent: 1.5,
            memory_bytes: 148_897_792,
            thread_count: 18,
            started_at: T.into(),
            uptime_seconds: 4342.0,
            status: ProcessStatus::Running,
            is_service: false,
        }
    }

    fn socket(protocol: Protocol, address: IpAddr, port: u16, pid: u32) -> RawEndpoint {
        RawEndpoint {
            protocol,
            address,
            scope_id: 0,
            port,
            pid,
        }
    }

    fn tcp4(a: [u8; 4], port: u16, pid: u32) -> RawEndpoint {
        socket(Protocol::Tcp, IpAddr::V4(Ipv4Addr::from(a)), port, pid)
    }

    fn tcp6(s: &str, port: u16, pid: u32) -> RawEndpoint {
        socket(
            Protocol::Tcp,
            IpAddr::V6(s.parse::<Ipv6Addr>().unwrap()),
            port,
            pid,
        )
    }

    // ------------------------------------------------------------- predicate

    #[test]
    fn a_process_with_no_sockets_is_not_a_service() {
        let j = join_services(&[row(8420, "node.exe")], &[]);
        assert!(j.services.is_empty());
        assert!(!j.is_service(&make_process_id(8420, T)));
    }

    #[test]
    fn a_process_with_one_qualifying_socket_is_a_service() {
        let j = join_services(
            &[row(8420, "node.exe")],
            &[tcp4([127, 0, 0, 1], 5173, 8420)],
        );
        assert_eq!(j.services.len(), 1);
        assert!(j.is_service(&make_process_id(8420, T)));
    }

    #[test]
    fn a_process_holding_only_system_ports_is_not_a_service() {
        // Ports below 1024 belong to the machine's own infrastructure.
        let j = join_services(
            &[row(4, "System")],
            &[
                tcp4([0, 0, 0, 0], 135, 4),
                tcp4([0, 0, 0, 0], 445, 4),
                socket(Protocol::Udp, IpAddr::V4(Ipv4Addr::UNSPECIFIED), 137, 4),
            ],
        );
        assert!(j.services.is_empty());
    }

    #[test]
    fn one_qualifying_port_is_enough_even_beside_system_ports() {
        let j = join_services(
            &[row(8420, "node.exe")],
            &[
                tcp4([0, 0, 0, 0], 80, 8420),
                tcp4([127, 0, 0, 1], 5173, 8420),
            ],
        );
        assert_eq!(j.services.len(), 1);
        // Both sockets are listed: the predicate decides membership, it does
        // not filter the description.
        assert_eq!(j.services[0].endpoints.len(), 2);
    }

    #[test]
    fn the_system_port_boundary_is_1024() {
        let just_below = join_services(&[row(1, "a.exe")], &[tcp4([127, 0, 0, 1], 1023, 1)]);
        let just_above = join_services(&[row(1, "a.exe")], &[tcp4([127, 0, 0, 1], 1024, 1)]);
        assert!(just_below.services.is_empty());
        assert_eq!(just_above.services.len(), 1);
    }

    #[test]
    fn a_socket_owned_by_a_process_we_have_no_row_for_creates_no_service() {
        // No row means the app could not open the process, which unelevated
        // means it is not ours. Ownership is enforced by the input.
        let j = join_services(&[], &[tcp4([0, 0, 0, 0], 5173, 1160)]);
        assert!(j.services.is_empty());
        assert!(j.labels().is_empty());
    }

    // -------------------------------------------------------------- grouping

    #[test]
    fn several_sockets_of_one_process_become_one_service() {
        let j = join_services(
            &[row(8420, "node.exe")],
            &[
                tcp4([127, 0, 0, 1], 5173, 8420),
                tcp4([127, 0, 0, 1], 24678, 8420),
                socket(Protocol::Udp, IpAddr::V4(Ipv4Addr::LOCALHOST), 5353, 8420),
            ],
        );
        assert_eq!(j.services.len(), 1, "one process is one service");
        assert_eq!(j.services[0].endpoints.len(), 3);
    }

    #[test]
    fn dual_stack_sockets_group_under_one_service_and_stay_separate_endpoints() {
        // The case docs/ARCHITECTURE.md § 5 is built around.
        let j = join_services(
            &[row(8420, "node.exe")],
            &[
                tcp4([127, 0, 0, 1], 5173, 8420),
                tcp6("::1", 5173, 8420),
                tcp4([0, 0, 0, 0], 5173, 8420),
                tcp6("::", 5173, 8420),
            ],
        );

        assert_eq!(j.services.len(), 1);
        let e = &j.services[0].endpoints;
        assert_eq!(e.len(), 4, "four sockets stay four endpoints");
        assert!(e.iter().all(|x| x.port == 5173));
        let addresses: HashSet<_> = e.iter().map(|x| x.address.as_str()).collect();
        assert_eq!(addresses.len(), 4, "not deduplicated by port");
        assert!(addresses.contains("127.0.0.1"));
        assert!(addresses.contains("[::1]"));
        assert!(addresses.contains("0.0.0.0"));
        assert!(addresses.contains("[::]"));
    }

    #[test]
    fn two_processes_sharing_a_port_become_two_services() {
        // Windows permits this for multicast UDP with SO_REUSEADDR.
        let j = join_services(
            &[row(100, "a.exe"), row(200, "b.exe")],
            &[
                socket(Protocol::Udp, IpAddr::V4(Ipv4Addr::UNSPECIFIED), 5353, 100),
                socket(Protocol::Udp, IpAddr::V4(Ipv4Addr::UNSPECIFIED), 5353, 200),
            ],
        );
        assert_eq!(j.services.len(), 2);
        assert_ne!(j.services[0].id, j.services[1].id);
    }

    #[test]
    fn every_endpoint_on_a_service_belongs_to_that_service_s_process() {
        let j = join_services(
            &[row(100, "a.exe"), row(200, "b.exe")],
            &[
                tcp4([127, 0, 0, 1], 5173, 100),
                tcp4([127, 0, 0, 1], 8000, 200),
                tcp6("::1", 5173, 100),
            ],
        );

        assert_eq!(j.services.len(), 2);
        let a = j.services.iter().find(|s| s.pid == 100).unwrap();
        let b = j.services.iter().find(|s| s.pid == 200).unwrap();
        assert_eq!(a.endpoints.len(), 2);
        assert_eq!(b.endpoints.len(), 1);
        assert!(a.endpoints.iter().all(|e| e.port == 5173));
        assert_eq!(b.endpoints[0].port, 8000);
    }

    #[test]
    fn services_are_never_duplicated_and_map_one_to_one_onto_processes() {
        let rows: Vec<_> = (1..25u32).map(|pid| row(pid, "node.exe")).collect();
        let sockets: Vec<_> = (1..25u32)
            .flat_map(|pid| {
                [
                    tcp4([127, 0, 0, 1], 5000 + pid as u16, pid),
                    tcp6("::1", 5000 + pid as u16, pid),
                ]
            })
            .collect();

        let j = join_services(&rows, &sockets);

        assert_eq!(j.services.len(), 24);
        let ids: HashSet<_> = j.services.iter().map(|s| s.id.clone()).collect();
        assert_eq!(ids.len(), 24, "no duplicate services");
        for s in &j.services {
            let matching = rows.iter().filter(|r| r.id == s.id).count();
            assert_eq!(matching, 1, "each service maps to exactly one process");
        }
    }

    // -------------------------------------------------------------- identity

    #[test]
    fn service_identity_is_the_process_identity_and_nothing_new() {
        let r = row(8420, "node.exe");
        let j = join_services(
            std::slice::from_ref(&r),
            &[tcp4([127, 0, 0, 1], 5173, 8420)],
        );
        let s = &j.services[0];

        assert_eq!(s.id, r.id);
        assert_eq!(s.id, make_process_id(s.pid, &s.started_at));
    }

    #[test]
    fn a_restarted_process_is_a_different_service() {
        // Same PID, later start: a different identity, so a stale row can never
        // point at the new process.
        let mut restarted = row(8420, "node.exe");
        restarted.started_at = "2026-08-28T10:00:00.000Z".into();
        restarted.id = make_process_id(8420, &restarted.started_at);

        let first = join_services(
            &[row(8420, "node.exe")],
            &[tcp4([127, 0, 0, 1], 5173, 8420)],
        );
        let second = join_services(&[restarted], &[tcp4([127, 0, 0, 1], 5173, 8420)]);

        assert_ne!(first.services[0].id, second.services[0].id);
    }

    #[test]
    fn process_facts_are_carried_onto_the_service_unchanged() {
        let r = row(8420, "node.exe");
        let j = join_services(
            std::slice::from_ref(&r),
            &[tcp4([127, 0, 0, 1], 5173, 8420)],
        );
        let s = &j.services[0];

        assert_eq!(s.process_name, r.name);
        assert_eq!(s.pid, r.pid);
        assert_eq!(s.parent_pid, r.parent_pid);
        assert_eq!(s.cpu_percent, r.cpu_percent);
        assert_eq!(s.memory_bytes, r.memory_bytes);
        assert_eq!(s.thread_count, r.thread_count);
        assert_eq!(s.started_at, r.started_at);
        assert_eq!(s.uptime_seconds, r.uptime_seconds);
        assert_eq!(s.status, ServiceStatus::Running);
        assert!(s.framework.is_none(), "framework detection is V2");
    }

    // ----------------------------------------------------------------- label

    #[test]
    fn the_label_is_the_executable_stem_and_the_primary_port() {
        let j = join_services(
            &[row(8420, "node.exe")],
            &[tcp4([127, 0, 0, 1], 5173, 8420)],
        );
        assert_eq!(j.services[0].label, "node:5173");
    }

    #[test]
    fn the_label_uses_the_lowest_port_the_service_holds() {
        // Matching `primaryPort()` in the frontend, so the label agrees with
        // the port the UI shows beside it.
        let j = join_services(
            &[row(8420, "node.exe")],
            &[
                tcp4([127, 0, 0, 1], 24678, 8420),
                tcp4([127, 0, 0, 1], 5173, 8420),
                tcp6("::1", 9229, 8420),
            ],
        );
        assert_eq!(j.services[0].label, "node:5173");
    }

    #[test]
    fn the_label_is_deterministic_and_independent_of_what_else_is_running() {
        let alone = join_services(&[row(1, "node.exe")], &[tcp4([127, 0, 0, 1], 5173, 1)]);
        let crowded = join_services(
            &[row(1, "node.exe"), row(2, "node.exe"), row(3, "python.exe")],
            &[
                tcp4([127, 0, 0, 1], 5173, 1),
                tcp4([127, 0, 0, 1], 3000, 2),
                tcp4([127, 0, 0, 1], 8000, 3),
            ],
        );
        let same = crowded.services.iter().find(|s| s.pid == 1).unwrap();
        assert_eq!(alone.services[0].label, same.label);
        assert_eq!(same.label, "node:5173");
    }

    #[test]
    fn repeating_the_join_gives_byte_identical_labels_and_ordering() {
        let rows = [row(2, "b.exe"), row(1, "a.exe")];
        let sockets = [
            tcp6("::1", 5173, 1),
            tcp4([127, 0, 0, 1], 8000, 2),
            tcp4([127, 0, 0, 1], 5173, 1),
        ];
        let a = join_services(&rows, &sockets);
        let b = join_services(&rows, &sockets);

        let shape = |j: &ServiceJoin| {
            j.services
                .iter()
                .map(|s| {
                    (
                        s.id.clone(),
                        s.label.clone(),
                        s.endpoints
                            .iter()
                            .map(|e| (e.port, e.address.clone()))
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(shape(&a), shape(&b));
    }

    #[test]
    fn endpoint_order_is_by_port_then_ipv4_before_ipv6() {
        let j = join_services(
            &[row(1, "node.exe")],
            &[
                tcp6("::1", 8000, 1),
                tcp6("::1", 5173, 1),
                tcp4([127, 0, 0, 1], 8000, 1),
                tcp4([127, 0, 0, 1], 5173, 1),
            ],
        );
        let order: Vec<_> = j.services[0]
            .endpoints
            .iter()
            .map(|e| (e.port, e.address.as_str()))
            .collect();
        assert_eq!(
            order,
            vec![
                (5173, "127.0.0.1"),
                (5173, "[::1]"),
                (8000, "127.0.0.1"),
                (8000, "[::1]"),
            ]
        );
    }

    #[test]
    fn an_executable_stem_drops_only_a_trailing_exe() {
        assert_eq!(executable_stem("node.exe"), "node");
        assert_eq!(executable_stem("NODE.EXE"), "NODE");
        assert_eq!(executable_stem("redis-server.exe"), "redis-server");
        assert_eq!(executable_stem("System"), "System");
        assert_eq!(executable_stem("my.app.exe"), "my.app");
        // Not a stem: dropping these would leave nothing to read.
        assert_eq!(executable_stem(".exe"), ".exe");
        assert_eq!(executable_stem(""), "unknown");
        assert_eq!(executable_stem("   "), "unknown");
    }

    #[test]
    fn a_process_with_an_unreadable_name_still_gets_a_findable_label() {
        let j = join_services(&[row(8420, "")], &[tcp4([127, 0, 0, 1], 5173, 8420)]);
        assert_eq!(j.services[0].label, "unknown:5173");
    }

    // ----------------------------------------------------------- the lookups

    #[test]
    fn labels_are_offered_for_exactly_the_processes_that_became_services() {
        let j = join_services(
            &[row(1, "node.exe"), row(2, "idle.exe")],
            &[tcp4([127, 0, 0, 1], 5173, 1)],
        );

        assert_eq!(j.labels().len(), 1);
        assert_eq!(
            j.labels().get(&make_process_id(1, T)).map(String::as_str),
            Some("node:5173")
        );
        assert!(j.labels().get(&make_process_id(2, T)).is_none());
    }

    #[test]
    fn is_service_answers_for_both_halves() {
        let j = join_services(
            &[row(1, "node.exe"), row(2, "idle.exe")],
            &[tcp4([127, 0, 0, 1], 5173, 1)],
        );
        assert!(j.is_service(&make_process_id(1, T)));
        assert!(!j.is_service(&make_process_id(2, T)));
        assert!(!j.is_service(&make_process_id(999, T)));
    }

    #[test]
    fn every_service_label_matches_the_label_offered_for_its_identity() {
        let j = join_services(
            &[row(1, "node.exe"), row(2, "python.exe")],
            &[tcp4([127, 0, 0, 1], 5173, 1), tcp4([127, 0, 0, 1], 8000, 2)],
        );
        for s in &j.services {
            assert_eq!(j.labels().get(&s.id), Some(&s.label));
        }
    }

    #[test]
    fn an_empty_tick_joins_to_nothing() {
        let j = join_services(&[], &[]);
        assert!(j.services.is_empty());
        assert!(j.labels().is_empty());
    }
}
