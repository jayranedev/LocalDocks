# LocalDocks — Roadmap

> Status: **V1 frontend contract locked.** Rust backend not started.
> Last updated: 2026-08-28

---

## Status legend

Every item in this document carries one of these. They describe the repository
as it actually is, not as it is hoped to be.

| Marker | Meaning |
|---|---|
| **IMPLEMENTED** | Built, tested, and running in the app today |
| **IN PROGRESS** | Partially built; the gap is named where it appears |
| **PLANNED** | Agreed for a named version; not started |
| **DEFERRED** | Wanted, but blocked on something named — not silently dropped |
| **NON-GOAL** | Deliberately not built, at any version |

---

## What LocalDocks is

A minimal desktop control plane for a local development environment.

The question it answers is not "which ports are open" — plenty of tools answer
that. It is:

> **What is my project actually running right now, and can I control it from one
> place?**

The distinction matters, because it decides what gets built. A port utility
treats a socket as the subject. LocalDocks treats the **service** as the
subject: a process, the endpoints it owns, the project it belongs to, the
resources it consumes, and the lifecycle actions available on it.

Everything on this roadmap is judged against that. A feature that makes
LocalDocks a better port list is not automatically a feature worth building. A
feature that deepens service, project or lifecycle understanding usually is.

### Positioning

LocalDocks earns its place through **context and control**, not through feature
count:

| Layer | What it means |
|---|---|
| **Service** | A process and its endpoints understood as one thing |
| **Project** | Several services understood as one development environment |
| **Lifecycle** | Start, stop, restart — not just observe and kill |
| **Safety** | Verified identity, no elevation, honest about what actions do |

Basic port discovery and process termination are table stakes. They are V1
because you cannot build the rest without them, not because they are the
differentiator.

---

## V1 — See and basic control

**Status: scope FROZEN.** No further V1 feature additions.

The goal is a correct foundation, not a complete product.

### In scope

| Area | Detail | Status |
|---|---|---|
| **Process discovery** | Toolhelp enumeration; `OpenProcess` for creation time, CPU time and working set | **IMPLEMENTED** |
| **Sampler** | Rust owns the cadence; one thread, no overlapping scans, `services:update` push | **IMPLEMENTED** |
| **Resources** | CPU % from cumulative-time deltas, real working-set memory, threads, uptime | **IMPLEMENTED** |
| **Port discovery — TCP IPv4** | `GetExtendedTcpTable`, `TCP_TABLE_OWNER_PID_LISTENER` | **IMPLEMENTED** |
| **Port discovery — TCP IPv6** | The separate `AF_INET6` call, with link-local scope preserved | **IMPLEMENTED** |
| **Port discovery — UDP** | `GetExtendedUdpTable`, both families | **IMPLEMENTED** |
| **Services** | Process joined to its endpoints; observable predicate, no name allowlist | **IMPLEMENTED** |
| **Identity** | `${pid}-${startedAt}` on every process-bearing row, verified before every action | **IMPLEMENTED** |
| **Process details** | Tier 2 on panel open: executable and command line | **IMPLEMENTED** |
| **Process details — working directory** | Needs a PEB walk with `PROCESS_VM_READ`; renders `unavailable` | **DEFERRED** |
| **Safe termination** | Force terminate, refused on identity mismatch | **IMPLEMENTED** |
| **Open external** | `http`/`https` only, validated before the OS sees it | **IMPLEMENTED** |
| **Developer / System mode** | One global switch; presentation only, never collection | **IMPLEMENTED** |
| **Themes** | Local Dark (default), Dark, Light — semantic tokens, AA verified | **IMPLEMENTED** |
| **State** | Live snapshot model, loading / empty / error, stale-snapshot on failure | **IMPLEMENTED** |
| **Settings** | Theme, refresh interval and mode, persisted | **IMPLEMENTED** |
| **System telemetry** | See the table below | **IN PROGRESS** |

### V1 system telemetry

V1 is intended to be a credible Windows developer dashboard, not only a service
list. That widens V1's telemetry beyond per-process resources. Nothing here may
be displayed as a fabricated value: a metric Windows does not expose reliably
renders as an explicit unavailable state, never as a plausible zero.

