# LocalDocks — Microsoft Store submission

Everything needed for the Store listing that **can** live in this repository,
plus an explicit list of what cannot and must be done by hand in Partner Center.

Nothing on this page is invented. Where a value is assigned by Microsoft it is
marked as such; where a requirement must be read off Microsoft's own page at
submission time, this document links to that page instead of restating a number
that could go stale between now and submission.

---

## 1 · Reserved identity — do not edit

These values are already reserved and are **not** free text. Every one of them
must be reproduced character-for-character; a typo in any of them produces a
package Partner Center will reject.

| Field | Value | Where it comes from |
|---|---|---|
| Product name | `LocalDocks` | Reserved app name |
| Publisher display name | `Jay Rane` | Partner Center account |
| Package Identity Name | `JayRane.LocalDocks` | Assigned on name reservation |
| Package Publisher | `CN=B46AFC48-B984-41DB-941B-581ABF4CCE85` | Assigned to the account |
| Package Family Name | `JayRane.LocalDocks_pp2s7rtz89eco` | Assigned by the Store |
| Store ID | `9NRJ5N4X20C5` | Assigned by the Store |
| Price | Free | Chosen |

The Tauri identifier `com.silentminds.localdocks` is a **different value for a
different system** and is not interchangeable with any of the above. It names
the app to Tauri and to Windows for `%LOCALAPPDATA%\com.silentminds.localdocks`.
Substituting the Store identity there would move the app's data directory and
orphan every existing user's settings.

---

## 2 · The package — **BUILT LOCALLY**

The Store accepts **MSIX**. Tauri produces **NSIS**, and an NSIS `.exe` cannot
be submitted. That gap is now closed inside this repository rather than left as
a manual step:

| File | What it is |
|---|---|
| [`msix/AppxManifest.xml`](../msix/AppxManifest.xml) | The package manifest, with the reserved identity from section 1 |
| [`scripts/package-msix.ps1`](../scripts/package-msix.ps1) | Lays out the release binary and logos, substitutes the version from `Cargo.toml`, packs with `makeappx` |
| [`scripts/validate-msix.ps1`](../scripts/validate-msix.ps1) | Test-signs, installs and runs the Windows App Certification Kit — **elevated** |

```powershell
npx tauri build
./scripts/package-msix.ps1
#  -> .release/msix/LocalDocks_0.9.0.0_x64.msix
```

The output is **unsigned, deliberately**. Partner Center signs Store
submissions with the publisher certificate itself, so an unsigned package is
what you upload. `package-msix.ps1 -Sign` exists only to make the file
installable on this machine for validation, using a throwaway certificate;
a package signed that way must never be submitted.

### What the manifest declares, and what it does not

| | |
|---|---|
| Identity | `JayRane.LocalDocks`, `CN=B46AFC48-…`, `0.9.0.0`, `x64` |
| Version | Four parts because MSIX requires four. The fourth stays `0` — Partner Center reserves it |
| Application | `Windows.FullTrustApplication` running `LocalDocks.exe`. It is a Win32 app and does not become a UWP app by being packaged |
| Min Windows | `10.0.17763.0` — Windows 10 1809, the floor the README states and the earliest WebView2 supports |
| MaxVersionTested | `10.0.26100.0` — the build it was actually packaged and exercised on, not the newest number available |
| Capabilities | **`runFullTrust` and nothing else** |

The capability list is worth a sentence of its own. `runFullTrust` is a
restricted capability, so Partner Center will ask why it is declared; the
answer is that this is a Win32 desktop application packaged for the Store,
which is the documented use. **`internetClient` is deliberately absent**: a
packaged LocalDocks makes no network requests at all, because the update
channel detects package identity at startup and stands down
([UPDATES.md](UPDATES.md)). If that ever stops being true, the manifest must
change in the same commit as the behaviour.

### What still has to happen off this machine

Packaging is solved. Two things are not:

1. **Certification.** `scripts/validate-msix.ps1` runs the App Certification
   Kit, and both installing a test-signed MSIX and running `appcert.exe`
   require **administrator rights**. It has not been run.
2. **Runtime behaviour under package identity.** An MSIX brings a virtualised
   registry and a redirected `%LOCALAPPDATA%`. Settings persistence, the log
   directory and process enumeration all need re-testing *inside* the package,
   not just outside it. The one thing already proven by construction is that
   the app detects the package and disables its own updater.

## 3 · Listing text — written, ready to paste

### Short description

> See what is running on your machine.

### Description

