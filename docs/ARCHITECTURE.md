# LocalDocks — Architecture

> Status: V1 frontend contract locked. Rust backend not started.
> Last updated: 2026-08-28

This document records **why** LocalDocks is shaped the way it is. The decisions
below were made deliberately and several of them exist to prevent a specific
bug or a specific rewrite. Changing one should be a conscious act, not a side
effect.

---

## The shape

```
┌──────────────────────────────────────────────┐
│  React + TypeScript                          │
│  presentation · interaction · filter · sort  │
└───────────────────┬──────────────────────────┘
                    │
              src/lib/ipc.ts          ← the only crossing
                    │
              Tauri IPC + capabilities
                    │
┌───────────────────┴──────────────────────────┐
│  Rust core                                   │
│                                              │
│   sampler ──── owns the cadence              │
│      │                                       │
│   State<Mutex<Snapshot>>                     │
│      │                                       │
│   ┌──┴──┐                                    │
│   join by PID  ← pure logic, unit-tested     │
│   └──┬──┘                                    │
│      │                                       │
│  processes   ports                           │
└──────┬─────────┬─────────────────────────────┘
       │         │
   Win32 process   Win32 networking
```

**The frontend performs no system operations.** It cannot. It asks, and Rust
decides. That is enforced by Tauri capabilities, not by convention.

---

## Architectural decisions

### 1 · Service is the primary domain object

A *Service* is a process holding at least one listening socket on a non-system
port, owned by the current user.

This definition is observable rather than heuristic. The alternative — an
allowlist of process names — is permanently wrong in both directions: it misses
new runtimes and it floods the list with Electron apps that happen to look like
`node`.

It also happens to be the **privilege boundary**. Inspecting your own processes
needs no elevation on Windows *or* macOS, which is why decision 7 costs nothing.

Processes and Ports remain as secondary, diagnostic views.

### 2 · Rust owns the scan cadence

The frontend does not poll. It subscribes.

The argument that settles it: **CPU percentage cannot be computed statelessly.**
Windows exposes cumulative kernel and user time via `GetProcessTimes`, so a
percentage requires remembering the previous sample. There is a stateful sampler
in this design whether or not it is built deliberately.

Three consequences follow for free:

- No overlapping scans when a scan runs longer than the interval
- A React render can never trigger a syscall
- Push-based notifications (V2) have somewhere to come from

`set_sample_interval` lets the UI *choose* the cadence. It never owns it.

### 3 · Process identity is `pid + creation time`

A bare PID is not an identity. Windows recycles PIDs, so:

```
T=0  UI renders   node.exe  PID 8420
T=1  that process exits
T=2  Windows reassigns 8420 to something else
T=3  user clicks Kill  →  wrong process dies
```

Every process-bearing row carries `id = ${pid}-${startedAt}`. Every destructive
command takes both fields, and the backend re-opens the PID, reads its creation
time, and **refuses on mismatch**. This is ~15 lines and it is the difference
between a tool people trust with kill rights and one they do not.

### 4 · Two-tier data

| Tier | Fields | When |
|---|---|---|
| 1 | PID, name, memory, CPU ticks, start time, threads, ports | every sampler tick |
| 2 | command line, working directory, executable path, parent | detail panel open only |

Command-line retrieval on Windows is expensive and awkward (BACKEND.md § Tier 2).
It must never sit inside the scan loop. This split is also what makes V2 project
detection affordable — it reuses tier-2 data rather than adding scan cost.

### 5 · Endpoints are plural, and identity is not the port

A dev server routinely binds `127.0.0.1:5173`, `[::1]:5173` and sometimes
`0.0.0.0:5173`. Windows requires separate IPv4 and IPv6 table calls.

These are **three endpoints of one service**, and endpoint identity is
`(protocol, address family, local address, local port)` — never the port alone.
Naive port-key deduplication produces one dev server appearing three times, or
two genuinely different services being merged.

The Ports view deliberately shows them unmerged; Services groups them.

### 6 · Force terminate only, honestly labelled

Windows has no universal SIGTERM equivalent. Graceful shutdown means
`GenerateConsoleCtrlEvent` for console apps or `WM_CLOSE` for windowed ones —
conditional, and not guaranteed.

