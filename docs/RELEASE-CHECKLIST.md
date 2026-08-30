# LocalDocks V1 — Release-candidate checklist

Version `0.9.0`. Every line was checked against a **packaged, installed build**,
not against `cargo test` and `tauri dev`. Where a line is not done it says why.

| Marker | Meaning |
|---|---|
| **DONE** | Verified on the installed release build |
| **NOT DONE** | Outstanding work inside this repository |
| **BLOCKED** | Cannot proceed without something outside this repository |
| **DEFERRED** | Deliberately not in V1, reason named |

---

## Functional

| Item | Status | Evidence |
|---|---|---|
| Processes | **DONE** | 132–241 rows on the live machine; identity is PID + creation time |
| Ports | **DONE** | 86–132 sockets; TCP v4, TCP v6 and UDP, unmerged |
| Services | **DONE** | Process + endpoints join; dual-stack grouped |
| Sampler | **DONE** | One thread, one cadence; 13 ms per tick in the release build |
| CPU | **DONE** | Machine and per-logical-processor |
| Memory | **DONE** | System and per-process, never conflated |
| Network | **DONE** | Rates from cumulative counters |
| Storage | **DONE** | Read, write and active time per physical drive |
| GPU | **DONE** | Utilisation and memory; unavailable state verified by construction |
| Thermal | **DONE** | ACPI zones; a zone reporting 0 K shows "not reporting" |
| Details | **DONE** | Executable and command line; working directory honestly `unavailable` |
| Termination | **DONE** | Identity re-verified; a stale identity is refused |
| URL actions | **DONE** | `http`/`https` allowlist, validated before the OS sees it |
| Developer / System | **DONE** | 0–2 of 15–29 services in Developer; all of them in System |
| Themes | **DONE** | Local Dark, Dark, Light; token-driven, AA verified |

## Reliability

| Item | Status | Evidence |
|---|---|---|
| Startup | **DONE** | 964 ms to first window from a clean profile |
| Shutdown | **DONE** | Graceful close; no orphan WebView2 processes |
| Restart | **DONE** | Relaunch clean; settings restored |
| Failure recovery | **DONE** | A listener killed mid-observation; app survived, memory and handles flat |
| Long-run stability | **IN PROGRESS** | 3-hour run; see the measurements section of the final report |
| No memory leak | **DONE (so far)** | 36.3 → 36.9 MB over the observed window |
| No handle leak | **DONE (so far)** | 386 → 388 handles, non-monotonic |

## Security

| Item | Status | Evidence |
|---|---|---|
| Unelevated | **DONE** | Per-user install, no manifest request, no admin prompt |
| No `SeDebugPrivilege` | **DONE** | No `AdjustTokenPrivileges` call exists |
| Safe termination | **DONE** | PID **and** creation time; never PID alone |
| Safe URL handling | **DONE** | Allowlist, plus control-character and NUL rejection before scheme parsing |
| No shell execution | **DONE** | `ShellExecuteW` with the `open` verb; no argument string is built |
| No secrets | **DONE** | 110 tracked files scanned; two real findings fixed |
| Minimal capabilities | **DONE** | `core:default` only; no fs, shell, http or dialog plugin |
| CSP | **DONE** | Was `null`; now `default-src 'self'` with `object-src 'none'` |
| No remote telemetry | **DONE** | No network client in the dependency tree |

## Packaging

| Item | Status | Evidence |
|---|---|---|
| Release build | **DONE** | `LocalDocks.exe`, 9.53 MB, optimised |
| Installer | **DONE** | `LocalDocks_0.9.0_x64-setup.exe`, 2.65 MB, NSIS per-user |
| Install | **DONE** | 2.1 s, exit 0, no elevation |
| Uninstall | **DONE** | No files, no shortcut, no registry entry left |
| Upgrade | **DONE** | 0.9.0 → 0.9.1 over the top: one registry entry, settings survived |
| Package identity | **DONE** | Product, identifier, publisher and version all cross-checked |
| Icon | **DONE** | Shortcut resolves to the executable's embedded icon |
| Version | **DONE** | One source; installer, binary, registry and About screen agree |
| Architecture | **DONE** | x64 |
| Code signing | **NOT DONE** | No certificate. SmartScreen will warn on first download |

## Open source

| Item | Status | Evidence |
|---|---|---|
| README | **DONE** | Rewritten against what the app does; requirements stated |
| Docs | **DONE** | ROADMAP, ARCHITECTURE, BACKEND, RELEASE reconciled with the code |
| Licence | **DONE** | MIT; `Cargo.toml` and `package.json` both declare it |
| Contributor docs | **DONE** | CONTRIBUTING.md, CODE_OF_CONDUCT.md, issue and PR templates |
| Security policy | **DONE** | SECURITY.md |
| No private data | **PARTIAL** | Working tree clean; one string remains in commit `1339aae` |
| Dependency audit | **DONE** | 6 Rust, 5 runtime npm; all MIT / Apache-2.0 / OFL-1.1; 0 vulnerabilities |
| Third-party attribution | **DONE** | THIRD-PARTY-NOTICES.md; IBM Plex OFL-1.1 attributed |

## Store

| Item | Status | Evidence |
|---|---|---|
| Identity | **BLOCKED** | `JayRane.LocalDocks` lives in an MSIX manifest that does not exist yet |
| Publisher | **BLOCKED** | Same |
| Version | **DONE** | `0.9.0`, and MSIX-compatible as `0.9.0.0` |
| Package validation | **BLOCKED** | Needs an MSIX to run the App Certification Kit against |
| Screenshots | **NOT DONE** | Plan written; none captured |
| Metadata | **NOT DONE** | Store listing text not written |
| Privacy information | **NOT DONE** | Trivial to answer — nothing is collected — but not filled in |
| System requirements | **DONE** | Stated in the README |

## Launch

| Item | Status |
|---|---|
| GitHub release | **NOT DONE** |
| Store release | **BLOCKED** |
| Website | **DEFERRED** — deliberately last |
| Screenshots | **NOT DONE** |
| Launch assets | **NOT DONE** |
