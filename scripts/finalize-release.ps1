<#
.SYNOPSIS
  Produces the final LocalDocks 0.9.0 release artifacts and proves the update
  channel works with the production signing key.

.DESCRIPTION
  Run this from an ordinary (non-elevated) PowerShell at the repository root.
  It asks for the updater key's passphrase once, keeps it in memory only, and
  never writes it anywhere.

  Three builds, in this order, because that is the minimum:

    A  0.9.1 against a loopback feed   - the "newer version" for the test
    B  0.9.0 against a loopback feed   - installed, then updated to A
    C  0.9.0 against the GitHub feed   - THE RELEASE ARTIFACT

  Only the updater endpoint is ever patched. The production public key stays in
  place for all three, so the update in phase B is verified against exactly the
  key that ships. The endpoint is restored in a finally block, so an
  interruption cannot leave a loopback URL in the config.

  Then it packages the MSIX from the phase C binary, installs the release
  installer, and checks the running app reports 0.9.0.

  Everything lands in .release/FINAL.txt.
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Continue'
$repo   = Split-Path -Parent $PSScriptRoot
$T      = Join-Path $repo '.release\updater-test'
$feed   = Join-Path $T 'feed'
$nsis   = Join-Path $repo 'src-tauri\target\release\bundle\nsis'
$conf   = Join-Path $repo 'src-tauri\tauri.conf.json'
$cargo  = Join-Path $repo 'src-tauri\Cargo.toml'
$out    = Join-Path $repo '.release\FINAL.txt'
$final  = Join-Path $repo '.release\final'
'' | Set-Content $out
function W($t) { $t | Add-Content $out; Write-Host $t }

$GITHUB_URL   = 'https://github.com/jayranedev/LocalDocks/releases/latest/download/latest.json'
$LOOPBACK_URL = 'http://127.0.0.1:41999/latest.json'
$keyPath      = Join-Path $env:USERPROFILE '.tauri\localdocks-updater.key'

# --- preflight ---------------------------------------------------------------
if (-not (Test-Path $keyPath)) { throw "No updater key at $keyPath" }
$prodPub = (Get-Content "$keyPath.pub" -Raw).Trim()
$original = Get-Content $conf -Raw
$confPub = (($original -split '"pubkey": "')[1] -split '"')[0]
if ($confPub -ne $prodPub) { throw "tauri.conf.json pubkey does not match $keyPath.pub. Refusing to build." }
if ($original -notmatch [regex]::Escape($GITHUB_URL)) { throw "tauri.conf.json is not pointing at the GitHub endpoint. Refusing to start." }

Write-Host ""
Write-Host "Passphrase for the updater signing key (not echoed, not stored):" -ForegroundColor Cyan
$sec = Read-Host -AsSecureString
$env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content $keyPath -Raw).Trim()
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD =
  [Runtime.InteropServices.Marshal]::PtrToStringAuto(
    [Runtime.InteropServices.Marshal]::SecureStringToBSTR($sec))
if ([string]::IsNullOrEmpty($env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD)) {
  throw "Empty passphrase. PowerShell cannot pass an empty value to the bundler; it would hang at a prompt."
}

# Keep the build machine's home directory out of the shipped binary. Every
# panic site embeds its source path as a static string; `strip` does not remove
# them because they are program data, not symbols.
$cargoHome  = if ($env:CARGO_HOME)  { $env:CARGO_HOME }  else { Join-Path $env:USERPROFILE '.cargo' }
$rustupHome = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $env:USERPROFILE '.rustup' }
$env:RUSTFLAGS = "--remap-path-prefix=$cargoHome=[cargo] --remap-path-prefix=$rustupHome=[rust] --remap-path-prefix=$repo=[localdocks]"

New-Item -ItemType Directory -Force -Path $final, $feed | Out-Null
Remove-Item "$feed\*" -Force -EA SilentlyContinue
Remove-Item "$final\*" -Force -EA SilentlyContinue