> LocalDocks is a local development dashboard for Windows.
>
> It shows the services, processes and listening ports you own — what is bound
> to which port, which process is holding it, how much memory and CPU it is
> using, and how long it has been up. Developer mode narrows all of it to the
> subgraph that is actually your development work: your dev servers, your
> databases, your toolchains, and the ports they hold.
>
> Alongside that it reports machine-wide telemetry: CPU across every logical
> processor, physical memory, network throughput, per-drive storage activity,
> GPU utilisation and memory, and ACPI thermal zones.
>
> LocalDocks runs without administrator rights and never asks for them. It has
> no network client, no analytics and no account. Every measurement stays on
> your machine.
>
> When something cannot be measured, LocalDocks says so. It does not turn an
> unavailable sensor into a zero.
>
> **What it does**
> - Lists every process you own, with CPU, memory, thread count and uptime
> - Lists every listening socket — TCP v4, TCP v6 and UDP — unmerged, with the
>   process holding it
> - Joins the two into services, grouping the dual-stack pairs dev servers
>   create
> - Classifies what it finds by what a program *is*, from a versioned registry
>   of real development software — never by which port it happens to hold
> - Shows machine CPU, memory, network, storage, GPU and thermal telemetry
> - Opens a service in your browser, or ends a process after re-verifying its
>   identity so a recycled PID can never be killed by mistake
> - Three themes, full keyboard navigation, and no motion if your system asks
>   for none
>
> **What it does not do**
> - It does not elevate, and it does not request `SeDebugPrivilege`
> - It does not show other users' or the system's processes — it cannot, by
>   design
> - It does not send anything anywhere

### Category

Developer tools

### Search terms

`ports` · `localhost` · `process monitor` · `dev server` · `port conflict` ·
`developer dashboard` · `system monitor` · `netstat`

### Age rating

The questionnaire is short for a tool with no user-generated content, no
communication features, no purchases and no gambling. The answers, all of which
follow from what the app does:

| Question | Answer |
|---|---|
| Violence, sexual content, profanity, controlled substances | No |
| User-generated content, chat, or user-to-user communication | No |
| Shares user location, personal information, or contacts | No |
| In-app purchases, adverts, loot boxes | No |
| Collects or transmits personal information | No |
| Unrestricted internet access | No — one request to one fixed public URL, off in the Store build |

Expected outcome: the lowest available rating. Answer the questionnaire itself;
do not paste this table into Partner Center as if it were the submission.

### Supported devices and architectures

| | |
|---|---|
| Device family | `Windows.Desktop` only |
| Architecture | **x64 only** |
| Minimum | Windows 10 version 1809 (build 17763) |

No arm64 build. LocalDocks reads processes and sockets through Win32 APIs and
has never been compiled or tested for arm64, so claiming it would be a claim
the product cannot support. It is a genuine gap for Windows-on-ARM machines and
is worth doing later; it is not a V1 blocker.

### URLs and legal

| Field | Value | Status |
|---|---|---|
| Support contact | `https://github.com/jayranedev/LocalDocks/issues` | Exists once the repository is public |
| Website | `https://localdocks.jayrane.dev` | **Being built separately** — a Store listing may omit it |
| Privacy policy | — | **REQUIRED, DOES NOT EXIST** (section 4) |
| Copyright | `Copyright (c) 2026 Silent Minds` | Matches `tauri.conf.json` |
| Licence | MIT | `LICENSE`, plus `THIRD-PARTY-NOTICES.md` for IBM Plex (OFL-1.1) |

### What's new in this version

For the first submission this is the release summary, not a changelog:

> First release. LocalDocks shows the services, processes and listening ports
> you own, alongside CPU, memory, network, storage, GPU and thermal telemetry
> for the machine. Developer mode narrows everything to your development work.

Later versions take the matching section from `CHANGELOG.md`.

### Certification notes for the reviewer

Worth writing in the submission's notes box, because a reviewer will otherwise
have to guess why a process monitor exists:

> LocalDocks is a developer tool. It enumerates the current user's own
> processes and listening sockets using standard Win32 APIs
> (`CreateToolhelp32Snapshot`, `GetExtendedTcpTable`, `GetExtendedUdpTable`)
> and reads machine-wide performance counters through PDH. It runs entirely
> unelevated and never requests administrator rights or `SeDebugPrivilege`, so
> it can only observe processes owned by the signed-in user.
>
> `runFullTrust` is declared because this is a Win32 desktop application
> packaged for the Store. No other capability is declared.
>
> The Store build makes no network requests. The app detects package identity
> at startup and disables its own update channel, leaving updates to the Store.
>
> The "End process" action terminates a process the user selected and owns,
> after re-verifying its identity (PID plus creation time) so a recycled PID
> cannot be terminated by mistake.

### System requirements

Windows 10 version 1809 (build 17763) or later, x64, with the Microsoft Edge
WebView2 Runtime — which ships with Windows 11 and current Windows 10 and is
installed by the NSIS package if it is missing.

The MSIX path must declare the same floor in its manifest
(`MinVersion="10.0.17763.0"`), and this requirement should be re-checked against
the WebView2 runtime's own supported-version statement before submission rather
than copied forward from here.

---

## 4 · Privacy and data — the easy section, answered honestly

| Partner Center question | Answer |
|---|---|
| Does this app collect personal information? | **No** |
| Does it transmit data off the device? | **No.** The Store build makes no network requests at all |
| Does it use advertising identifiers? | **No** |
| Does it require an account? | **No** |
| Does it collect analytics or crash reports? | **No** |
| Privacy policy URL | **REQUIRED, DOES NOT EXIST YET** |

