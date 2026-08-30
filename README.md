# LocalDocks

> A minimal, developer-focused dashboard for managing everything running on your local machine.

LocalDocks is an open-source desktop application designed to give developers a clean and focused view of the services, processes, and ports running on their local machine.

Instead of searching through Task Manager or repeatedly using terminal commands to find which process is occupying a port, LocalDocks brings the information developers actually care about into one place.

## Status

**V1 is functionally complete and in release-candidate hardening.** Version
`0.9.0` builds a working, installable Windows package. It is not yet released:
packaging for the Microsoft Store, code signing and the public repository audit
are outstanding. See [docs/RELEASE.md](docs/RELEASE.md) for exactly what is done
and what is not.

**Windows only.** LocalDocks reads processes and sockets through Win32 APIs
directly. Other platforms are future scope, not a near-term plan.

### Requirements

- Windows 10 1809 or later (x64)
- WebView2 runtime — preinstalled on Windows 11 and current Windows 10
- **No administrator rights.** LocalDocks never elevates and never requests
  `SeDebugPrivilege`. It can therefore only see processes your own account owns,
  which is a deliberate limit rather than a gap.

## Vision

LocalDocks aims to become a lightweight control center for local development environments.

The long-term goal is to make it easy to:

- See what is running locally
- Identify which process is using a port
- Monitor resource usage
- Start, stop, restart, and terminate development services
- Group services by project
- Work with Docker and WSL
- View service logs
- Detect development environment problems
- Quickly perform common developer actions

LocalDocks is intentionally designed to remain minimal and developer-focused rather than becoming another general-purpose system task manager.

## What V1 does

**Sees what is running.** Every process your account owns, every listening
socket across TCP IPv4, TCP IPv6 and UDP, and the relationship between them. A
*service* is derived rather than guessed: a process holding a listening socket
on a non-system port.

**Tells development apart from noise.** A machine typically has around thirty
processes listening on non-system ports, and on a normal desktop nearly all of
them are Chrome, Spotify, iCloud, Steam and vendor helpers. Developer mode shows
the ones that are development work, decided by a versioned registry against the
executable and its command line — never by port number, never by what spawned
it. Every verdict carries the sentence that produced it, and services the
registry does not recognise are reported as *unclassified* rather than guessed
in either direction.

**Measures the machine honestly.** CPU and per-logical-processor CPU, memory,
network throughput, disk throughput and active time, GPU utilisation and memory,
and ACPI thermal zones. A reading this machine cannot provide says so, naming
the reason. Nothing renders as `0%` or `0 MB/s` unless that is the measurement.

**Acts safely.** A process is identified by PID *and* creation time, and every
destructive action re-verifies that identity before touching anything, so a
recycled PID cannot be terminated by a stale row. Opening a service URL is
validated against an `http`/`https` allowlist before the OS ever sees the
string; no shell is invoked.

**Stays out of the way.** One Rust-owned sampler owns the cadence — the UI never
triggers a scan. Measured at roughly 13 ms per tick and 0.09% CPU at idle. Three
themes: Local Dark, Dark and Light.

**Sends nothing anywhere.** No network client, no analytics, no crash reporter,
no account.

## Roadmap

### V1 — Local visibility · functionally complete

- [x] Detect local processes
- [x] Detect listening ports — TCP v4, TCP v6, UDP
- [x] Map ports to processes
- [x] Service model: process + endpoints
- [x] Developer / System mode and the Developer Registry
- [x] Process CPU, memory, threads, uptime
- [x] System CPU, per-core CPU and memory
- [x] Network, storage, GPU and thermal telemetry
- [x] Search and filter
- [x] Process details
- [x] Open local services in browser
- [x] Copy local URLs
- [x] Safe, identity-verified process termination
- [x] Live updates
- [x] Three themes
- [ ] Release packaging, signing and Store submission — *in progress*

Detail depth is tracked in [docs/ROADMAP.md](docs/ROADMAP.md), which marks every
item IMPLEMENTED, RELEASE-READY, PLANNED, DEFERRED or NON-GOAL against the code
rather than against intent.

### V2 — Developer Control

- [ ] Start services
- [ ] Stop services
- [ ] Restart services
- [ ] Process trees
- [ ] Project detection
- [ ] Group services by project
- [ ] Detect frameworks and runtimes
- [ ] Open project directory
- [ ] Open terminal
- [ ] Resource graphs
- [ ] Port conflict assistance

### Future

- [ ] Logs
- [ ] Docker integration
- [ ] WSL integration
- [ ] Database/service detection
- [ ] Notifications
- [ ] Command palette
- [ ] Keyboard shortcuts
- [ ] Integrations
- [ ] Plugin system
- [ ] Additional platforms

The roadmap is subject to change as the project develops.

## Why LocalDocks?

Developers frequently run multiple services at the same time:

- Frontends
- APIs
- Workers
- Databases
- Development servers
- Background services

This often results in multiple ports and processes being active simultaneously.

When something goes wrong, developers typically end up switching between terminal commands, Task Manager, browser tabs, and project directories to figure out what is happening.

LocalDocks aims to make that process simpler.

## Philosophy

LocalDocks follows a few principles:

**Minimal**  
Show developers what they need without overwhelming them with unrelated system information.

**Local-first**  
The application is designed around the local development environment.

**Developer-focused**  
Features should solve problems developers actually encounter while building and testing software.

**Fast**  
The application should remain lightweight and responsive.

**Open source**  
LocalDocks is intended to become an open-source project that developers can use, inspect, improve, and contribute to.

## Documentation

| Document | What it covers |
|---|---|
| [docs/ROADMAP.md](docs/ROADMAP.md) | What exists, what is planned, and what is deliberately not built |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | The decisions and why each was made |
| [docs/BACKEND.md](docs/BACKEND.md) | The Windows API surface, with costs and rejected alternatives |
| [docs/RELEASE.md](docs/RELEASE.md) | How a release is built and what is verified |

## Contributing

Contributions will be welcomed once the project reaches its public open-source release.

See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidelines.

## Security

For security-related issues, please see [SECURITY.md](SECURITY.md).

## License

LocalDocks is licensed under the MIT License.

See [LICENSE](LICENSE) for details.

---

Built by [Silent Minds](https://github.com/Silent-Minds).