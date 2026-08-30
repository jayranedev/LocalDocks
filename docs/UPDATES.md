# LocalDocks — Update architecture

A review, not an implementation. **No updater is present in v0.9.0** and none
was added during release preparation, deliberately: an update mechanism is the
one subsystem that can break a working installation on a user's machine
remotely, and it changes a privacy promise this product currently makes in
plain language.

This page says how updates work today, what adding an updater would actually
cost, and what the safest path is.

---

## 1 · How LocalDocks updates today

By reinstalling. The NSIS installer performs an in-place upgrade: the
[release checklist](RELEASE-CHECKLIST.md) records `0.9.0 → 0.9.1` installed over
the top, producing one registry entry, no duplicate uninstall entry, and
settings intact. `allowDowngrades` is `false`, so an older installer refuses to
run over a newer install.

Users find out that a new version exists by looking. That is the whole mechanism
and it is honest, but it does not scale past an audience that follows the
repository.

---

## 2 · The thing that makes this decision non-obvious

> **LocalDocks has no network client.**

That is not an implementation detail. It is:

- a line in the Settings screen — *"Local only. Nothing here leaves this machine."*
- a **DONE** row in the security section of the release checklist — *"No remote telemetry: no network client in the dependency tree"*
- a paragraph in `README.md` and in the Store listing text
- verifiable by anyone, in seconds, by reading `Cargo.toml`: six direct
  dependencies, none of which speak HTTP

Adding `tauri-plugin-updater` puts an HTTP client in the dependency tree and
makes the application talk to a server. Every one of those claims then needs
rewriting, and "it only contacts the update endpoint" is a materially weaker
promise than "it has no network client" — weaker in a way that is invisible to a
user who read the old sentence and believed it.

**This is the actual cost of an updater here, and it is larger than the
engineering.** It should be paid deliberately or not at all.

---

## 3 · What implementing it would involve

For the record, so the decision is made with the shape of the work visible.
Verified against the Tauri CLI's own config schema at `2.11.4`; the plugin's
current documentation is authoritative for the details.

| Piece | What it means here |
|---|---|
| `tauri-plugin-updater` | A new Rust dependency, plus its JS counterpart |
| `bundle.createUpdaterArtifacts: true` | The build additionally emits a signed update artifact next to the installer |
| A minisign key pair | Generated once with the Tauri CLI. **The public key goes in `tauri.conf.json`; the private key must never touch the repository** — it lives in `TAURI_SIGNING_PRIVATE_KEY` in a secret store, and losing it means no future build can ever update an installed one |
| `plugins.updater.endpoints` | An HTTPS URL serving a small JSON manifest. GitHub Releases can host it as a static asset — **no server is required**, which removes the obvious objection |
| A capability permission | `capabilities/default.json` currently grants `core:default` and nothing else. The updater needs its own permission added — the first widening of the capability set since it was locked down |
| UI | Somewhere to say "an update is available", and to fail quietly when the endpoint is unreachable |

The signature check is the part worth being glad about: the plugin verifies the
downloaded artifact against the embedded public key before running it, so a
compromised endpoint alone is not enough to ship code to users.

---

## 4 · The constraint nobody hits until submission

**An MSIX in the Microsoft Store updates through the Store, and only through the
Store.** An app that downloads and installs its own updates inside a Store
package is not acceptable there, and the mechanism would not work anyway — an
MSIX install is immutable and the NSIS installer has nothing to update.

So an updater is not a single switch. It is **a second build configuration**:

| Channel | Updater |
|---|---|
| MSIX, from the Store | **Off.** The Store owns updates |
| NSIS, from GitHub | **On**, if it exists at all |

Deciding this *after* wiring the updater in means discovering it during Store
certification. It is cheap now and expensive then.

---

## 5 · Options, and what each is actually good for

### A · No updater — where the product is now

Zero risk, zero maintenance, no new dependency, and every privacy claim stays
literally true. The cost is entirely on users who never learn a fix shipped.

Appropriate for a release candidate. Not a permanent answer for a shipped tool
with a security-relevant surface.

### B · Check only — tell, do not install *(the recommendation)*

The app checks a static JSON file, and if a newer version exists it shows a
line: *a newer version is available*, with a link. It downloads nothing, runs
nothing, and replaces nothing.

- No installer is ever executed by the app, so the entire "an update broke my
  machine" failure class does not exist.
- No elevation, no restart handling, no rollback design.
- The privacy claim narrows honestly and minimally, and can be made
  user-controlled: **default off, with an explicit toggle in Settings**, so
  "nothing here leaves this machine" remains true for anyone who does not opt
  in.
- Naturally disabled in the Store build — one flag, and the Store's own update
  notice is doing the job anyway.

This captures most of the value of an updater for a fraction of the risk.

### C · Full `tauri-plugin-updater` — download and install

The real thing: download, verify the minisign signature, run the installer,
restart. Best user experience, and the correct end state for a widely-installed
tool.

It also brings the whole surface — rollback when an update fails halfway, a
user mid-action when the restart lands, an endpoint that must stay correct
forever, and a signing key whose loss is unrecoverable.

Worth doing. **Not worth doing in the same pass that first publishes the
application**, when the failure mode is "the first version anyone installed
broke itself".

---

## 6 · Recommendation

1. **v0.9.0 ships with no updater.** No change. (Option A)
2. **Do not implement anything during release preparation.** The decision below
   should be made with a shipped v1.0 and real users in view, not inferred now.
3. **When updates are added, add option B first** — check-only, opt-in, default
   off, disabled in the Store build. It is a small, reversible change that can
   be validated against real endpoints before anything is ever installed
   automatically.
4. **Generate the minisign key pair before the first signed release, not after.**
   The public key is baked into the binary. A build shipped without it can never
   be updated by a later build — the key must exist before there is an installed
   base to reach, or that installed base is permanently stranded on manual
   reinstalls.
5. **Whatever is added, update the privacy language in the same commit** — the
   Settings screen, the README, the Store listing and the release checklist.
   Changing behaviour and leaving the promise behind is the failure this whole
   page exists to prevent.

---

## 7 · What was deliberately not done

- No `tauri-plugin-updater` dependency was added.
- No `createUpdaterArtifacts` flag was set.
- No signing key pair was generated — generating one and leaving it unused
  invites it being committed by accident.
- No endpoint URL was written down anywhere, real or placeholder. A placeholder
  URL in a config file is a live network request to a domain nobody controls.
