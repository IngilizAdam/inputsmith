# Removes the per-user inputsmith installation (programs, Start Menu shortcut,
# start-at-login entry). Settings and backups in %APPDATA%\inputsmith are kept
# unless -RemoveSettings is given.
# usage: powershell -ExecutionPolicy Bypass -File uninstall.ps1 [-RemoveSettings]
param([switch]$RemoveSettings)
$ErrorActionPreference = 'Stop'
$dest = Join-Path $env:LOCALAPPDATA 'Programs\inputsmith'

Get-Process inputsmith-gui, inputsmith-agent -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500

Remove-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name inputsmith -ErrorAction SilentlyContinue
$lnk = Join-Path ([Environment]::GetFolderPath('Programs')) 'inputsmith.lnk'
if (Test-Path $lnk) { Remove-Item -Force $lnk }
if (Test-Path $dest) { Remove-Item -Recurse -Force $dest }
if ($RemoveSettings) {
    Remove-Item -Recurse -Force (Join-Path $env:APPDATA 'inputsmith') -ErrorAction SilentlyContinue
    Remove-Item -Recurse -Force (Join-Path $env:LOCALAPPDATA 'inputsmith') -ErrorAction SilentlyContinue
}
Write-Host 'inputsmith removed.'
