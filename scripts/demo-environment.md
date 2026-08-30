# Demo environment

A reproducible set of local services for capturing LocalDocks screenshots.

**Why this file exists.** The development machine is not a screenshot set. Its
process list contains real project directories, real usernames and real private
infrastructure, and a marketing image is the worst possible place to discover
that. This environment produces a machine state that looks like a developer's
day and contains nothing belonging to anyone.

Nothing here is part of the application. It is a capture harness.

---

## Rules

Everything below is generic on purpose.

| Never | Instead |
|---|---|
| The real username in a path | `C:\projects\...` |
| A private project name | `storefront`, `orders-api` |
| A LAN or public IP | `127.0.0.1` and `[::1]` only |
| A real token, key or connection string | none are needed; nothing here authenticates |
| A personal port habit | the framework defaults below |

If a capture accidentally includes something from the real machine, the capture
is discarded. It is not cropped.

---

## The services

Six processes, chosen to exercise every part of the UI that has a visual state:
two runtimes, a dedicated database, dual-stack binding, an idle worker, and one
service the registry deliberately does *not* recognise.

| Role | Command | Port | Exercises |
|---|---|---|---|
| Frontend | `npx vite --port 5173 --host` | 5173 v4+v6 | Dual-stack grouping; the Vite command-line signature |
| Backend | `python -m uvicorn app:app --port 8000` | 8000 | A runtime classified by signature, not by name |
| Worker | `celery -A app.worker worker` | — | A developer process holding **no** socket |
| Database | `mongod --dbpath ./data --port 27017` | 27017 | A *dedicated* program, classified on its name alone |
| Cache | `redis-server --port 6379` | 6379 | A second dedicated program; a low, well-known port |
| Unrecognised | `node scripts/helper.js` | 4000 | The **Unknown** classification, which must be visible |

The last row matters more than it looks. A screenshot in which everything is
neatly classified would misrepresent the product: the registry is not
exhaustive, `unknown` is a real outcome, and the UI's honesty about it is a
feature worth showing rather than hiding.

### Layout

```
C:\projects\
  storefront\        frontend + worker
  orders-api\        backend
  sandbox\           the unrecognised helper
```

---

## What to capture

Each shot is taken twice — **Developer** and **System** mode — because the
difference between them is the product's central idea, and one screenshot
cannot show it.

| # | Screen | Must show |
|---|---|---|
| 1 | Overview | Services list, the four stat tiles, all six telemetry cards |
| 2 | Services | Dual-stack `5173` grouped under one service |
| 3 | Processes | The narrowing: a handful in Developer, hundreds in System |
| 4 | Ports | v4 and v6 rows unmerged, with owners |
| 5 | Detail panel | Executable, command line, and **the classification reason** |
| 6 | Terminate dialog | The identity being confirmed, never a bare PID |
| 7 | Themes | The same screen in Local Dark, Dark and Light |
| 8 | Telemetry | At least one card in its genuine **unavailable** state |

Shot 8 is a requirement, not an accident. Showing only populated cards would
imply every machine reports everything, which is exactly the impression the
telemetry layer was built to avoid.

---

## Capture procedure

The application has no screenshot mode and will not get one. Capture is external
and manual for V1.

1. Start the six services above.
2. Launch the **packaged** build from `%LOCALAPPDATA%\LocalDocks`, never
   `tauri dev` — the dev build has a different window title and a debug port.
3. Set the window to a fixed size for consistency between shots.
4. Capture with the OS tool.
5. Review every image for anything from the real machine before it leaves the
   folder.

**Automation is possible but not built.** The webview can be driven over CDP
with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<port>` and
`Page.captureScreenshot`, which is how this release's UI verification was done.
It is recorded here as a known route, not as a thing that exists.

**Status: NOT DONE.** No screenshots have been taken. This file is the plan.
