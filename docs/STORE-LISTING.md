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

## 2 · The blocker: there is no package to submit — **BLOCKED**

The Store accepts **MSIX**. Tauri produces **NSIS** and **WiX MSI**, and neither
is submittable. This is not a configuration gap in `tauri.conf.json`; the target
does not exist.

Producing the MSIX is real work outside this repository:

1. Author an `AppxManifest.xml` whose `<Identity>` carries
   `Name="JayRane.LocalDocks"`,
   `Publisher="CN=B46AFC48-B984-41DB-941B-581ABF4CCE85"` and a four-part
   `Version` (`0.9.0.0`; Partner Center rejects a non-zero revision field, so
   the fourth part stays `0`).
2. Lay out the payload: `LocalDocks.exe` plus the `Assets\` logos.
3. `makeappx pack` — or drive the whole thing with the MSIX Packaging Tool.
4. Validate with the **Windows App Certification Kit** before uploading.
5. Upload to Partner Center, which signs Store submissions with the publisher
   certificate itself (see [`docs/CODE-SIGNING.md`](CODE-SIGNING.md)).

**No MSIX and no manifest exist in this repository, and none has been
fabricated.** A manifest that has never been packed or validated is a claim the
product cannot support.

An MSIX also changes runtime behaviour that this build has never been tested
under — package identity, a virtualised registry and a redirected
`%LOCALAPPDATA%`. Settings persistence, the log directory and the process
enumeration path all need re-testing inside the package, not just outside it.

---

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
| Does it transmit data off the device? | **No — it has no network client** |
| Does it use advertising identifiers? | **No** |
| Does it require an account? | **No** |
| Privacy policy URL | **REQUIRED, DOES NOT EXIST YET** |

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

Everything in this section happens **outside this repository** and cannot be
automated from it.

| # | Step | Where | Blocked on |
|---|---|---|---|
| 1 | Publish a privacy-policy page and get its URL | Anywhere stable | A hosting decision |
| 2 | Produce and validate the MSIX | Local, with the Windows SDK | Step 1 is independent; this one is the real work |
| 3 | Create the submission and enter the listing text from section 3 | Partner Center | Manual |
| 4 | Upload the screenshots from section 5 | Partner Center | Manual |
| 5 | Complete the age-rating questionnaire | Partner Center | Manual — a developer tool with no user content, so the questionnaire is short |
| 6 | Answer the privacy questions from section 4 | Partner Center | Manual |
| 7 | Upload the MSIX; Partner Center signs it | Partner Center | Step 2 |
| 8 | Submit for certification | Partner Center | All of the above |

**Nothing in this repository is waiting on anything.** The Store track is
blocked entirely on the MSIX and on a published privacy-policy URL.

---

## Sources

- [App screenshots, images, and trailers for MSIX apps — Microsoft Learn](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/screenshots-and-images)
- [Publish apps and games to the Microsoft Store — Microsoft Learn](https://learn.microsoft.com/en-us/windows/apps/publish/faq/submit-your-app)
- [Code signing options for Windows app developers — Microsoft Learn](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options)
