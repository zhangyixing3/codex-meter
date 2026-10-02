param([string]$TargetDir = $env:CARGO_TARGET_DIR)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if (-not $TargetDir) { $TargetDir = Join-Path $root 'target' }
$source = Join-Path $TargetDir 'release\codex-meter.exe'
if (-not (Test-Path -LiteralPath $source)) { throw '请先执行 cargo build --release --locked' }
$destination = Join-Path $root 'dist\CodexMeter'
New-Item -ItemType Directory -Path $destination -Force | Out-Null
Copy-Item -LiteralPath $source -Destination (Join-Path $destination 'CodexMeter.exe') -Force
foreach ($name in @('Install.ps1','Uninstall.ps1','Install.cmd','Uninstall.cmd')) { Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination $destination -Force }
Copy-Item -LiteralPath (Join-Path $root 'README.md') -Destination $destination -Force
Copy-Item -LiteralPath (Join-Path $root 'VERIFICATION.md') -Destination $destination -Force
Copy-Item -LiteralPath (Join-Path $root 'LICENSE') -Destination $destination -Force
$zip = Join-Path $root 'dist\CodexMeter-Windows-x64.zip'
Compress-Archive -LiteralPath $destination -DestinationPath $zip -Force
Get-FileHash -LiteralPath (Join-Path $destination 'CodexMeter.exe') -Algorithm SHA256
Write-Host ('发行包：' + $zip)
