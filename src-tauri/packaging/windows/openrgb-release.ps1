# Runs elevated (after openrgb-common.ps1). Undoes openrgb-adopt.ps1: removes
# Lumen's OpenRGB service, its copy in "Program Files\Lumen\OpenRGB" and any
# Lumen 0.2 task, and gives OpenRGB back to its own Windows service,
# re-enabled and started, if that is installed. Without it, Lumen goes back to
# running its bundled OpenRGB as a normal process.
param([string]$Log)
$ErrorActionPreference = 'Stop'
$Name = 'LumenOpenRGB'
try {
    if (Get-Service -Name $Name -ErrorAction SilentlyContinue) {
        Stop-ServiceHard $Name
        & sc.exe delete $Name | Out-Null
    }
    Remove-LegacyTask

    $dest = Join-Path $env:ProgramFiles 'Lumen\OpenRGB'
    for ($i = 0; (Test-Path -LiteralPath $dest) -and $i -lt 10; $i++) {
        # The process may hold its files for a moment after stopping.
        try { Remove-Item -LiteralPath $dest -Recurse -Force } catch { Start-Sleep -Milliseconds 500 }
    }
    if (Test-Path -LiteralPath $dest) { throw "Could not remove $dest." }
    $parent = Split-Path -Parent $dest
    if ((Test-Path -LiteralPath $parent) -and -not (Get-ChildItem -LiteralPath $parent -Force)) {
        Remove-Item -LiteralPath $parent -Force
    }

    # The priority the adopt step gave OpenRGB.exe; keys left empty go too.
    $perf = $OpenRgbPerfOptions
    if (Test-Path -LiteralPath $perf) {
        Remove-ItemProperty -LiteralPath $perf -Name CpuPriorityClass -ErrorAction SilentlyContinue
        foreach ($key in @($perf, (Split-Path -Parent $perf))) {
            $item = Get-Item -LiteralPath $key -ErrorAction SilentlyContinue
            if ($item -and $item.SubKeyCount -eq 0 -and $item.ValueCount -eq 0) { Remove-Item -LiteralPath $key -Force }
        }
    }

    if (Get-Service -Name OpenRGB -ErrorAction SilentlyContinue) {
        Set-Service -Name OpenRGB -StartupType Automatic
        if ((Get-Service -Name OpenRGB).Status -ne 'Running') { Start-Service -Name OpenRGB }
    }
    exit 0
} catch {
    Write-Failure $Log $_
    exit 1
}
