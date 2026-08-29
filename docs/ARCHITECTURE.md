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

**Amendment (Developer classification).** The classifier needs command lines,
because a general-purpose runtime's name proves nothing about what it is
running. So the sampler now reads command lines inside the tick — but under
three bounds that keep the original rule's intent:

- **Only services**, never all ~400 processes.
- **Only services the answer could change**: a general-purpose runtime. A
  dedicated program is already decided by its name and an excluded one is
  already refused, so neither costs a handle. On the machine this was measured
  against, that was **1 process out of 409**.
- **Once per process lifetime**, cached against the process identity and
  pruned to what is still running. A service up for an hour costs one handle,
  not 3,600. A read that failed is cached as a failure rather than retried.

The read goes through the same `open_verified` path as every other command, so
a recycled PID yields nothing rather than another process's command line — and
therefore never another process's classification.

---

### 4b · Presentation mode is not collection

**IMPLEMENTED.** LocalDocks has one global switch: Developer or System.

The distinction is strictly about what is *shown*. The sampler always collects
everything it can see — every process the user owns, every socket it can
attribute — and the mode narrows that for presentation in exactly one function
(`src/lib/view.ts`), which all four screens consume.

Developer mode is **one coherent subgraph**, derived from a single decision:

1. Take the services classified `developer` (decision 4c).
2. Show exactly those services.
3. Show exactly the processes that own them.
4. Show exactly the sockets those processes hold.

Steps 2–4 read off step 1, so the screens cannot disagree: a service is never
shown without its process, and a port is never shown without the service that
owns it.

Two things it is deliberately not:

- **Not ancestry.** An earlier version treated every Service as relevant and
  spread one hop through the process tree. Both halves were wrong, and § 4c
  explains why.
- **Not localhost-only.** Addresses are never consulted, so `0.0.0.0:8000` is
  exactly as eligible as `127.0.0.1:8000`. Filtering by address would hide the
  bindings a developer most needs to notice.

Because mode is presentation, switching costs nothing, changes no syscall, and
can never make the app miss something. System mode is the raw view and hides
nothing the backend can observe.

---

### 4c · Developer relevance is a registry, not a heuristic

**IMPLEMENTED.** `src-tauri/src/logic/registry.rs` is the single, versioned
place LocalDocks decides what "developer" means;
`src-tauri/src/logic/classify.rs` is the single place that applies it.

#### Why a registry, when decision 1 rejects allowlists

Decision 1 still holds: a *Service* is an observation, and nothing in the
service model looks at a name. But **relevance is a different question**, and it
has no syscall. "Is this service part of my development work?" cannot be
observed — only decided. The honest options are a registry or a guess, and a
guess that fails cannot be explained, argued with, or corrected.

#### What was wrong before

Developer mode previously treated every Service as developer-relevant and then
spread outward one hop through the process tree.

- **Every Service is not developer work.** A Service is a process the user owns
  holding a listening socket on a non-system port. On a real workstation that
  set was **28 services**, of which Chrome, Brave, Spotify, Steam, the Epic
  launcher, iCloud, the Apple device services, the NVIDIA helpers, WhatsApp,
  the Claude desktop app and five VS Code sockets accounted for 26. Developer
  mode showed all of them, which made it indistinguishable from System mode.
- **Ancestry is not evidence.** One hop from a service reaches whatever else its
  parent started. From a terminal or an editor, that is everything running under
  it. Being spawned by the same thing as a dev server does not make a process
  part of the development picture.

#### The rules

First match wins. There is no score, no weight and no threshold; the order *is*
the algorithm:

| # | Condition | Verdict |
|---|---|---|
| 1 | Executable is in the exclusion table | **System** |
| 2 | Executable is a registered *dedicated* development program | **Developer** |
| 3 | Executable is a registered *runtime* **and** its command line matches a registered signature | **Developer** |
| 4 | Executable is a registered runtime with no matching signature, or no readable command line | **Unknown** |
| 5 | Anything else | **Unknown** |

Every branch produces a sentence naming the entry that fired, carried on the
service as `relevanceReason` and shown in the detail panel. A verdict the user
cannot check is one they cannot correct.

#### The two-table split

The split between **dedicated programs** and **runtimes** is what stops "it is a
Node process" from meaning "it is a development service".

- **Dedicated** — running the binary *is* the development activity: `mongod`,
  `postgres`, `redis-server`, `dockerd`, `nginx`, `adb`. The name alone is
  sufficient.
- **Runtime** — general-purpose: `node`, `python`, `java`, `dotnet`. Consumer
  applications ship these too, so the name alone proves nothing. These require a
  command-line signature and, without one, produce `Unknown` rather than
  `Developer`.

