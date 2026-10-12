# Runs elevated. Undoes openrgb-adopt.ps1: removes Lumen's OpenRGB task and
# gives OpenRGB back to its Windows service, re-enabled and started if it is
# installed. Lumen ends the task's process before calling this.
param([string]$Log)
$ErrorActionPreference = 'Stop'
try {
    Unregister-ScheduledTask -TaskName 'LumenOpenRGB' -Confirm:$false -ErrorAction SilentlyContinue
    $svc = Get-Service -Name OpenRGB -ErrorAction SilentlyContinue
    if ($svc) {
        Set-Service -Name OpenRGB -StartupType Automatic
        if ((Get-Service -Name OpenRGB).Status -ne 'Running') { Start-Service -Name OpenRGB }
    }
    exit 0
} catch {
    if ($Log) { "$($_.Exception.Message)" | Out-File -FilePath $Log -Encoding utf8 }
    exit 1
}
