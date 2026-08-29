//! Listening-socket enumeration via the IP Helper extended tables.
//!
//! Four calls, not one. Windows keeps TCP and UDP in separate tables, and
//! keeps IPv4 and IPv6 in separate tables again, so a dev server bound to both
//! families appears in two of them. docs/BACKEND.md puts it bluntly:
//! forgetting the second call "is how half a dev server goes missing".
//!
//! Every table uses the same awkward two-step: ask with a null buffer to learn
//! the size, allocate, ask again. The table can grow between those two calls —
//! a process can bind a socket in the microseconds in between — so the second
//! call can still answer `ERROR_INSUFFICIENT_BUFFER`. That is expected, and
//! `query` retries rather than treating it as a failure.
//!
//! Nothing here needs elevation. The extended tables report the owning PID for
//! every socket to an ordinary user; the app never opens a socket of its own.

use std::ffi::c_void;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, NO_ERROR, WIN32_ERROR};
// Only the row types are named: `table_rows` is generic, so the enclosing
// MIB_*TABLE_OWNER_PID structs never need to be spelled out.
use windows::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, GetExtendedUdpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID,
    MIB_UDP6ROW_OWNER_PID, MIB_UDPROW_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER, UDP_TABLE_OWNER_PID,
};
use windows::Win32::Networking::WinSock::{AF_INET, AF_INET6};

use crate::errors::SystemError;
use crate::models::Protocol;

/// How many times to re-ask when the table grows between sizing and reading.
///
/// Three is generous: each retry starts from the size Windows just reported, so
/// it only loops again if the table grew twice in a row inside a few
/// microseconds. Bounded so a pathologically busy machine cannot spin here.
const MAX_ATTEMPTS: usize = 4;

/// One bound socket as Windows reported it.
///
/// `IpAddr` rather than a Win32 type on purpose: the address arrives here as
/// raw bytes and leaves as a `std` value, so `logic` can format and test it
/// without knowing Windows exists.
///
/// `Protocol` is reused from `models` rather than duplicated. The two would be
/// the same two variants with a mapping function between them, and
/// docs/ARCHITECTURE.md permits domain and IPC types to diverge — it does not
/// require inventing a difference that is not there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEndpoint {
    pub protocol: Protocol,
    pub address: IpAddr,
    /// IPv6 interface scope. Non-zero only for link-local addresses, where two
    /// interfaces can legitimately carry the same `fe80::` address — dropping
    /// it would merge two distinct endpoints into one.
    pub scope_id: u32,
    pub port: u16,
    pub pid: u32,
}

/// Every listening TCP socket and every bound UDP socket, both families.
///
/// A failure in any one table fails the whole scan: a partial port list looks
/// exactly like "that service is not running", which is a worse answer than an
/// error the UI can show.
pub fn enumerate() -> Result<Vec<RawEndpoint>, SystemError> {
    let mut endpoints = Vec::with_capacity(128);
    endpoints.extend(tcp_v4()?);
    endpoints.extend(tcp_v6()?);
    endpoints.extend(udp_v4()?);
    endpoints.extend(udp_v6()?);
    Ok(endpoints)
}

// ---------------------------------------------------------------------- TCP

fn tcp_v4() -> Result<Vec<RawEndpoint>, SystemError> {
    let buffer = query("GetExtendedTcpTable(AF_INET)", |ptr, size| unsafe {
        GetExtendedTcpTable(
            ptr,
            size,
            false,
            AF_INET.0 as u32,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        )
    })?;

    // SAFETY: `query` returned a buffer Windows filled with a
    // MIB_TCPTABLE_OWNER_PID, and Vec<u32> gives the 4-byte alignment every
    // field in it needs.
    let rows: &[MIB_TCPROW_OWNER_PID] = unsafe { table_rows(&buffer) };

    Ok(rows
        .iter()
        .map(|r| RawEndpoint {
            protocol: Protocol::Tcp,
            address: IpAddr::V4(ipv4(r.dwLocalAddr)),
            scope_id: 0,
            port: port(r.dwLocalPort),
            pid: r.dwOwningPid,
        })
        .collect())
}

