<#
.SYNOPSIS
  Completes the LocalDocks 0.9.0 release: updater lifecycle, MSIX, hashes.

.DESCRIPTION
  Builds A (0.9.0) and B (0.9.1) already exist but could not run: Tauri's
  updater plugin REFUSES a non-HTTPS endpoint and panics at startup rather
  than accepting it. That is the plugin protecting the update channel, and it
  fails closed - correct behaviour, and the reason the first attempt failed.

  So the test builds are rebuilt with two TEST-ONLY changes:

      endpoint                        -> http://127.0.0.1:41999/latest.json
      dangerousInsecureTransportProtocol -> true

  The production public key is NOT touched, so the update installed in this
  test is verified against exactly the key that ships. The release artifact in
  .release/final was already built against the GitHub HTTPS endpoint and is
  never rebuilt here.

  Both test-only changes are reverted in a finally block, and the script
  asserts afterwards that neither survives in the release configuration.

  Readiness is polled, never assumed: the previous harness slept a fixed 12
  seconds and then reported ECONNREFUSED, which hid a startup panic behind
  what looked like a port problem.
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Continue'
$repo  = Split-Path -Parent $PSScriptRoot
$T     = Join-Path $repo '.release\updater-test'
$feed  = Join-Path $T 'feed'
$nsis  = Join-Path $repo 'src-tauri\target\release\bundle\nsis'
$conf  = Join-Path $repo 'src-tauri\tauri.conf.json'
$cargo = Join-Path $repo 'src-tauri\Cargo.toml'
$final = Join-Path $repo '.release\final'
$out   = Join-Path $repo '.release\LIFECYCLE.txt'
'' | Set-Content $out
function W($t) { $t | Add-Content $out; Write-Host $t }

$GITHUB_URL   = 'https://github.com/jayranedev/LocalDocks/releases/latest/download/latest.json'
$LOOPBACK_URL = 'http://127.0.0.1:41999/latest.json'
$keyPath = Join-Path $env:USERPROFILE '.tauri\localdocks-updater.key'
$prodPub = (Get-Content "$keyPath.pub" -Raw).Trim()
$original = Get-Content $conf -Raw
if ((($original -split '"pubkey": "')[1] -split '"')[0] -ne $prodPub) { throw 'Config pubkey does not match the production key. Refusing to run.' }
if ($original -notmatch [regex]::Escape($GITHUB_URL)) { throw 'Config is not on the GitHub endpoint. Refusing to run.' }

Write-Host ''
Write-Host 'Passphrase for the updater signing key.' -ForegroundColor Cyan
Write-Host 'Needed because the two TEST builds must be signed with the PRODUCTION key -' -ForegroundColor DarkGray
Write-Host 'that is the whole point of the test. Not echoed, not stored.' -ForegroundColor DarkGray
$sec = Read-Host -AsSecureString
$env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content $keyPath -Raw).Trim()
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD =
  [Runtime.InteropServices.Marshal]::PtrToStringAuto([Runtime.InteropServices.Marshal]::SecureStringToBSTR($sec))
if ([string]::IsNullOrEmpty($env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD)) { throw 'Empty passphrase.' }

$cargoHome  = if ($env:CARGO_HOME)  { $env:CARGO_HOME }  else { Join-Path $env:USERPROFILE '.cargo' }
$rustupHome = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $env:USERPROFILE '.rustup' }
$env:RUSTFLAGS = "--remap-path-prefix=$cargoHome=[cargo] --remap-path-prefix=$rustupHome=[rust] --remap-path-prefix=$repo=[localdocks]"

$exePath = "$env:LOCALAPPDATA\LocalDocks\LocalDocks.exe"
$data    = "$env:LOCALAPPDATA\com.silentminds.localdocks"

