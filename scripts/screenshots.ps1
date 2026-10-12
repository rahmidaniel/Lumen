# Captures README screenshots from the running Lumen window (Windows).
#
#   powershell -ExecutionPolicy Bypass -File scripts\screenshots.ps1
#
# Drives the UI by posting mouse messages to the WebView (the cursor never
# moves) and grabs the client area with PrintWindow, so the window may sit
# behind other windows. Coordinates assume the default 1280x800 client area.
# Writes docs/screenshots/{devices,effects,paint,settings}.png.
$ErrorActionPreference = 'Stop'
$code = @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public class LumenShot {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hWnd, ref POINT pt);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr hWnd, EnumProc cb, IntPtr lParam);
  [DllImport("user32.dll")] public static extern int GetClassName(IntPtr hWnd, StringBuilder name, int max);
  public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  public static IntPtr FindChildByClass(IntPtr parent, string cls) {
    IntPtr found = IntPtr.Zero;
    EnumChildWindows(parent, (h, l) => {
      var sb = new StringBuilder(256);
      GetClassName(h, sb, 256);
      if (sb.ToString() == cls) { found = h; return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }
}
'@
Add-Type -TypeDefinition $code
Add-Type -AssemblyName System.Drawing

$proc = Get-Process lumen -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $proc) { throw 'Lumen window not found (is the app running with its window open?)' }
$hwnd = $proc.MainWindowHandle
$target = [LumenShot]::FindChildByClass($hwnd, 'Chrome_RenderWidgetHostHWND')
if ($target -eq [IntPtr]::Zero) { $target = $hwnd }

$outDir = Join-Path (Split-Path -Parent $PSScriptRoot) 'docs\screenshots'
New-Item -ItemType Directory -Force $outDir | Out-Null

function Click([int]$x, [int]$y) {
  $lp = [IntPtr](($y -shl 16) -bor ($x -band 0xFFFF))
  [LumenShot]::PostMessage($target, 0x0200, [IntPtr]0, $lp) | Out-Null       # WM_MOUSEMOVE
  Start-Sleep -Milliseconds 40
  [LumenShot]::PostMessage($target, 0x0201, [IntPtr]1, $lp) | Out-Null       # WM_LBUTTONDOWN, MK_LBUTTON
  Start-Sleep -Milliseconds 60
  [LumenShot]::PostMessage($target, 0x0202, [IntPtr]0, $lp) | Out-Null       # WM_LBUTTONUP
  Start-Sleep -Milliseconds 120
}
function Key([int]$vk) {
  [LumenShot]::PostMessage($target, 0x0100, [IntPtr]$vk, [IntPtr]0) | Out-Null  # WM_KEYDOWN
  Start-Sleep -Milliseconds 40
  [LumenShot]::PostMessage($target, 0x0101, [IntPtr]$vk, [IntPtr]0) | Out-Null  # WM_KEYUP
  Start-Sleep -Milliseconds 120
}
function Shot([string]$name) {
  $wr = New-Object LumenShot+RECT; [LumenShot]::GetWindowRect($hwnd, [ref]$wr) | Out-Null
  $cr = New-Object LumenShot+RECT; [LumenShot]::GetClientRect($hwnd, [ref]$cr) | Out-Null
  $origin = New-Object LumenShot+POINT; [LumenShot]::ClientToScreen($hwnd, [ref]$origin) | Out-Null
  $w = $wr.Right - $wr.Left; $h = $wr.Bottom - $wr.Top
  $bmp = New-Object System.Drawing.Bitmap $w, $h
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $hdc = $g.GetHdc()
  [LumenShot]::PrintWindow($hwnd, $hdc, 2) | Out-Null
  $g.ReleaseHdc($hdc); $g.Dispose()
  $crop = New-Object System.Drawing.Rectangle ($origin.X - $wr.Left), ($origin.Y - $wr.Top), ($cr.Right - $cr.Left), ($cr.Bottom - $cr.Top)
  $client = $bmp.Clone($crop, $bmp.PixelFormat)
  $path = Join-Path $outDir "$name.png"
  $client.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $client.Dispose(); $bmp.Dispose()
  "saved $path ($($crop.Width)x$($crop.Height))"
}

# Rail button centres: Effects (29,86), Devices (29,134), Profiles (29,182), Settings (29,230).
Click 29 134; Start-Sleep -Milliseconds 400; Click 820 700; Start-Sleep -Milliseconds 700; Shot 'devices'

Click 29 86; Start-Sleep -Milliseconds 900; Shot 'effects'

# Paint layer: Add layer -> Paint keys, then a few keys on the keyboard.
Click 1218 76; Start-Sleep -Milliseconds 500
Click 1004 253; Start-Sleep -Milliseconds 700
foreach ($pt in @(@(318,410),@(332,410),@(346,410),@(360,410),@(374,410),@(330,437),@(344,437),@(358,437))) { Click $pt[0] $pt[1] }
Start-Sleep -Milliseconds 900; Shot 'paint'
# Leave paint mode and remove the demo layer (it is the top entry in the list).
Key 0x1B; Start-Sleep -Milliseconds 300
Click 1241 125; Start-Sleep -Milliseconds 500

Click 29 230; Start-Sleep -Milliseconds 900; Shot 'settings'

Click 29 134; Start-Sleep -Milliseconds 300
