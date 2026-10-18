# Runs elevated (Lumen launches it through a UAC prompt, after
# openrgb-common.ps1). Hands OpenRGB over to Lumen:
#   1. OpenRGB's own Windows service (from its installer), if any, is stopped
#      and disabled, so it no longer holds the devices and the SDK port;
#   2. the scheduled task Lumen 0.2 used, if any, is removed;
#   3. the OpenRGB folder in $Source (Lumen's bundled copy) is mirrored to
#      "Program Files\Lumen\OpenRGB", which only administrators can change;
#   4. a service "LumenOpenRGB" runs that copy as LocalSystem, started on
#      demand, and every interactive user may start and stop it;
#   5. OpenRGB.exe starts with above-normal priority (Image File Execution
#      Options, the only way to give a service one), so its device threads
#      keep the lights in step while games or builds keep every core busy.
# Afterwards a normal-user Lumen starts, stops and restarts OpenRGB without
# further prompts, and OpenRGB keeps the elevation it needs for SMBus devices
# such as motherboards and RAM. In service mode OpenRGB keeps its settings in
# "service_config" next to the executable; the SDK server is pinned to the
# loopback interface there. Re-running the script updates the copy.
param(
    [Parameter(Mandatory = $true)][string]$Source,
    [string]$Log
)
$ErrorActionPreference = 'Stop'
$Name = 'LumenOpenRGB'
try {
    if (-not (Test-Path -LiteralPath (Join-Path $Source 'OpenRGB.exe'))) { throw "OpenRGB.exe not found in $Source" }
    $dest = Join-Path $env:ProgramFiles 'Lumen\OpenRGB'
    $exe = Join-Path $dest 'OpenRGB.exe'

    if (Get-Service -Name OpenRGB -ErrorAction SilentlyContinue) {
        Set-Service -Name OpenRGB -StartupType Disabled
        Stop-ServiceHard 'OpenRGB'
    }
    Remove-LegacyTask

    Stop-ServiceHard $Name
    New-Item -ItemType Directory -Force -Path $dest | Out-Null
    & robocopy.exe $Source $dest /MIR /XD service_config /R:2 /W:1 /NFL /NDL /NJH /NJS /NP | Out-Null
    if ($LASTEXITCODE -ge 8) { throw "Copying OpenRGB to $dest failed (robocopy exit code $LASTEXITCODE)." }
    $config = Join-Path $dest 'service_config'
    New-Item -ItemType Directory -Force -Path $config | Out-Null
    $settings = Join-Path $config 'OpenRGB.json'
    if (-not (Test-Path -LiteralPath $settings)) {
        [System.IO.File]::WriteAllText($settings, '{ "Server": { "default_host": "127.0.0.1", "default_port": 6742 } }')
    }

    $binary = '"' + $exe + '"'
    if (Get-Service -Name $Name -ErrorAction SilentlyContinue) {
        # Through the registry: Windows PowerShell mangles embedded quotes when
        # it hands an argument to sc.exe.
        Set-ItemProperty -LiteralPath "HKLM:\SYSTEM\CurrentControlSet\Services\$Name" -Name ImagePath -Value $binary
        Set-Service -Name $Name -StartupType Manual
    } else {
        New-Service -Name $Name -BinaryPathName $binary -DisplayName 'Lumen OpenRGB' -Description 'OpenRGB SDK server for Lumen; started and stopped together with Lumen.' -StartupType Manual | Out-Null
    }
    # Windows' default service permissions, plus start and stop for interactive users (IU).
    & sc.exe sdset $Name 'D:(A;;CCLCSWRPWPDTLOCRRC;;;SY)(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)(A;;CCLCSWRPWPLOCRRC;;;IU)(A;;CCLCSWLOCRRC;;;SU)' | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Could not set the service's permissions (sc sdset exit code $LASTEXITCODE)." }
    # Bring it back if it crashes.
    & sc.exe failure $Name reset= 3600 actions= restart/5000/restart/10000/restart/30000 | Out-Null
    # 6 = above normal. Applies from the next start; Lumen starts the service right after this.
    $perf = $OpenRgbPerfOptions
    if (-not (Test-Path -LiteralPath $perf)) { New-Item -Path $perf -Force | Out-Null }
    Set-ItemProperty -LiteralPath $perf -Name CpuPriorityClass -Value 6 -Type DWord
    exit 0
} catch {
    Write-Failure $Log $_
    exit 1
}