function SetEndpoint([string]$url) {
  (Get-Content $conf -Raw).Replace($GITHUB_URL, $url).Replace($LOOPBACK_URL, $url) | Set-Content $conf -NoNewline
}
function SetVersion([string]$v) {
  (Get-Content $cargo -Raw) -replace '(?m)^version = "[0-9]+\.[0-9]+\.[0-9]+"', "version = `"$v`"" | Set-Content $cargo -NoNewline
}
function Build([string]$version, [string]$label) {
  SetVersion $version
  Remove-Item "$nsis\*" -Force -EA SilentlyContinue
  Get-Process LocalDocks -EA SilentlyContinue | Stop-Process -Force -EA SilentlyContinue
  Start-Sleep 2
  npx tauri build 2>&1 | Out-Null
  $code = $LASTEXITCODE
  $exe = Get-ChildItem $nsis -Filter '*-setup.exe' -EA SilentlyContinue | Select-Object -First 1
  $sig = Get-ChildItem $nsis -Filter '*-setup.exe.sig' -EA SilentlyContinue | Select-Object -First 1
  W ("  $label  $version  exit $code  installer=$([bool]$exe)  signature=$([bool]$sig)")
  if ($code -ne 0 -or -not $exe -or -not $sig) { throw "$label build failed or was not signed." }
  @{ exe = $exe; sig = $sig }
}
function Drive([int]$port, [string]$action) { (node "$T\drive.mjs" $port $action 2>&1 | Out-String).Trim() }
function Feed([string]$json) { [IO.File]::WriteAllText("$feed\latest.json", $json, (New-Object Text.UTF8Encoding $false)) }
function Manifest([string]$version, [string]$file, [string]$sigFile) {
  $s = (Get-Content $sigFile -Raw).Trim()
  "{`"version`":`"$version`",`"notes`":`"Test build.`",`"pub_date`":`"2026-08-30T09:00:00Z`",`"platforms`":{`"windows-x86_64`":{`"signature`":`"$s`",`"url`":`"http://127.0.0.1:41999/$file`"}}}"
}

try {
  W "=== A. build 0.9.1 (loopback endpoint, PRODUCTION key) ==="
  SetEndpoint $LOOPBACK_URL
  $a = Build '0.9.1' 'A'
  Copy-Item $a.exe.FullName $feed -Force; Copy-Item $a.sig.FullName $feed -Force

  W ""
  W "=== B. build 0.9.0 (loopback endpoint, PRODUCTION key) ==="
  $b = Build '0.9.0' 'B'
  Copy-Item $b.exe.FullName $feed -Force; Copy-Item $b.sig.FullName $feed -Force

  W ""
  W "=== C. build 0.9.0 THE RELEASE (GitHub endpoint, PRODUCTION key) ==="
  SetEndpoint $GITHUB_URL
  $c = Build '0.9.0' 'C'
  Copy-Item $c.exe.FullName $final -Force
  Copy-Item $c.sig.FullName $final -Force
  $relExe = Join-Path $final $c.exe.Name
  $relSig = Join-Path $final $c.sig.Name
  $relBin = Join-Path $repo 'src-tauri\target\release\LocalDocks.exe'
  Copy-Item $relBin (Join-Path $final 'LocalDocks.exe') -Force
}
finally {
  # The endpoint must never be left pointing at loopback, whatever happened.
  SetEndpoint $GITHUB_URL
  SetVersion '0.9.0'
  W ""
  W ("config restored: endpoint=" + (((Get-Content $conf -Raw) -match [regex]::Escape($GITHUB_URL))) + "  version=0.9.0  pubkey=" + ((((Get-Content $conf -Raw) -split '"pubkey": "')[1] -split '"')[0] -eq $prodPub))
}

# --- privacy check on the artifact that ships --------------------------------
W ""
W "=== D. no build-machine paths in the release binary ==="
$ascii = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes((Join-Path $final 'LocalDocks.exe')))
$me = @($env:USERNAME, (Split-Path $env:USERPROFILE -Leaf)) | Where-Object { $_ } | Sort-Object -Unique
foreach ($n in $me) { W ("  occurrences of '$n': " + ([regex]::Matches($ascii, [regex]::Escape($n))).Count) }
W ("  GitHub endpoint compiled in : " + ($ascii -match 'github\.com/jayranedev'))
W ("  loopback endpoint absent    : " + (-not ($ascii -match '127\.0\.0\.1:41999')))

# --- E. the lifecycle, against the production key -----------------------------
W ""
W "=== E. update lifecycle, verified against the PRODUCTION public key ==="
Get-CimInstance Win32_Process -Filter "Name='node.exe'" | Where-Object { $_.CommandLine -like '*serve.mjs*' } |
  ForEach-Object { Stop-Process -Id $_.ProcessId -Force -EA SilentlyContinue }
Start-Process node -ArgumentList "$T\serve.mjs" -WindowStyle Hidden; Start-Sleep 3
Feed (Manifest '0.9.1' 'LocalDocks_0.9.1_x64-setup.exe' "$feed\LocalDocks_0.9.1_x64-setup.exe.sig")

$exePath = "$env:LOCALAPPDATA\LocalDocks\LocalDocks.exe"
$data    = "$env:LOCALAPPDATA\com.silentminds.localdocks"
Get-Process LocalDocks -EA SilentlyContinue | Stop-Process -Force; Start-Sleep 2
if (Test-Path "$env:LOCALAPPDATA\LocalDocks\uninstall.exe") { Start-Process "$env:LOCALAPPDATA\LocalDocks\uninstall.exe" -ArgumentList '/S' -Wait; Start-Sleep 4 }
Remove-Item $data -Recurse -Force -EA SilentlyContinue
Start-Process "$feed\LocalDocks_0.9.0_x64-setup.exe" -ArgumentList '/S' -Wait; Start-Sleep 3
W ("  installed: " + (Get-Item $exePath).VersionInfo.ProductVersion)

