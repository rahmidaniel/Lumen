# Runs elevated (Lumen launches it through a UAC prompt). Hands OpenRGB over
# to Lumen:
#   1. the OpenRGB Windows service is stopped and disabled, so it no longer
#      holds the devices and the SDK port;
#   2. a Task Scheduler task running the given OpenRGB.exe "with highest
#      privileges" is registered for the given user.
# Afterwards a normal-user Lumen can start, end and restart that task without
# further prompts, and OpenRGB keeps the elevation it needs for I2C/SMBus
# devices such as graphics cards and motherboards.
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$User,
    [string]$Log
)
$ErrorActionPreference = 'Stop'
$TaskName = 'LumenOpenRGB'
try {
    if (-not (Test-Path -LiteralPath $Exe)) { throw "OpenRGB executable not found: $Exe" }

    $svc = Get-Service -Name OpenRGB -ErrorAction SilentlyContinue
    if ($svc) {
        Set-Service -Name OpenRGB -StartupType Disabled
        if ($svc.Status -ne 'Stopped') {
            & sc.exe stop OpenRGB | Out-Null
            $deadline = (Get-Date).AddSeconds(8)
            while ((Get-Service -Name OpenRGB).Status -ne 'Stopped' -and (Get-Date) -lt $deadline) {
                Start-Sleep -Milliseconds 250
            }
            if ((Get-Service -Name OpenRGB).Status -ne 'Stopped') {
                # A wedged service ignores the stop request; end its process instead.
                & taskkill.exe /F /FI "SERVICES eq OpenRGB" | Out-Null
            }
        }
    }

    $action = New-ScheduledTaskAction -Execute $Exe -Argument '--server --server-host 127.0.0.1 --noautoconnect' -WorkingDirectory (Split-Path -Parent $Exe)
    $principal = New-ScheduledTaskPrincipal -UserId $User -LogonType Interactive -RunLevel Highest
    # PT0S = no execution time limit (the default would end the task after three days).
    $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew
    Register-ScheduledTask -TaskName $TaskName -Action $action -Principal $principal -Settings $settings -Description 'OpenRGB SDK server managed by Lumen: started and stopped together with Lumen.' -Force | Out-Null
    exit 0
} catch {
    if ($Log) { "$($_.Exception.Message)" | Out-File -FilePath $Log -Encoding utf8 }
    exit 1
}
