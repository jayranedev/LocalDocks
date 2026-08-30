<#
.SYNOPSIS
  Builds the Microsoft Store MSIX package for LocalDocks.

.DESCRIPTION
  Tauri produces an NSIS installer, not an MSIX, and an NSIS .exe cannot be
  submitted to the Store. This script is the packaging step in between: it
  takes the release binary Tauri already built, lays it out with the manifest
  in msix/AppxManifest.xml and the Store logos, and packs it with makeappx
  from the Windows SDK.

  The version is read from src-tauri/Cargo.toml, which stays the single source
  of the version number for every artifact this project produces. It is
  written into the layout's manifest as Major.Minor.Patch.0 — MSIX requires
  four parts, and Partner Center reserves the fourth.

  The output is UNSIGNED, deliberately. Partner Center signs Store submissions
  with the publisher certificate itself, so an unsigned package is what you
  upload. Signing it locally is only needed to install it on this machine for
  validation, which -Sign does with a temporary self-signed certificate that
  matches the manifest's publisher — see scripts/validate-msix.ps1.

.PARAMETER Sign
  Also sign the package with a temporary self-signed certificate so it can be
  installed locally for testing. NEVER submit a package signed this way.

.EXAMPLE
  npx tauri build
  ./scripts/package-msix.ps1
#>
[CmdletBinding()]
param(
  [switch]$Sign
)

$ErrorActionPreference = 'Stop'

$repo   = Split-Path -Parent $PSScriptRoot
$exe    = Join-Path $repo 'src-tauri\target\release\LocalDocks.exe'
$icons  = Join-Path $repo 'src-tauri\icons'
$srcMan = Join-Path $repo 'msix\AppxManifest.xml'
$stage  = Join-Path $repo '.release\msix'
$layout = Join-Path $stage 'layout'

function Find-SdkTool([string]$name) {
  $root = 'C:\Program Files (x86)\Windows Kits\10\bin'
  if (-not (Test-Path $root)) { throw "Windows SDK not found. Install the Windows 10/11 SDK." }
  $hit = Get-ChildItem $root -Recurse -Filter $name -ErrorAction SilentlyContinue |
         Where-Object { $_.FullName -like '*\x64\*' } |
         Sort-Object FullName -Descending | Select-Object -First 1
  if (-not $hit) { throw "$name not found under $root. Install the Windows SDK build tools." }
  $hit.FullName
}

if (-not (Test-Path $exe)) {
  throw "No release binary at $exe. Run 'npx tauri build' first."
}

# --- version, from the one place it lives ------------------------------------
$cargo = Get-Content (Join-Path $repo 'src-tauri\Cargo.toml') -Raw
if ($cargo -notmatch '(?m)^version = "([0-9]+\.[0-9]+\.[0-9]+)"') {
  throw 'Could not read the version from src-tauri/Cargo.toml.'
}
$version = $Matches[1]
$msixVersion = "$version.0"
Write-Host "LocalDocks $version  ->  MSIX $msixVersion"

# --- layout ------------------------------------------------------------------
Remove-Item $layout -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path (Join-Path $layout 'Assets') | Out-Null

Copy-Item $exe (Join-Path $layout 'LocalDocks.exe')

# Only the logos the manifest references. An asset nobody points at is dead
# weight in a package the user downloads.
foreach ($logo in 'StoreLogo.png','Square44x44Logo.png','Square71x71Logo.png',
                  'Square150x150Logo.png') {
  $src = Join-Path $icons $logo
  if (-not (Test-Path $src)) { throw "Missing Store logo: $src" }
  Copy-Item $src (Join-Path $layout "Assets\$logo")
}

# The manifest is copied with the version substituted, so msix/AppxManifest.xml
# stays readable source and never has to be edited to cut a release.
$manifest = Get-Content $srcMan -Raw
# The lookbehind matters. A bare 'Version="..."' pattern also matches the
# MinVersion attribute in <TargetDeviceFamily>, which rewrote the Windows
# floor to the app's own version - a package claiming to require Windows
# 0.9.0.0. makeappx accepts that happily; only reading the packed manifest
# catches it.
$manifest = $manifest -replace '(?<![A-Za-z])Version="[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+"', "Version=`"$msixVersion`""
Set-Content (Join-Path $layout 'AppxManifest.xml') $manifest -Encoding UTF8 -NoNewline

# --- pack --------------------------------------------------------------------
$makeappx = Find-SdkTool 'makeappx.exe'
$package  = Join-Path $stage "LocalDocks_${msixVersion}_x64.msix"
Remove-Item $package -Force -ErrorAction SilentlyContinue

Write-Host "makeappx: $makeappx"
& $makeappx pack /d $layout /p $package /o
if ($LASTEXITCODE -ne 0) { throw "makeappx failed with exit code $LASTEXITCODE" }

$item = Get-Item $package
Write-Host ""
Write-Host "Package:  $($item.FullName)"
Write-Host "Size:     $([math]::Round($item.Length/1MB,2)) MB"
Write-Host "SHA256:   $((Get-FileHash $package -Algorithm SHA256).Hash)"
Write-Host "Signature: unsigned - which is what Partner Center expects."

# --- optional local signing --------------------------------------------------
if ($Sign) {
  Write-Host ""
  Write-Host "Signing with a temporary self-signed certificate (LOCAL TESTING ONLY)."
  $subject = 'CN=B46AFC48-B984-41DB-941B-581ABF4CCE85'
  $cert = Get-ChildItem Cert:\CurrentUser\My |
          Where-Object { $_.Subject -eq $subject } | Select-Object -First 1
  if (-not $cert) {
    $cert = New-SelfSignedCertificate -Type Custom -Subject $subject `
      -KeyUsage DigitalSignature -FriendlyName 'LocalDocks MSIX test signing' `
      -CertStoreLocation 'Cert:\CurrentUser\My' `
      -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
  }
  $signtool = Find-SdkTool 'signtool.exe'
  & $signtool sign /fd SHA256 /sha1 $cert.Thumbprint $package
  if ($LASTEXITCODE -ne 0) { throw "signtool failed with exit code $LASTEXITCODE" }
  Write-Host "Signed with $($cert.Thumbprint). Do NOT submit this file to the Store."
}
