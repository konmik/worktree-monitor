param(
    [string]$Binary = "$PSScriptRoot/../target/debug/wts-ui.exe"
)

$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path $Binary).Path
$fixture = Join-Path ([IO.Path]::GetTempPath()) "wts-opacity-$([Guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $fixture | Out-Null
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class WtsOpacityProbe {
    public struct Rect { public int Left, Top, Right, Bottom; }
    public struct Point { public int X, Y; }
    public delegate bool EnumProc(IntPtr window, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr window, EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsWindowEnabled(IntPtr window);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr window, int id);
    [DllImport("user32.dll", EntryPoint="GetWindowLongPtrW")] public static extern IntPtr GetWindowLongPtr(IntPtr window, int index);
    [DllImport("user32.dll")] public static extern bool GetLayeredWindowAttributes(IntPtr window, out uint color, out byte alpha, out uint flags);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out Point point);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr window, uint flags);
    [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll", EntryPoint="SystemParametersInfoW")] public static extern bool SystemParametersInfo(uint action, uint parameter, out Rect rect, uint flags);
    public static IntPtr Find(uint process) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((window, data) => {
            uint owner;
            GetWindowThreadProcessId(window, out owner);
            if (owner == process && IsWindowVisible(window)) { found = window; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }
    public static IntPtr[] Children(IntPtr parent) {
        var children = new List<IntPtr>();
        EnumChildWindows(parent, (window, data) => { if (IsWindowVisible(window)) children.Add(window); return true; }, IntPtr.Zero);
        return children.ToArray();
    }
}
'@

function Assert-Alpha([byte]$Expected, [string]$Name) {
    $deadline = [DateTime]::UtcNow.AddSeconds(4)
    do {
        [uint32]$color = 0
        [byte]$alpha = 0
        [uint32]$flags = 0
        if (-not [WtsOpacityProbe]::GetLayeredWindowAttributes($window, [ref]$color, [ref]$alpha, [ref]$flags)) { throw 'Window alpha unavailable' }
        if (($flags -band 2) -eq 0) { throw 'Window does not use alpha opacity' }
        if ($alpha -eq $Expected) {
            $style = [WtsOpacityProbe]::GetWindowLongPtr($window, -20).ToInt64()
            if (($style -band 0x80000) -eq 0) { throw 'Window lost its layered style' }
            if (($style -band 8) -eq 0) { throw 'Window lost its topmost style' }
            if (($style -band 0x20) -ne 0) { throw 'Window became click-through' }
            if (-not [WtsOpacityProbe]::IsWindowVisible($window) -or -not [WtsOpacityProbe]::IsWindowEnabled($window)) { throw 'Window is hidden or disabled' }
            Write-Host "PASS. $Name alpha=$alpha"
            return
        }
        if ($process.HasExited) { throw 'Fixture exited before opacity changed' }
        Start-Sleep -Milliseconds 40
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "$Name expected alpha=$Expected, observed alpha=$alpha"
}

function Move-Pointer([int]$X, [int]$Y, [bool]$Inside) {
    if (-not [WtsOpacityProbe]::SetCursorPos($X, $Y)) { throw 'Cannot move the cursor' }
    $point = [WtsOpacityProbe+Point]::new()
    if (-not [WtsOpacityProbe]::GetCursorPos([ref]$point) -or $point.X -ne $X -or $point.Y -ne $Y) { throw 'Cursor did not reach the requested point' }
    $root = [WtsOpacityProbe]::GetAncestor([WtsOpacityProbe]::WindowFromPoint($point), 2)
    if (($root -eq $window) -ne $Inside) { throw 'Cursor target is not the expected fixture surface' }
}

$process = $null
$window = [IntPtr]::Zero
$savedCursor = [WtsOpacityProbe+Point]::new()
if (-not [WtsOpacityProbe]::GetCursorPos([ref]$savedCursor)) { throw 'Cannot save cursor position' }
$previousDpi = [WtsOpacityProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
try {
    $start = [Diagnostics.ProcessStartInfo]::new($Binary)
    $start.WorkingDirectory = $fixture
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.Environment['HOME'] = $fixture
    $start.Environment['USERPROFILE'] = $fixture
    $process = [Diagnostics.Process]::Start($start)
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        $window = [WtsOpacityProbe]::Find([uint32]$process.Id)
        if ($window -ne [IntPtr]::Zero) { break }
        if ($process.HasExited) { throw 'Fixture exited without a window' }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($window -eq [IntPtr]::Zero) { throw 'Fixture window unavailable' }
    $work = [WtsOpacityProbe+Rect]::new()
    if (-not [WtsOpacityProbe]::SystemParametersInfo(0x30, 0, [ref]$work, 0)) { throw 'Monitor work area unavailable' }
    $width = [Math]::Min(640, $work.Right - $work.Left - 64)
    $height = [Math]::Min(480, $work.Bottom - $work.Top - 64)
    if ($width -lt 200 -or $height -lt 200) { throw 'Monitor work area too small' }
    if (-not [WtsOpacityProbe]::SetWindowPos($window, [IntPtr](-1), $work.Left + 32, $work.Top + 32, $width, $height, 0x10)) { throw 'Cannot position fixture window' }
    $bounds = [WtsOpacityProbe+Rect]::new()
    if (-not [WtsOpacityProbe]::GetWindowRect($window, [ref]$bounds)) { throw 'Fixture bounds unavailable' }
    $outsideX = $work.Left + 2
    $outsideY = $work.Top + 2
    Move-Pointer $outsideX $outsideY $false
    Assert-Alpha 51 'Outside window'
    $targets = @(
        @{ Name = 'Status child'; Handle = [WtsOpacityProbe]::GetDlgItem($window, 1002) },
        @{ Name = 'Viewport'; Handle = [WtsOpacityProbe]::GetDlgItem($window, 1001) }
    )
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        $children = [WtsOpacityProbe]::Children($targets[1].Handle)
        if ($children.Count -gt 0) { break }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($children.Count -eq 0) { throw 'Content child unavailable' }
    $targets += @{ Name = 'Content child'; Handle = $children[0] }
    foreach ($target in $targets) {
        $rect = [WtsOpacityProbe+Rect]::new()
        if ($target.Handle -eq [IntPtr]::Zero -or -not [WtsOpacityProbe]::GetWindowRect($target.Handle, [ref]$rect)) { throw "$($target.Name) unavailable" }
        Move-Pointer ([int](($rect.Left + $rect.Right) / 2)) ([int](($rect.Top + $rect.Bottom) / 2)) $true
        Assert-Alpha 255 $target.Name
        Move-Pointer $outsideX $outsideY $false
        Assert-Alpha 51 "After $($target.Name)"
    }
    foreach ($point in @(
        @(($bounds.Left + 1), [int](($bounds.Top + $bounds.Bottom) / 2)),
        @(($bounds.Right - 2), [int](($bounds.Top + $bounds.Bottom) / 2)),
        @([int](($bounds.Left + $bounds.Right) / 2), ($bounds.Top + 1)),
        @([int](($bounds.Left + $bounds.Right) / 2), ($bounds.Bottom - 2))
    )) {
        Move-Pointer $point[0] $point[1] $true
        Assert-Alpha 255 'Window edge'
        Move-Pointer $outsideX $outsideY $false
        Assert-Alpha 51 'After window edge'
    }
    Write-Host 'PASS. Real pointer movement changes the entire window between 20% and 100% opacity.'
} finally {
    if ($null -ne $process -and -not $process.HasExited) {
        if ($window -ne [IntPtr]::Zero) { [void][WtsOpacityProbe]::PostMessageW($window, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) }
        if (-not $process.WaitForExit(5000)) { $process.Kill(); $process.WaitForExit() }
    }
    [void][WtsOpacityProbe]::SetCursorPos($savedCursor.X, $savedCursor.Y)
    if ($previousDpi -ne [IntPtr]::Zero) { [void][WtsOpacityProbe]::SetThreadDpiAwarenessContext($previousDpi) }
    if ($null -ne $process) { $process.Dispose() }
}