| Metric | Status | Note |
|---|---|---|
| Per-process CPU % | **IMPLEMENTED** | Δ(kernel+user) / Δwall / logical cores |
| Per-process memory | **IMPLEMENTED** | Working set, via `GetProcessMemoryInfo` |
| Per-process threads, uptime | **IMPLEMENTED** | From the Toolhelp snapshot and creation time |
| Aggregate CPU/memory across services | **IMPLEMENTED** | Summed in the Overview from real per-process values |
| Total system CPU | **IMPLEMENTED** | `GetSystemTimes`; same delta shape as per-process. `null` on the first tick — a rate needs two samples, and a lifetime-since-boot average presented beside a live readout would be believed |
| Per logical processor | **IMPLEMENTED** | `NtQuerySystemInformation(SystemProcessorPerformanceInformation)`, one bar per core in the Overview. `null` if the processor count changes between ticks rather than mispairing cores |
| System memory | **IMPLEMENTED** | `GlobalMemoryStatusEx`. Used = total − *available*, which counts the reclaimable cache, because that is what Windows itself reports as usable |
| Network activity | **DEFERRED** | `GetIfTable2` gives per-adapter byte counters, so a machine-wide rate is reachable. Per-*process* attribution — the number worth showing beside a service — needs an ETW session (`Microsoft-Windows-Kernel-Network`), and starting one needs elevation. A machine-wide rate on a per-service dashboard invites being read as the service's |
| Storage activity | **DEFERRED** | `GetProcessIoCounters` is cheap and per-process, but counts file, network *and* device I/O together, so it cannot answer "how much is this touching the disk". Reporting it as disk activity would be wrong in a way the user could not detect |
| GPU metrics | **DEFERRED** | Reachable only through vendor SDKs (NVML, ADL) or the performance-counter path Task Manager uses. Neither is a Win32 call; shipping an NVIDIA dependency to read one number is not the V1 trade |
| Thermal | **DEFERRED** | `MSAcpi_ThermalZoneTemperature` is unimplemented on most consumer hardware and returns a fixed value where it exists. A field that is wrong on most machines is worse than no field |

The four deferred rows are deferred on evidence, not appetite. Each is
revisited when there is a way to report it that is honest at a glance. All four
are present in the IPC contract as `null`, not absent from it — the day one
becomes measurable, nothing in the frontend changes shape.

### V1 Developer classification

**IMPLEMENTED.** Developer mode narrows Services, Processes *and* Ports from one
classification decision, made by a central versioned registry
(`src-tauri/src/logic/registry.rs`) and applied by a deterministic classifier
(`src-tauri/src/logic/classify.rs`). ARCHITECTURE.md § 4c is the full design.

Measured against the development machine it was built on — 409 processes, 130
listening sockets, 28 services:

| Verdict | Count | Examples |
|---|---|---|
| Developer | 2 | `node` running Vite; `adb` |
| System | 19 | Chrome, Brave, Spotify, Steam, Epic, iCloud ×4, Apple device services ×2, NVIDIA ×2, WhatsApp, Claude, Edge WebView2 |
| Unclassified | 7 | VS Code ×5, and two helpers not in either table |

Every one of the 28 carries a sentence naming the rule that classified it.

**Not release-complete.** The registry is not exhaustive and does not claim to
be — that is what the `unknown` verdict is for. Known gaps, all deliberate:

- **Editors are unclassified by design.** A VS Code port-forward or Live Preview
  socket is hidden in Developer mode. See ARCHITECTURE.md § 4c.
- **Compiled binaries are unreachable.** A Go or Rust dev server compiles to an
  executable with an arbitrary name and no signature-bearing command line, so it
  classifies `unknown`. `go run` is the same case — the listener is a temporary
  binary, not `go.exe`. There is no observable data that fixes this in V1;
  V2 project detection is what would.
- **Java is thin.** JVM dev servers are launched through wrappers whose command
  lines rarely carry a recognisable token.
- **The registry will need entries.** Adding one is a code change and a version
  bump, deliberately — it is a decision, not configuration.

### Explicitly not in V1

Docker · WSL · logs · project detection · start/stop/restart · resource history
· notifications · tray · conflict detection · overlay · local intelligence ·
plugins · cloud · accounts · cross-platform.

### Known V1 polish debt

Tracked, deliberately not blocking:

- Responsive layout below ~1100px
- Keyboard navigation within tables
- Retry affordance on the error state
- Command palette covers services only
- Working directory renders `unavailable` (see the table above)
- System telemetry rows marked **DEFERRED** above
- Developer Registry coverage gaps (see § V1 Developer classification)
- No in-app way to report or override a classification

---

## V2 — Local development control

**Everything in V2 is PLANNED.** Nothing below is built. V2 does not begin
until V1 is stable and released.

The release where LocalDocks stops being primarily a monitor.

**The central shift:** V1 knows *how a process is running*. V2 needs to know
*how to reproduce it*. That single sentence is the source of most of the work
below.

### 2.1 · Service lifecycle

Start · Stop · Restart · Force terminate.

Requires capturing enough to relaunch a service:

