param([string]$Exe = (Join-Path (Split-Path -Parent $PSScriptRoot) 'dist\CodexMeter\CodexMeter.exe'))
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
[StructLayout(LayoutKind.Sequential)] public struct MeterRect { public int Left,Top,Right,Bottom; }
public static class MeterRefreshSmoke {
 [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string cls,string title);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd,uint msg,IntPtr wp,IntPtr lp);
 [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hwnd,out MeterRect rect);
 [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd,IntPtr after,int x,int y,int width,int height,uint flags);
}
'@
if ([MeterRefreshSmoke]::FindWindow('CodexMeterWindow',$null) -ne [IntPtr]::Zero) { throw '请先退出 Codex Meter' }
$cacheFile = Join-Path $env:LOCALAPPDATA 'CodexMeter\snapshot.json'
function Wait-Quota([long]$After) {
    $deadline = [DateTime]::UtcNow.AddSeconds(35)
    do {
        Start-Sleep -Milliseconds 200
        try { $snapshot=Get-Content -LiteralPath $cacheFile -Raw | ConvertFrom-Json } catch { continue }
        if ($snapshot.quota_at -gt $After -and -not $snapshot.error) { return $snapshot }
    } while ([DateTime]::UtcNow -lt $deadline)
    throw ('额度未更新: ' + $snapshot.error)
}
$started = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
$process=Start-Process -FilePath $Exe -ArgumentList '--hidden' -WindowStyle Hidden -PassThru
try {
    $initial=Wait-Quota ($started-1)
    $hwnd=[MeterRefreshSmoke]::FindWindow('CodexMeterWindow',$null)
    if ($hwnd -eq [IntPtr]::Zero) {throw '窗口不存在'}
    # Exercise the mouse route at 125% of logical size; hit testing must use actual client dimensions.
    [MeterRefreshSmoke]::SetWindowPos($hwnd,[IntPtr]::Zero,0,0,425,810,6) | Out-Null
    Start-Sleep -Milliseconds 1200
    $rect=New-Object MeterRect
    [MeterRefreshSmoke]::GetClientRect($hwnd,[ref]$rect) | Out-Null
    $x=[int][Math]::Floor(286.0*$rect.Right/340)
    $y=[int][Math]::Floor(626.0*$rect.Bottom/648)
    $lp=[IntPtr](($y -shl 16) -bor $x)
    [MeterRefreshSmoke]::PostMessage($hwnd,0x202,[IntPtr]::Zero,$lp) | Out-Null
    $updated=Wait-Quota $initial.quota_at
    [ordered]@{InitialRead=$true;MouseRefresh=$true;ScaledHitTest=$true;QuotaTimestampAdvanced=($updated.quota_at -gt $initial.quota_at);TokenRead=($null -eq $updated.usage_error)} | ConvertTo-Json
} finally {
    $hwnd=[MeterRefreshSmoke]::FindWindow('CodexMeterWindow',$null)
    if ($hwnd -ne [IntPtr]::Zero) {[MeterRefreshSmoke]::PostMessage($hwnd,0x111,[IntPtr]6,[IntPtr]::Zero) | Out-Null}
    if (-not $process.WaitForExit(30000)) {throw '读取进程未及时退出'}
}
