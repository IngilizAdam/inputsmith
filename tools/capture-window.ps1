# Saves a PNG of a process's main window (works even when partially covered).
# usage: powershell -File tools/capture-window.ps1 <process-name> <out.png>
param([string]$ProcessName, [string]$Out)
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class Cap {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint f);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
}
"@
[Cap]::SetProcessDPIAware() | Out-Null
$p = Get-Process $ProcessName | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $p) { Write-Error "no window for $ProcessName"; exit 1 }
$r = New-Object Cap+RECT
[Cap]::GetWindowRect($p.MainWindowHandle, [ref]$r) | Out-Null
$w = $r.R - $r.L; $h = $r.B - $r.T
$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
$dc = $g.GetHdc()
# PW_RENDERFULLCONTENT = 2 captures GPU-rendered content.
[Cap]::PrintWindow($p.MainWindowHandle, $dc, 2) | Out-Null
$g.ReleaseHdc($dc)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
"$w x $h -> $Out"
