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

The shape it actually took, as of the dashboard milestone:

```
src-tauri/src/
├── main.rs
├── lib.rs                     wiring only: builder, state, handlers, shutdown
├── commands.rs                the five IPC handlers — thin, no logic
├── sampler.rs                 cadence, state, orchestration
├── logic/                     pure, syscall-free, unit-tested
│   ├── classify.rs            registry + observable data -> one verdict
│   ├── cpu.rs                 delta maths
│   ├── identity.rs            parsing `{pid}-{startedAt}`
│   ├── ports.rs               address presentation, PID attribution
│   ├── process.rs             raw processes -> ProcessRow
│   ├── registry.rs            the Developer Registry — the only file that names a program
│   ├── service.rs             the Process + Endpoint[] join
│   ├── telemetry.rs           machine CPU deltas, memory arithmetic
│   └── url.rs                 the open_external allowlist
├── platform/windows/          every `use windows::…`, behind #[cfg]
│   ├── control.rs             detail, terminate, ShellExecuteW, command lines
│   ├── gpu.rs                 PDH engine and memory counters, DXGI identity
│   ├── network.rs             GetIfTable2
│   ├── pdh.rs                 the shared PDH query wrapper
│   ├── ports.rs               the four extended-table calls
│   ├── process.rs             Toolhelp + OpenProcess
│   ├── storage.rs             IOCTL_DISK_PERFORMANCE per physical drive
│   ├── system.rs              GetSystemTimes, per-core, GlobalMemoryStatusEx
│   └── thermal.rs             PDH ACPI thermal zones
├── models.rs                  serde types shared with TypeScript
└── errors.rs
```

`models` and `errors` stayed single files rather than becoming directories,
because nothing needed splitting. `commands/` likewise: the five handlers are
forty lines together. The rule that modules appear when something fills them
applies to this document too.

Two rules that matter more than the layout:

1. **`platform/windows/` is the only place Win32 is called.** Declare the crate
   as `[target.'cfg(windows)'.dependencies]` from the first commit. The retrofit
   cost once `windows::` imports are scattered across a dozen files is a
   miserable afternoon; the cost today is one line.
2. **`logic/` never calls a syscall.** Endpoint grouping, CPU deltas, the
   service predicate, dual-stack detection, developer classification, telemetry
   arithmetic, conflict detection — all plain functions over plain data. This is
   where the bugs are and where the tests go.
3. **`registry.rs` is the only file that names a program.** Every executable
   name and command-line token in the codebase lives there, versioned. Scattering
   name checks through the UI or the screens is how four screens start
   disagreeing about what a developer service is.

---

## Windows API surface

### V1 — process discovery

| Need | API | Status | Notes |
|---|---|---|---|
| Enumerate | `CreateToolhelp32Snapshot` + `Process32First/Next` | **IMPLEMENTED** | Simplest. `EnumProcesses` is an alternative; `NtQuerySystemInformation` is faster but undocumented. |
| Open | `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` | **IMPLEMENTED** | Least privilege that works. Fails with `ERROR_ACCESS_DENIED` on other accounts — that is a `denied` field, not an error. Measured: 215 of 370 processes openable unelevated, and every one of them owned by the current user, which is why this doubles as the ownership predicate. |
| Times | `GetProcessTimes` | **IMPLEMENTED** | Creation, exit, kernel, user as FILETIME. Source of **both** uptime and the identity discriminator, and of the cumulative CPU time the sampler differences. |
| Memory | `GetProcessMemoryInfo` | **IMPLEMENTED** | `WorkingSetSize` is what Task Manager shows. |
| Image path | `QueryFullProcessImageNameW` | **IMPLEMENTED** | Tier 2, on panel open. |
| Threads | from the Toolhelp snapshot | **IMPLEMENTED** | Free — already enumerated. |

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
open.

