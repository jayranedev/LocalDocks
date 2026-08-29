//! Turning bound sockets into `PortRow`s.
//!
//! Two jobs, both pure:
//!
//!   * **Presentation.** Windows hands over an address as bytes; the contract
//!     wants `127.0.0.1` or `[::1]`. The bracket form is not decoration — the
//!     Ports screen tells IPv6 from IPv4 with `address.startsWith('[')`.
//!   * **Attribution.** Every socket arrives with an owning PID. Turning that
//!     PID into a process *identity* is the part that has to be careful, and
//!     the part that decides whether a row is actionable.
//!
//! Endpoint identity is `(protocol, address family, local address, local port)`
//! — never the port alone (docs/ARCHITECTURE.md § 5). `127.0.0.1:5173`,
//! `[::1]:5173` and `0.0.0.0:5173` are three different endpoints, and the Ports
//! view exists precisely to show them unmerged.

use std::collections::{HashMap, HashSet};
use std::net::IpAddr;

use crate::models::{PortRow, PortState, ProcessId, ProcessRow, Protocol};
use crate::platform::windows::ports::RawEndpoint;
use crate::platform::windows::process::RawProcess;

/// Render an address the way the contract and the UI expect it.
///
/// IPv6 is bracketed, which is both the URL convention and how the frontend
/// recognises the family. A link-local address keeps its scope — `fe80::1` on
/// two interfaces is two different endpoints, and dropping the `%17` would
/// silently merge them.
///
/// Wildcards need no special case and deliberately get none: `0.0.0.0` and
/// `[::]` format like any other address. Rewriting them as "localhost" would
/// claim a socket is local-only when it is in fact reachable from the network —
/// the opposite of what a developer checking exposure needs to see.
pub fn format_address(address: IpAddr, scope_id: u32) -> String {
    match address {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) if scope_id == 0 => format!("[{v6}]"),
        IpAddr::V6(v6) => format!("[{v6}%{scope_id}]"),
    }
}

/// What a socket's owning PID could be resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Owner {
    name: String,
    /// `Some` only when the process is one we could read a creation time for.
    /// See `attribute` for why this is not simply "the PID we were given".
    id: Option<ProcessId>,
}

/// Build `PortRow`s from this tick's sockets and this tick's processes.
///
/// Takes both the raw process list and the mapped rows because they answer
/// different questions. `rows` holds identities, but only for processes whose
/// creation time was readable. `processes` holds a name for *every* process,
/// including the protected ones excluded from `rows` — and protected system
/// processes own most of the listening sockets on a Windows machine. Using both
/// means a row can name `svchost.exe` honestly while still refusing to give it
/// an identity.
///
/// Nothing here opens a handle: every fact was already gathered by the process
/// scan earlier in the same tick.
pub fn map_ports(
    endpoints: &[RawEndpoint],
    processes: &[RawProcess],
    rows: &[ProcessRow],
) -> Vec<PortRow> {
    let names: HashMap<u32, &str> = processes.iter().map(|p| (p.pid, p.name.as_str())).collect();
    let identities: HashMap<u32, &ProcessId> = rows.iter().map(|r| (r.pid, &r.id)).collect();

    let mut seen = HashSet::with_capacity(endpoints.len());
    let mut out = Vec::with_capacity(endpoints.len());

    for e in endpoints {
        // Windows really does repeat rows. A process may bind one address and
        // port several times with SO_REUSEADDR — that is how multicast
        // discovery works, and WS-Discovery on `0.0.0.0:3702` does it on an
        // ordinary machine. The extended table has no socket handle, so the
        // repeats are identical in every field the contract carries.
        //
        // The key includes the PID on purpose: two *different* processes
        // sharing a multicast port are two real rows and must both survive.
        // It is exactly the key Ports.tsx renders with, so anything dropped
        // here would otherwise have collided a React key while showing the
        // user nothing new.
        if !seen.insert((e.protocol, e.address, e.scope_id, e.port, e.pid)) {
            continue;
        }

        let owner = attribute(e.pid, &names, &identities);

        out.push(PortRow {
            port: e.port,
            protocol: e.protocol,
            address: format_address(e.address, e.scope_id),
            pid: e.pid,
            process_id: owner.id,
            process_name: owner.name,
            // Service joining is the next milestone (docs/ROADMAP.md § 2.2).
            // `None` renders as "—", which is what "not grouped yet" looks
            // like; inventing a label from the process name would be a claim
            // the backend cannot support.
            service_label: None,
            // Truthful by construction: the TCP table is queried with
            // TCP_TABLE_OWNER_PID_LISTENER, so every TCP row is a listener,
            // and a bound UDP socket is receiving. UDP has no connection state
            // to report and the contract offers no other value.
            state: PortState::Listening,
        });
    }

    // A stable order, so rows do not swap places between ticks just because
    // Windows walked its table differently. The frontend sorts too, but its
    // comparator ties on same-port-same-address pairs that differ only by
    // protocol, and a tie on unstable input is a visibly jittery table.
    out.sort_by(|a, b| {
        a.port
            .cmp(&b.port)
            .then_with(|| a.address.starts_with('[').cmp(&b.address.starts_with('[')))
            .then_with(|| a.address.cmp(&b.address))
            .then_with(|| protocol_order(a.protocol).cmp(&protocol_order(b.protocol)))
    });

    out
}

