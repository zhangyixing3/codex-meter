param([switch]$NoShortcut)
$ErrorActionPreference = 'Stop'
$source = Join-Path $PSScriptRoot 'CodexMeter.exe'
if (-not (Test-Path -LiteralPath $source)) { throw '请在发行版文件夹中运行 Install.ps1' }
$destination = Join-Path $env:LOCALAPPDATA 'Programs\CodexMeter'
New-Item -ItemType Directory -Path $destination -Force | Out-Null
if ([IO.Path]::GetFullPath($source) -ne [IO.Path]::GetFullPath((Join-Path $destination 'CodexMeter.exe'))) {
    Copy-Item -LiteralPath $source -Destination $destination -Force
    foreach ($name in @('Uninstall.ps1', 'Uninstall.cmd', 'LICENSE', 'README.md')) {
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination $destination -Force
    }
}
if (-not $NoShortcut) {
    $shell = New-Object -ComObject WScript.Shell
    $startMenu = Join-Path ([Environment]::GetFolderPath('Programs')) 'Codex Meter.lnk'
    $desktop = Join-Path ([Environment]::GetFolderPath('Desktop')) 'Codex Meter.lnk'
    foreach ($shortcutPath in @($startMenu, $desktop)) {
        $shortcut = $shell.CreateShortcut($shortcutPath)
        $shortcut.TargetPath = Join-Path $destination 'CodexMeter.exe'
        $shortcut.WorkingDirectory = $destination
        $shortcut.Description = 'Codex 额度监测'
        $shortcut.Save()
    }
}
Write-Host ('已安装到 ' + $destination)
Start-Process -FilePath (Join-Path $destination 'CodexMeter.exe') -WindowStyle Hidden
