param(
    [string]$Binary = "$PSScriptRoot/../target/debug/wts.exe",
    [string]$Output = "$env:LOCALAPPDATA/Temp/opencode/wts-resize-$([Guid]::NewGuid().ToString('N'))"
)

$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path $Binary).Path
New-Item -ItemType Directory -Path $Output -Force | Out-Null
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Collections.Generic;
public static class WtsResizeProbe {
    public struct Rect { public int Left, Top, Right, Bottom; }
    public struct Point { public int X, Y; }
    public delegate bool EnumProc(IntPtr handle, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr handle, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr handle, StringBuilder text, int count);
    [DllImport("user32.dll")] public static extern IntPtr GetParent(IntPtr handle);
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr handle, int id);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr handle);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr handle, out Rect rect);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr handle, out Rect rect);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr handle, ref Point point);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr handle, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] public static extern bool RedrawWindow(IntPtr handle, IntPtr rect, IntPtr region, uint flags);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr handle, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr handle, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("dwmapi.dll")] public static extern int DwmFlush();
    public static string Text(IntPtr handle) {
        var text = new StringBuilder(65536);
        GetWindowText(handle, text, text.Capacity);
        return text.ToString();
    }
    public static IntPtr Find(uint pid) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((handle, data) => {
            uint owner;
            GetWindowThreadProcessId(handle, out owner);
            if (owner == pid && IsWindowVisible(handle) && Text(handle).StartsWith("WTS")) { found = handle; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }
    public static string Geometry(IntPtr parent, bool captions = true) {
        var rows = new List<string>();
        EnumChildWindows(parent, (handle, data) => {
            Rect rect;
            GetWindowRect(handle, out rect);
            rows.Add(handle + " " + GetParent(handle) + " " + rect.Left + "," + rect.Top + "," + rect.Right + "," + rect.Bottom + (captions ? " " + Text(handle).Replace("\n", " | ") : ""));
            return true;
        }, IntPtr.Zero);
        return string.Join("\n", rows);
    }
    public static Bitmap Capture(IntPtr window) {
        Rect rect;
        var origin = new Point();
        if (!GetClientRect(window, out rect) || !ClientToScreen(window, ref origin)) { throw new InvalidOperationException("Capture bounds unavailable"); }
        var bitmap = new Bitmap(rect.Right, rect.Bottom);
        using (var graphics = Graphics.FromImage(bitmap)) { graphics.CopyFromScreen(origin.X, origin.Y, 0, 0, bitmap.Size); }
        return bitmap;
    }
    public static int Difference(Bitmap first, Bitmap second) {
        int changed = 0;
        for (int y = 0; y < first.Height; y++) {
            for (int x = 0; x < first.Width; x++) { if (first.GetPixel(x, y) != second.GetPixel(x, y)) { changed++; } }
        }
        return changed;
    }
}
'@ -ReferencedAssemblies System.Drawing.Common, System.Drawing.Primitives, System.Collections, System.Private.Windows.GdiPlus, System.Private.Windows.Core

function Wait-For([scriptblock]$Condition, [string]$Label) {
    $deadline = [DateTime]::UtcNow.AddSeconds(25)
    do {
        if (& $Condition) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for $Label"
}

function Run-Git([string[]]$Arguments) {
    & git -c core.fsmonitor=false -c core.autocrlf=false -c commit.gpgsign=false -c "core.hooksPath=$Output" -C $repo @Arguments
    if ($LASTEXITCODE -ne 0) { throw 'Fixture Git command failed' }
}

function Compare-Repaint([string]$Name) {
    [void][WtsResizeProbe]::RedrawWindow($script:handle, [IntPtr]::Zero, [IntPtr]::Zero, 0x0180)
    [void][WtsResizeProbe]::DwmFlush()
    $before = [WtsResizeProbe]::Capture($script:handle)
    try {
        $before.Save((Join-Path $Output "$Name-normal.png"))
        $geometry = [WtsResizeProbe]::Geometry($script:handle, $false)
        [IO.File]::WriteAllText((Join-Path $Output "$Name-geometry.txt"), [WtsResizeProbe]::Geometry($script:handle))
        if (-not [WtsResizeProbe]::RedrawWindow($script:handle, [IntPtr]::Zero, [IntPtr]::Zero, 0x0181)) { throw 'Full repaint failed' }
        [void][WtsResizeProbe]::DwmFlush()
        $after = [WtsResizeProbe]::Capture($script:handle)
        try {
            $after.Save((Join-Path $Output "$Name-repaint.png"))
            $difference = [WtsResizeProbe]::Difference($before, $after)
            $script:results.Add([ordered]@{ action = $Name; changed_pixels = $difference; geometry_unchanged = $geometry -eq [WtsResizeProbe]::Geometry($script:handle, $false) })
            Write-Host "$Name`: $difference pixels changed by full repaint"
        } finally { $after.Dispose() }
    } finally { $before.Dispose() }
}

$fixtureHome = Join-Path $Output 'home'
New-Item -ItemType Directory -Path $fixtureHome -Force | Out-Null
$oldHome = $env:HOME
$oldProfile = $env:USERPROFILE
$process = $null
$previousDpiContext = [WtsResizeProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
$results = [System.Collections.Generic.List[object]]::new()
try {
    $env:HOME = Join-Path $Output 'unix-home'
    $env:USERPROFILE = $fixtureHome
    foreach ($name in @('signal-core', 'edge-gateway', 'infra-tools')) {
        $repo = Join-Path $Output $name
        New-Item -ItemType Directory -Path $repo | Out-Null
        Run-Git @('init', '--quiet')
        [IO.File]::WriteAllText((Join-Path $repo 'lines.txt'), "before`n")
        Run-Git @('add', '.')
        Run-Git @('-c', 'user.name=Wts Test', '-c', 'user.email=wts@example.invalid', 'commit', '--quiet', '-m', 'Fixture')
        [IO.File]::WriteAllText((Join-Path $repo 'lines.txt'), "before`nadded`n")
        & $Binary add $repo
        if ($LASTEXITCODE -ne 0) { throw 'Fixture registration failed' }
    }
    $process = Start-Process -FilePath $Binary -ArgumentList '--live', '--live-child' -WorkingDirectory $fixtureHome -NoNewWindow -PassThru
    $script:handle = [IntPtr]::Zero
    Wait-For { $script:handle = [WtsResizeProbe]::Find([uint32]$process.Id); $script:handle -ne [IntPtr]::Zero } 'window'
    $viewport = [WtsResizeProbe]::GetDlgItem($script:handle, 1001)
    Wait-For { [WtsResizeProbe]::Geometry($viewport) -match 'infra-tools.*\+1' } 'fixture counts'
    $index = 0
    foreach ($size in @(@(640, 540), @(800, 720), @(460, 330), @(740, 620), @(600, 130), @(640, 540), @(380, 620), @(800, 720))) {
        $index++
        if (-not [WtsResizeProbe]::SetWindowPos($script:handle, [IntPtr]::Zero, 32, 32, $size[0], $size[1], 20)) { throw 'Resize failed' }
        Compare-Repaint "$index-resize-$($size[0])x$($size[1])"
        [void][WtsResizeProbe]::SendMessage($viewport, 0x0115, [IntPtr]3, [IntPtr]::Zero)
        Compare-Repaint "$index-scroll-down"
        [void][WtsResizeProbe]::SendMessage($viewport, 0x0115, [IntPtr]6, [IntPtr]::Zero)
        Compare-Repaint "$index-scroll-top"
    }
    $failed = @($results | Where-Object { $_.changed_pixels -ne 0 -or -not $_.geometry_unchanged })
    [ordered]@{ binary = $Binary; sha256 = (Get-FileHash $Binary).Hash; results = $results.ToArray(); result = $(if ($failed.Count) { 'FAIL' } else { 'PASS' }) } | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $Output 'proof.json')
    Write-Host "Evidence at $Output"
    if ($failed.Count) { throw "$($failed.Count) actions left stale pixels" }
    [void][WtsResizeProbe]::PostMessage($script:handle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
    if (-not $process.WaitForExit(10000) -or $process.ExitCode -ne 0) { throw 'Window did not close cleanly' }
    Write-Host 'PASS. Resize and scroll pixels match a full repaint.'
} finally {
    $env:HOME = $oldHome
    $env:USERPROFILE = $oldProfile
    if ($null -ne $process -and -not $process.HasExited) { $process.Kill($true); $process.WaitForExit() }
    if ($previousDpiContext -ne [IntPtr]::Zero) { [void][WtsResizeProbe]::SetThreadDpiAwarenessContext($previousDpiContext) }
}
