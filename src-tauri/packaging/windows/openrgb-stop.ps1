# Runs elevated (after openrgb-common.ps1): ends the given OpenRGB service even
# when it ignores a normal stop request; with -Then start, starts it again.
param(
    [Parameter(Mandatory = $true)][string]$Service,
    [string]$Then = '',
    [string]$Log
)
$ErrorActionPreference = 'Stop'
try {
    Stop-ServiceHard $Service
    if ($Then -eq 'start') { Start-Service -Name $Service }
    exit 0
} catch {
    Write-Failure $Log $_
    exit 1
}