**Amended for the Developer classifier — IMPLEMENTED.** The classifier cannot
work without command lines: a general-purpose runtime's name proves nothing
about what it is running, and the required reasons ("launched with the Vite
signature") cannot exist without the line that carries the token. So a bounded
read *is* now in the tick, and the bounds are what keep the original rule's
intent:

| Bound | Effect |
|---|---|
| Services only | ~28 candidates, not ~400 |
| Only general-purpose runtimes | 1 of 409 processes on the measured machine — a dedicated program is decided by its name, an excluded one is already refused |
| Cached by process identity | Read once per process lifetime, not once per tick; failures cached too |
| Pruned each tick | The cache is bounded by what is running, not by uptime |
| Through `open_verified` | A recycled PID yields nothing, never another process's command line — and so never another process's classification |

**Decided.** Command line uses `ProcessCommandLineInformation` (class 60) —
**IMPLEMENTED**. It needs no `PROCESS_VM_READ`, no `ReadProcessMemory` and is
not bitness-fragile, which is what ruled out the PEB walk. The size is queried
first rather than guessed, so a long command line is never silently truncated.

**Working directory is DEFERRED.** There is no route to it but the PEB walk this
table already rates as fragile, and it needs a wider handle than anything else
in the app. It renders as `FieldState::Unavailable` — an honest absence rather
than a blank that reads like an answer. Revisit when V2 project detection
forces the issue, exactly as this section originally advised.

### V1 — system telemetry

| Need | API | Status |
|---|---|---|
| Machine CPU | `GetSystemTimes` | **IMPLEMENTED** |
| Per logical processor | `NtQuerySystemInformation(SystemProcessorPerformanceInformation)`, class 8 | **IMPLEMENTED** |
| Physical memory | `GlobalMemoryStatusEx` | **IMPLEMENTED** |

All three are counters or levels the kernel already maintains: no handle, no
privilege, one call each. The class-8 struct is declared locally because the
`windows` crate does not expose it; only the first three fields are read.

The trap worth naming: **kernel time already includes idle time.** The busy
share is `(kernel + user − idle) / (kernel + user)`. Subtracting idle from the
denominator as well produces numbers that look plausible and are wrong, so
`logic::telemetry` has a test for exactly that.

Every reading is independently optional. A failed CPU query must not blank the
memory figures, and neither may take a tick down — telemetry is decoration on a
process dashboard. ROADMAP.md records what is deferred and why; none of it is
fabricated to fill a slot.

### V1 — network, storage, GPU and thermal

All four were chosen after measuring the candidates **unelevated on a real
machine**, not from documentation alone.

| Need | API / provider | Elevation | Measured cost | Status |
|---|---|---|---|---|
| Network throughput | `GetIfTable2`, `MIB_IF_ROW2.InOctets` / `OutOctets` | none | 0.90 ms, 50 interfaces | **IMPLEMENTED** |
| Storage throughput and active time | `DeviceIoControl(IOCTL_DISK_PERFORMANCE)` on `\\.\PhysicalDriveN` | none | 0.11 ms per drive, 0.05 ms to sweep 0–15 | **IMPLEMENTED** |
| GPU utilisation | PDH `\GPU Engine(*)\Utilization Percentage` | none | 0.50 ms, 599 instances | **IMPLEMENTED** |
| GPU memory | PDH `\GPU Adapter Memory(*)\Dedicated Usage` / `Shared Usage` | none | 0.008 ms | **IMPLEMENTED** |
| GPU identity | DXGI `EnumAdapters1` → `DXGI_ADAPTER_DESC1` | none | once, cached | **IMPLEMENTED** |
| ACPI thermal zones | PDH `\Thermal Zone Information(*)\Temperature` | none | 0.09 ms, 3 zones | **IMPLEMENTED** |

**Rejected, with the reason each was rejected:**

| Candidate | Why not |
|---|---|
| WMI `MSAcpi_ThermalZoneTemperature` | **Measured returning access denied unelevated.** The obvious route to a temperature, and it needs administrator rights |
| `D3DKMTQueryStatistics` | Undocumented gdi32 internals |
| NVML, AMD ADL | Vendor SDKs, and the development machine has one adapter of each vendor |
| `IDXGIAdapter3::QueryVideoMemoryInfo` | Reports the *calling process's* memory budget, so it would show LocalDocks' own usage as the GPU's |
| `GetProcessIoCounters` for disk | Counts file, network and device I/O in one number |
| ETW `Microsoft-Windows-Kernel-Network` | Starting a session needs elevation |
| PDH `\PhysicalDisk` counters | Would work, but returns rates already differenced rather than the cumulative counters the delta model uses, costs a held-open query, and carries the counter-name localisation problem |

**Four things that are easy to get wrong, each with a test:**

1. **`PdhAddEnglishCounterW`, not `PdhAddCounterW`.** Counter names are
   localised. A hard-coded `\GPU Engine(*)\Utilization Percentage` fails on a
   German or Japanese Windows through the localised call.
2. **A zero-access handle opens a physical drive.** `CreateFileW` with a desired
   access of `0` can issue `IOCTL_DISK_PERFORMANCE` but cannot read a byte of
   the device, which is exactly why an unelevated user is permitted to open it.
   Asking for `GENERIC_READ` would demand administrator rights.
3. **Disk active time comes from idle time.** `ReadTime + WriteTime` exceeds the
   elapsed time on any device that services requests concurrently — every NVMe
   drive made this decade — which is why Windows' own `% Disk Time` reports
   several hundred percent. `DISK_PERFORMANCE`'s time fields are in 100 ns
   units, which the reference page does not state, so a test asserts it against
   a measured interval rather than assuming it.
4. **GPU engines are summed within a type and maximised across types.** 3D,
   Copy and Video Decode are separate hardware queues that run concurrently, so
   adding them reports well over 100% for a machine doing one thing.

### V1 — actions

| Need | API | Status |
|---|---|---|
| Verify before acting | `OpenProcess` → `GetProcessTimes` → compare creation time | **IMPLEMENTED** |
| Terminate | `TerminateProcess` with `PROCESS_TERMINATE` | **IMPLEMENTED** |
| Open a URL | `ShellExecuteW`, after an `http`/`https` allowlist check | **IMPLEMENTED** |

`open_external` adds no dependency: the URL is validated in `logic::url` — a
pure function with its own tests — and handed to the OS's own "open this the
way the user would" verb. No shell is invoked and no argument string is built.
An allowlist rather than a blocklist, because any installed application can
register a new scheme at any time.

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
1. feature/project-setup      Tauri foundation, IPC round-trip          DONE
2. feature/process-discovery  Win32 enumeration, plain command          DONE
3. feature/sampler            Managed state, CPU deltas, events         DONE
4. feature/port-discovery     GetExtendedTcpTable, v4 + v6, and UDP     DONE
5. feature/service-model      Join by PID, the service predicate        DONE
6. feature/dashboard          Wire the existing UI to real data         DONE
7. feature/process-actions    Verified terminate, open, copy
```

Step 3 must not be deferred past step 4: CPU percentage is impossible without
it, and retrofitting the sampler after the port code exists means rewriting
both.
