# Installs the native prerequisites Lumen needs on Windows. Must run elevated.
#   OpenRGB 1.0 (the device backend)  +  VS 2022 Build Tools, C++ workload (Rust's MSVC linker)
param([switch]$SkipOpenRGB, [switch]$SkipBuildTools)
$ErrorActionPreference = 'Continue'
$log = Join-Path $PSScriptRoot 'setup-windows.log'
function Log($m) { "[{0:HH:mm:ss}] {1}" -f (Get-Date), $m | Tee-Object -FilePath $log -Append }

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
Log "setup start (elevated: $isAdmin)"
if (-not $isAdmin) { Log 'not elevated - aborting'; exit 1 }

if (-not $SkipOpenRGB) {
    if (Test-Path 'C:\Program Files\OpenRGB\OpenRGB.exe') {
        Log 'OpenRGB already installed'
    } else {
        $msi = Get-ChildItem "$env:USERPROFILE\Downloads\OpenRGB_*_Windows_64_*.msi" -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if ($msi) {
            Log "installing OpenRGB from $($msi.Name)"
            $p = Start-Process msiexec.exe -ArgumentList "/i `"$($msi.FullName)`" /qn /norestart" -Wait -PassThru
            Log "msiexec exit $($p.ExitCode)"
        } else {
            Log 'installing OpenRGB via winget'
            winget install -e --id OpenRGB.OpenRGB --silent --accept-source-agreements --accept-package-agreements --disable-interactivity | Out-Null
            Log "winget exit $LASTEXITCODE"
        }
    }
}

if (-not $SkipBuildTools) {
    if (Test-Path 'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC') {
        Log 'VS Build Tools already installed'
    } else {
        Log 'installing VS 2022 Build Tools (C++ workload); downloads ~3 GB, takes a while'
        winget install -e --id Microsoft.VisualStudio.2022.BuildTools --silent --accept-source-agreements --accept-package-agreements --disable-interactivity --override '--quiet --wait --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended' | Out-Null
        Log "winget exit $LASTEXITCODE"
    }
}
Log 'setup done'
