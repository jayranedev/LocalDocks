# LocalDocks — Release

How a LocalDocks release is built, what it contains, and what is verified before
it ships. Written to be followed, not admired: every number in it was measured
on a real machine and every gap in it is named.

---

## Status legend

| Marker | Meaning |
|---|---|
| **DONE** | Verified on a packaged, installed build |
| **NOT DONE** | Genuinely outstanding |
| **BLOCKED** | Cannot proceed without something outside this repository |
| **DEFERRED** | Wanted, deliberately not in V1, with the reason named |

---

## 1 · Version

**One number, one place: `src-tauri/Cargo.toml`'s `[package] version`.**

```
src-tauri/Cargo.toml   version = "0.9.0"     <-- edit here, and only here
        |
        +-- tauri.conf.json      has no `version` key; Tauri falls back to Cargo
        +-- LocalDocks.exe       ProductVersion / FileVersion
        +-- installer filename   LocalDocks_0.9.0_x64-setup.exe
        +-- uninstall entry      DisplayVersion
        +-- the About screen     via getVersion() over IPC
        +-- git tag              v0.9.0, created by hand from this number
```

`package.json` deliberately has **no** version field. It is private and never
published, so a number there could only ever drift out of sync with the one that
matters.

The About screen and the title bar read the version at runtime through
`getVersion()` rather than hard-coding it. That is not fussiness: before this
was fixed, the UI claimed `v0.1.0` while the installer, the executable metadata
and the uninstall entry all said `0.9.0`.

**Why `0.9.0` and not `1.0.0-rc.1`.** A Windows package version must be plain
`Major.Minor.Build.Revision`; the format cannot carry a pre-release suffix. So
the pre-release marker lives in the git tag and the GitHub release flag, and the
package number stays honest about the product not having shipped yet. Each RC
iteration bumps the patch, which keeps two RC builds from sharing a version —
something the upgrade test depends on.

### Cutting a release

```bash
# 1. Set the version (the only place)
#    src-tauri/Cargo.toml  ->  version = "X.Y.Z"

# 2. Full verification
cd src-tauri && cargo fmt --check && cargo check --all-targets \
  && cargo clippy --all-targets -- -D warnings && cargo test
cd .. && npm run build && npm test && npm run lint

# 3. Build the package
npx tauri build
#    -> src-tauri/target/release/bundle/nsis/LocalDocks_X.Y.Z_x64-setup.exe

# 4. Tag from the same number
git tag -a vX.Y.Z -m "LocalDocks vX.Y.Z"
```

---

## 2 · Package

### What is built

| Artifact | Size | Purpose |
|---|---|---|
| `LocalDocks.exe` | 9.5 MB | The application |
| `LocalDocks_0.9.0_x64-setup.exe` | 2.7 MB | NSIS installer, **x64**, per-user |

**NSIS, not MSI.** `installMode: currentUser` installs to
`%LOCALAPPDATA%\LocalDocks` and needs no administrator rights. That matches the
application: LocalDocks never elevates, so its installer should not either. WiX
MSI is available in Tauri and would be the choice if per-machine deployment were
ever needed; it is not needed for V1 and would have meant an admin prompt for a
tool whose entire security posture is that it does not ask for one.

**The binary is named `LocalDocks.exe`,** via a `[[bin]]` section in Cargo.toml.
The scaffold's crate name was `app`, and without that section the shipped
executable is `app.exe` — which for a process monitor means listing *itself* in
its own Processes table under a meaningless name.

### Microsoft Store — **BLOCKED**, and honestly so

The Store requires an **MSIX** package. **Tauri does not produce MSIX**, and no
amount of configuration in this repository will make it. The NSIS installer
above cannot be submitted.

The remaining work is real and outside this repo:

1. Take `LocalDocks.exe` and its assets, and produce an MSIX — either with the
   **MSIX Packaging Tool**, or by writing an `AppxManifest.xml` by hand and
   running `makeappx pack`.
2. The manifest's `<Identity>` must carry the reserved values exactly:
   `Name="JayRane.LocalDocks"`,
   `Publisher="CN=B46AFC48-B984-41DB-941B-581ABF4CCE85"`,
   `Version="0.9.0.0"` (MSIX requires four parts and a non-zero revision policy
   set by Partner Center).
3. Sign it, or upload unsigned to Partner Center, which signs Store submissions
   with the publisher certificate itself.
4. Validate with the **Windows App Certification Kit**.

None of that is fabricated here. There is no MSIX in this repository and no
manifest claiming one, because a manifest that has never been packaged or
validated is a claim the product cannot support.

### Identity — every value cross-checked

