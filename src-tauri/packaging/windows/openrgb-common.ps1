# Helpers shared by Lumen's elevated OpenRGB scripts. Lumen dot-sources this
# before running one of them, all inside a single elevated PowerShell.

# Where Windows keeps the start-up priority for every OpenRGB.exe (Lumen reads
# it back to tell whether its service is set up the current way).
$OpenRgbPerfOptions = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\OpenRGB.exe\PerfOptions'

# Stop a service and wait for it; end its process when it ignores the request
# (OpenRGB's server has been seen wedging in STOP_PENDING).
function Stop-ServiceHard([string]$ServiceName) {
    $s = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
    if (-not $s -or $s.Status -eq 'Stopped') { return }
    & sc.exe stop $ServiceName | Out-Null
    $deadline = (Get-Date).AddSeconds(8)
    while ((Get-Service -Name $ServiceName).Status -ne 'Stopped' -and (Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 250
    }
    if ((Get-Service -Name $ServiceName).Status -ne 'Stopped') {
        & taskkill.exe /F /FI "SERVICES eq $ServiceName" | Out-Null
        Start-Sleep -Milliseconds 500
    }
}

# Lumen 0.2 ran OpenRGB through this scheduled task, from a folder the user's
# programs can change. End it and remove it.
function Remove-LegacyTask {
    if (Get-ScheduledTask -TaskName 'LumenOpenRGB' -ErrorAction SilentlyContinue) {
        Stop-ScheduledTask -TaskName 'LumenOpenRGB' -ErrorAction SilentlyContinue
        Unregister-ScheduledTask -TaskName 'LumenOpenRGB' -Confirm:$false
    }
}

function Write-Failure([string]$Log, $ErrorRecord) {
    if ($Log) { "$($ErrorRecord.Exception.Message)" | Out-File -FilePath $Log -Encoding utf8 }
}
