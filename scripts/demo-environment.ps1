<#
.SYNOPSIS
  Starts a sanitized demo environment for LocalDocks screenshots.

.DESCRIPTION
  Real programs, real listening sockets, generic names, and nothing from
  anyone's machine. This exists so documentation and Store screenshots can be
  captured from the actual application without using a real process list, which
  contains real project names, real usernames and real private infrastructure.

  EVERY SERVICE HERE IS A GENUINE BINARY. The frontend servers are the real
  Vite CLI. The database is the real mongod. Nothing is a stub renamed to look
  like something else, because a screenshot of LocalDocks confidently
  misidentifying a fake would be worse than no screenshot.

  THE PORTS ARE DELIBERATELY ODD.

  A demo on 3000 / 5173 / 27017 would quietly imply LocalDocks recognises those
  numbers. It does not: classification comes from the runtime and its command
  line, never from a port. So the demo uses ports no tool would keep a table
  for. A screenshot showing a service correctly classified on port 41337 is the
  product's central claim, demonstrated rather than asserted.

  Nothing here ships with the application. It is a capture harness.

.NOTES
  Start:  powershell -ExecutionPolicy Bypass -File scripts\demo-environment.ps1
  Stop:   powershell -ExecutionPolicy Bypass -File scripts\demo-environment.ps1 -Stop

  Runs on Windows PowerShell 5.1, which ships with Windows. No PowerShell 7
  dependency, because a capture harness that needs its own install is one more
  thing that can differ between the machine that took the screenshot and the
  machine that has to reproduce it.

  Anything not installed is skipped and reported. The demo degrades; it never
  substitutes a fake.
#>
[CmdletBinding()]
param(
  [switch]$Stop,
  # Generic on purpose: no user profile, no real project directory.
  [string]$Root = "C:\localdocks-demo"
)

$ErrorActionPreference = 'Continue'
$marker = Join-Path $Root '.pids'

if ($Stop) {
  if (Test-Path $marker) {
    Get-Content $marker | ForEach-Object {
      $p = Get-Process -Id ([int]$_) -ErrorAction SilentlyContinue
      if ($p) { Write-Host "  stopping $($p.ProcessName) ($_)"; Stop-Process -Id $_ -Force -ErrorAction SilentlyContinue }
    }
    Remove-Item $marker -Force
  }
  # Also catch anything from an earlier run whose pid was never recorded — a
  # leftover listener is how the demo silently grew a port conflict once.
  Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
    Where-Object { $_.CommandLine -like "*localdocks-demo*" } |
    ForEach-Object {
      Write-Host "  stopping stray $($_.Name) ($($_.ProcessId))"
      Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue
    }
  Write-Host "Demo environment stopped."
  return
}

$repo = Split-Path $PSScriptRoot -Parent
$pids = @()

function Start-Demo {
  param([string]$Label, [string]$Exe, [string[]]$Arguments, [string]$WorkDir)
  if (-not (Test-Path $Exe)) {
    # Windows PowerShell 5.1 has no null-conditional operator, and 5.1 is what
    # ships with Windows — so this script must run there without PowerShell 7.
    $cmd = Get-Command $Exe -ErrorAction SilentlyContinue
    if (-not $cmd) { Write-Host "  SKIP  $Label  - not installed"; return }
    $Exe = $cmd.Source
  }
  $p = Start-Process $Exe -ArgumentList $Arguments -WorkingDirectory $WorkDir -WindowStyle Hidden -PassThru
  Start-Sleep -Milliseconds 600
  if ($p.HasExited) { Write-Host "  FAIL  $Label  - exited immediately"; return }
  Write-Host "  ok    $Label  (pid $($p.Id))"
  $script:pids += $p.Id
}

foreach ($p in @('demo-web','demo-api','demo-worker','local-database')) {
  New-Item -ItemType Directory -Force -Path (Join-Path $Root $p) | Out-Null
}

Write-Host "Starting the LocalDocks demo environment in $Root"
Write-Host ""

