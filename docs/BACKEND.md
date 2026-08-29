# LocalDocks — Backend

> Status: **not implemented.** This document is the plan the Rust core is
> written against, and the reference for what each capability actually costs.
> Last updated: 2026-08-28

---

## Domain entities

Every entity, who owns it, and when it comes into existence. **Nothing here
should be created before something needs it.**

| Entity | Owned by | Version | What it is |
|---|---|---|---|
| **Process** | `processes` | V1 | A Windows process the user owns. Identity is `pid + createdAt`. |
| **Endpoint** | `ports` | V1 | One listening socket: protocol, address family, local address, port. |
| **Service** | `services` | V1 | A Process joined to the Endpoints it owns. |
| **Snapshot** | `sampler` | V1 | One tick: services, processes, ports, conflicts, `capturedAt`. |
| **ProcessDetail** | `processes` | V1 | Tier-2 fields, fetched on demand. |
| **Resource** | `resources` | V2 | A time series per process, bounded. |
| **Project** | `projects` | V2 | Several Services sharing a working directory or repository. |
| **ServiceInstance** | `services` | V2 | A Service plus enough runtime to reproduce it. |
| **Event** | `events` | V2 | A state change derived from snapshot diffs. |
| **Log** | `logs` | V2.2 | A bounded stream from a process LocalDocks started. |
| **Provider** | `providers` | V3 | A non-Windows source of services: Docker, WSL, infrastructure. |

### The relationship model

This is the long-term shape everything is designed toward:

```
PROCESS ──owns──► ENDPOINT
   │
   └──is a──► SERVICE ──belongs to──► PROJECT
                                         │
                          ┌──────────────┼──────────────┐
                          │              │              │
                   Windows service   Docker        WSL / infra
                                    container       service
```

V3 providers join at the **Service** level, not the Process level. A Docker
container is not a Windows process, but it *is* a service belonging to a
project. Designing the join there is what stops V3 from requiring a rewrite.

---

## Subsystem map

Conceptual. **Do not create these directories in advance.**

```
src-tauri/src/
├── main.rs
├── lib.rs
├── commands/     IPC surface only — thin, no logic
├── platform/
│   └── windows/  every `use windows::…` lives here, behind #[cfg]
├── logic/        pure, syscall-free, unit-tested
├── models/       serde types shared with TypeScript
└── errors/
```

Two rules that matter more than the layout:

1. **`platform/windows/` is the only place Win32 is called.** Declare the crate
   as `[target.'cfg(windows)'.dependencies]` from the first commit. The retrofit
   cost once `windows::` imports are scattered across a dozen files is a
   miserable afternoon; the cost today is one line.
2. **`logic/` never calls a syscall.** Endpoint grouping, CPU deltas, the
   service predicate, dual-stack detection, conflict detection — all plain
   functions over plain data. This is where the bugs are and where the tests go.

---

## Windows API surface

### V1 — process discovery

| Need | API | Notes |
|---|---|---|
| Enumerate | `CreateToolhelp32Snapshot` + `Process32First/Next` | Simplest. `EnumProcesses` is an alternative; `NtQuerySystemInformation` is faster but undocumented. |
| Open | `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` | Least privilege that works. Fails with `ERROR_ACCESS_DENIED` on other accounts — that is a `denied` field, not an error. |
| Times | `GetProcessTimes` | Returns creation, exit, kernel, user as FILETIME. Source of **both** uptime and the identity discriminator. |
| Memory | `GetProcessMemoryInfo` | `WorkingSetSize` is what Task Manager shows. |
| Image path | `QueryFullProcessImageNameW` | Cheaper than the tier-2 fields; still needs an open handle. |
| Threads | from the Toolhelp snapshot | Free — already enumerated. |

**CPU percentage is computed, not read:**

```
cpu% = Δ(kernel + user) / Δ(wall clock) / core_count
```

Requires the previous sample. This is the whole reason the sampler holds state.

### V1 — port discovery

| Need | API | Notes |
|---|---|---|
| TCP IPv4 | `GetExtendedTcpTable(TCP_TABLE_OWNER_PID_LISTENER, AF_INET)` | Gives port→PID directly. |
| TCP IPv6 | same, `AF_INET6` | **Separate call.** Forgetting this is how half a dev server goes missing. |
| UDP | `GetExtendedUdpTable` | V1 where practical; no connection state. |

Both calls follow the query-size-then-allocate pattern and can return
`ERROR_INSUFFICIENT_BUFFER` when the table grows between the two calls — retry,
do not panic.

### V1 — termination

