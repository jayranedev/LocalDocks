# LocalDocks — Code signing

**Current status: NOT SIGNED.** No certificate exists, none has been bought, and
no account has been created. Nothing in this document has been purchased or
provisioned; it is an investigation and a recommendation, and every step that
costs money or creates an account is marked as needing a decision first.

---

## 1 · What "unsigned" actually costs, today

The build is an unsigned NSIS installer. Concretely:

| Where | What a user sees |
|---|---|
| Downloading in Edge or Chrome | A download warning on an unrecognised publisher |
| First run of the installer | SmartScreen: *"Windows protected your PC"* — with **More info → Run anyway** as the only way through |
| Installer UAC | None. The install is per-user and never elevates, so there is no "Unknown publisher" elevation prompt |
| Properties → Digital Signatures | The tab is absent |
| Corporate/managed machines | Some WDAC or AppLocker policies refuse unsigned binaries outright |

SmartScreen's judgement is reputation-based, not signature-based: a *newly*
signed binary from a *new* publisher is also unknown, and reputation accrues
with download volume over time. Signing does not switch the warning off on day
one; it gives the reputation somewhere to accumulate, and it makes the publisher
name visible instead of "Unknown". Microsoft's current statement on how that
reputation is earned is the authoritative one — see
[SmartScreen reputation for Windows app developers](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation).
Do not plan around a rule of thumb from a blog post; read that page before
choosing between the options below on SmartScreen grounds.

---

## 2 · The Store path needs no certificate at all

This is the single most important fact on this page, and it changes the priority
of everything else.

**Microsoft Store submissions are signed by Partner Center**, with the account's
own publisher certificate — the `CN=B46AFC48-B984-41DB-941B-581ABF4CCE85`
recorded in [`STORE-LISTING.md`](STORE-LISTING.md). An MSIX uploaded to Partner
Center does not need a certificate bought from anyone, and a Store-installed
LocalDocks does not trip SmartScreen.

So the Store channel is **not blocked on signing**. It is blocked on the MSIX
package, which is a different problem entirely.

Signing only matters for the **standalone NSIS installer distributed from
GitHub** — the secondary channel.

---

## 3 · The options, with what each one actually requires

### A · Azure Trusted Signing *(recently renamed Azure Artifact Signing)*

Microsoft's own managed signing service. Keys live in a Microsoft-run HSM; there
is no token to buy, receive or lose.

