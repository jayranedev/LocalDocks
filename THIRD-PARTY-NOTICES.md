# Third-party notices

LocalDocks is distributed under the MIT License (see [LICENSE](LICENSE)). It
bundles or depends on the components below. Every one is under a permissive
licence compatible with MIT redistribution.

This file exists because two of them — the IBM Plex fonts — are shipped *inside*
the application binary and carry an attribution requirement that MIT does not
cover. The rest are listed for completeness, because a user reading the source
should be able to see the whole surface without running a tool.

---

## Bundled in the application

These ship inside `LocalDocks.exe` and reach the user.

### IBM Plex Sans · IBM Plex Mono

- **Copyright** 2019 IBM Corp. All rights reserved.
- **Licence** SIL Open Font License, Version 1.1
- **Source** https://github.com/IBM/plex
- Delivered through `@fontsource/ibm-plex-sans` and `@fontsource/ibm-plex-mono`

The OFL requires this notice to accompany the font software. The full licence
text ships with the packages and is reproduced at
https://openfontlicense.org/open-font-license-official-text/.

LocalDocks does not modify the fonts, does not sell them, and does not use
"IBM Plex" as the name of any part of the product.

### React · React DOM

- **Licence** MIT · Copyright (c) Meta Platforms, Inc. and affiliates
- **Source** https://github.com/facebook/react

### Tauri (`@tauri-apps/api`, `tauri`, `tauri-build`, `tauri-plugin-log`)

- **Licence** Apache-2.0 OR MIT · Copyright (c) 2019–2025 Tauri Programme within The Commons Conservancy
- **Source** https://github.com/tauri-apps/tauri

### Rust crates linked into the binary

| Crate | Licence |
|---|---|
| `windows` | Apache-2.0 OR MIT · Microsoft |
| `serde`, `serde_json` | Apache-2.0 OR MIT |
| `log` | Apache-2.0 OR MIT |

The full transitive set is pinned in `src-tauri/Cargo.lock`, which is committed
deliberately: LocalDocks is a binary rather than a library, so exact versions are
what make a build reproducible.

---

## Build-time only

These are used to build LocalDocks and are **not** distributed with it:
TypeScript (Apache-2.0), Vite, Vitest, Tailwind CSS, oxlint, `@vitejs/plugin-react`,
`@tauri-apps/cli` and the `@types/*` packages (all MIT, except `@tauri-apps/cli`
which is Apache-2.0 OR MIT).

---

## Microsoft Edge WebView2

LocalDocks renders its interface in the WebView2 runtime, which is a **component
of Windows** rather than something LocalDocks bundles or redistributes. It is
preinstalled on Windows 11 and current Windows 10, and is governed by the
Microsoft Software License Terms that accompany it.

---

## What LocalDocks does not include

No analytics SDK, no crash reporter, no telemetry client, no advertising library,
no font or icon set beyond the two above, and no network client of any kind. The
dependency list is short on purpose: every entry is a thing that has to be
audited before a release, and a process-monitoring tool is the wrong place to be
generous about that.
