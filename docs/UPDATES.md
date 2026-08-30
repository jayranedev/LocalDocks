# LocalDocks — Updates

How LocalDocks updates itself, what it sends, and why it does nothing at all
when it came from the Microsoft Store.

---

## 1 · Two channels, one binary

LocalDocks is distributed two ways, and they update in incompatible ways.

| Channel | Artifact | Who updates it |
|---|---|---|
| GitHub / direct download | `LocalDocks_x.y.z_x64-setup.exe` (NSIS, per-user) | **LocalDocks** |
| Microsoft Store | MSIX | **The Store** |

There is one binary, not two builds. At startup it asks Windows whether it is
running with package identity — `GetCurrentPackageFullName`, in
[`platform/windows/packaging.rs`](../src-tauri/src/platform/windows/packaging.rs).
A packaged copy reports `managedByStore`, the Settings screen shows a sentence
instead of a button, and **no network request is ever made**.

That is not politeness. A Store app that downloads and runs its own installer
is against Store policy, and it would not work anyway: an MSIX install is
immutable, and the NSIS installer it would fetch has nothing there to update.
Two build configurations would have been the obvious way to arrange this, and
would have drifted apart the first time one was changed without the other.

---

## 2 · The feed

One endpoint, compiled into the binary from `tauri.conf.json`:

```
https://github.com/jayranedev/LocalDocks/releases/latest/download/latest.json
```

**`/releases/latest/` is not a synonym for "most recent".** GitHub resolves it
to the newest release that is neither a draft nor a prerelease. That *is* the
stable channel, enforced by GitHub rather than by a filter of ours that could
be wrong — and it is why this project has no channel system. There is one
channel, and the rule for it is "stable releases only".

Publishing a release therefore means attaching `latest.json` to it. The
procedure is in [`releases/v0.9.0.md`](releases/v0.9.0.md).

```jsonc
{
  "version": "0.9.1",
  "notes": "What changed.",
  "pub_date": "2026-09-01T10:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "signature": "<contents of the .sig file the build produced>",
      "url": "https://github.com/jayranedev/LocalDocks/releases/download/v0.9.1/LocalDocks_0.9.1_x64-setup.exe"
    }
  }
}
```

Note the `url` is version-pinned, not a `/latest/` redirect. Only `latest.json`
itself is fetched through `/latest/`; what it points at is an exact file that
never changes under an installed user.

---

## 3 · The policy, and where it is enforced

Two rules, each enforced twice, on purpose.

| Rule | First guard | Second guard |
|---|---|---|
| Never install a prerelease | GitHub excludes prereleases from `/releases/latest` | [`logic::release`](../src-tauri/src/logic/release.rs) rejects any version with a prerelease component |
| Never downgrade | The updater plugin refuses an older version | `logic::release` compares semver itself and refuses |

`logic/release.rs` is pure, syscall-free and has thirteen tests, because it is
the part that can be wrong in a way nobody notices until an update ships. The
cases it pins:

```
0.9.0  →  0.9.1        offer
0.9.1  →  0.9.1        nothing
0.9.2  →  0.9.1        never — a downgrade
0.9.0  →  0.9.1-rc.1   never — a prerelease, even though semver ranks it higher
0.9.1-rc.1 → 0.9.1     offer — a release candidate should get the release
0.9.9  →  0.9.10       offer — compared numerically, not as text
0.9.1  →  0.9.1+build  nothing — build metadata is not a new release
0.9.0  →  "latest"     nothing — and the app is unaffected
```

---

## 4 · What it sends, and when

**Content: a `GET` and nothing else.** No body, no identifier, no machine
information, no usage data, no account. GitHub sees an IP address and a request
for a public file, the same as anyone visiting the releases page.

**Timing:**

- Never during startup. The first automatic check waits eight seconds after the
  window has rendered and the first snapshot has arrived.
- At most once every 24 hours, remembered across restarts. Opening the app
  eleven times in an afternoon is one check.
- A stored timestamp from the future is treated as due rather than trusted —
  clocks go backwards, and a bad value must not suppress checking forever.

**Consent:** one toggle in Settings, on by default, and turning it off means no
automatic request is ever made. The manual **Check now** button still works,
because pressing it is consent.

---

## 5 · What happens when it fails

Nothing. That is the design.

Offline, DNS failure, TLS failure, a GitHub outage, a rate limit, a 404, a
truncated body, an HTML error page where a manifest should be, a feed
advertising `"latest"` instead of a version — every one of them ends as a
`failed` state that the Settings screen renders as one quiet line, and the
application carries on doing the job it was opened for.

There is no path from a network error to a rejected IPC promise. The command
returns a state, never an error; a GitHub outage is not a fault in a process
monitor. This was verified by taking the feed away mid-session and confirming
the app kept scanning.

---

## 6 · Security

**Every artifact is verified before it runs.** The updater checks the download
against a minisign public key compiled into the binary. An artifact that fails
verification never reaches disk as something executable. This is the reason the
official plugin is used rather than a hand-rolled downloader: an update channel
that gets signature verification subtly wrong is a remote code execution
channel with a progress bar.

- **HTTPS only** in the shipped configuration. There is no HTTP fallback.
- **One hard-coded endpoint.** It is not configurable at runtime, not settable
  from the frontend, and not reachable through any IPC command.
- **The app never chooses what to download.** `install_update` can only install
  the artifact `check_for_update` already found, verified against policy and
  showed the user. It cannot be handed a URL.
- **No credentials.** The feed is a public file; there is no token anywhere in
  the app or the build.
- **The webview is not involved.** All of it runs in Rust. The Content Security
  Policy stayed `default-src 'self'` — the updater did not widen it — and the
  updater plugin was deliberately **not** added to `capabilities/default.json`,
  so its JavaScript API is unreachable from the frontend. The frontend can only
  call the three commands above.

### The key

Signing uses a minisign key pair. The public half is in `tauri.conf.json` and
is not a secret. The private half is **not in this repository** and must never
be:

```
%USERPROFILE%\.tauri\localdocks-updater.key
```

Builds read it from `TAURI_SIGNING_PRIVATE_KEY` (the key's contents, not its
path — `TAURI_SIGNING_PRIVATE_KEY_PATH` is not consulted by the bundler, which
produces an installer with no `.sig` beside it and an exit code that is easy to
miss).

**Losing this key strands every installed copy on manual reinstalls, forever.**
The public key is compiled into every binary already shipped; a build signed
with a different key cannot update them. Back it up somewhere that survives
this machine.

---

## 7 · What the user sees

Three places, and no more than three:

- **Settings → Updates.** A toggle, a "Check now" button, and one line of
  status. When an update exists, the button becomes "Install 0.9.1" and the
  release notes appear beneath it.
- **The status bar.** One quiet, clickable word when an update is available. An
  update is worth mentioning; it is never worth interrupting a scan for.
- **Nothing else.** No modal, no toast, no badge, no nag on launch.

Installing downloads the artifact, verifies it, runs the per-user NSIS
installer passively, and restarts the app. Settings survive, because they live
in the app's own data directory and the installer does not touch it.