/// Resolve an owning PID to a name and, where it is safe to do so, an identity.
///
/// Three outcomes, and the difference between them is the safety model:
///
///   * **Known process** — in this tick's rows, so its creation time was
///     readable. It gets `{pid}-{startedAt}`, and the UI will let the user open
///     it and act on it.
///   * **Named but unidentifiable** — enumerated, so the name is real, but its
///     creation time could not be read (a protected system process). The row
///     names it and stops there: `processId` is `None`, so the UI renders it as
///     informational and refuses to act. Synthesising an identity here is
///     exactly the failure the identity model exists to prevent — the user
///     would be one click from terminating whatever now holds that PID.
///   * **Gone** — the socket outlived the process, or the process appeared
///     after the process scan and before the port scan. Both are ordinary in a
///     non-atomic scan, so the row is kept as informational rather than dropped:
///     the socket is genuinely bound, and hiding it would be the bigger lie.
fn attribute(pid: u32, names: &HashMap<u32, &str>, identities: &HashMap<u32, &ProcessId>) -> Owner {
    match names.get(&pid) {
        Some(name) => Owner {
            name: (*name).to_string(),
            id: identities.get(&pid).map(|id| (*id).clone()),
        },
        None => Owner {
            // Not a name we invented and not an empty cell: the process really
            // was there when Windows attributed the socket, and is not there
            // now. Saying so is more useful than a blank.
            name: "(exited)".to_string(),
            id: None,
        },
    }
}

