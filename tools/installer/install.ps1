# Installs inputsmith for the current user: copies the programs to
# %LOCALAPPDATA%\Programs\inputsmith, adds a Start Menu shortcut and starts it.
# Settings (%APPDATA%\inputsmith) are left untouched. "Start at login" is a
# switch in the app (Application Settings).
# usage: powershell -ExecutionPolicy Bypass -File install.ps1
$ErrorActionPreference = 'Stop'
$src = $PSScriptRoot
$dest = Join-Path $env:LOCALAPPDATA 'Programs\inputsmith'

# Running copies would keep the files locked.
Get-Process inputsmith-gui, inputsmith-agent -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500

New-Item -ItemType Directory -Force $dest | Out-Null
foreach ($f in 'inputsmith-gui.exe', 'inputsmith-agent.exe', 'inputsmith.exe', 'README.md', 'uninstall.ps1') {
    Copy-Item (Join-Path $src $f) $dest -Force
}

$programs = [Environment]::GetFolderPath('Programs')
$shell = New-Object -ComObject WScript.Shell
$lnk = $shell.CreateShortcut((Join-Path $programs 'inputsmith.lnk'))
$lnk.TargetPath = Join-Path $dest 'inputsmith-gui.exe'
$lnk.WorkingDirectory = $dest
$lnk.Description = 'Settings for Logitech mice and keyboards'
$lnk.Save()

# The "Start at login" entry, if enabled, must point at the installed copy.
$run = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
if (Get-ItemProperty -Path $run -Name inputsmith -ErrorAction SilentlyContinue) {
    Set-ItemProperty -Path $run -Name inputsmith -Value ('"{0}" --hidden' -f (Join-Path $dest 'inputsmith-gui.exe'))
}

Write-Host "Installed to $dest"
Write-Host 'Note: Logi Options+ must not run at the same time (both would configure the devices).'
Start-Process (Join-Path $dest 'inputsmith-gui.exe')
