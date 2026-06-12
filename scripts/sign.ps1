# Sign localrecord.exe with an Authenticode certificate.
#
# Prerequisites:
#   - Windows SDK (signtool.exe)
#   - A code signing certificate (.pfx) from a trusted CA, or your dev cert
#
# Usage:
#   .\scripts\sign.ps1 -ExePath "target\release\localrecord.exe" -PfxPath "C:\certs\codesign.pfx" -PfxPassword "secret"
#
# Optional timestamp (recommended for long-lived trust):
#   -TimestampUrl "http://timestamp.digicert.com"

param(
    [Parameter(Mandatory = $true)]
    [string]$ExePath,

    [Parameter(Mandatory = $true)]
    [string]$PfxPath,

    [Parameter(Mandatory = $true)]
    [string]$PfxPassword,

    [string]$TimestampUrl = "http://timestamp.digicert.com"
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $ExePath)) {
    throw "Executable not found: $ExePath"
}

if (-not (Test-Path $PfxPath)) {
    throw "Certificate not found: $PfxPath"
}

$signtool = @(
    "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe",
    "${env:ProgramFiles}\Windows Kits\10\bin\*\x64\signtool.exe"
) | ForEach-Object { Get-Item $_ -ErrorAction SilentlyContinue } |
    Sort-Object FullName -Descending |
    Select-Object -First 1

if (-not $signtool) {
    throw "signtool.exe not found. Install the Windows SDK."
}

Write-Host "Signing with $($signtool.FullName)"

& $signtool.FullName sign `
    /fd SHA256 `
    /f $PfxPath `
    /p $PfxPassword `
    /tr $TimestampUrl `
    /td SHA256 `
    $ExePath

& $signtool.FullName verify /pa $ExePath
Write-Host "Signed successfully: $ExePath"