$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9470'
Start-Process $exePath | Out-Null; Start-Sleep 12
W ("  seeded settings : " + (Drive 9470 'seed'))
W ("  check           : " + (Drive 9470 'status'))
W ("  install click   : " + (Drive 9470 'install'))
$ok = $false
for ($i = 0; $i -lt 48; $i++) {
  Start-Sleep 5
  if ((Get-Item $exePath -EA SilentlyContinue).VersionInfo.ProductVersion -eq '0.9.1') {
    W ("  updated to 0.9.1 after $((($i+1)*5)) s"); $ok = $true; break
  }
}
if (-not $ok) { W ("  UPDATE DID NOT LAND - still " + (Get-Item $exePath -EA SilentlyContinue).VersionInfo.ProductVersion) }
Start-Sleep 10
Get-Process LocalDocks -EA SilentlyContinue | Stop-Process -Force; Start-Sleep 3
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9471'
Start-Process $exePath | Out-Null; Start-Sleep 12
W ("  after restart   : " + (Drive 9471 'read'))
Feed (Manifest '0.9.0' 'LocalDocks_0.9.0_x64-setup.exe' "$feed\LocalDocks_0.9.0_x64-setup.exe.sig")
W ("  downgrade feed  : " + (Drive 9471 'status'))
Get-Process LocalDocks -EA SilentlyContinue | Stop-Process -Force
Get-CimInstance Win32_Process -Filter "Name='node.exe'" | Where-Object { $_.CommandLine -like '*serve.mjs*' } |
  ForEach-Object { Stop-Process -Id $_.ProcessId -Force -EA SilentlyContinue }

# --- F. install the real release and confirm it ------------------------------
W ""
W "=== F. install the RELEASE artifact and confirm ==="
if (Test-Path "$env:LOCALAPPDATA\LocalDocks\uninstall.exe") { Start-Process "$env:LOCALAPPDATA\LocalDocks\uninstall.exe" -ArgumentList '/S' -Wait; Start-Sleep 4 }
Remove-Item $data -Recurse -Force -EA SilentlyContinue
Start-Process $relExe -ArgumentList '/S' -Wait; Start-Sleep 3
W ("  installed version : " + (Get-Item $exePath).VersionInfo.ProductVersion)
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9472'
Start-Process $exePath | Out-Null; Start-Sleep 12
W ("  app reports       : " + (Drive 9472 'read'))
W ("  update check      : " + (Drive 9472 'status'))
Get-Process LocalDocks -EA SilentlyContinue | Stop-Process -Force

# --- G. MSIX ------------------------------------------------------------------
W ""
W "=== G. MSIX ==="
$msixOut = & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $repo 'scripts\package-msix.ps1') 2>&1 | Out-String
($msixOut -split "`n" | Select-String 'Package:|Size:|SHA256:|Signature:|succeeded|error') | ForEach-Object { W ('  ' + $_.ToString().Trim()) }
$msix = Get-ChildItem (Join-Path $repo '.release\msix') -Filter '*.msix' | Select-Object -Last 1
Copy-Item $msix.FullName $final -Force

# --- H. hashes and the feed manifest -----------------------------------------
W ""
W "=== H. FINAL ARTIFACTS ==="
$hash = (Get-FileHash $relExe -Algorithm SHA256).Hash
"$($hash.ToLower())  $(Split-Path $relExe -Leaf)" | Set-Content (Join-Path $final 'SHA256SUMS.txt') -Encoding ascii
$latest = "{`"version`":`"0.9.0`",`"notes`":`"See https://github.com/jayranedev/LocalDocks/blob/main/CHANGELOG.md`",`"pub_date`":`"$((Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ'))`",`"platforms`":{`"windows-x86_64`":{`"signature`":`"$((Get-Content $relSig -Raw).Trim())`",`"url`":`"https://github.com/jayranedev/LocalDocks/releases/download/v0.9.0/$(Split-Path $relExe -Leaf)`"}}}"
[IO.File]::WriteAllText((Join-Path $final 'latest.json'), $latest, (New-Object Text.UTF8Encoding $false))
Get-ChildItem $final | ForEach-Object { W ("  {0,-34} {1,10}  {2}" -f $_.Name, $_.Length, (Get-FileHash $_.FullName -Algorithm SHA256).Hash) }
W ""
W "FINALIZE-COMPLETE"