fn tcp_v6() -> Result<Vec<RawEndpoint>, SystemError> {
    let buffer = query("GetExtendedTcpTable(AF_INET6)", |ptr, size| unsafe {
        GetExtendedTcpTable(
            ptr,
            size,
            false,
            AF_INET6.0 as u32,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        )
    })?;

    // SAFETY: as above, for MIB_TCP6TABLE_OWNER_PID.
    let rows: &[MIB_TCP6ROW_OWNER_PID] = unsafe { table_rows(&buffer) };

    Ok(rows
        .iter()
        .map(|r| RawEndpoint {
            protocol: Protocol::Tcp,
            address: IpAddr::V6(Ipv6Addr::from(r.ucLocalAddr)),
            scope_id: r.dwLocalScopeId,
            port: port(r.dwLocalPort),
            pid: r.dwOwningPid,
        })
        .collect())
}

// ---------------------------------------------------------------------- UDP

fn udp_v4() -> Result<Vec<RawEndpoint>, SystemError> {
    let buffer = query("GetExtendedUdpTable(AF_INET)", |ptr, size| unsafe {
        GetExtendedUdpTable(ptr, size, false, AF_INET.0 as u32, UDP_TABLE_OWNER_PID, 0)
    })?;

    // SAFETY: as above, for MIB_UDPTABLE_OWNER_PID.
    let rows: &[MIB_UDPROW_OWNER_PID] = unsafe { table_rows(&buffer) };

    Ok(rows
        .iter()
        .map(|r| RawEndpoint {
            protocol: Protocol::Udp,
            address: IpAddr::V4(ipv4(r.dwLocalAddr)),
            scope_id: 0,
            port: port(r.dwLocalPort),
            pid: r.dwOwningPid,
        })
        .collect())
}

fn udp_v6() -> Result<Vec<RawEndpoint>, SystemError> {
    let buffer = query("GetExtendedUdpTable(AF_INET6)", |ptr, size| unsafe {
        GetExtendedUdpTable(ptr, size, false, AF_INET6.0 as u32, UDP_TABLE_OWNER_PID, 0)
    })?;

    // SAFETY: as above, for MIB_UDP6TABLE_OWNER_PID.
    let rows: &[MIB_UDP6ROW_OWNER_PID] = unsafe { table_rows(&buffer) };

    Ok(rows
        .iter()
        .map(|r| RawEndpoint {
            protocol: Protocol::Udp,
            address: IpAddr::V6(Ipv6Addr::from(r.ucLocalAddr)),
            scope_id: r.dwLocalScopeId,
            port: port(r.dwLocalPort),
            pid: r.dwOwningPid,
        })
        .collect())
}

// ------------------------------------------------------------------- shared