```
ServiceInstance
├── Identity     pid, createdAt
├── Runtime      executable, arguments, workingDirectory, environment
├── Endpoints[]
└── Resources
```

**Backend cost: high.** Reconstructing a command line on Windows is the hard
part (see BACKEND.md). Environment capture is security-sensitive and must be
handled deliberately — a dev server's environment routinely contains secrets.

### 2.2 · Project awareness

The largest differentiator. Turns this:

```
node.exe    python.exe    python.exe
```

into this:

```
my-project
  Frontend    React / Vite      localhost:5173
  API         FastAPI           localhost:8000
  Worker      Python            localhost:8001
```

Requires: working-directory discovery, process ancestry, argument inspection,
Git repository and branch discovery, manifest detection (`package.json`,
`pyproject.toml`, `Cargo.toml`, …), runtime and package-manager detection, and
the process→project→service associations.

**Backend cost: high**, but it builds on 2.1's tier-2 data rather than needing
new Windows APIs.

### 2.3 · Port intelligence

- **Conflict detection** — the same port claimed by more than one owner
- **Availability** — is port 3000 free right now?
- **Exposure** — `127.0.0.1:3000` and `0.0.0.0:3000` are materially different
  and must not render identically
- **Endpoint grouping** — one service owning `127.0.0.1:5173`, `[::1]:5173` and
  `0.0.0.0:5173` is one service, not three

**Backend cost: low-medium.** Mostly logic over data V1 already collects.
`Snapshot.conflicts` is already typed `number | null` and awaits a real number.

### 2.4 · Resource monitoring

From point-in-time readings to **history**: not "CPU is 6%" but "CPU has
averaged 4.8% over ten minutes." Adds threads, handle count, network and disk
I/O.

**Backend cost: medium.** Requires a bounded ring buffer in the sampler. The
sampler already holds previous samples for CPU deltas — this extends that.

### 2.5 · Developer actions

Open Browser · Copy URL · Open Project · Open Terminal · Reveal in Explorer ·
Copy Command.

**Backend cost: low, security cost: high.** This is where LocalDocks stops
observing and starts launching. Every one of these needs an explicit,
narrow, non-arbitrary command path — see ARCHITECTURE.md § Security.

### 2.6 · Tray and background mode

Background sampler, tray summary, close-vs-minimize semantics, and a defensible
idle resource footprint.

**Backend cost: medium.** Mostly Tauri application lifecycle.

### 2.7 · Notifications

"API stopped unexpectedly" · "Port 8000 became available" · "Backend restarted".

These are *derived from snapshot diffs*, not separately polled. The event model
should be designed once, in V2, so V3 does not force a rewrite.

### 2.8 · Service history and diagnostics

Bounded, local, in-memory-or-file event log: last started, last stopped, restart
count, exit codes. Enables "why did this service disappear?"

**Backend cost: medium.** Needs a bounded store and a retention policy.

### 2.9 · Overlay foundation — PLANNED

A compact always-on-top readout for while you are working in another window.

The architectural constraint is fixed now, before anything is written: the
overlay is a second **consumer** of the sampler, never a second sampler.

```
Snapshot
├── Main UI
├── Overlay
└── Notifications
```

It subscribes to the same `services:update` and owns no timer. An overlay that
polled independently would double the scan cost and could disagree with the main
window about what is running — two clocks, two truths.

### 2.10 · Local intelligence foundation — PLANNED

Groundwork only: the structured context a local model would read, not the model.

```
small local model  +  structured context  +  controlled tools  +  selective retrieval
```

Constraints fixed now, so the option stays open without distorting V2:

- **Local only.** No cloud call, no account, no telemetry, no model download in
  V2.
- **Structured context.** It reads Snapshots and Events — the typed data the UI
  already reads — never scraped screens.
- **Controlled tools.** A named, audited, read-only set. Never a shell, never
  arbitrary command execution, never elevation.
- **It explains; it does not decide.** Conflict and project detection stay
  rule-based and deterministic.

### 2.11 · Logs — *candidate, likely V2.2*

Deliberately last in V2, and possibly deferred.

**Hard constraint:** Windows offers no supported way to attach to an
already-running process's stdout. Log capture is only possible for processes
LocalDocks started itself, which means **logs depend on service lifecycle
landing first**. Scoping this honestly avoids promising something the platform
cannot deliver.

**Backend cost: high** — stream lifecycle, buffering, backpressure, retention,
child-process handling, cleanup on exit.

---

## V3 — Local development environment

**Everything in V3 is PLANNED.** V3 joins non-Windows-process sources at the
*Service* level, which is why V1 spent the effort getting that entity right.