| Value | Required | In the build | Status |
|---|---|---|---|
| Product name | LocalDocks | `LocalDocks` | **DONE** |
| Tauri identifier | `com.silentminds.localdocks` | `com.silentminds.localdocks` | **DONE** |
| Publisher (display) | Jay Rane | `Jay Rane` | **DONE** |
| Version | — | `0.9.0` everywhere | **DONE** |
| Architecture | x64 | x64 | **DONE** |
| Store Identity Name | `JayRane.LocalDocks` | *not in this repo* | **BLOCKED** — MSIX only |
| Store Publisher | `CN=B46AFC48-...` | *not in this repo* | **BLOCKED** — MSIX only |
| PFN | `JayRane.LocalDocks_pp2s7rtz89eco` | *not in this repo* | **BLOCKED** — assigned by the Store |
| Store ID | `9NRJ5N4X20C5` | *not in this repo* | **BLOCKED** — assigned by the Store |

The Tauri identifier and the Store identity are **different values for different
systems** and neither substitutes for the other. `com.silentminds.localdocks`
names the app to Tauri and to Windows for its data directory;
`JayRane.LocalDocks` names the package to the Store. Replacing one with the
other would break the app's own data path.

---

## 3 · Logging and crash reporting

**Release:** warnings and errors only, to a rotating 512 KB file in
`%LOCALAPPDATA%\com.silentminds.localdocks\logs`. One file, kept — a log that
grows without limit on a machine nobody looks at is a defect, not a diagnostic.

**Nothing is sent anywhere.** LocalDocks has no network client, no analytics and
no crash reporter. This file is the only thing it writes outside its own
settings.

What can appear in it: executable names, PIDs, port numbers, Windows error text.
What cannot: command lines, file paths, working directories. Those are read for
the detail panel and the classifier, and neither logs at warn or above.

**Crash reporting is DEFERRED**, deliberately. Every option is a network client
that uploads a memory dump from a process-monitoring tool, and that is a
materially different privacy promise from the one on the Settings screen. If it
is ever added it will be opt-in and announced, not switched on quietly.

---

## 4 · Demo environment and screenshots — **DONE**

No image may be taken from the development machine as it stands: its process
list contains real project names, real usernames and real private
infrastructure. So none was.

`scripts/demo-environment.ps1` starts a reproducible, sanitised environment —
six **real** processes (two Vite dev servers, a Uvicorn-signature Python
service, a Celery-signature Python worker with no socket at all, `mongod`, and
`adb`) under generic `C:\localdocks-demo\` paths, on deliberately unusual
ports. Nothing is simulated and no port is hardcoded into the product to make
the demo work; the classifier recognises all of them on ports no convention
would.

`scripts/capture-screenshots.mjs` then drives the **installed production build**
over the Chrome DevTools Protocol and writes thirteen 2560 × 1600 frames to
`docs/assets/screenshots/`. One pipeline, one viewport, reproducible.

The rule, from docs/ROADMAP.md and unchanged: **every image everywhere comes
from one pipeline capturing the real application.** No mockups. A screenshot
that cannot be produced by running the app is a claim the product cannot
support.

Sanitisation, the checks applied and the two cases that needed a deliberate
decision are documented in `docs/assets/screenshots/README.md`. The most
important of them: the System-mode socket table exposes the capture machine's
LAN address, so that frame is narrowed to loopback through the screen's own
search box — and the capture script asserts no routable address is on screen
before it will write the file.

---

## 5 · Code signing — **NOT DONE**, deliberately

The build is unsigned. No certificate has been bought and no account created.

The Store channel does not need one — Partner Center signs submissions with the
account's publisher certificate — so signing matters only for the standalone
installer on GitHub. The options, what each actually costs, the SmartScreen
reality, and the exact `tauri.conf.json` keys for both signing routes are in
**[CODE-SIGNING.md](CODE-SIGNING.md)**.

The recommendation there, in one line: ship v0.9.0 unsigned and visibly so,
publish the SHA-256, and resolve Azure Trusted Signing eligibility before
spending anything.

---

## 6 · Updates — **DEFERRED**, with the decision written down

There is no in-app updater and none was added during release preparation.
Upgrading means running a newer installer over the top, which is verified to
work.

The reason it is not a quick win: LocalDocks currently has **no network client
at all**, and that is a claim made in the Settings screen, the README, the Store
listing and the security checklist. Adding an updater is not just a dependency;
it is a change to a promise. **[UPDATES.md](UPDATES.md)** reviews the options
against this architecture, records the constraint that an MSIX in the Store must
not self-update, and recommends a check-only, opt-in, default-off step as the
first move — not a full auto-installer in the same pass that first publishes the
application.

---

## 7 · Store submission — see STORE-LISTING.md

Listing text, the privacy answers, the reserved identity values, the screenshot
selection and the exact list of manual Partner Center steps are in
**[STORE-LISTING.md](STORE-LISTING.md)**. The blocker is unchanged and is
restated there: the Store needs an MSIX, and Tauri does not produce one.

---

## 8 · GitHub release — prepared, not published

**[releases/v0.9.0.md](releases/v0.9.0.md)** holds the release body, the asset
list, the checksum procedure and the nine-step order of operations. No tag
exists, no release has been created, and `feature/release-hardening` has not
been pushed.
