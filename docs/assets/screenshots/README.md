# Screenshots

Thirteen frames, 2560 × 1600 PNG, captured from the **installed production
build** of LocalDocks 0.9.0 running against the sanitized demo environment.

Nothing here is a mockup, a composite or a retouch. Every image is a real frame
of the real application rendering real data, produced by one script:
[`scripts/capture-screenshots.mjs`](../../../scripts/capture-screenshots.mjs).

## Reproducing the set

```powershell
./scripts/demo-environment.ps1
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9448"
Start-Process "$env:LOCALAPPDATA\LocalDocks\LocalDocks.exe"
node ./scripts/capture-screenshots.mjs 9448
./scripts/demo-environment.ps1 -Stop
```

The viewport is fixed at 1280 × 800 at device scale factor 2, so every frame in
the set is the same size and the set stays consistent between captures.

The remote-debugging port is a capture-time affordance only. Shipped builds set
no such variable, and LocalDocks itself never opens a debugging port.

## The set

| File | Screen | Mode |
|---|---|---|
| `01-overview-developer.png` | Overview | Developer |
| `02-services-developer.png` | Services | Developer |
| `03-ports-developer.png` | Ports | Developer |
| `04-processes-developer.png` | Processes | Developer |
| `05-overview-system.png` | Overview | System |
| `06-services-system.png` | Services, with classification chips | System |
| `07-processes-system.png` | Processes, the whole machine | System |
| `08-ports-system.png` | Ports, narrowed to loopback | System |
| `09-detail-panel.png` | Service detail, with the classification reason | Developer |
| `10-system-telemetry.png` | All six telemetry cards | Developer |
| `11-theme-dark.png` | Overview, Dark theme | Developer |
| `12-theme-light.png` | Overview, Light theme | Developer |
| `13-settings.png` | Settings | Developer |

## What the demo environment shows, and why the ports look odd

`scripts/demo-environment.ps1` starts six real processes — two Vite dev servers,
a Python service with a Uvicorn signature, a Python worker with a Celery
signature and no socket at all, a real `mongod`, and `adb`. They bind
**deliberately unusual ports**: 41337, 41338, 52080, 37017, 5037.

That is the point. Developer mode classifies them correctly on ports no
convention would recognise, which demonstrates the property the classifier is
built around: **classification is by what a program is, never by which port it
holds.** A screenshot showing a dev server on 3000 would prove nothing.

See [`scripts/demo-environment.md`](../../../scripts/demo-environment.md).

## Sanitisation

Every frame was inspected before being kept. The rules:

- No usernames, no home-directory paths, no private project names, no private
  repository paths
- No routable or LAN network addresses
- Nothing that identifies the capture machine beyond the hardware facts the app
  exists to display

Two things follow from that and are worth stating rather than leaving to be
noticed:

**`08-ports-system.png` is filtered to loopback.** Unfiltered, the System socket
table lists the capture machine's LAN address and its link-local IPv6 on the
NetBIOS and SSDP rows. The frame is narrowed using the screen's own search box —
a real control, not an edit — and the capture script *asserts* that no routable
address remains on screen before it keeps the file. If one does, the script
aborts rather than writing the image.

**The System-mode frames show real installed software** — Chrome, VS Code, an
NVIDIA helper, an Apple service. That is what System mode *is*: everything the
app can observe. Those are ordinary consumer applications, not private
information, and a System-mode screenshot with nothing in it would misrepresent
the feature. `09-detail-panel.png`, which shows a command line, deliberately
shows one from the demo environment under a generic `C:\localdocks-demo\` path.