**On the update check.** The GitHub build makes one kind of request: a `GET`
for a public release manifest, at most once a day, carrying no body and no
identifier. **The Store build makes none** — it detects package identity and
disables the update channel, because the Store owns updates there. So for the
Store submission the honest answer to "does it transmit data off the device" is
plainly No, and the privacy policy still has to describe the GitHub build,
because the same source produces both.

Partner Center requires a reachable privacy-policy URL for a submission even
when the honest answer to every question above is "nothing". A page saying
exactly that must be published somewhere stable and the URL entered by hand.
`SECURITY.md` and the Settings screen already state the same position; nothing
new has to be *decided*, only published.

What the app writes locally, and nowhere else:

- Settings (theme, refresh interval) under `%LOCALAPPDATA%\com.silentminds.localdocks`
- A rotating 512 KB warning-and-above log in that directory's `logs\`

The log can contain executable names, PIDs, port numbers and Windows error
text. It cannot contain command lines, file paths or working directories:
those are read for the detail panel and the classifier, and neither logs at
warn or above.

---

## 5 · Assets

### Screenshots — **DONE**

Thirteen PNGs at 2560 × 1600, in [`docs/assets/screenshots/`](assets/screenshots/),
captured from the installed production build by
[`scripts/capture-screenshots.mjs`](../scripts/capture-screenshots.mjs) against
the sanitized demo environment. Provenance, the full index and the sanitisation
checks are documented in that folder's
[README](assets/screenshots/README.md).

A Store listing takes far fewer than thirteen. The suggested order, strongest
first:

1. `01-overview-developer.png` — the product in one frame
2. `10-system-telemetry.png` — all six telemetry cards
3. `02-services-developer.png` — the services table
4. `09-detail-panel.png` — the classification reason, in the app's own words
5. `03-ports-developer.png` — the diagnostic socket view
6. `12-theme-light.png` — that it is not a dark-only app

Exact format, count and dimension limits must be read off Microsoft's current
requirements page at submission — see the source list at the end of this
document — rather than trusted from a checklist. 2560 × 1600 PNG clears every
published minimum by a wide margin, but the *count* and the per-file size cap
are the fields that actually change.

### Package logos — **PRESENT, UNVERIFIED IN A PACKAGE**

`src-tauri/icons/` already contains the MSIX square-logo set, generated by the
Tauri icon pipeline from `icon.png` (512 × 512):

| File | Size |
|---|---|
| `Square30x30Logo.png` | 30 × 30 |
| `Square44x44Logo.png` | 44 × 44 |
| `Square71x71Logo.png` | 71 × 71 |
| `Square89x89Logo.png` | 89 × 89 |
| `Square107x107Logo.png` | 107 × 107 |
| `Square142x142Logo.png` | 142 × 142 |
| `Square150x150Logo.png` | 150 × 150 |
| `Square284x284Logo.png` | 284 × 284 |
| `Square310x310Logo.png` | 310 × 310 |
| `StoreLogo.png` | 50 × 50 |

These are the right *files*; they have never been referenced by a manifest or
run through the App Certification Kit, so treat them as an input to step 2 of
section 2, not as a completed item. The Store listing's own logo (as opposed to
the package's) is uploaded separately in Partner Center.

### Not produced, and not needed for a first submission

Trailer video, promotional artwork, and the "Xbox / mobile" device-family
screenshot sets. LocalDocks is Windows-desktop only.

---

## 6 · What is left, and who has to do it

Packaging, listing text, privacy answers, age-rating answers, screenshots and
logos are all done and in this repository. What remains is genuinely external.

| # | Step | Where | Needs |
|---|---|---|---|
| 1 | Publish a privacy-policy page and get its URL | Wherever the site lives | A hosting decision. Nothing has to be *decided* — the answer is "nothing is collected" — only published |
| 2 | Run `scripts/validate-msix.ps1` | **Elevated** PowerShell on this machine | Administrator rights. Installing a test-signed MSIX and running `appcert.exe` both require them |
| 3 | Fix anything the App Certification Kit reports | This repository | Step 2 first |
| 4 | Smoke-test the app *inside* the package | This machine | Step 2 installs it. Virtualised registry and redirected `%LOCALAPPDATA%` are the parts that have never run |
| 5 | Create the submission and paste the listing text from section 3 | Partner Center | Manual |
| 6 | Upload the six screenshots named in section 5 | Partner Center | Manual |
| 7 | Complete the age-rating questionnaire using section 3 | Partner Center | Manual |
| 8 | Answer the privacy questions from section 4, with the URL from step 1 | Partner Center | Steps 1 |
| 9 | Upload `LocalDocks_0.9.0.0_x64.msix` — the **unsigned** one | Partner Center | Steps 2–4 |
| 10 | Paste the certification notes from section 3 and submit | Partner Center | Everything above |

**Nothing in this repository is waiting on anything.** The Store track is
blocked on an elevated certification run and a published privacy-policy URL.

## Sources

- [App screenshots, images, and trailers for MSIX apps — Microsoft Learn](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/screenshots-and-images)
- [Publish apps and games to the Microsoft Store — Microsoft Learn](https://learn.microsoft.com/en-us/windows/apps/publish/faq/submit-your-app)
- [Code signing options for Windows app developers — Microsoft Learn](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options)
