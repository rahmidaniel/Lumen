# Measures Lumen's footprint: CPU and memory of lumen.exe plus its WebView2
# child processes, with the window open and after closing it to the tray.
#
#   powershell -ExecutionPolicy Bypass -File scripts\measure-footprint.ps1 [-Exe path] [-Seconds 10] [-KeepOpen]
param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\release\lumen.exe'),
    [int]$Seconds = 10,
    [switch]$KeepOpen
)
$ErrorActionPreference = 'Continue'

function Get-Tree($root) {
    $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name, CommandLine
    $ids = @($root); $queue = @($root)
    while ($queue.Count -gt 0) {
        $next = @()
        foreach ($id in $queue) {
            foreach ($c in ($all | Where-Object { $_.ParentProcessId -eq $id })) { $ids += $c.ProcessId; $next += $c.ProcessId }
        }
        $queue = $next
    }
    return $ids
}

function Get-Role($id) {
    $cl = (Get-CimInstance Win32_Process -Filter "ProcessId=$id").CommandLine
    if (-not $cl) { return '?' }
    if ($cl -match '--type=([a-z-]+)') { return $Matches[1] }
    if ($cl -match 'msedgewebview2') { return 'browser' }
    return 'app'
}

function Measure-Tree($label, $ids) {
    $before = @{}
    foreach ($id in $ids) { $p = Get-Process -Id $id -ErrorAction SilentlyContinue; if ($p) { $before[$id] = $p.TotalProcessorTime.TotalSeconds } }
    Start-Sleep $Seconds
    $rows = @(); $cpu = 0.0; $ws = 0; $priv = 0
    foreach ($id in $ids) {
        $p = Get-Process -Id $id -ErrorAction SilentlyContinue
        if (-not $p -or -not $before.ContainsKey($id)) { continue }
        $pc = ($p.TotalProcessorTime.TotalSeconds - $before[$id]) / $Seconds * 100
        $cpu += $pc; $ws += $p.WorkingSet64; $priv += $p.PrivateMemorySize64
        $rows += [pscustomobject]@{ pid = $id; role = (Get-Role $id); cpu_pct = [math]::Round($pc, 1); ws_mb = [math]::Round($p.WorkingSet64 / 1MB); priv_mb = [math]::Round($p.PrivateMemorySize64 / 1MB) }
    }
    "`n== $label =="
    $rows | Sort-Object cpu_pct -Descending | Format-Table -AutoSize | Out-String -Width 120
    "total: CPU {0:N1}% of one core · working set {1:N0} MB · private {2:N0} MB" -f $cpu, ($ws / 1MB), ($priv / 1MB)
}

$existing = Get-Process lumen -ErrorAction SilentlyContinue | Select-Object -First 1
if ($existing) {
    "attaching to running lumen.exe (pid $($existing.Id))"
    $root = $existing.Id
} else {
    $env:RUST_LOG = 'info'
    $p = Start-Process -FilePath $Exe -WorkingDirectory (Split-Path $Exe) -PassThru
    "started $Exe (pid $($p.Id)); waiting for it to settle"
    Start-Sleep 10
    $root = $p.Id
}

Measure-Tree 'window open' (Get-Tree $root)

if (-not $KeepOpen) {
    $null = (Get-Process -Id $root).CloseMainWindow()
    Start-Sleep 6
    Measure-Tree 'window closed (tray)' (Get-Tree $root)
    "lumen alive: " + [bool](Get-Process -Id $root -ErrorAction SilentlyContinue)
}
