<#
.SYNOPSIS
  Installs the LocalDocks MSIX locally and runs the Windows App Certification
  Kit against it.

.DESCRIPTION
  MUST BE RUN ELEVATED. Two things here need administrator rights and neither
  can be worked around:

    * Installing an MSIX signed by a self-signed certificate requires that
      certificate in LocalMachine\TrustedPeople.
    * appcert.exe refuses to run unelevated.

  Nothing in this script touches the package you submit. It signs a local copy
  with a throwaway certificate purely so Windows will install it; Partner
  Center signs the real submission with your publisher certificate, and the
  file produced by scripts/package-msix.ps1 without -Sign is the one to upload.

  Run scripts/package-msix.ps1 first.

.EXAMPLE
  # From an elevated PowerShell, at the repository root:
  ./scripts/validate-msix.ps1
#>
[CmdletBinding()]
param(
  [string]$ReportPath = "$PSScriptRoot\..\.release\msix\wack-report.xml"
)

$ErrorActionPreference = 'Stop'

if (-not ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
        ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
  throw "This script must be run from an elevated PowerShell. Installing a test-signed MSIX and running appcert.exe both require administrator rights."
}

$repo  = Split-Path -Parent $PSScriptRoot
$stage = Join-Path $repo '.release\msix'

$package = Get-ChildItem $stage -Filter '*.msix' -ErrorAction SilentlyContinue |
           Sort-Object LastWriteTime | Select-Object -Last 1
if (-not $package) { throw "No .msix in $stage. Run scripts/package-msix.ps1 first." }
Write-Host "Package: $($package.Name)"

# --- 1. sign a throwaway copy so Windows will install it ---------------------
$subject = 'CN=B46AFC48-B984-41DB-941B-581ABF4CCE85'
$signed  = Join-Path $stage ($package.BaseName + '_testsigned.msix')
Copy-Item $package.FullName $signed -Force

$cert = Get-ChildItem Cert:\CurrentUser\My |
        Where-Object { $_.Subject -eq $subject } | Select-Object -First 1
if (-not $cert) {
  $cert = New-SelfSignedCertificate -Type Custom -Subject $subject `
    -KeyUsage DigitalSignature -FriendlyName 'LocalDocks MSIX test signing' `
    -CertStoreLocation 'Cert:\CurrentUser\My' `
    -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
}
Write-Host "Test certificate: $($cert.Thumbprint)"

$signtool = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin' -Recurse -Filter signtool.exe |
            Where-Object { $_.FullName -like '*\x64\*' } |
            Sort-Object FullName -Descending | Select-Object -First 1
& $signtool.FullName sign /fd SHA256 /sha1 $cert.Thumbprint $signed
if ($LASTEXITCODE -ne 0) { throw "signtool failed ($LASTEXITCODE)" }

# Trust it for this machine, so Add-AppxPackage will accept the signature.
$store = New-Object System.Security.Cryptography.X509Certificates.X509Store('TrustedPeople','LocalMachine')
$store.Open('ReadWrite'); $store.Add($cert); $store.Close()
Write-Host "Certificate placed in LocalMachine\TrustedPeople."

# --- 2. install ---------------------------------------------------------------
Get-AppxPackage -Name 'JayRane.LocalDocks' -ErrorAction SilentlyContinue |
  ForEach-Object { Remove-AppxPackage $_.PackageFullName }
Add-AppxPackage -Path $signed
$installed = Get-AppxPackage -Name 'JayRane.LocalDocks'
Write-Host "Installed: $($installed.PackageFullName)"
Write-Host "Family:    $($installed.PackageFamilyName)"

# --- 3. certify ----------------------------------------------------------------
$appcert = 'C:\Program Files (x86)\Windows Kits\10\App Certification Kit\appcert.exe'
if (-not (Test-Path $appcert)) { throw "App Certification Kit not installed." }

New-Item -ItemType Directory -Force -Path (Split-Path $ReportPath) | Out-Null
Write-Host ""
Write-Host "Running the Windows App Certification Kit. This takes several minutes"
Write-Host "and will launch LocalDocks. Do not use the machine while it runs."
& $appcert reset
& $appcert test -packagefullname $installed.PackageFullName -reportoutputpath $ReportPath

if (Test-Path $ReportPath) {
  [xml]$report = Get-Content $ReportPath
  $overall = $report.REPORT.OVERALL_RESULT
  Write-Host ""
  Write-Host "OVERALL: $overall"
  $report.SelectNodes('//*[@RESULT]') |
    Where-Object { $_.RESULT -ne 'PASS' } |
    ForEach-Object { "  $($_.RESULT)  $($_.NAME)" }
  Write-Host ""
  Write-Host "Full report: $ReportPath"
} else {
  Write-Warning "appcert produced no report at $ReportPath"
}

# --- 4. leave the machine as it was --------------------------------------------
Write-Host ""
Write-Host "To remove the test install:"
Write-Host "  Get-AppxPackage -Name JayRane.LocalDocks | Remove-AppxPackage"
Write-Host "  Get-ChildItem Cert:\LocalMachine\TrustedPeople | Where-Object Thumbprint -eq $($cert.Thumbprint) | Remove-Item"
