# Offline test of scripts/install.ps1 under PowerShell 7 (Linux in CI, any OS locally): the
# cmdlets that reach GitHub, run the installer, look for a running Lockra or read the registry are
# replaced by functions of the same name in this scope (a function wins over a cmdlet), so the
# script's own logic runs as it does on Windows. Nothing is installed and nothing goes online.
# Run by scripts/verify-all.sh (when pwsh is installed) and CI.
$ErrorActionPreference = "Stop"
$Script = Join-Path $PSScriptRoot "install.ps1"
$Version = "9.8.7"
$Tmp = New-Item -ItemType Directory -Path (Join-Path ([System.IO.Path]::GetTempPath()) ("lockra-install-test-" + [guid]::NewGuid()))
$Release = New-Item -ItemType Directory -Path (Join-Path $Tmp "v$Version")

foreach ($Arch in "x64", "arm64") {
  Set-Content -Path (Join-Path $Release "Lockra_${Version}_${Arch}-setup.exe") -Value "installer $Arch" -NoNewline
}
function Write-Sums {
  $Lines = Get-ChildItem $Release -Filter "*.exe" | Sort-Object Name | ForEach-Object {
    "{0}  {1}" -f (Get-FileHash -Algorithm SHA256 -Path $_.FullName).Hash.ToLowerInvariant(), $_.Name
  }
  Set-Content -Path (Join-Path $Release "SHA256SUMS") -Value $Lines
  # GitHub's latest-release redirect serves the newest release's assets.
  $Latest = New-Item -ItemType Directory -Force -Path (Join-Path $Tmp "latest/download")
  Copy-Item (Join-Path $Release "SHA256SUMS") (Join-Path $Latest "SHA256SUMS")
}
Write-Sums

# The stand-ins below run in install.ps1's scope when it calls them, where `$script:` means that
# script: what they share with this test lives in globals.
$global:LockraTestCalls = [System.Collections.Generic.List[string]]::new()
$global:LockraTestRoot = $Tmp
$global:LockraTestVersion = $Version

function Invoke-RestMethod {
  param($Uri, $Headers, [switch]$UseBasicParsing)
  $global:LockraTestCalls.Add("api $Uri")
  [pscustomobject]@{ tag_name = "v$($global:LockraTestVersion)" }
}

function Invoke-WebRequest {
  param($Uri, $OutFile, [switch]$UseBasicParsing)
  $Path = $Uri -replace '^https://github\.com/sunerpy/lockra/releases/(download/)?', ''
  $Source = Join-Path $global:LockraTestRoot $Path
  $global:LockraTestCalls.Add("get $Path")
  if (-not (Test-Path $Source)) { throw "404 $Uri" }
  if ($OutFile) {
    Copy-Item $Source $OutFile
  } else {
    # An asset's body, as Windows PowerShell 5.1 hands an octet-stream over: bytes.
    [pscustomobject]@{ Content = [System.IO.File]::ReadAllBytes($Source) }
  }
}

function Get-Process {
  param($Name, $ErrorAction)
}

function Get-ItemProperty {
  param($Path, $ErrorAction)
}

function Start-Process {
  param($FilePath, $ArgumentList, [switch]$Wait, [switch]$PassThru)
  $global:LockraTestCalls.Add("start $(Split-Path -Leaf $FilePath) $ArgumentList".Trim())
  if ($FilePath -like "*-setup.exe") {
    # What the per-user NSIS installer leaves behind.
    $Dir = New-Item -ItemType Directory -Force -Path (Join-Path $env:LOCALAPPDATA "Lockra")
    Set-Content -Path (Join-Path $Dir "lockra-desktop.exe") -Value "app"
  }
  [pscustomobject]@{ ExitCode = 0 }
}

$Failures = 0
function Check($Label, $Condition) {
  if (-not $Condition) {
    Write-Host "test-install.ps1: FAIL: $Label" -ForegroundColor Red
    Write-Host ("  calls: " + ($global:LockraTestCalls -join " | "))
    $script:Failures += 1
  }
}

