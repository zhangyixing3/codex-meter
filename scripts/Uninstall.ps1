param([switch]$RemoveData)
$ErrorActionPreference = 'Stop'
$destination = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'Programs\CodexMeter'))
$expected = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'Programs'))
if ([IO.Path]::GetDirectoryName($destination) -ne $expected -or [IO.Path]::GetFileName($destination) -ne 'CodexMeter') { throw '安装路径校验失败' }
Get-Process -Name 'CodexMeter' -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq (Join-Path $destination 'CodexMeter.exe') } | Stop-Process
Remove-ItemProperty -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name 'CodexMeter' -ErrorAction SilentlyContinue
foreach ($shortcutPath in @((Join-Path ([Environment]::GetFolderPath('Programs')) 'Codex Meter.lnk'), (Join-Path ([Environment]::GetFolderPath('Desktop')) 'Codex Meter.lnk'))) {
    if (Test-Path -LiteralPath $shortcutPath) { Remove-Item -LiteralPath $shortcutPath }
}
if (Test-Path -LiteralPath $destination) { Remove-Item -LiteralPath $destination -Recurse -Force }
if ($RemoveData) {
    $dataDir = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'CodexMeter'))
    if ([IO.Path]::GetDirectoryName($dataDir) -ne [IO.Path]::GetFullPath($env:LOCALAPPDATA) -or [IO.Path]::GetFileName($dataDir) -ne 'CodexMeter') { throw '缓存路径校验失败' }
    if (Test-Path -LiteralPath $dataDir) { Remove-Item -LiteralPath $dataDir -Recurse -Force }
}
Write-Host 'Codex Meter 已卸载'
