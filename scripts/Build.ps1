param([string]$MingwBin, [string]$TargetDir = (Join-Path $env:LOCALAPPDATA 'CodexMeterBuild'))
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
# Older GNU linkers cannot resolve Chinese source paths. Keep build output in an ASCII path.
$env:CARGO_TARGET_DIR = $TargetDir
if (-not $MingwBin) {
    $candidate = Join-Path $env:USERPROFILE 'miniconda3\envs\r_4.3\Library\bin'
    if (Test-Path -LiteralPath (Join-Path $candidate 'x86_64-w64-mingw32-gcc.exe')) { $MingwBin = $candidate }
}
if ($MingwBin -and ((rustc -vV | Out-String) -match 'host: x86_64-pc-windows-gnu')) {
    $env:PATH = (Join-Path (Split-Path -Parent $MingwBin) 'x86_64-w64-mingw32\bin') + ';' + $MingwBin + ';' + $env:PATH
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = Join-Path $MingwBin 'x86_64-w64-mingw32-gcc.exe'
}
Push-Location $root
try {
    cargo test --locked
    if ($LASTEXITCODE -ne 0) { throw '测试失败' }
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw '编译失败' }
    & (Join-Path $PSScriptRoot 'Package.ps1') -TargetDir $TargetDir
} finally { Pop-Location }
