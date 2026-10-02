param([string]$Exe = (Join-Path (Split-Path -Parent $PSScriptRoot) 'dist\CodexMeter\CodexMeter.exe'))
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
[StructLayout(LayoutKind.Sequential)] public struct MeterSmokeRect { public int Left,Top,Right,Bottom; }
public static class MeterSmoke {
 public delegate bool EnumCallback(IntPtr hwnd,IntPtr data);
 [DllImport("user32.dll")] static extern bool EnumWindows(EnumCallback callback,IntPtr data);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint pid);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr hwnd,System.Text.StringBuilder name,int count);
 public static IntPtr FindOwned(uint pid,string cls){IntPtr found=IntPtr.Zero;EnumWindows((h,l)=>{uint p;GetWindowThreadProcessId(h,out p);if(p==pid){var n=new System.Text.StringBuilder(256);GetClassName(h,n,256);if(n.ToString()==cls){found=h;return false;}}return true;},IntPtr.Zero);return found;}
 [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string cls, string title);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr wp, IntPtr lp);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
 [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hwnd,out MeterSmokeRect rect);
 [DllImport("user32.dll", EntryPoint="GetWindowLongPtrW")] public static extern IntPtr GetStyle(IntPtr hwnd, int index);
 [DllImport("gdi32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr CreateFont(int h, int w, int e, int o, int weight, uint italic, uint underline, uint strike, uint charset, uint output, uint clip, uint quality, uint pitch, string face);
 [DllImport("gdi32.dll")] public static extern IntPtr CreateCompatibleDC(IntPtr dc);
 [DllImport("gdi32.dll")] public static extern IntPtr SelectObject(IntPtr dc, IntPtr obj);
 [DllImport("gdi32.dll", CharSet=CharSet.Unicode)] public static extern int GetTextFace(IntPtr dc, int count, System.Text.StringBuilder face);
 [DllImport("gdi32.dll")] public static extern bool DeleteObject(IntPtr obj);
 [DllImport("gdi32.dll")] public static extern bool DeleteDC(IntPtr dc);
}
'@
if ([MeterSmoke]::FindWindow('CodexMeterWindow', $null) -ne [IntPtr]::Zero) { throw '请先退出正在运行的 Codex Meter，再做 UI smoke test' }
$errorLog=Join-Path ([IO.Path]::GetTempPath()) 'CodexMeter-smoke-error.txt'
$process = Start-Process -FilePath $Exe -ArgumentList '--demo','--hidden' -WindowStyle Hidden -RedirectStandardError $errorLog -PassThru
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        Start-Sleep -Milliseconds 100
        $hwnd = [MeterSmoke]::FindWindow('CodexMeterWindow', $null)
    } while ($hwnd -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
    if ($hwnd -eq [IntPtr]::Zero) { throw '窗口未创建' }
    if ([MeterSmoke]::IsWindowVisible($hwnd)) { throw '--hidden 未隐藏窗口' }
    [MeterSmoke]::PostMessage($hwnd,0x111,[IntPtr]1,[IntPtr]::Zero) | Out-Null
    Start-Sleep -Milliseconds 200
    if (-not [MeterSmoke]::IsWindowVisible($hwnd)) { throw '托盘恢复命令失败' }
    $process.Refresh()
    $visibleWorkingSet = [Math]::Round($process.WorkingSet64/1MB,2)
    $rect=New-Object MeterSmokeRect
    [MeterSmoke]::GetClientRect($hwnd,[ref]$rect) | Out-Null
    $cardX=[int][Math]::Floor(160.0*$rect.Right/340)
    $cardY=[int][Math]::Floor(315.0*$rect.Bottom/648)
    [MeterSmoke]::PostMessage($hwnd,0x202,[IntPtr]::Zero,[IntPtr](($cardY -shl 16) -bor $cardX)) | Out-Null
    $dialogDeadline=[DateTime]::UtcNow.AddSeconds(3)
    do {Start-Sleep -Milliseconds 100;$dialog=[MeterSmoke]::FindOwned([uint32]$process.Id,'CodexMeterResetPanel')} while ($dialog -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $dialogDeadline)
    if ($dialog -eq [IntPtr]::Zero) {throw ('重置卡详情未打开: '+(Get-Content -LiteralPath $errorLog -Raw))}
    [MeterSmoke]::PostMessage($hwnd,0x202,[IntPtr]::Zero,[IntPtr](($cardY -shl 16) -bor $cardX)) | Out-Null
    Start-Sleep -Milliseconds 100
    if ([MeterSmoke]::FindOwned([uint32]$process.Id,'CodexMeterResetPanel') -ne $dialog) {throw '重复打开创建了新详情窗口'}
    $panelRect=New-Object MeterSmokeRect
    [MeterSmoke]::GetClientRect($dialog,[ref]$panelRect) | Out-Null
    $closeX=[int][Math]::Floor(342.0*$panelRect.Right/408)
    $closeY=[int][Math]::Floor(436.0*$panelRect.Bottom/466)
    [MeterSmoke]::PostMessage($dialog,0x202,[IntPtr]::Zero,[IntPtr](($closeY -shl 16) -bor $closeX)) | Out-Null
    Start-Sleep -Milliseconds 100
    if ([MeterSmoke]::FindOwned([uint32]$process.Id,'CodexMeterResetPanel') -ne [IntPtr]::Zero) {throw '详情关闭按钮未关闭窗口'}
    [MeterSmoke]::PostMessage($hwnd,0x111,[IntPtr]3,[IntPtr]::Zero) | Out-Null
    Start-Sleep -Milliseconds 100
    if (([MeterSmoke]::GetStyle($hwnd,-20).ToInt64() -band 8) -eq 0) { throw '置顶失败' }
    [MeterSmoke]::PostMessage($hwnd,0x10,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
    Start-Sleep -Milliseconds 100
    if ([MeterSmoke]::IsWindowVisible($hwnd)) { throw '关闭未收到托盘' }
    $process.Refresh()
    $hiddenWorkingSet = [Math]::Round($process.WorkingSet64/1MB,2)
    $second = Start-Process -FilePath $Exe -ArgumentList '--demo','--hidden' -WindowStyle Hidden -PassThru
    if (-not $second.WaitForExit(3000)) { throw '单实例检测失败' }
    $process.Refresh()
    $report = [ordered]@{
        HiddenStart = $true
        Restore = $true
        ResetDetailsDialog = $true
        ResetDetailsReuse = $true
        ResetDetailsCloseButton = $true
        AlwaysOnTop = $true
        CloseToTray = $true
        SingleInstance = $true
        ExeBytes = (Get-Item -LiteralPath $Exe).Length
        VisibleWorkingSetMB = $visibleWorkingSet
        HiddenWorkingSetMB = $hiddenWorkingSet
        PrivateMemoryMB = [Math]::Round($process.PrivateMemorySize64/1MB,2)
    }
    $dc = [MeterSmoke]::CreateCompatibleDC([IntPtr]::Zero)
    $font = [MeterSmoke]::CreateFont(-29,0,0,0,600,0,0,0,1,0,0,5,0,'Segoe UI')
    $old = [MeterSmoke]::SelectObject($dc,$font)
    $face = New-Object Text.StringBuilder 128
    [MeterSmoke]::GetTextFace($dc,128,$face) | Out-Null
    $report.FontFace = $face.ToString()
    [MeterSmoke]::SelectObject($dc,$old) | Out-Null
    [MeterSmoke]::DeleteObject($font) | Out-Null
    [MeterSmoke]::DeleteDC($dc) | Out-Null
    [MeterSmoke]::PostMessage($hwnd,0x111,[IntPtr]6,[IntPtr]::Zero) | Out-Null
    if (-not $process.WaitForExit(3000)) { throw '正常退出失败' }
    $report.CleanExit = $true
    $report | ConvertTo-Json
} finally {
    if (-not $process.HasExited) { [MeterSmoke]::PostMessage($hwnd,0x111,[IntPtr]6,[IntPtr]::Zero) | Out-Null }
}