- **Cost model:** a low monthly subscription plus usage, rather than a large
  annual certificate purchase. The current figure is on
  [the product page](https://azure.microsoft.com/en-us/products/artifact-signing) —
  it is not restated here, because a price copied into a repository is a price
  that will be wrong.
- **The catch, and it is the whole decision:** eligibility. The service
  validates the signer's identity, and access for **individual** developers (as
  opposed to established organisations) has been the gating question. There is
  an open Microsoft Q&A thread on exactly this
  ([Trusted Signing for Individual](https://learn.microsoft.com/en-us/answers/questions/2113965/trusted-signing-for-individual)).
  **This must be confirmed before anything else is considered** — if an
  individual account cannot be approved, options B and C are the only ones left.
- **Certificates are short-lived** (days, not years) and re-issued
  automatically. That is fine, because every signature is timestamped: a
  timestamped signature stays valid after the certificate expires.
- **Build integration:** via `bundle.windows.signCommand` (see section 4), or a
  GitHub Action in CI.

### B · An OV code-signing certificate from a commercial CA

- Identity validation of the individual or business, then an annual certificate.
- **Since the CA/Browser Forum's June 2023 rule change, the private key must
  live on FIPS 140-2 Level 2 hardware** — a physical USB token that is shipped
  to you, or the CA's own cloud HSM. That has two consequences worth stating
  plainly: it costs more than the certificate used to, and **a physical token
  cannot be used from a CI runner**. Signing becomes a manual step on one
  machine, or requires paying for the cloud-HSM variant.
- SmartScreen reputation still starts at zero.

### C · An EV code-signing certificate

- The same hardware requirement, stricter identity validation, higher price.
- EV was historically described as granting immediate SmartScreen reputation.
  **Do not buy on that basis without re-reading Microsoft's current
  documentation** (linked in section 1) — the guidance has changed, and it is
  the single most expensive assumption available here.

### D · Self-signed

Useful for exactly one thing: proving the signing pipeline in section 4 works
end to end before money is spent. It does **nothing** for users — a self-signed
binary is no better than an unsigned one, and asking users to install a root
certificate to make a warning go away is a worse security posture than shipping
unsigned honestly.

### E · Ship unsigned, and say so

Publish the installer with its SHA-256 and document the SmartScreen warning and
how to get past it. This is what a large number of open-source Windows tools do,
and it is defensible **as long as it is stated rather than hidden**.

---

## 4 · Build integration — prepared, not activated

`tauri.conf.json` has been left **without any signing configuration**, because a
placeholder thumbprint or a fake timestamp URL would be a configuration that
looks done and is not, and would fail the build the moment anyone ran it.

Tauri 2 (CLI `2.11.4`, verified against the bundled config schema) supports two
routes under `bundle.windows`. Both sign the application binary *and* the NSIS
installer.

**Route 1 — a certificate in the Windows certificate store** (options B and C):

```jsonc
// src-tauri/tauri.conf.json  ->  bundle.windows
"certificateThumbprint": "<SHA-1 thumbprint of the certificate, uppercase hex, no spaces>",
"digestAlgorithm": "sha256",
"timestampUrl": "<the CA's RFC 3161 timestamp server>",
"tsp": true
```

**Route 2 — a custom signer** (option A, and any cloud HSM):

```jsonc
// src-tauri/tauri.conf.json  ->  bundle.windows
"signCommand": "<signing tool> %1"
```

`%1` is substituted with the path of each file to sign.

Notes that matter more than the syntax:

- **Always timestamp.** Without `timestampUrl`, every signature this project
  ever produces becomes invalid the day the certificate expires — including
  installers already on users' disks.
- **A thumbprint is not a secret** and is safe to commit. The *certificate* and
  any credential the sign command uses are not, and `.gitignore` already blocks
  `*.pfx`, `*.pem` and `*.key`.
- **Signing changes the installer's hash.** Every SHA-256 recorded in this
  repository, in `docs/RELEASE.md` and in any GitHub release note describes an
  *unsigned* artifact. Signing means rebuilding, re-hashing and re-publishing
  the checksum — not re-signing a published file.

---

## 5 · Recommendation

**For v0.9.0, ship unsigned — deliberately and visibly — and do not buy anything
yet.**

The reasoning, in order:

1. **The Store, which is the primary channel, does not need a certificate.**
   Buying one now would not unblock the thing that is actually blocked.
2. **v0.9.0 is a release candidate.** SmartScreen reputation earned on an RC's
   download volume is worth close to nothing, and the certificate's clock would
   start running against a build that is going to be superseded.
3. **Option A is the only one worth paying for**, and its feasibility is an
   unanswered eligibility question, not a purchase decision. Answering it is
   free.

So, concretely:

| Step | Cost | Needs your decision |
|---|---|---|
| Publish the SHA-256 with the GitHub release | Free | No — already planned, see [`RELEASE.md`](RELEASE.md) |
| Add a plain SmartScreen note to the README and the release notes | Free | No |
| Check whether an individual can be approved for Azure Trusted Signing | Free | No — but it creates no account until you say so |
| Create an Azure account / subscribe to the signing service | **Costs money** | **YES — nothing has been created** |
| Buy an OV or EV certificate | **Costs money** | **YES — nothing has been bought** |

The first three are safe to do now. The last two are yours.

---

## Sources

- [Code signing options for Windows app developers — Microsoft Learn](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options)
- [SmartScreen reputation for Windows app developers — Microsoft Learn](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation)
- [Azure Artifact Signing (formerly Trusted Signing)](https://azure.microsoft.com/en-us/products/artifact-signing)
- [Artifact Signing FAQ — Microsoft Learn](https://learn.microsoft.com/en-us/azure/artifact-signing/faq)
- [Trusted Signing for Individual — Microsoft Q&A](https://learn.microsoft.com/en-us/answers/questions/2113965/trusted-signing-for-individual)
- Tauri signing keys verified against `node_modules/@tauri-apps/cli/config.schema.json` at CLI 2.11.4
