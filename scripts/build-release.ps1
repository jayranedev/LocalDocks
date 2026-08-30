<#
.SYNOPSIS
  Builds the LocalDocks release artifacts, signed for the update channel.

.DESCRIPTION
  Wraps `npx tauri build` with the two things that are easy to get wrong and
  expensive to notice late:

  1. **It refuses to prompt.** Tauri asks for the updater key's passphrase on
     stdin when TAURI_SIGNING_PRIVATE_KEY_PASSWORD is not set. In a hidden
     window, a CI job or a background shell that is not a prompt, it is a hang
     with no output — the build looks like it is compiling and is in fact
     waiting forever. This script checks the environment first and fails in one
     second with a sentence saying what to set.

  2. **It verifies the signature was actually produced.** `tauri build` can
     write an installer and then fail to sign it, leaving a perfectly good
     .exe with no .sig beside it. Shipping that means every installed copy
     rejects the update. The check below is the difference between finding that
     out now and finding out from a bug report.

  Produces, in .release/:
    LocalDocks_<version>_x64-setup.exe       the installer
    LocalDocks_<version>_x64-setup.exe.sig   its minisign signature
    SHA256SUMS.txt                           for the GitHub release body
    latest.json                              the update feed manifest

.PARAMETER SkipTests
  Skip the test suites. For iterating only — never for a real release.

.EXAMPLE
  $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content "$env:USERPROFILE\.tauri\localdocks-updater.key" -Raw
  $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = Read-Host -AsSecureString | ConvertFrom-SecureString -AsPlainText
  ./scripts/build-release.ps1
#>
[CmdletBinding()]
param([switch]$SkipTests)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Set-Location $repo

# --- the environment, checked before anything slow happens -------------------
if (-not $env:TAURI_SIGNING_PRIVATE_KEY) {
  throw @'
TAURI_SIGNING_PRIVATE_KEY is not set.

It wants the CONTENTS of the updater key, not its path:

  $env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content "$env:USERPROFILE\.tauri\localdocks-updater.key" -Raw).Trim()

(TAURI_SIGNING_PRIVATE_KEY_PATH is not consulted by the bundler. Setting that
one instead produces an installer with no signature and an exit code that is
easy to miss.)
'@
}

if ($null -eq $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD) {
  throw @'
TAURI_SIGNING_PRIVATE_KEY_PASSWORD is not set.

Set it even if the key has no passphrase - use an empty string in that case.
Unset, the bundler prompts on stdin, and in a non-interactive shell that is an
indefinite hang with no output.

  $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = 'your passphrase'
'@
}

# --- version, from the one place it lives ------------------------------------
$cargoPath = Join-Path $repo 'src-tauri\Cargo.toml'
if ((Get-Content $cargoPath -Raw) -notmatch '(?m)^version = "([0-9]+\.[0-9]+\.[0-9]+)"') {
  throw 'Could not read the version from src-tauri/Cargo.toml.'
}
$version = $Matches[1]
Write-Host "LocalDocks $version" -ForegroundColor Cyan

# --- the endpoint must not be a test endpoint --------------------------------
$conf = Get-Content (Join-Path $repo 'src-tauri\tauri.conf.json') -Raw
if ($conf -notmatch 'https://github\.com/[^"]+/latest\.json') {
  throw "The updater endpoint in tauri.conf.json is not an https://github.com URL. Refusing to build a release against a test feed."
}

# --- keep the build machine out of the binary --------------------------------
#
# Every panic site embeds its source path as a static string. Without this, the
# shipped executable contains hundreds of copies of the building user's home
# directory. `strip` in Cargo.toml does not touch these - they are program
# data, not symbols.
#
# Computed from this machine's environment rather than written down, so no
# absolute path from anyone's disk ever enters the repository. It also has to
# be set for every cargo invocation below, not just the bundle step: changing
# RUSTFLAGS between steps makes cargo rebuild everything from scratch.
$cargoHome  = if ($env:CARGO_HOME)  { $env:CARGO_HOME }  else { Join-Path $env:USERPROFILE '.cargo' }
$rustupHome = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $env:USERPROFILE '.rustup' }
$env:RUSTFLAGS = @(
  "--remap-path-prefix=$cargoHome=[cargo]"
  "--remap-path-prefix=$rustupHome=[rust]"
  "--remap-path-prefix=$repo=[localdocks]"
) -join ' '
Write-Host "  path remapping active" -ForegroundColor DarkGray

