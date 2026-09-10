# Builds LightCapture-Setup-{version}.exe with Inno Setup 6 (ISCC).
# Manual check after a tag: per-user install, uninstall, all-users install, upgrade while the tray app is running.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string] $AppVersion,
    [string] $ExePath = ''
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = Resolve-Path (Join-Path $PSScriptRoot '..')
Set-Location $root

if ([string]::IsNullOrWhiteSpace($ExePath)) {
    $ExePath = Join-Path $root 'target\release\lightcapture.exe'
}

if (-not (Test-Path -LiteralPath $ExePath)) {
    throw "missing tray exe: $ExePath (build with: cargo build --release --locked -p lightcapture-app)"
}

$iscc = 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe'
if (-not (Test-Path -LiteralPath $iscc)) {
    throw "missing ISCC.exe: $iscc (install Inno Setup 6)"
}

$dist = Join-Path $root 'dist'
New-Item -ItemType Directory -Force -Path $dist | Out-Null
$staged = Join-Path $dist 'LightCapture.exe'
$srcFull = [System.IO.Path]::GetFullPath((Resolve-Path -LiteralPath $ExePath).Path)
$destFull = [System.IO.Path]::GetFullPath($staged)
if (-not $srcFull.Equals($destFull, [StringComparison]::OrdinalIgnoreCase)) {
    Copy-Item -LiteralPath $ExePath -Destination $staged -Force
}

$iss = Join-Path $root 'installer\LightCapture.iss'
& $iscc /Q "/DMyAppVersion=$AppVersion" $iss
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$setupName = "LightCapture-Setup-$AppVersion.exe"
$setup = Join-Path $dist $setupName
if (-not (Test-Path -LiteralPath $setup)) {
    throw "ISCC did not write $setup"
}

$hash = (Get-FileHash -LiteralPath $setup -Algorithm SHA256).Hash.ToLower()
Set-Content -Path "$setup.sha256" -Value "$hash  $setupName" -Encoding ascii
Write-Host "wrote $setup"
Write-Host "wrote $setup.sha256"