/// Size, allocate, read — retrying while the table keeps outgrowing the buffer.
///
/// The buffer is a `Vec<u32>` rather than a `Vec<u8>` purely for alignment:
/// every field in these tables is a `DWORD` or a byte array, so 4-byte
/// alignment is enough, and a `Vec<u8>` would only be aligned by luck.
fn query<F>(call: &'static str, fetch: F) -> Result<Vec<u32>, SystemError>
where
    F: Fn(Option<*mut c_void>, *mut u32) -> u32,
{
    let mut size: u32 = 0;

    // First call with no buffer: this is expected to fail, and the failure is
    // how the size is returned.
    let probe = fetch(None, &mut size);
    if probe != ERROR_INSUFFICIENT_BUFFER.0 && probe != NO_ERROR.0 {
        return Err(win32_error(call, probe));
    }

    for _ in 0..MAX_ATTEMPTS {
        // An empty table reports size 0 on some builds; one word still gives a
        // valid pointer to hand Windows, and dwNumEntries will read as 0.
        let words = (size as usize).div_ceil(4).max(1);
        let mut buffer = vec![0u32; words];
        size = (words * 4) as u32;

        match fetch(Some(buffer.as_mut_ptr() as *mut c_void), &mut size) {
            code if code == NO_ERROR.0 => return Ok(buffer),
            // The table grew between sizing and reading. `size` now holds the
            // new requirement, so the next attempt allocates from fact rather
            // than from a guess.
            code if code == ERROR_INSUFFICIENT_BUFFER.0 => continue,
            code => return Err(win32_error(call, code)),
        }
    }

    Err(win32_error(call, ERROR_INSUFFICIENT_BUFFER.0))
}

/// Reinterpret a filled table buffer as its row slice.
///
/// Every one of these tables is `{ dwNumEntries: DWORD, table: [ROW; ANY] }`,
/// so the rows begin one aligned `ROW` boundary in and run `dwNumEntries` long.
///
/// # Safety
///
/// `buffer` must be a buffer a successful `GetExtended*Table` call filled with
/// the table type matching `R`, and must be aligned for `R`.
unsafe fn table_rows<R>(buffer: &[u32]) -> &[R] {
    // The header is one DWORD, but the row array starts at the struct's own
    // alignment, which `offset_of` would give if these were stable across the
    // four table types. They all place `table` immediately after the count with
    // 4-byte alignment, so one word in is correct for every one of them.
    let count = buffer[0] as usize;
    if count == 0 {
        return &[];
    }
    let rows = buffer.as_ptr().add(1) as *const R;
    std::slice::from_raw_parts(rows, count)
}

/// A `DWORD` holding an IPv4 address in network byte order.
///
/// `to_ne_bytes` returns the bytes as they sit in memory, which is what network
/// order means here, so this is correct on either endianness rather than
/// accidentally correct on x86.
fn ipv4(addr: u32) -> Ipv4Addr {
    Ipv4Addr::from(addr.to_ne_bytes())
}

/// A `DWORD` whose low two bytes hold the port in network byte order.
///
/// The upper two bytes are reserved and must be ignored — reading the DWORD as
/// a port would give values in the millions.
fn port(raw: u32) -> u16 {
    let b = raw.to_ne_bytes();
    u16::from_be_bytes([b[0], b[1]])
}

fn win32_error(call: &'static str, code: u32) -> SystemError {
    let message = WIN32_ERROR(code).to_hresult().message();
    SystemError::api_failure(call, code, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::net::TcpListener;

    #[test]
    fn an_ipv4_dword_is_read_in_network_order() {
        // 127.0.0.1 as Windows stores it: the first octet in the first byte.
        let raw = u32::from_ne_bytes([127, 0, 0, 1]);
        assert_eq!(ipv4(raw), Ipv4Addr::new(127, 0, 0, 1));

        assert_eq!(
            ipv4(u32::from_ne_bytes([0, 0, 0, 0])),
            Ipv4Addr::UNSPECIFIED
        );
        assert_eq!(
            ipv4(u32::from_ne_bytes([192, 168, 68, 103])),
            Ipv4Addr::new(192, 168, 68, 103)
        );
    }

    #[test]
    fn a_port_dword_uses_only_its_low_two_bytes_big_endian() {
        // 5173 = 0x1435, network order 0x14 0x35, reserved bytes after it.
        assert_eq!(port(u32::from_ne_bytes([0x14, 0x35, 0, 0])), 5173);
        assert_eq!(port(u32::from_ne_bytes([0x00, 0x50, 0, 0])), 80);
        assert_eq!(port(u32::from_ne_bytes([0xFF, 0xFF, 0, 0])), 65535);
        // Reserved bytes set: they must not leak into the value.
        assert_eq!(port(u32::from_ne_bytes([0x14, 0x35, 0xAB, 0xCD])), 5173);
    }

    #[test]
    fn a_byte_swapped_port_would_be_wrong_and_is_not_what_we_produce() {
        // Guards against reading the DWORD little-endian, which turns 5173
        // into 13844 — a plausible-looking port, which is what makes the bug
        // survive a casual look.
        assert_ne!(port(u32::from_ne_bytes([0x14, 0x35, 0, 0])), 13844);
    }

    #[test]
    fn an_empty_table_yields_no_rows() {
        let buffer = vec![0u32; 4];
        // SAFETY: dwNumEntries is 0, so no row is ever dereferenced.
        let rows: &[MIB_TCPROW_OWNER_PID] = unsafe { table_rows(&buffer) };
        assert!(rows.is_empty());
    }

    // ------------------------------------------------------- live enumeration

    #[test]
    fn enumeration_returns_structurally_valid_endpoints() {
        let endpoints = enumerate().expect("enumerating listening sockets must succeed");
        assert!(
            !endpoints.is_empty(),
            "a running Windows machine always has bound sockets"
        );

        for e in &endpoints {
            assert!(e.port > 0, "port 0 is not a bound port: {e:?}");
            assert!(matches!(e.protocol, Protocol::Tcp | Protocol::Udp));
            match e.address {
                IpAddr::V4(_) => assert_eq!(e.scope_id, 0, "IPv4 has no scope id: {e:?}"),
                IpAddr::V6(_) => {}
            }
        }
    }

    #[test]
    fn both_families_and_both_protocols_are_reachable() {
        // Not "there must be N of each" — that depends on the machine. But a
        // Windows box always has some of each, and a missing table call is
        // exactly the bug docs/BACKEND.md warns about, so all four buckets
        // being non-empty is the invariant worth holding.
        let endpoints = enumerate().expect("enumerate");
        let has = |p: Protocol, v6: bool| {
            endpoints
                .iter()
                .any(|e| e.protocol == p && e.address.is_ipv6() == v6)
        };
        assert!(has(Protocol::Tcp, false), "no TCP IPv4 rows");
        assert!(has(Protocol::Tcp, true), "no TCP IPv6 rows");
        assert!(has(Protocol::Udp, false), "no UDP IPv4 rows");
        assert!(has(Protocol::Udp, true), "no UDP IPv6 rows");
    }

    #[test]
    fn a_tcp_listener_is_never_reported_twice() {
        // Two sockets cannot hold the same TCP address and port, so a repeat
        // here would mean the table was misread.
        //
        // UDP is deliberately excluded: a process can bind one address and
        // port several times with SO_REUSEADDR, which is how multicast
        // discovery works. This machine really does report 0.0.0.0:3702
        // (WS-Discovery) twice from one PID, and the extended table has no
        // socket handle to tell the two apart — which is why `logic::ports`
        // deduplicates rather than trusting this.
        let endpoints = enumerate().expect("enumerate");
        let mut seen = HashSet::new();
        for e in endpoints.iter().filter(|e| e.protocol == Protocol::Tcp) {
            let key = (e.address, e.scope_id, e.port);
            assert!(seen.insert(key), "TCP listener enumerated twice: {e:?}");
        }
    }

    #[test]
    fn any_repeated_socket_is_udp_and_is_repeated_identically() {
        // Documents the shape of the duplication rather than denying it: when
        // a socket repeats it is UDP, and every field matches, so the row the
        // deduplication drops carries no information the kept row lacks.
        let endpoints = enumerate().expect("enumerate");
        let mut seen: HashSet<(Protocol, std::net::IpAddr, u32, u16, u32)> = HashSet::new();
        for e in &endpoints {
            let key = (e.protocol, e.address, e.scope_id, e.port, e.pid);
            if !seen.insert(key) {
                assert_eq!(
                    e.protocol,
                    Protocol::Udp,
                    "only UDP may repeat an address and port: {e:?}"
                );
            }
        }
    }

    #[test]
    fn a_socket_this_test_opens_is_discovered_on_its_own_port() {
        // Port 0 asks the OS to choose, so this asserts discovery without
        // hardcoding a port number that may already be taken.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback listener");
        let bound = listener.local_addr().expect("read the bound address");

        let endpoints = enumerate().expect("enumerate");
        let found = endpoints
            .iter()
            .find(|e| e.port == bound.port() && e.address == bound.ip())
            .unwrap_or_else(|| panic!("the listener on {bound} was not discovered"));

        assert_eq!(found.protocol, Protocol::Tcp);
        assert_eq!(
            found.pid,
            std::process::id(),
            "the socket must be attributed to the process that opened it"
        );

        drop(listener);
        let after = enumerate().expect("enumerate after close");
        assert!(
            !after
                .iter()
                .any(|e| e.port == bound.port() && e.address == bound.ip()),
            "a closed listener must stop being reported"
        );
    }

    #[test]
    fn an_ipv6_listener_is_discovered_and_stays_distinct_from_ipv4() {
        // The bug this guards is the one docs/BACKEND.md names: a single table
        // call, so half a dual-stack server goes missing.
        let v6 = TcpListener::bind("[::1]:0").expect("bind a v6 loopback listener");
        let v6_addr = v6.local_addr().expect("local_addr");

        let endpoints = enumerate().expect("enumerate");
        let found = endpoints
            .iter()
            .find(|e| e.port == v6_addr.port() && e.address == v6_addr.ip())
            .unwrap_or_else(|| panic!("the IPv6 listener on {v6_addr} was not discovered"));

        assert!(found.address.is_ipv6());
        assert_eq!(found.pid, std::process::id());
    }
}