# --- verification ------------------------------------------------------------
if (-not $SkipTests) {
  Push-Location (Join-Path $repo 'src-tauri')
  foreach ($step in @(
    @{ name = 'cargo fmt --check';   run = { cargo fmt --check } },
    @{ name = 'cargo clippy';        run = { cargo clippy --all-targets -- -D warnings } },
    @{ name = 'cargo test';          run = { cargo test } }
  )) {
    Write-Host "  $($step.name)" -NoNewline
    & $step.run *> $null
    if ($LASTEXITCODE -ne 0) { Pop-Location; throw "$($step.name) failed ($LASTEXITCODE)" }
    Write-Host "  ok" -ForegroundColor Green
  }
  Pop-Location
  foreach ($step in @(
    @{ name = 'npm test';       run = { npm test } },
    @{ name = 'npm run lint';   run = { npm run lint } }
  )) {
    Write-Host "  $($step.name)" -NoNewline
    & $step.run *> $null
    if ($LASTEXITCODE -ne 0) { throw "$($step.name) failed ($LASTEXITCODE)" }
    Write-Host "  ok" -ForegroundColor Green
  }
}

# --- build -------------------------------------------------------------------
$nsis = Join-Path $repo 'src-tauri\target\release\bundle\nsis'
Remove-Item "$nsis\*" -Force -ErrorAction SilentlyContinue
Write-Host "  npx tauri build" -NoNewline
npx tauri build *> $null
if ($LASTEXITCODE -ne 0) { throw "tauri build failed ($LASTEXITCODE). Run it directly to see why." }
Write-Host "  ok" -ForegroundColor Green

$installer = Get-ChildItem $nsis -Filter '*-setup.exe' | Select-Object -First 1
$signature = Get-ChildItem $nsis -Filter '*-setup.exe.sig' | Select-Object -First 1
if (-not $installer) { throw "No installer in $nsis." }
if (-not $signature) {
  throw @"
The installer was built but NOT SIGNED - there is no .sig beside it.

An unsigned artifact cannot be used as an update: every installed copy will
reject it. Check TAURI_SIGNING_PRIVATE_KEY and its passphrase, then rebuild.
Do not publish $($installer.Name).
"@
}

# --- outputs -----------------------------------------------------------------
$stage = Join-Path $repo '.release'
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item $installer.FullName $stage -Force
Copy-Item $signature.FullName $stage -Force

# --- prove the remapping worked ------------------------------------------------
# A privacy check on the actual artifact, not on the flags that were meant to
# produce it. Cheap, and the failure it catches is one nobody would notice.
$exePath = Join-Path $repo 'src-tauri\target\release\LocalDocks.exe'
$ascii   = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($exePath))
$leaked  = @($env:USERNAME, (Split-Path $env:USERPROFILE -Leaf)) |
           Where-Object { $_ } | Sort-Object -Unique |
           Where-Object { $ascii.Contains($_) }
if ($leaked) {
  throw "The release binary contains this machine's user name ($($leaked -join ', ')). Path remapping did not take effect - do not publish this build."
}
Write-Host "  no build-machine paths in the binary" -ForegroundColor Green

$hash = (Get-FileHash $installer.FullName -Algorithm SHA256).Hash
"$($hash.ToLower())  $($installer.Name)" | Set-Content (Join-Path $stage 'SHA256SUMS.txt') -Encoding ascii

# The update feed manifest. The url is version-pinned rather than a /latest/
# redirect: only latest.json is fetched through /latest/, and what it points at
# must be an exact file that never changes under an installed user.
$notes = "See https://github.com/jayranedev/LocalDocks/blob/main/CHANGELOG.md"
@{
  version   = $version
  notes     = $notes
  pub_date  = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
  platforms = @{
    'windows-x86_64' = @{
      signature = (Get-Content $signature.FullName -Raw).Trim()
      url       = "https://github.com/jayranedev/LocalDocks/releases/download/v$version/$($installer.Name)"
    }
  }
} | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $stage 'latest.json') -Encoding utf8

Write-Host ""
Write-Host "installer  $($installer.Name)  $([math]::Round($installer.Length/1MB,2)) MB"
Write-Host "sha256     $hash"
Write-Host "signature  $($signature.Name)  ok"
Write-Host "signed exe $((Get-AuthenticodeSignature $installer.FullName).Status)  (Authenticode - separate from the updater signature)"
Write-Host ""
Write-Host "Attach to the GitHub release for v$version:"
Write-Host "  $($installer.Name)"
Write-Host "  SHA256SUMS.txt"
Write-Host "  latest.json      <- without this, no installed copy will ever see the release"
