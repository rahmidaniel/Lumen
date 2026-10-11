# Downloads the official OpenRGB portable build and unpacks it into
# src-tauri/resources/openrgb so the Lumen installer can ship it.
#
# OpenRGB is GPL-2.0-or-later. Lumen talks to it over TCP as a separate
# program; bundling the binary is "mere aggregation", but the licence text and
# a pointer to the exact source must travel with it — see THIRD_PARTY_NOTICES.md.
#
#   powershell -ExecutionPolicy Bypass -File scripts\fetch-openrgb.ps1 [-Force]
param([switch]$Force)
$ErrorActionPreference = 'Stop'

$Version = '1.0'
$Commit  = '81bbe18'
$Url     = "https://codeberg.org/OpenRGB/OpenRGB/releases/download/release_$Version/OpenRGB_${Version}_Windows_64_$Commit.zip"
# SHA-256 of the official zip (verified 2026-10-11).
$Sha256  = '182a52a3c97c4c4ae52c80286b4260c9666c51c3447dbc416ee8945f66192e90'

$root  = Split-Path -Parent $PSScriptRoot
$dest  = Join-Path $root 'src-tauri\resources\openrgb'
$stamp = Join-Path $dest 'LUMEN_BUNDLE.txt'

if ((Test-Path (Join-Path $dest 'OpenRGB.exe')) -and -not $Force) {
    Write-Host "OpenRGB $Version already present in $dest (use -Force to refetch)"
    exit 0
}

$tmp = Join-Path $env:TEMP "openrgb-$Version-$Commit.zip"
Write-Host "Downloading $Url"
Invoke-WebRequest -Uri $Url -OutFile $tmp -UseBasicParsing

$hash = (Get-FileHash $tmp -Algorithm SHA256).Hash.ToLower()
if ($Sha256 -and $hash -ne $Sha256.ToLower()) {
    Remove-Item $tmp -Force
    throw "SHA-256 mismatch: got $hash, expected $Sha256"
}
Write-Host "SHA-256 $hash"

if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }
New-Item -ItemType Directory -Force $dest | Out-Null
Expand-Archive -Path $tmp -DestinationPath $dest -Force

# The zip may wrap everything in one top-level folder; flatten it.
$inner = Get-ChildItem $dest -Directory | Where-Object { Test-Path (Join-Path $_.FullName 'OpenRGB.exe') } | Select-Object -First 1
if ($inner -and -not (Test-Path (Join-Path $dest 'OpenRGB.exe'))) {
    Get-ChildItem $inner.FullName -Force | Move-Item -Destination $dest -Force
    Remove-Item $inner.FullName -Recurse -Force
}
if (-not (Test-Path (Join-Path $dest 'OpenRGB.exe'))) { throw "OpenRGB.exe not found after extraction" }

# GPL-2.0 text must travel with the binary.
if (-not (Test-Path (Join-Path $dest 'LICENSE'))) {
    Invoke-WebRequest -Uri 'https://www.gnu.org/licenses/old-licenses/gpl-2.0.txt' -OutFile (Join-Path $dest 'LICENSE') -UseBasicParsing
}

@"
OpenRGB $Version (commit $Commit), bundled unmodified from
$Url
SHA-256: $hash
Licence: GPL-2.0-or-later — see LICENSE in this folder. Source: https://codeberg.org/OpenRGB/OpenRGB/src/tag/release_$Version
"@ | Set-Content -Path $stamp -Encoding utf8

$size = (Get-ChildItem $dest -Recurse -File | Measure-Object Length -Sum).Sum / 1MB
Write-Host ("OpenRGB unpacked to {0} ({1:N1} MB)" -f $dest, $size)
Remove-Item $tmp -Force