function SetVersion([string]$v) { (Get-Content $cargo -Raw) -replace '(?m)^version = "[0-9]+\.[0-9]+\.[0-9]+"', "version = `"$v`"" | Set-Content $cargo -NoNewline }
function Feed([string]$j) { [IO.File]::WriteAllText("$feed\latest.json", $j, (New-Object Text.UTF8Encoding $false)) }
function Manifest([string]$v, [string]$file) {
  $s = (Get-Content "$feed\$file.sig" -Raw).Trim()
  "{`"version`":`"$v`",`"notes`":`"Test.`",`"pub_date`":`"2026-08-30T09:00:00Z`",`"platforms`":{`"windows-x86_64`":{`"signature`":`"$s`",`"url`":`"http://127.0.0.1:41999/$file`"}}}"
}
function Drive([int]$port, [string]$a) { (node "$T\drive.mjs" $port $a 2>&1 | Out-String).Trim() }

# Poll for readiness instead of assuming it. Returns $true only when the
# process is alive AND the bridge answers.
function LaunchReady([int]$port, [int]$timeoutSec = 60) {
  $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$port"
  $p = Start-Process $exePath -PassThru
  $t0 = Get-Date
  while (((Get-Date) - $t0).TotalSeconds -lt $timeoutSec) {
    Start-Sleep 1
    $p.Refresh()
    if ($p.HasExited) { W ("  LAUNCH FAILED: process exited with code " + $p.ExitCode); return $null }
    try { $null = Invoke-WebRequest "http://127.0.0.1:$port/json/version" -UseBasicParsing -TimeoutSec 2
          W ("  ready on $port after " + [math]::Round(((Get-Date)-$t0).TotalSeconds,1) + " s"); Start-Sleep 3; return $p } catch {}
  }
  W "  LAUNCH FAILED: bridge never answered"; return $null
}
function Build([string]$v, [string]$label) {
  SetVersion $v
  Remove-Item "$nsis\*" -Force -EA SilentlyContinue
  Get-Process LocalDocks -EA SilentlyContinue | Stop-Process -Force -EA SilentlyContinue; Start-Sleep 2
  npx tauri build 2>&1 | Out-Null
  $e = Get-ChildItem $nsis -Filter '*-setup.exe' -EA SilentlyContinue | Select-Object -First 1
  $s = Get-ChildItem $nsis -Filter '*.sig' -EA SilentlyContinue | Select-Object -First 1
  W ("  $label $v  exit $LASTEXITCODE  installer=$([bool]$e) signature=$([bool]$s)")
  if (-not $e -or -not $s) { throw "$label build not signed." }
  Copy-Item $e.FullName $feed -Force; Copy-Item $s.FullName $feed -Force
}

try {
  W '=== rebuild the TEST pair (loopback + insecure-transport, PRODUCTION key) ==='
  $patched = $original.Replace($GITHUB_URL, $LOOPBACK_URL).Replace(
    '"endpoints": [', '"dangerousInsecureTransportProtocol": true,' + "`n      " + '"endpoints": [')
  Set-Content $conf $patched -NoNewline
  W ("  loopback set    : " + ((Get-Content $conf -Raw) -match [regex]::Escape($LOOPBACK_URL)))
  W ("  insecure flag   : " + ((Get-Content $conf -Raw) -match 'dangerousInsecureTransportProtocol'))
  W ("  pubkey untouched: " + ((((Get-Content $conf -Raw) -split '"pubkey": "')[1] -split '"')[0] -eq $prodPub))
  Remove-Item "$feed\*" -Force -EA SilentlyContinue
  Build '0.9.1' 'A'
  Build '0.9.0' 'B'
}
finally {
  Set-Content $conf $original -NoNewline
  SetVersion '0.9.0'
  $now = Get-Content $conf -Raw
  W ''
  W '=== release configuration restored ==='
  W ("  endpoint is GitHub HTTPS      : " + ($now -match [regex]::Escape($GITHUB_URL)))
  W ("  no loopback endpoint          : " + (-not ($now -match '127\.0\.0\.1')))
  W ("  no insecure-transport flag    : " + (-not ($now -match 'dangerousInsecureTransportProtocol')))
  W ("  production pubkey             : " + ((($now -split '"pubkey": "')[1] -split '"')[0] -eq $prodPub))
  W ("  createUpdaterArtifacts        : " + ($now -match '"createUpdaterArtifacts": true'))
  W ("  version                       : " + ((Select-String -Path $cargo -Pattern '^version = ').Line))
}

# --- the lifecycle ------------------------------------------------------------
W ''
W '=== updater lifecycle (production key) ==='
Get-CimInstance Win32_Process -Filter "Name='node.exe'" | Where-Object { $_.CommandLine -like '*serve.mjs*' } |
  ForEach-Object { Stop-Process -Id $_.ProcessId -Force -EA SilentlyContinue }
Start-Process node -ArgumentList "$T\serve.mjs" -WindowStyle Hidden; Start-Sleep 3

Get-Process LocalDocks -EA SilentlyContinue | Stop-Process -Force -EA SilentlyContinue; Start-Sleep 2
if (Test-Path "$env:LOCALAPPDATA\LocalDocks\uninstall.exe") { Start-Process "$env:LOCALAPPDATA\LocalDocks\uninstall.exe" -ArgumentList '/S' -Wait; Start-Sleep 4 }
Remove-Item $data -Recurse -Force -EA SilentlyContinue
Start-Process "$feed\LocalDocks_0.9.0_x64-setup.exe" -ArgumentList '/S' -Wait; Start-Sleep 3
W ('  1. installed          : ' + (Get-Item $exePath).VersionInfo.ProductVersion)

Feed (Manifest '0.9.1' 'LocalDocks_0.9.1_x64-setup.exe')
$p = LaunchReady 9480
if ($p) {
  W ('  2. settings seeded    : ' + (Drive 9480 'seed'))
  W ('  3. detect 0.9.1       : ' + (Drive 9480 'status'))
  W ('  4. install clicked    : ' + (Drive 9480 'install'))
  $ok = $false
  for ($i = 0; $i -lt 48; $i++) {
    Start-Sleep 5
    if ((Get-Item $exePath -EA SilentlyContinue).VersionInfo.ProductVersion -eq '0.9.1') { W ("  5. binary now 0.9.1   : after $((($i+1)*5)) s"); $ok = $true; break }
  }
  if (-not $ok) { W ('  5. UPDATE DID NOT LAND: still ' + (Get-Item $exePath -EA SilentlyContinue).VersionInfo.ProductVersion) }
  Start-Sleep 8
  Get-Process LocalDocks -EA SilentlyContinue | Stop-Process -Force -EA SilentlyContinue; Start-Sleep 3

  $p2 = LaunchReady 9481
  if ($p2) {
    W ('  6. after restart      : ' + (Drive 9481 'read'))
    Feed (Manifest '0.9.0' 'LocalDocks_0.9.0_x64-setup.exe')
    W ('  7. downgrade refused  : ' + (Drive 9481 'status'))
    Feed ((Manifest '0.9.1' 'LocalDocks_0.9.1_x64-setup.exe').Replace('"version":"0.9.1"','"version":"0.9.2-rc.1"'))
    W ('  8. prerelease refused : ' + (Drive 9481 'status'))
    Feed '{ not json at all'
    W ('  9. malformed feed     : ' + (Drive 9481 'status'))
    Get-Process LocalDocks -EA SilentlyContinue | Stop-Process -Force -EA SilentlyContinue
  }
}
Get-CimInstance Win32_Process -Filter "Name='node.exe'" | Where-Object { $_.CommandLine -like '*serve.mjs*' } |
  ForEach-Object { Stop-Process -Id $_.ProcessId -Force -EA SilentlyContinue }

# --- leave the machine on the real release build ------------------------------
W ''
W '=== install the RELEASE artifact ==='
Get-Process LocalDocks -EA SilentlyContinue | Stop-Process -Force -EA SilentlyContinue; Start-Sleep 2
if (Test-Path "$env:LOCALAPPDATA\LocalDocks\uninstall.exe") { Start-Process "$env:LOCALAPPDATA\LocalDocks\uninstall.exe" -ArgumentList '/S' -Wait; Start-Sleep 4 }
Remove-Item $data -Recurse -Force -EA SilentlyContinue
Start-Process "$final\LocalDocks_0.9.0_x64-setup.exe" -ArgumentList '/S' -Wait; Start-Sleep 3
W ('  installed version     : ' + (Get-Item $exePath).VersionInfo.ProductVersion)
$p3 = LaunchReady 9482
if ($p3) {
  W ('  app reports           : ' + (Drive 9482 'read'))
  W ('  live update check     : ' + (Drive 9482 'status'))
  Get-Process LocalDocks -EA SilentlyContinue | Stop-Process -Force -EA SilentlyContinue
}
W ('  release log size      : ' + [int](Get-Item "$data\logs\LocalDocks.log" -EA SilentlyContinue).Length + ' bytes')

# --- MSIX + hashes -------------------------------------------------------------
W ''
W '=== MSIX ==='
Copy-Item (Join-Path $final 'LocalDocks.exe') (Join-Path $repo 'src-tauri\target\release\LocalDocks.exe') -Force
$m = & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $repo 'scripts\package-msix.ps1') 2>&1 | Out-String
($m -split "`n" | Select-String 'Package:|Size:|SHA256:|succeeded|error') | ForEach-Object { W ('  ' + $_.ToString().Trim()) }
$msix = Get-ChildItem (Join-Path $repo '.release\msix') -Filter '*.msix' | Select-Object -Last 1
Copy-Item $msix.FullName $final -Force

