# Changelog

All notable changes to LocalDocks are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
the project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
The version number lives in exactly one place — `src-tauri/Cargo.toml` — and
everything else derives from it.

## [Unreleased]

Nothing yet.

## [0.9.0] — 2026-08-30

First public release candidate. `0.9.x` rather than `1.0.0` because the product
has not shipped to anyone yet; see [docs/RELEASE.md](docs/RELEASE.md#1--version)
for why a Windows package version cannot carry a `-rc` suffix.

### Added

- **Processes** — every process the current user owns, with CPU, working set,
  thread count and uptime. Process identity is PID **plus** creation time, so a
  recycled PID can never be mistaken for the process that used to hold it.
- **Ports** — every listening socket, TCP v4, TCP v6 and UDP, unmerged. This is
  the diagnostic view: one row per socket, exactly as Windows reports it.
- **Services** — processes joined to the sockets they hold, with the dual-stack
  pairs dev servers create grouped by PID.
- **Developer and System modes** — Developer narrows the view to one coherent
  subgraph: classified development services, the processes behind them, and the
  ports they hold. System shows everything observable.
- **A versioned Developer Registry** — the single place any program is named.
  Classification is by what a program *is*, from a dedicated-program table and a
  runtime-plus-command-line-signature table. **A port number is never sufficient
  to classify anything**, and the classifier's signature structurally admits no
  port.
- **System telemetry** — CPU machine-wide and per logical processor, physical
  memory, network throughput, per-drive storage read/write/active time, GPU
  utilisation and memory, and ACPI thermal zones. Unavailable hardware reports
  as unavailable, with a reason; it never becomes a zero.
- **Service detail panel** — executable path, command line where it can be read,
  and the classifier's reason in plain words.
- **Actions** — open a service in the browser through an `http`/`https`
  allowlist; end a process after re-verifying its identity.
- **Three themes** — Local Dark, Dark and Light, token-driven, AA contrast
  verified.
- **Accessibility** — full keyboard navigation, visible focus, focus management
  in the terminate dialog, and a global reduced-motion path.
- **Release logging** — warnings and above to a rotating 512 KB file in the
  app's own data directory. Nothing is sent anywhere.

### Security

- Runs unelevated and never requests elevation or `SeDebugPrivilege`.
- Content Security Policy tightened from Tauri's default `null` to
  `default-src 'self'` with `object-src 'none'`.
- Tauri capabilities reduced to `core:default` — no filesystem, shell, HTTP or
  dialog plugin.
- No network client anywhere in the dependency tree, and no telemetry.

### Known limitations

- Windows x64 only.
- The installer is **not code signed**; SmartScreen will warn on first run. See
  [docs/CODE-SIGNING.md](docs/CODE-SIGNING.md).
- No in-app updater. See [docs/UPDATES.md](docs/UPDATES.md).
- No Microsoft Store package — the Store requires MSIX, which Tauri does not
  produce. See [docs/STORE-LISTING.md](docs/STORE-LISTING.md).
- Port-conflict detection is not implemented; the Overview card says so rather
  than showing a zero.
- Working directory is reported as unavailable — it cannot be read unelevated
  for another process.

[Unreleased]: https://github.com/jayranedev/LocalDocks/compare/v0.9.0...HEAD
[0.9.0]: https://github.com/jayranedev/LocalDocks/releases/tag/v0.9.0