Where LocalDocks becomes an orchestrator rather than a manager.

### 3.1 · Docker
Containers, images, networks, published ports, Compose projects. Integrate with
Docker's own API rather than reimplementing container management.

### 3.2 · WSL
Distributions and the services inside them. **Treated as a separate provider,
not as more Windows processes** — WSL2 has its own network namespace and its
own process domain.

### 3.3 · Local infrastructure
PostgreSQL, Redis, MongoDB, MySQL, RabbitMQ, Elasticsearch.

**Rule: do not write fifteen bespoke integrations.** Define one
`LocalServiceProvider` concept and add providers against it.

### 3.4 · Service definitions and workspaces
Declared projects that LocalDocks can *launch*:

```
My Project
  Frontend   npm run dev            :5173
  Backend    uv run fastapi dev     :8000
  Worker     python worker.py       —
  Database   Docker container       :5432
```

Then: **Start Project.** This is the point where LocalDocks stops observing an
environment and starts producing one.

### 3.5 · Command palette expansion
Once there are enough verbs, `Ctrl+K` becomes the fastest path to all of them.

### 3.6 · Advanced diagnostics
Rule-based, not model-based. A deterministic engine that says "Worker failed:
port unavailable" is more useful and more trustworthy than a probabilistic one.

### 3.7 · Editor and tool integrations
VS Code, JetBrains, terminal, Git.

### 3.8 · Richer overlay — PLANNED

The V2 overlay, extended with the data V3 sources add: containers, distributions
and infrastructure alongside Windows processes. Same rule — one sampler.

### 3.9 · Richer local intelligence — PLANNED

The V2 foundation with a model actually attached, still local, still read-only,
still explaining rather than deciding.

### 3.10 · Cross-platform
macOS and Linux become realistic only once the platform-adapter boundary has a
second real implementation behind it. **Do not build a speculative
cross-platform abstraction before then.**

### 3.11 · Plugin system — DEFERRED

Last, and only when there is a concrete third-party need. Never built simply
because it appears on a roadmap.

---

## Future — beyond V3

Not a version. Things that need a reason to exist before they get one.

| Item | Status | Gate |
|---|---|---|
| Broader cross-platform support | **DEFERRED** | A real second platform with real users, not an abstraction built in advance |
| macOS as that second platform | **DEFERRED** | The V1 predicate already works there without elevation, which is the argument for it being first |
| Plugin ecosystem | **DEFERRED** | A concrete third-party need |

---

## Website and launch — PLANNED, not started

Documented so the shape is agreed; no work happens here before V1 ships.

Site: **localdocks.jayrane.dev**

A scroll-driven journey following the product's own model, which is also the
order the app teaches it in:

```
machine → processes → ports → services → project → environment → control
```

Every image on the site, in the README, in the documentation, on the Microsoft
Store, on GitHub and in social posts comes from **one screenshot pipeline
capturing the real application**. No mockups, no rendered approximations. A
screenshot that cannot be produced by running the app is a claim the product
cannot support.

---

## Open source

The repository stays **private until V1 is genuinely stable**. Shipping the
history of a tool that reads processes and holds kill rights is not something to
do in a hurry.

Before making it public, in order:

1. Security audit of every command reachable from the frontend
2. Git history audit — every commit, not just the tip
3. Secrets scan across the full history
4. Dependency and licence review
5. Microsoft Store package review
6. Documentation review against the actual repository
7. Screenshot sanitisation — real machines contain real names
8. Release build verification

Then: public GitHub → GitHub release → Microsoft Store release → website.

---

## Non-goals

Things LocalDocks will not become, at any version:

- **A general system monitor.** Task Manager exists and is good at that.
- **A cloud or remote-machine tool.** Local-first is a design constraint, not a
  starting point.
- **An account-based product.** No login, no sync, no telemetry.
- **An AI feature vehicle.** Diagnostics stay rule-based and deterministic. The
  local intelligence layer described under V3 is an optional reader of the same
  structured data — it never becomes the thing that decides whether a port is
  conflicted, and it never becomes a reason to add a cloud dependency.
- **An elevated tool.** LocalDocks never requests administrator rights; it
  degrades visibly instead.
- **A prettier `netstat`.** If a release adds only port-list features, it has
  missed the point.

---

## How versions are decided

A feature belongs in a version when:

1. Its **data model** is settled — not just its UI.
2. The **backend capability it needs already exists**, or is in the same version.
3. It can be built **without elevation**.
4. It is honest about platform limits — no feature that has to pretend Windows
   can do something it cannot.

When those are not all true, the feature moves to the next version. That is how
Logs moved to V2.2 and how cross-platform moved behind a real second platform.