#### Signatures are whole tokens, never substrings

This is the part that has to be right, and the failure is not hypothetical.
Both of these are real command lines observed on a development machine:

```text
--utility-sub-type=node.mojom.NodeService     (VS Code)
--inspect-port=0                              (VS Code)
```

A substring search for `node` matches the first; one for `inspect` matches the
second. The tokenizer splits on whitespace, quotes, both path separators and
argument punctuation, strips leading dashes and a closed list of script
extensions, and compares whole tokens. It deliberately does **not** split on
`.`, because that would turn `node.mojom.NodeService` back into a bare `node` —
the exact bug it exists to prevent.

#### What is never consulted

- **The port.** Not the number, not a range, not a "known ports" table. There is
  no `3000–9000 is developer` rule, and a test asserts no registry entry
  anywhere parses as a number. A service on 61123 is not disqualified and one on
  3000 is not promoted.
- **The address.** See § 4b.
- **Ancestry.** No parent, no child, no tree walk.
- **Resource use.** CPU and memory say nothing about relevance.

#### The exclusion table is a guard, not the mechanism

Relevance defaults to `Unknown`, and `Unknown` is already hidden. Deleting the
whole exclusion table would not put Chrome into Developer mode — it would only
move Chrome from "System" to "Unrecognised" in the report. It exists so that
(a) the report can make the stronger, checkable claim, and (b) nothing in it can
ever be promoted by a future signature. It is kept small on purpose.

#### Editors are in neither table

VS Code, Cursor, the JetBrains suite and Visual Studio hold real listening
sockets. They are neither development services nor system infrastructure, so
they are in neither table and classify as `Unknown` — hidden from Developer
mode without asserting something false about them.

The cost is real and documented: a VS Code port-forward or Live Preview socket
is indistinguishable from the editor's internal IPC without data LocalDocks does
not have, so it is hidden in Developer mode. It remains one click away in System
mode. A dev server started *from* the editor's terminal is a separate `node.exe`
process and is unaffected.

#### The registry does not claim to be exhaustive

`Unknown` is the default and a real answer. `REGISTRY_VERSION` ships in every
snapshot so a classification someone disagrees with can be pinned to a specific
version of the tables rather than to "the app".

---

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

### Implementation status

The decisions above are all **IMPLEMENTED** as described, with one exception
noted in decision 4: the tier-2 *working directory* field is **DEFERRED**, and
renders as `unavailable` rather than being guessed. See docs/BACKEND.md.

---

### Future architecture — documented, not built

Two directions are recorded here so that today's decisions do not foreclose
them. Neither is implemented, and neither may be started before V2 ships.

**Telemetry overlay — PLANNED (V2).** An always-on-top compact readout.

```
Snapshot
├── Main UI
├── Overlay
└── Notifications
```

The constraint that matters: the overlay is a second *consumer* of the existing
sampler state, never a second sampler. It subscribes to the same
`services:update` and holds no timer of its own. An overlay that polls
independently would double the scan cost and could disagree with the main
window about what is running — two clocks, two truths.

**Local intelligence — PLANNED (V3).** A small local model over the structured
data LocalDocks already has:

```
small local model  +  structured context  +  controlled tools  +  selective retrieval
```

Four constraints, fixed now:

- **Local only.** No cloud call, no account, no telemetry. If it cannot run on
  the machine, it does not ship.
- **Structured context, not scraping.** It reads Snapshots and Events — the
  same typed data the UI reads — rather than parsing screens.
- **Controlled tools.** A named, audited set of read-only operations. Never a
  shell, never arbitrary command execution, never elevation.
- **It explains; it does not decide.** Conflict detection, project detection and
  diagnostics stay rule-based and deterministic. The model is a reader.

---

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
  get_snapshot()                       -> Snapshot          IMPLEMENTED
  get_process_detail(processId)        -> ProcessDetail     IMPLEMENTED
  terminate_process(pid, startedAt)    -> TerminateResult   IMPLEMENTED
  set_sample_interval(intervalMs)      -> ()                IMPLEMENTED
  open_external(url)                   -> ()                IMPLEMENTED

events
  services:update  -> Snapshot                              IMPLEMENTED
  services:error   -> string                                IMPLEMENTED
```

The V1 IPC surface is complete. `get_snapshot` is a cached state read and never
triggers a scan; `get_process_detail` is the only tier-2 path and is never
called from the sampler; `terminate_process` and `get_process_detail` both
re-verify `pid + startedAt` against a fresh creation-time reading before they
touch anything; `open_external` validates against an `http`/`https` allowlist
before the OS sees the string.

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
