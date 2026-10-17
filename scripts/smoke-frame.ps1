param(
    [string]$Binary = "$PSScriptRoot/../target/debug/wtm-ui.exe",
    [string]$Output = "$env:LOCALAPPDATA/Temp/opencode/wtm-frame-$([Guid]::NewGuid().ToString('N'))"
)

$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path $Binary).Path
New-Item -ItemType Directory -Path $Output -Force | Out-Null
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition (@'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class WtmFrameProbe {
    public struct Rect { public int Left, Top, Right, Bottom; }
    public struct Point { public int X, Y; }
    public struct Message { public IntPtr Window; public uint Id; public UIntPtr WParam; public IntPtr LParam; public uint Time; public Point Position; public uint Private; }
    public delegate bool EnumProc(IntPtr handle, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr handle, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr handle, StringBuilder text, int count);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr handle);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr handle, out Rect rect);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr handle, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr handle);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr CreateWindowExW(uint extended, string name, string title, uint style, int x, int y, int width, int height, IntPtr parent, IntPtr menu, IntPtr instance, IntPtr data);
    [DllImport("user32.dll")] public static extern bool DestroyWindow(IntPtr handle);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out Point point);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr handle, uint flags);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint x, uint y, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern bool PeekMessageW(out Message message, IntPtr window, uint first, uint last, uint remove);
    [DllImport("user32.dll")] public static extern IntPtr DispatchMessageW(ref Message message);
    public static void Pump() {
        Message message;
        while (PeekMessageW(out message, IntPtr.Zero, 0, 0, 1)) DispatchMessageW(ref message);
    }
    [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr handle, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll", EntryPoint="SystemParametersInfoW")] public static extern bool SystemParametersInfo(uint action, uint parameter, out Rect rect, uint flags);
    [DllImport("dwmapi.dll")] public static extern int DwmFlush();
    public static IntPtr Find(uint pid) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((handle, data) => {
            uint owner;
            GetWindowThreadProcessId(handle, out owner);
            var text = new StringBuilder(256);
            GetWindowText(handle, text, text.Capacity);
            if (owner == pid && IsWindowVisible(handle) && text.ToString().StartsWith("WTM")) { found = handle; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }
}
'@ + (Get-Content -LiteralPath "$PSScriptRoot/native-probe.cs" -Raw))

function Capture-Frame([string]$Name) {
    [void][WtmFrameProbe]::DwmFlush()
    Start-Sleep -Milliseconds 200
    $rect = [WtmFrameProbe+Rect]::new()
    if (-not [WtmFrameProbe]::GetWindowRect($script:handle, [ref]$rect)) { throw 'Window bounds unavailable' }
    $bitmap = [Drawing.Bitmap]::new($rect.Right - $rect.Left + 48, $rect.Bottom - $rect.Top + 48)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($rect.Left - 24, $rect.Top - 24, 0, 0, $bitmap.Size)
        $bitmap.Save((Join-Path $Output "$Name.png"))
        $expected = [Drawing.Color]::FromArgb(11, 16, 18).ToArgb()
        $changed = 0
        for ($x = 26; $x -lt $bitmap.Width - 26; $x++) {
            if ($bitmap.GetPixel($x, 24).ToArgb() -ne $expected) { $changed++ }
            if ($bitmap.GetPixel($x, $bitmap.Height - 25).ToArgb() -ne $expected) { $changed++ }
        }
        for ($y = 26; $y -lt $bitmap.Height - 26; $y++) {
            if ($bitmap.GetPixel(24, $y).ToArgb() -ne $expected) { $changed++ }
            if ($bitmap.GetPixel($bitmap.Width - 25, $y).ToArgb() -ne $expected) { $changed++ }
        }
        Write-Host "$Name unexpected_edge_pixels=$changed"
        return $changed -eq 0
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
}

function Capture-States([string]$Name) {
    Focus-Window $script:handle
    $suppressed = Capture-Frame "$Name-active"
    Focus-Window $script:previousWindow
    $inactiveSuppressed = Capture-Frame "$Name-inactive"
    return $suppressed -and $inactiveSuppressed
}

function Focus-Window([IntPtr]$Window) {
    $rect = [WtmFrameProbe+Rect]::new()
    if (-not [WtmFrameProbe]::GetWindowRect($Window, [ref]$rect)) { throw 'Focus target bounds unavailable' }
    $X = $rect.Left + [Math]::Min(40, ($rect.Right - $rect.Left) / 2)
    $Y = $rect.Top + [Math]::Min(40, ($rect.Bottom - $rect.Top) / 2)
    $point = [WtmFrameProbe+Point]::new()
    $point.X = $X
    $point.Y = $Y
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        if (-not [WtmFrameProbe]::SetWindowPos($Window, [IntPtr](-1), 0, 0, 0, 0, 0x13)) { throw 'Focus fixture could not be brought forward' }
        if ([WtmFrameProbe]::GetAncestor([WtmFrameProbe]::WindowFromPoint($point), 2) -ne $Window) { throw 'Focus click target is not the fixture' }
        [void][WtmFrameProbe]::SetForegroundWindow($Window)
        [void][WtmFrameProbe]::SetCursorPos($X, $Y)
        [WtmFrameProbe]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
        [WtmFrameProbe]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
        for ($i = 0; $i -lt 10; $i++) {
            [WtmFrameProbe]::Pump()
            if ([WtmFrameProbe]::GetForegroundWindow() -eq $Window) { return }
            Start-Sleep -Milliseconds 20
        }
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'Fixture did not receive focus'
}

$oldHome = $env:HOME
$oldProfile = $env:USERPROFILE
$process = $null
$savedCursor = [WtmFrameProbe+Point]::new()
$script:previousWindow = [WtmFrameProbe]::CreateWindowExW(0, 'BUTTON', 'WTM frame focus fixture', 2415919104, 800, 100, 160, 100, [IntPtr]::Zero, [IntPtr]::Zero, [IntPtr]::Zero, [IntPtr]::Zero)
$previousDpi = [WtmFrameProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
[void][WtmFrameProbe]::GetCursorPos([ref]$savedCursor)
try {
    $work = [WtmFrameProbe+Rect]::new()
    if (-not [WtmFrameProbe]::SystemParametersInfo(0x0030, 0, [ref]$work, 0)) { throw 'Monitor work area unavailable' }
    $width = [Math]::Min(640, $work.Right - $work.Left - 224)
    $height = [Math]::Min(540, $work.Bottom - $work.Top - 48)
    if ($width -lt 120 -or $height -lt 120) { throw 'Display work area is too small for two focus fixtures' }
    [void][WtmFrameProbe]::SetWindowPos($script:previousWindow, [IntPtr](-1), $work.Right - 176, $work.Top + 24, 160, 100, 0)
    $env:HOME = Join-Path $Output 'unix-home'
    $env:USERPROFILE = Join-Path $Output 'home'
    New-Item -ItemType Directory -Path $env:USERPROFILE -Force | Out-Null
    [WtmNative]::Opaque($env:USERPROFILE)
    $process = Start-Process -FilePath $Binary -WorkingDirectory $env:USERPROFILE -PassThru
    $script:handle = [IntPtr]::Zero
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        $script:handle = [WtmFrameProbe]::Find([uint32]$process.Id)
        if ($script:handle -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($script:handle -eq [IntPtr]::Zero) { throw 'Window unavailable' }
    if (-not [WtmFrameProbe]::SetWindowPos($script:handle, [IntPtr](-1), $work.Left + 24, $work.Top + 24, $width, $height, 0x20)) { throw 'Window positioning failed' }
    [void][WtmFrameProbe]::SetForegroundWindow($script:handle)
    foreach ($cycle in 1..3) {
        if (-not (Capture-States "focus-$cycle")) { throw 'FAIL. A window frame appeared after focus changed.' }
    }
    Write-Host "Evidence at $Output"
    Write-Host 'PASS. No frame appeared across three real focus changes.'
} finally {
    $env:HOME = $oldHome
    $env:USERPROFILE = $oldProfile
    if ($null -ne $process -and -not $process.HasExited) {
        [void][WtmFrameProbe]::PostMessageW($script:handle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
        if (-not $process.WaitForExit(10000)) { $process.Kill(); $process.WaitForExit() }
    }
    [void][WtmFrameProbe]::DestroyWindow($script:previousWindow)
    [void][WtmFrameProbe]::SetCursorPos($savedCursor.X, $savedCursor.Y)
    if ($previousDpi -ne [IntPtr]::Zero) { [void][WtmFrameProbe]::SetThreadDpiAwarenessContext($previousDpi) }
}
