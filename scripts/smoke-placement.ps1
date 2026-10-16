param([string]$Binary = "$PSScriptRoot/../target/debug/wtm-ui.exe")

$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path $Binary).Path
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class WtmPlacementProbe {
    public struct Rect { public int Left, Top, Right, Bottom; }
    public delegate bool EnumProc(IntPtr window, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr window, StringBuilder name, int size);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll", EntryPoint="SystemParametersInfoW")] public static extern bool SystemParametersInfo(uint action, uint parameter, out Rect rect, uint flags);
    public static IntPtr Find(uint process) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((window, data) => {
            uint owner;
            GetWindowThreadProcessId(window, out owner);
            var name = new StringBuilder(256);
            GetClassName(window, name, name.Capacity);
            if (owner == process && name.ToString() == "WtmStatusWindow" && IsWindowVisible(window)) {
                found = window;
                return false;
            }
            return true;
        }, IntPtr.Zero);
        return found;
    }
}
'@

function Start-Window {
    $start = [Diagnostics.ProcessStartInfo]::new($Binary)
    $start.UseShellExecute = $false
    $start.Environment['USERPROFILE'] = $fixture
    $start.WorkingDirectory = $fixture
    $script:process = [Diagnostics.Process]::Start($start)
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        $script:window = [WtmPlacementProbe]::Find([uint32]$script:process.Id)
        if ($script:window -ne [IntPtr]::Zero) { return }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline -and -not $script:process.HasExited)
    throw 'Window did not appear'
}

function Close-Window {
    [void][WtmPlacementProbe]::PostMessage($script:window, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
    if (-not $script:process.WaitForExit(10000)) { throw 'Window did not close' }
    if ($script:process.ExitCode -ne 0) { throw 'App exited with an error' }
}

$fixture = Join-Path $env:TEMP "wtm-placement-$([Guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path (Join-Path $fixture '.config/wtm') -Force | Out-Null
Set-Content -LiteralPath (Join-Path $fixture '.config/wtm/config.json') -Value '{"repos":[]}'
$previousDpi = [WtmPlacementProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
$script:process = $null
try {
    $area = [WtmPlacementProbe+Rect]::new()
    if (-not [WtmPlacementProbe]::SystemParametersInfo(0x0030, 0, [ref]$area, 0)) { throw 'Work area unavailable' }
    foreach ($scenario in @('inside work area', 'across right edge', 'behind taskbar', 'across top edge', 'across bottom edge', 'fully offscreen')) {
        Start-Window
        $width = 640
        $height = 540
        $x = $area.Left + 100
        $y = $area.Top + 100
        if ($scenario -eq 'across right edge') { $x = $area.Right - 200 }
        if ($scenario -eq 'behind taskbar') { $x = $area.Left - 20 }
        if ($scenario -eq 'across top edge') { $y = $area.Top - 20 }
        if ($scenario -eq 'across bottom edge') { $y = $area.Bottom - 200 }
        if ($scenario -eq 'fully offscreen') { $x = $area.Right + 20 }
        if (-not [WtmPlacementProbe]::SetWindowPos($script:window, [IntPtr]::Zero, $x, $y, $width, $height, 0x14)) { throw 'Move failed' }
        Close-Window
        $saved = Get-Content -LiteralPath (Join-Path $fixture '.config/wtm/config.placement.json') -Raw | ConvertFrom-Json
        if ($saved.normal.left -ne $x -or $saved.normal.top -ne $y) { throw 'Move was not saved' }
        Start-Window
        $actual = [WtmPlacementProbe+Rect]::new()
        if (-not [WtmPlacementProbe]::GetWindowRect($script:window, [ref]$actual)) { throw 'Bounds unavailable' }
        Close-Window
        if ($scenario -eq 'fully offscreen') {
            if ($actual.Left -lt $area.Left -or $actual.Top -lt $area.Top -or $actual.Right -gt $area.Right -or $actual.Bottom -gt $area.Bottom) {
                throw 'FAIL. Fully offscreen placement did not fall back into the work area'
            }
            Write-Host 'PASS. Fully offscreen placement falls back into the work area.'
            continue
        }
        if ($actual.Left -ne $x -or $actual.Top -ne $y -or $actual.Right -ne ($x + $width) -or $actual.Bottom -ne ($y + $height)) {
            throw "FAIL. $scenario expected $x,$y,$($x + $width),$($y + $height), got $($actual.Left),$($actual.Top),$($actual.Right),$($actual.Bottom)"
        }
        Write-Host "PASS. Restart preserves bounds $x,$y,$($x + $width),$($y + $height) for $scenario."
    }
} finally {
    if ($script:process -and -not $script:process.HasExited) { $script:process.Kill(); $script:process.WaitForExit() }
    [void][WtmPlacementProbe]::SetThreadDpiAwarenessContext($previousDpi)
    Write-Host "Evidence at $fixture"
}