/// TCP before UDP when everything else ties, so a machine binding both on one
/// port reads in a predictable order.
fn protocol_order(p: Protocol) -> u8 {
    match p {
        Protocol::Tcp => 0,
        Protocol::Udp => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ProcessStatus;
    use std::net::{Ipv4Addr, Ipv6Addr};

    fn endpoint(protocol: Protocol, address: IpAddr, port: u16, pid: u32) -> RawEndpoint {
        RawEndpoint {
            protocol,
            address,
            scope_id: 0,
            port,
            pid,
        }
    }

    fn v4(a: [u8; 4], port: u16, pid: u32) -> RawEndpoint {
        endpoint(Protocol::Tcp, IpAddr::V4(Ipv4Addr::from(a)), port, pid)
    }

    fn v6(s: &str, port: u16, pid: u32) -> RawEndpoint {
        endpoint(
            Protocol::Tcp,
            IpAddr::V6(s.parse::<Ipv6Addr>().unwrap()),
            port,
            pid,
        )
    }

    fn raw_process(pid: u32, name: &str) -> RawProcess {
        RawProcess {
            pid,
            parent_pid: 4,
            name: name.into(),
            thread_count: 3,
            probe: crate::platform::windows::process::ProcessProbe::AccessDenied,
        }
    }

    fn row(pid: u32, name: &str, started_at: &str) -> ProcessRow {
        ProcessRow {
            id: crate::models::make_process_id(pid, started_at),
            pid,
            parent_pid: 4,
            name: name.into(),
            cpu_percent: 0.0,
            memory_bytes: 1024,
            thread_count: 3,
            started_at: started_at.into(),
            uptime_seconds: 1.0,
            status: ProcessStatus::Running,
            is_service: false,
        }
    }

    const T: &str = "2026-08-28T09:00:00.000Z";

    // ------------------------------------------------------------ formatting

    #[test]
    fn ipv4_addresses_render_plain() {
        assert_eq!(
            format_address(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0),
            "127.0.0.1"
        );
        assert_eq!(
            format_address(IpAddr::V4(Ipv4Addr::new(192, 168, 68, 103)), 0),
            "192.168.68.103"
        );
    }

    #[test]
    fn ipv6_addresses_render_bracketed() {
        // The brackets are load-bearing: Ports.tsx uses them to tell the
        // families apart.
        assert_eq!(
            format_address(IpAddr::V6("::1".parse().unwrap()), 0),
            "[::1]"
        );
        assert_eq!(
            format_address(IpAddr::V6("fe80::1".parse().unwrap()), 0),
            "[fe80::1]"
        );
        assert!(format_address(IpAddr::V6("::1".parse().unwrap()), 0).starts_with('['));
    }

    #[test]
    fn wildcard_addresses_are_rendered_as_wildcards_not_as_localhost() {
        // A wildcard bind is reachable from the network. Calling it localhost
        // would understate exposure, which is the opposite of useful.
        assert_eq!(
            format_address(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
            "0.0.0.0"
        );
        assert_eq!(format_address(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0), "[::]");
    }

    #[test]
    fn a_link_local_address_keeps_its_scope() {
        // Two interfaces can carry the same fe80:: address; the scope is what
        // keeps them apart.
        assert_eq!(
            format_address(IpAddr::V6("fe80::7495:bc12:fb98:1161".parse().unwrap()), 17),
            "[fe80::7495:bc12:fb98:1161%17]"
        );
    }

    #[test]
    fn ipv6_output_is_never_malformed() {
        for s in [
            "::",
            "::1",
            "fe80::1",
            "2001:db8::8a2e:370:7334",
            "::ffff:127.0.0.1",
        ] {
            for scope in [0u32, 17] {
                let out = format_address(IpAddr::V6(s.parse().unwrap()), scope);
                assert!(out.starts_with('['), "{out} lost its opening bracket");
                assert!(out.ends_with(']'), "{out} lost its closing bracket");
                assert_eq!(out.matches('[').count(), 1, "{out} has stray brackets");
                assert_eq!(out.matches(']').count(), 1, "{out} has stray brackets");
            }
        }
    }

    // -------------------------------------------------------------- identity

    #[test]
    fn the_same_port_on_different_addresses_stays_several_rows() {
        // The exact case docs/ARCHITECTURE.md § 5 describes: one dev server,
        // three endpoints. Collapsing them by port is the bug.
        let endpoints = vec![
            v4([127, 0, 0, 1], 5173, 8420),
            v6("::1", 5173, 8420),
            v4([0, 0, 0, 0], 5173, 8420),
            v6("::", 5173, 8420),
        ];
        let ports = map_ports(&endpoints, &[raw_process(8420, "node.exe")], &[]);

        assert_eq!(ports.len(), 4);
        let addresses: Vec<_> = ports.iter().map(|p| p.address.as_str()).collect();
        assert!(addresses.contains(&"127.0.0.1"));
        assert!(addresses.contains(&"[::1]"));
        assert!(addresses.contains(&"0.0.0.0"));
        assert!(addresses.contains(&"[::]"));
        assert!(ports.iter().all(|p| p.port == 5173));
    }

    #[test]
    fn the_same_port_on_different_protocols_stays_two_rows() {
        let endpoints = vec![
            v4([127, 0, 0, 1], 8000, 1),
            endpoint(Protocol::Udp, IpAddr::V4(Ipv4Addr::LOCALHOST), 8000, 1),
        ];
        let ports = map_ports(&endpoints, &[raw_process(1, "python.exe")], &[]);

        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].protocol, Protocol::Tcp, "TCP sorts first");
        assert_eq!(ports[1].protocol, Protocol::Udp);
    }

    #[test]
    fn rows_are_unique_on_the_key_the_frontend_renders_with() {
        // Ports.tsx keys on `${protocol}-${address}-${port}-${pid}`.
        let endpoints = vec![
            v4([127, 0, 0, 1], 5173, 8420),
            v6("::1", 5173, 8420),
            v4([127, 0, 0, 1], 5174, 8420),
            endpoint(Protocol::Udp, IpAddr::V4(Ipv4Addr::LOCALHOST), 5173, 8420),
        ];
        let ports = map_ports(&endpoints, &[raw_process(8420, "node.exe")], &[]);

        let mut keys = HashSet::new();
        for p in &ports {
            let key = format!("{:?}-{}-{}-{}", p.protocol, p.address, p.port, p.pid);
            assert!(keys.insert(key), "duplicate React key for {p:?}");
        }
        assert_eq!(keys.len(), 4);
    }

    #[test]
    fn one_process_binding_a_multicast_port_twice_yields_one_row() {
        // Real behaviour, not hypothetical: WS-Discovery binds 0.0.0.0:3702
        // twice from a single PID via SO_REUSEADDR, and the extended table
        // reports both with every field identical. The dropped row carries
        // nothing the kept one lacks.
        let e = endpoint(Protocol::Udp, IpAddr::V4(Ipv4Addr::UNSPECIFIED), 3702, 6872);
        let ports = map_ports(&[e.clone(), e], &[raw_process(6872, "svchost.exe")], &[]);

        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].port, 3702);
        assert_eq!(ports[0].address, "0.0.0.0");
    }

    #[test]
    fn two_processes_sharing_a_multicast_port_stay_two_rows() {
        // The other half of the same rule. These differ in PID and in process
        // name, so collapsing them would hide a real socket owner.
        let a = endpoint(Protocol::Udp, IpAddr::V4(Ipv4Addr::UNSPECIFIED), 5353, 100);
        let b = endpoint(Protocol::Udp, IpAddr::V4(Ipv4Addr::UNSPECIFIED), 5353, 200);
        let ports = map_ports(
            &[a, b],
            &[
                raw_process(100, "mdns-a.exe"),
                raw_process(200, "mdns-b.exe"),
            ],
            &[],
        );

        assert_eq!(ports.len(), 2);
        let names: HashSet<_> = ports.iter().map(|p| p.process_name.as_str()).collect();
        assert_eq!(names.len(), 2, "both owners must be visible");
    }

    #[test]
    fn a_link_local_socket_on_two_interfaces_is_two_rows() {
        let mut a = v6("fe80::1", 5173, 8420);
        a.scope_id = 12;
        let mut b = v6("fe80::1", 5173, 8420);
        b.scope_id = 17;

        let ports = map_ports(&[a, b], &[raw_process(8420, "node.exe")], &[]);
        assert_eq!(ports.len(), 2);
        assert_ne!(ports[0].address, ports[1].address);
    }

    #[test]
    fn rows_come_out_in_a_stable_order() {
        let endpoints = vec![
            v6("::1", 8000, 1),
            v4([127, 0, 0, 1], 5173, 1),
            v4([127, 0, 0, 1], 8000, 1),
            v6("::1", 5173, 1),
        ];
        let ports = map_ports(&endpoints, &[raw_process(1, "node.exe")], &[]);
        let order: Vec<_> = ports.iter().map(|p| (p.port, p.address.as_str())).collect();

        // By port, then IPv4 before IPv6 — the order the Ports screen also
        // uses, so the default view needs no re-sorting to look sensible.
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

    // ----------------------------------------------------------- attribution

    #[test]
    fn a_known_process_gives_the_row_a_usable_identity() {
        let ports = map_ports(
            &[v4([127, 0, 0, 1], 5173, 8420)],
            &[raw_process(8420, "node.exe")],
            &[row(8420, "node.exe", T)],
        );

        assert_eq!(ports[0].pid, 8420);
        assert_eq!(ports[0].process_name, "node.exe");
        assert_eq!(
            ports[0].process_id.as_deref(),
            Some("8420-2026-08-28T09:00:00.000Z")
        );
    }

    #[test]
    fn a_protected_process_is_named_but_gets_no_identity() {
        // The common case on Windows: svchost owns the socket, is enumerable,
        // but cannot be opened unelevated. The name is real; the identity is
        // not available, so the row must stay informational.
        let ports = map_ports(
            &[v4([0, 0, 0, 0], 135, 1160)],
            &[raw_process(1160, "svchost.exe")],
            &[], // not in rows: its creation time was unreadable
        );

        assert_eq!(ports[0].process_name, "svchost.exe");
        assert_eq!(ports[0].process_id, None, "must not be actionable");
        assert_eq!(
            ports[0].pid, 1160,
            "the PID Windows reported is still shown"
        );
    }

    #[test]
    fn a_socket_whose_process_vanished_is_kept_as_informational() {
        // The scan is not atomic. The socket was really bound; the process is
        // really gone. Dropping the row would hide a real socket.
        let ports = map_ports(&[v4([127, 0, 0, 1], 5173, 9999)], &[], &[]);

        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].pid, 9999);
        assert_eq!(ports[0].process_id, None);
        assert_eq!(ports[0].process_name, "(exited)");
    }

    #[test]
    fn an_identity_is_never_synthesised_from_a_pid_alone() {
        // The whole safety model in one assertion: a PID with no readable
        // creation time must not produce `{pid}-{something}`.
        let ports = map_ports(
            &[v4([127, 0, 0, 1], 5173, 8420)],
            &[raw_process(8420, "node.exe")],
            &[],
        );
        assert!(ports[0].process_id.is_none());
    }

    #[test]
    fn a_reused_pid_attributes_to_the_process_that_is_there_now() {
        // The port table gives a PID; the identity comes from this tick's
        // process rows, so a recycled PID resolves to the current occupant
        // rather than to whatever used to hold it.
        let ports = map_ports(
            &[v4([127, 0, 0, 1], 5173, 8420)],
            &[raw_process(8420, "python.exe")],
            &[row(8420, "python.exe", "2026-08-28T10:00:00.000Z")],
        );
        assert_eq!(
            ports[0].process_id.as_deref(),
            Some("8420-2026-08-28T10:00:00.000Z")
        );
        assert_eq!(ports[0].process_name, "python.exe");
    }

    #[test]
    fn several_sockets_of_one_process_all_carry_the_same_identity() {
        let ports = map_ports(
            &[
                v4([127, 0, 0, 1], 5173, 8420),
                v6("::1", 5173, 8420),
                v4([127, 0, 0, 1], 24678, 8420),
            ],
            &[raw_process(8420, "node.exe")],
            &[row(8420, "node.exe", T)],
        );

        let ids: HashSet<_> = ports.iter().map(|p| p.process_id.clone()).collect();
        assert_eq!(ids.len(), 1, "one process, one identity across its sockets");
    }

    // ------------------------------------------------------ contract details

    #[test]
    fn protocols_map_straight_through() {
        let ports = map_ports(
            &[
                v4([127, 0, 0, 1], 1, 1),
                endpoint(Protocol::Udp, IpAddr::V4(Ipv4Addr::LOCALHOST), 2, 1),
            ],
            &[raw_process(1, "x.exe")],
            &[],
        );
        assert_eq!(ports[0].protocol, Protocol::Tcp);
        assert_eq!(ports[1].protocol, Protocol::Udp);
    }

    #[test]
    fn every_row_reports_the_listening_state() {
        let ports = map_ports(
            &[v4([127, 0, 0, 1], 5173, 1)],
            &[raw_process(1, "node.exe")],
            &[],
        );
        assert_eq!(ports[0].state, PortState::Listening);
    }

    #[test]
    fn no_row_claims_a_service_label_while_the_service_model_is_unimplemented() {
        let ports = map_ports(
            &[v4([127, 0, 0, 1], 5173, 1)],
            &[raw_process(1, "node.exe")],
            &[row(1, "node.exe", T)],
        );
        assert!(ports[0].service_label.is_none());
    }

    #[test]
    fn an_empty_scan_maps_to_no_rows() {
        assert!(map_ports(&[], &[], &[]).is_empty());
    }
}
