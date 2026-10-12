# Builds release binaries and packs them into dist\inputsmith-<version>-windows-x64.msi
# (per-user install, no administrator rights needed). Needs the WiX Toolset v5:
#   dotnet tool install --global wix --version 5.0.2
#   wix extension add --global WixToolset.Util.wixext/5.0.2
# usage: powershell -ExecutionPolicy Bypass -File tools\package.ps1
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

cargo build --release --locked -p inputsmith-agent -p inputsmith-gui -p inputsmith-cli
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
# MSI versions are numeric only: 0.2.0-rc.1 becomes 0.2.0.
$msiVersion = ($version -split '[-+]')[0]
New-Item -ItemType Directory -Force (Join-Path $root dist) | Out-Null
$out = Join-Path $root "dist\inputsmith-$version-windows-x64.msi"

wix build tools\installer\inputsmith.wxs -arch x64 -ext WixToolset.Util.wixext `
    -d "Version=$msiVersion" -d "BinDir=$(Join-Path $root 'target\release')" -d "DocDir=$root" -o $out
if ($LASTEXITCODE -ne 0) { throw 'wix build failed' }
Remove-Item -Force -ErrorAction SilentlyContinue ([IO.Path]::ChangeExtension($out, '.wixpdb'))
Write-Host "created $out"