| Need | API |
|---|---|
| Verify | `OpenProcess` → `GetProcessTimes` → compare creation time |
| Terminate | `TerminateProcess` |

Verification is not optional. See ARCHITECTURE.md § 3.

### Tier 2 — the awkward ones

Command line is the field V2 project detection depends on, and Windows makes it
genuinely unpleasant:

| Approach | Verdict |
|---|---|
| WMI `Win32_Process.CommandLine` | Works, documented — but COM, and ~100–500 ms for a full query |
| `NtQueryInformationProcess` + read the PEB | Fast, but undocumented internals and bitness-fragile |
| `NtQueryInformationProcess(ProcessCommandLineInformation)` | Win10 1511+, much saner, still semi-documented |

**Recommendation: none of these in the scan loop, ever.** Fetch on detail-panel
open. Decide between them when V2 forces the issue, not before.

---

## V2 capability costs

Honest estimates, so scope decisions are made with the price visible.

| Capability | Cost | The hard part |
|---|---|---|
| Conflict detection | **Low** | Pure logic over data V1 already has. `Snapshot.conflicts` is already `number \| null`, awaiting a real number. |
| Endpoint grouping / exposure | **Low** | Logic only. Distinguishing `127.0.0.1` from `0.0.0.0` is a rendering and modelling decision. |
| Port availability | **Low** | Attempt a bind, or diff against the table. |
| Resource history | **Medium** | A bounded ring buffer per process. Memory growth must be capped, and dead processes evicted. |
| Tray / background | **Medium** | Tauri lifecycle, not Win32. Idle footprint is the real constraint. |
| Events / notifications | **Medium** | Snapshot diffing. Design the event model once — V3 depends on it. |
| Project detection | **High** | Depends entirely on tier-2 data. Filesystem walking up from cwd, Git discovery, manifest parsing. No new Win32. |
| Service lifecycle | **High** | Reproducing a launch. Command line, cwd and environment. `CreateProcessW` with correct handle inheritance. |
| Logs | **High** | *Only possible for processes LocalDocks started.* Windows offers no supported way to attach to a running process's stdout. Blocked on lifecycle. |

### The environment problem

Capturing a process environment to enable restart is the sharpest security edge
in V2. A dev server's environment routinely contains database URLs, API keys and
tokens. Rules, decided now:

- Never log it
- Never persist it unencrypted
- Never display it in full without explicit user action
- Prefer re-deriving from a project definition over capturing from a live process

---

## Error taxonomy

System calls fail routinely and most failures are **normal**, not exceptional.
Modelling them as ordinary outcomes is what keeps the UI honest.

```rust
enum LocalDocksError {
    ProcessGone,            // exited between enumeration and query — routine
    AccessDenied,           // another account. Renders "Requires elevation"
    IdentityMismatch,       // PID recycled. A refusal, not a failure
    InvalidPid,
    ApiFailure { code: u32, call: &'static str },
    Unsupported,            // this Windows build lacks the API
}
```

Rules:

- **No `unwrap` / `expect` on any path reachable from a command.** A process
  exiting mid-scan is expected behaviour, not a panic.
- **`AccessDenied` is a value, not an error.** It becomes `FieldState::Denied`
  and renders as "Requires elevation".
- **`IdentityMismatch` is a success path** for the safety model working.
- API failures carry the call name — `GetExtendedTcpTable failed: 122` is
  debuggable; "scan failed" is not.

---

## Testing

| Layer | How |
|---|---|
| `logic/` | Rust unit tests. Pure functions, no mocking needed. The highest-value tests in the project. |
| `platform/windows/` | Integration tests against real processes; assert shape and invariants, not exact values. |
| `commands/` | Thin by design — if a command needs heavy testing, logic leaked into it. |
| Frontend pure logic | Vitest. 38 tests today over format, detail and settings. |

**Do not build a `ProcessSource` trait to enable mocking.** Separating pure
logic from syscalls already gives testability without the abstraction. The trait
becomes justified when a second platform is real — not before.

---

## Implementation order

```
1. feature/project-setup      Tauri foundation, IPC round-trip
2. feature/process-discovery  Win32 enumeration, plain command
3. feature/sampler            Managed state, CPU deltas, events
4. feature/port-discovery     GetExtendedTcpTable, v4 + v6
5. feature/service-model      Join by PID, the service predicate
6. feature/dashboard          Wire the existing UI to real data
7. feature/process-actions    Verified terminate, open, copy
```

Step 3 must not be deferred past step 4: CPU percentage is impossible without
it, and retrofitting the sampler after the port code exists means rewriting
both.