V1 ships force terminate, says so in the confirmation dialog, and does not
pretend otherwise. Graceful stop gets its own design pass in V2.

### 7 · Never elevate

LocalDocks runs as a normal user application. Target processes — `node`,
`python`, `postgres` — run as the user, so the common case needs no privileges.

When a field cannot be read, it is modelled as `denied` and rendered as
"Requires elevation", never as a blank. The type system enforces this:

```ts
type FieldState<T> = { kind: 'ok'; value: T } | { kind: 'denied' } | { kind: 'unavailable' }
```

`SeDebugPrivilege` is deliberately not used. An open-source system tool that
enables debug privilege is one people read very suspiciously.

### 8 · One IPC seam

`src/lib/ipc.ts` is the only file that imports `@tauri-apps/api`. Two payoffs:
the entire UI runs in a plain browser against a mock with no Rust toolchain, and
when the backend lands only that file changes.

---

## Evolution

```
V1 ─────────────────────► V2 ─────────────────────► V3
                                          
processes                 + lifecycle              + providers
ports                     + projects                 (docker, wsl,
join by PID               + history                   infra)
sampler                   + events                  + workspaces
                          + logs (2.2)              + platform adapters
```

The V1 core does not get replaced by V2. Every V2 subsystem hangs off the
sampler and the join that V1 establishes. That is the point of getting V1's
model right before writing V2's features.

### Growth rules

- **Modules appear when they are needed**, not because the diagram has a box.
  Do not create `projects/`, `logs/` or `events/` before something fills them.
- **Pure logic stays separate from syscalls.** CPU delta maths, endpoint
  grouping, dual-stack detection and the service predicate are plain functions
  over plain data. That is where the bugs live and where the tests go.
- **Domain types and IPC types are allowed to diverge.** Keeping them separate
  means internal restructuring does not break the TypeScript contract.
- **No cross-platform abstraction until a second platform is real.** A
  `ProcessSource` trait built today would be a guess. Built alongside macOS, it
  would be a fact.

---

## IPC contract

### Implemented by the frontend, awaited from Rust

```
commands
  get_snapshot()                       -> Snapshot
  get_process_detail(processId)        -> ProcessDetail
  terminate_process(pid, startedAt)    -> TerminateResult
  set_sample_interval(intervalMs)      -> ()
  open_external(url)                   -> ()

events
  services:update  -> Snapshot
  services:error   -> string
```

`TerminateResult` is a discriminated union — `terminated | stale | denied |
failed` — because "the PID was recycled" is a normal outcome the UI must render
differently from a failure.

### Planned — V2

```
commands
  start_service(definition)            -> ServiceInstance
  stop_service(processId, graceful)    -> StopResult
  restart_service(processId)           -> ServiceInstance
  get_resource_history(processId)      -> ResourceSeries
  get_project(projectId)               -> Project
  list_projects()                      -> Project[]
  open_path(path, target)              -> ()      // terminal | explorer | editor

events
  service:started    -> ServiceInstance
  service:stopped    -> { processId, exitCode }
  port:conflict      -> { port, owners[] }
  port:available     -> { port }
```

### Planned — V3

```
commands
  list_providers()                     -> Provider[]
  provider_action(providerId, action)  -> ActionResult
  start_workspace(workspaceId)         -> WorkspaceRun
```

**Contract discipline:** the frontend was locked before the backend was written
so the IPC shape would stop changing while Rust is being learned. Changing a
type here should be a deliberate, discussed act.

---

## Security posture

| Principle | How it is enforced |
|---|---|
| Least privilege | No elevation, ever. Denials are modelled and displayed. |
| No arbitrary execution | Every action is a named command with typed arguments. There is no "run this string" path, and there must never be one. |
| Verified destruction | `pid + startedAt` re-checked before terminate. |
| Minimal capabilities | Tauri `capabilities/` grants only what is used. Nothing by default. |
| Minimal surface | No network access, no filesystem access beyond what a named command needs. |
| Local only | No cloud dependency for any core function. |

**The V2 boundary to watch:** developer actions (open terminal, open editor,
start service) move LocalDocks from observing processes to launching them. Each
one needs an explicit, narrow implementation. Environment capture for restart is
the sharpest edge — a dev server's environment routinely contains secrets, and
they must not be logged, persisted, or displayed casually.