function Install-Lockra($Name, $Arch, $Wow = $null, $LockraVersion = $null) {
  $global:LockraTestCalls.Clear()
  $env:PROCESSOR_ARCHITECTURE = $Arch
  if ($Wow) { $env:PROCESSOR_ARCHITEW6432 = $Wow } else { Remove-Item Env:PROCESSOR_ARCHITEW6432 -ErrorAction SilentlyContinue }
  if ($LockraVersion) { $env:LOCKRA_VERSION = $LockraVersion } else { Remove-Item Env:LOCKRA_VERSION -ErrorAction SilentlyContinue }
  $env:LOCALAPPDATA = Join-Path $Tmp "local-$Name"
  try {
    & $Script *> $null
    return $null
  } catch {
    return $_.Exception.Message
  }
}

try {
  $Error1 = Install-Lockra "x64" "AMD64"
  Check "x64 installs" ($null -eq $Error1)
  Check "x64 finds the latest release from its checksums" ($global:LockraTestCalls -contains "get latest/download/SHA256SUMS")
  Check "x64 asks no rate-limited API" (-not ($global:LockraTestCalls | Where-Object { $_ -like "api *" }))
  Check "x64 downloads its installer" ($global:LockraTestCalls -contains "get v$Version/Lockra_${Version}_x64-setup.exe")
  Check "x64 runs it silently" ($global:LockraTestCalls -contains "start Lockra_${Version}_x64-setup.exe /S")
  Check "x64 starts Lockra" ($global:LockraTestCalls -contains "start lockra-desktop.exe")

  $Error2 = Install-Lockra "arm64" "ARM64" -LockraVersion "v$Version"
  Check "ARM64 installs a given version" ($null -eq $Error2)
  Check "ARM64 takes the ARM64 installer" ($global:LockraTestCalls -contains "start Lockra_${Version}_arm64-setup.exe /S")
  Check "a given version does not look up the latest" (-not ($global:LockraTestCalls | Where-Object { $_ -like "*latest*" }))

  $Error3 = Install-Lockra "wow" "x86" -Wow "ARM64" -LockraVersion $Version
  Check "a 32-bit shell on ARM64 takes the ARM64 installer" (($null -eq $Error3) -and ($global:LockraTestCalls -contains "start Lockra_${Version}_arm64-setup.exe /S"))

  Set-Content -Path (Join-Path $Tmp "latest/download/SHA256SUMS") -Value "not a checksum list"
  $ErrorLatest = Install-Lockra "no-latest" "AMD64"
  Check "a latest release without packages is refused" ($ErrorLatest -like "*could not find the latest release*")
  Write-Sums

  $Error4 = Install-Lockra "x86" "x86"
  Check "32-bit Windows is refused" ($Error4 -like "*64-bit Windows on x64 and ARM64 only*")

  $Error5 = Install-Lockra "bad-version" "AMD64" -LockraVersion "latest"
  Check "a version that is not one is refused" ($Error5 -like "*not a release version*")

  $Error6 = Install-Lockra "no-release" "AMD64" -LockraVersion "9.8.6"
  Check "a release that does not exist is refused" ($Error6 -like "*has no SHA256SUMS*")

  Add-Content -Path (Join-Path $Release "Lockra_${Version}_x64-setup.exe") -Value "tampered"
  $Error7 = Install-Lockra "tampered" "AMD64" -LockraVersion $Version
  Check "a tampered installer is refused" ($Error7 -like "*checksum mismatch*")
  Check "a tampered installer never runs" (-not ($global:LockraTestCalls | Where-Object { $_ -like "start *-setup.exe*" }))
} finally {
  Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue
  Remove-Variable -Scope Global -Name LockraTestCalls, LockraTestRoot, LockraTestVersion -ErrorAction SilentlyContinue
}

if ($Failures -gt 0) { exit 1 }
Write-Host "test-install.ps1: install.ps1 passed on x64, ARM64 and every refusal"
