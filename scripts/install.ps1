# One-line install of Lockra on Windows (x64 and ARM64) from a GitHub release (Linux and macOS:
# scripts/install.sh):
#
#   irm https://raw.githubusercontent.com/sunerpy/lockra/main/scripts/install.ps1 | iex
#
# It downloads the per-user installer for this processor and SHA256SUMS from the same release,
# runs nothing unless the installer's SHA-256 matches its line there, installs silently for the
# current user (no administrator prompt) and starts Lockra. Run it again to update; from Lockra
# 0.2.0 the app also updates itself (Settings > About).
#
#   $env:LOCKRA_VERSION = "0.2.0"    a given release instead of the latest
#
# Works in Windows PowerShell 5.1 (what `irm | iex` runs in by default) and PowerShell 7.
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$Repo = "sunerpy/lockra"
$ChecksumFile = "SHA256SUMS"

function Stop-Install($Message) {
  Write-Host "lockra-install: $Message" -ForegroundColor Red
  throw "lockra-install: $Message"
}

function Say($Message) {
  Write-Host "lockra-install: $Message"
}

# Windows PowerShell 5.1 on an older Windows 10 may still offer TLS 1.0 only; GitHub needs 1.2.
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

# The native build for this processor: a 32-bit shell on 64-bit Windows reports its real
# architecture in PROCESSOR_ARCHITEW6432.
$Machine = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
$Arch = switch ($Machine) {
  "AMD64" { "x64" }
  "ARM64" { "arm64" }
  default { Stop-Install "Lockra ships for 64-bit Windows on x64 and ARM64 only (this is $Machine)" }
}

if ($env:LOCKRA_VERSION) {
  $Version = $env:LOCKRA_VERSION -replace '^v', ''
} else {
  Say "finding the latest release"
  $Release = Invoke-RestMethod -UseBasicParsing `
    -Uri "https://api.github.com/repos/$Repo/releases/latest" `
    -Headers @{ "User-Agent" = "lockra-install" }
  $Version = $Release.tag_name -replace '^v', ''
}
# The version goes into URLs and file names: digits and dots, and an optional pre-release tail.
if ($Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$') {
  Stop-Install "not a release version: '$Version'"
}

$Asset = "Lockra_${Version}_${Arch}-setup.exe"
$BaseUrl = "https://github.com/$Repo/releases/download/v$Version"
$TempDir = New-Item -ItemType Directory -Path (Join-Path ([System.IO.Path]::GetTempPath()) ("lockra-" + [System.Guid]::NewGuid()))

try {
  $Installer = Join-Path $TempDir $Asset
  $Checksums = Join-Path $TempDir $ChecksumFile
  Say "downloading $Asset (Lockra $Version)"
  try {
    Invoke-WebRequest -UseBasicParsing -Uri "$BaseUrl/$ChecksumFile" -OutFile $Checksums
  } catch {
    Stop-Install "release v$Version has no $ChecksumFile (is $Version a Lockra release?)"
  }
  $Escaped = [Regex]::Escape($Asset)
  $Line = Get-Content $Checksums | Where-Object { $_ -match "^[0-9a-fA-F]{64}\s+\*?$Escaped$" } | Select-Object -First 1
  if (-not $Line) { Stop-Install "release v$Version has no $Asset" }
  Invoke-WebRequest -UseBasicParsing -Uri "$BaseUrl/$Asset" -OutFile $Installer

  $Expected = ($Line -split '\s+')[0].ToLowerInvariant()
  $Actual = (Get-FileHash -Algorithm SHA256 -Path $Installer).Hash.ToLowerInvariant()
  if ($Actual -ne $Expected) { Stop-Install "checksum mismatch for ${Asset}: nothing was installed" }
  Say "SHA-256 matches $ChecksumFile"

  # A running Lockra holds its files; the installer would stop at it.
  $Running = Get-Process -Name "lockra-desktop" -ErrorAction SilentlyContinue
  if ($Running) {
    Say "closing the running Lockra"
    $Running | Stop-Process -Force
    $Running | Wait-Process -Timeout 15 -ErrorAction SilentlyContinue
  }

  Say "installing for the current user"
  $Setup = Start-Process -FilePath $Installer -ArgumentList "/S" -Wait -PassThru
  if ($Setup.ExitCode -ne 0) { Stop-Install "the installer exited with code $($Setup.ExitCode)" }

  # The per-user installer's directory, as the NSIS template names it; the uninstall entry says
  # where it went when that differs.
  $Candidates = @((Join-Path $env:LOCALAPPDATA "Lockra\lockra-desktop.exe"))
  $Uninstall = Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*" -ErrorAction SilentlyContinue |
    Where-Object { $_.DisplayName -eq "Lockra" } | Select-Object -First 1
  if ($Uninstall -and $Uninstall.InstallLocation) {
    $Candidates = @((Join-Path ($Uninstall.InstallLocation.Trim('"')) "lockra-desktop.exe")) + $Candidates
  }
  $Exe = $Candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
  if ($Exe) {
    Say "installed Lockra $Version; starting it"
    Start-Process -FilePath $Exe
  } else {
    Say "installed Lockra $Version; start it from the Start menu"
  }
} finally {
  Remove-Item -Recurse -Force $TempDir -ErrorAction SilentlyContinue
}