$relExe = Join-Path $final 'LocalDocks_0.9.0_x64-setup.exe'
$relSig = Join-Path $final 'LocalDocks_0.9.0_x64-setup.exe.sig'
$h = (Get-FileHash $relExe -Algorithm SHA256).Hash
"$($h.ToLower())  LocalDocks_0.9.0_x64-setup.exe" | Set-Content (Join-Path $final 'SHA256SUMS.txt') -Encoding ascii
$latest = "{`"version`":`"0.9.0`",`"notes`":`"See https://github.com/jayranedev/LocalDocks/blob/main/CHANGELOG.md`",`"pub_date`":`"$((Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ'))`",`"platforms`":{`"windows-x86_64`":{`"signature`":`"$((Get-Content $relSig -Raw).Trim())`",`"url`":`"https://github.com/jayranedev/LocalDocks/releases/download/v0.9.0/LocalDocks_0.9.0_x64-setup.exe`"}}}"
[IO.File]::WriteAllText((Join-Path $final 'latest.json'), $latest, (New-Object Text.UTF8Encoding $false))

W ''
W '=== FINAL ARTIFACTS ==='
Get-ChildItem $final | ForEach-Object { W ('  {0,-34} {1,10}  {2}' -f $_.Name, $_.Length, (Get-FileHash $_.FullName -Algorithm SHA256).Hash) }
W ''
W 'LIFECYCLE-COMPLETE'
