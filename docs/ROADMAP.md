# LocalDocks — Roadmap

> Status: **V1 frontend contract locked.** Rust backend not started.
> Last updated: 2026-08-28

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

| Area | Detail |
|---|---|
| **Services** | Processes holding a listening socket on a non-system port, owned by the current user |
| **Processes** | Every process the user owns; search, filter, sort, detail |
| **Ports** | One row per socket, unmerged; the diagnostic view |
| **Resources** | CPU %, memory, threads, uptime — live |
| **Identity** | `${pid}-${startedAt}` on every process-bearing row |
| **Details** | Two-tier: cheap fields every tick, expensive fields on open |
| **Actions** | Force terminate (verified), open localhost URL, copy URL |
| **State** | Live snapshot model, loading / empty / error states |
| **Settings** | Theme and refresh interval, persisted |

### Explicitly not in V1

Docker · WSL · logs · project detection · start/stop/restart · resource history
· notifications · tray · conflict detection · plugins · cloud · accounts ·
cross-platform.

### Known V1 polish debt

Tracked, deliberately not blocking:

- Responsive layout below ~1100px
- Keyboard navigation within tables
- Retry affordance on the error state
- Command palette covers services only

---

## V2 — Local development control

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

### 2.9 · Logs — *candidate, likely V2.2*

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

### 3.8 · Cross-platform
macOS and Linux become realistic only once the platform-adapter boundary has a
second real implementation behind it. **Do not build a speculative
cross-platform abstraction before then.**

### 3.9 · Plugin system
Last, and only when there is a concrete third-party need. Never built simply
because it appears on a roadmap.

---

## Non-goals

Things LocalDocks will not become, at any version:

- **A general system monitor.** Task Manager exists and is good at that.
- **A cloud or remote-machine tool.** Local-first is a design constraint, not a
  starting point.
- **An account-based product.** No login, no sync, no telemetry.
- **An AI feature vehicle.** Diagnostics are rule-based by choice.
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