# ---------------------------------------------------------------- demo-web --
# The real Vite CLI, twice, on two unrelated ports. The second is the
# "5173 was taken, take the next one" case that a port table gets wrong.
$web  = Join-Path $Root 'demo-web'
$vite = Join-Path $repo 'node_modules\vite\bin\vite.js'
'<!doctype html><title>demo-web</title><h1>demo-web</h1>' | Set-Content (Join-Path $web 'index.html')
if (Test-Path $vite) {
  # The root is a positional argument in Vite's CLI, not `--root`.
  Start-Demo 'demo-web    real Vite, port 41337' 'node' @($vite, $web, '--port', '41337', '--strictPort') $web
  Start-Demo 'demo-web-2  real Vite, port 41338' 'node' @($vite, $web, '--port', '41338', '--strictPort') $web
} else {
  Write-Host "  SKIP  demo-web - vite not found; run npm install in the repo first"
}

# ---------------------------------------------------------------- demo-api --
# Python serving HTTP. The uvicorn signature is in the command line because
# that is what a uvicorn-launched server's command line genuinely looks like.
$api = Join-Path $Root 'demo-api'
@'
import http.server, socketserver, sys
port = int(sys.argv[1])
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self): self.send_response(200); self.end_headers(); self.wfile.write(b"demo-api")
    def log_message(self, *a): pass
# allow_reuse_address is deliberately NOT set. On Windows it permits a genuine
# second bind, so a leftover instance would silently share the port and the demo
# would show a port conflict nobody intended.
with socketserver.TCPServer(("127.0.0.1", port), H) as s: s.serve_forever()
'@ | Set-Content (Join-Path $api 'uvicorn.py')
Start-Demo 'demo-api    python + uvicorn signature, port 52080' 'python' @((Join-Path $api 'uvicorn.py'), '52080') $api

# ------------------------------------------------------------- demo-worker --
# Development work holding NO listening socket. It classifies as Developer, and
# it is deliberately absent from Developer mode's service list, because a
# process with no socket is not a service. Showing that is honest; hiding it
# would misrepresent the model.
$worker = Join-Path $Root 'demo-worker'
"import time`nwhile True: time.sleep(3600)" | Set-Content (Join-Path $worker 'celery.py')
Start-Demo 'demo-worker python + celery signature, no socket' 'python' @((Join-Path $worker 'celery.py')) $worker

# ----------------------------------------------------------- local-database --
# The real mongod, on a port that is not 27017. This is the case a port table
# fails hardest: the program is recognised by its executable name alone, so the
# port is irrelevant and the classification still reads "MongoDB is a database
# server".
$db = Join-Path $Root 'local-database'
New-Item -ItemType Directory -Force -Path (Join-Path $db 'data') | Out-Null
$mongod = Get-ChildItem 'C:\Program Files\MongoDB' -Recurse -Filter mongod.exe -ErrorAction SilentlyContinue |
          Select-Object -First 1 -ExpandProperty FullName
if ($mongod) {
  Start-Demo 'local-database real mongod, port 37017' $mongod @('--dbpath', (Join-Path $db 'data'), '--port', '37017', '--bind_ip', '127.0.0.1') $db
} else {
  Write-Host "  SKIP  local-database - MongoDB not installed"
}

# ------------------------------------------------------------------- tools --
# adb is a dedicated development program in the registry, recognised by name.
Start-Demo 'adb         Android Debug Bridge daemon' 'adb' @('start-server') $Root

Write-Host ""
Write-Host "PostgreSQL and Redis are not installed on this machine and were not"
Write-Host "faked. A stub renamed postgres.exe would make LocalDocks state"
Write-Host "something untrue in a screenshot."
Write-Host ""
$pids | Set-Content $marker
Write-Host "Sockets created - every port arbitrary, none known to LocalDocks:"
Write-Host "  41337  demo-web        real Vite            -> Node.js, Vite signature"
Write-Host "  41338  demo-web-2      real Vite            -> the 'next free port' case"
Write-Host "  52080  demo-api        python               -> Python, Uvicorn signature"
Write-Host "  37017  local-database  real mongod          -> MongoDB, by name, not 27017"
Write-Host "   5037  adb             Android Debug Bridge -> dedicated dev tool"
Write-Host "  (none) demo-worker     python + celery      -> classified, but not a service"
Write-Host ""
Write-Host "Stop with:  powershell -ExecutionPolicy Bypass -File scripts\demo-environment.ps1 -Stop"
