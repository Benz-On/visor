param(
    [int]$ClickX = -1,
    [int]$ClickY = -1,
    [string]$Name = 'visor-window',
    [switch]$Restart
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms, System.Drawing

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
[StructLayout(LayoutKind.Sequential)]
public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
public static class Win32 {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern int GetWindowTextLength(IntPtr hWnd);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdcBlt, uint nFlags);
  [DllImport("user32.dll")] public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, IntPtr dwExtraInfo);
}
"@

function Get-VisorWindows([uint32]$target) {
  $found = New-Object System.Collections.ArrayList
  $callback = [Win32+EnumWindowsProc] {
    param($h, $unused)
    $pid2 = 0
    [void][Win32]::GetWindowThreadProcessId($h, [ref]$pid2)
    if ($pid2 -eq $target -and [Win32]::IsWindowVisible($h)) {
      $r = New-Object RECT
      [void][Win32]::GetWindowRect($h, [ref]$r)
      $sb = New-Object System.Text.StringBuilder 256
      [void][Win32]::GetWindowText($h, $sb, 256)
      [void]$found.Add([pscustomobject]@{
          Handle = $h; Title = $sb.ToString()
          Width = $r.Right - $r.Left; Height = $r.Bottom - $r.Top
          Left = $r.Left; Top = $r.Top
        })
    }
    return $true
  }
  [void][Win32]::EnumWindows($callback, [IntPtr]::Zero)
  return $found
}

if ($Restart) {
  Get-Process visor -ErrorAction SilentlyContinue | Stop-Process -Force
  Start-Sleep -Seconds 2
  $root = Split-Path -Parent $PSScriptRoot
  Start-Process -FilePath (Join-Path $root 'src-tauri\target\debug\visor.exe') | Out-Null
  Start-Sleep -Seconds 14
}

# A second launch forwards its request and exits without ever showing a window,
# so the process list can hold several entries and only one of them is the UI.
$candidates = Get-Process visor -ErrorAction Stop
$window = $null
foreach ($candidate in $candidates) {
  $found = Get-VisorWindows ([uint32]$candidate.Id) |
    Where-Object { $_.Width -gt 200 -and $_.Height -gt 200 } |
    Sort-Object -Property { $_.Width * $_.Height } -Descending |
    Select-Object -First 1
  if ($found) { $window = $found; break }
}
if (-not $window) { throw "no visible VISOR window among $($candidates.Count) process(es)" }
Write-Output ("target '{0}' {1}x{2} at {3},{4}" -f $window.Title, $window.Width, $window.Height, $window.Left, $window.Top)

[void][Win32]::SetForegroundWindow($window.Handle)
Start-Sleep -Milliseconds 800

if ($ClickX -ge 0 -and $ClickY -ge 0) {
  # WebView2 ignores the click that merely activates the window, so the first
  # press wakes it and the second one reaches the control.
  for ($attempt = 1; $attempt -le 2; $attempt++) {
    [void][Win32]::SetCursorPos($window.Left + $ClickX, $window.Top + $ClickY)
    Start-Sleep -Milliseconds 250
    [Win32]::mouse_event(0x0002, 0, 0, 0, [IntPtr]::Zero)
    [Win32]::mouse_event(0x0004, 0, 0, 0, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 400
  }
  Write-Output ("clicked twice at window-relative {0},{1}" -f $ClickX, $ClickY)
  Start-Sleep -Milliseconds 1400
}

$rect = New-Object RECT
[void][Win32]::GetWindowRect($window.Handle, [ref]$rect)
$width = $rect.Right - $rect.Left
$height = $rect.Bottom - $rect.Top

# PrintWindow renders the window's own surface, so an occluded or background
# VISOR window still captures correctly; a screen grab would photograph whatever
# happens to be on top of it.
$bitmap = New-Object System.Drawing.Bitmap $width, $height
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$deviceContext = $graphics.GetHdc()
[void][Win32]::PrintWindow($window.Handle, $deviceContext, 2)   # PW_RENDERFULLCONTENT
$graphics.ReleaseHdc($deviceContext)
$out = Join-Path $env:TEMP "$Name.png"
$bitmap.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()
Write-Output "saved $out"
