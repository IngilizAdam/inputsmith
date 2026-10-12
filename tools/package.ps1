# Builds release binaries and packs dist\inputsmith-<version>-windows-x64.zip with
# install.ps1 / uninstall.ps1 (per-user install, no administrator rights needed).
# With -Msi it also builds dist\inputsmith-<version>-windows-x64.msi; that needs the WiX Toolset v5:
#   dotnet tool install --global wix --version 5.0.2
#   wix extension add --global WixToolset.Util.wixext/5.0.2
# usage: powershell -ExecutionPolicy Bypass -File tools\package.ps1 [-Msi]
param([switch]$Msi)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

cargo build --release --locked -p inputsmith-agent -p inputsmith-gui -p inputsmith-cli
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
$name = "inputsmith-$version-windows-x64"
$stage = Join-Path $root "dist\$name"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force $stage | Out-Null

foreach ($exe in 'inputsmith-gui.exe', 'inputsmith-agent.exe', 'inputsmith.exe') {
    Copy-Item (Join-Path $root "target\release\$exe") $stage
}
Copy-Item README.md, LICENSE, tools\installer\install.ps1, tools\installer\uninstall.ps1 $stage

$zip = Join-Path $root "dist\$name.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path "$stage\*" -DestinationPath $zip
Write-Host "created $zip"

if ($Msi) {
    # MSI versions are numeric only: 0.2.0-rc.1 becomes 0.2.0.
    $msiVersion = ($version -split '[-+]')[0]
    $out = Join-Path $root "dist\$name.msi"
    wix build tools\installer\inputsmith.wxs -arch x64 -ext WixToolset.Util.wixext `
        -d "Version=$msiVersion" -d "BinDir=$stage" -o $out
    if ($LASTEXITCODE -ne 0) { throw 'wix build failed' }
    Remove-Item -Force -ErrorAction SilentlyContinue ([IO.Path]::ChangeExtension($out, '.wixpdb'))
    Write-Host "created $out"
}
