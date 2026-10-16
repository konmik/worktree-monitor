param(
    [Parameter(Mandatory)][int]$ProcessId,
    [string]$Output = "$env:LOCALAPPDATA/Temp/wtm-worktree-spacing",
    [switch]$Startup
)

$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Path $Output -Force | Out-Null
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class WtmSpacingProbe {
    public struct Rect { public int Left, Top, Right, Bottom; }
    public sealed class Row { public IntPtr Handle; public Rect Bounds; public string Text; }
    public delegate bool EnumProc(IntPtr window, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern IntPtr GetParent(IntPtr window);
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr window, int id);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr window, StringBuilder value, int count);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr window, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    public static string Text(IntPtr window) {
        var value = new StringBuilder(65536);
        GetWindowText(window, value, value.Capacity);
        return value.ToString();
    }
    public static Row[] Rows(IntPtr viewport) {
        var rows = new List<Row>();
        EnumChildWindows(viewport, (window, data) => {
            if (GetParent(window) != viewport) return true;
            var text = new StringBuilder(65536);
            GetWindowText(window, text, text.Capacity);
            Rect rect;
            if (GetWindowRect(window, out rect)) rows.Add(new Row { Handle = window, Bounds = rect, Text = text.ToString() });
            return true;
        }, IntPtr.Zero);
        rows.Sort((a, b) => a.Bounds.Top.CompareTo(b.Bounds.Top));
        return rows.ToArray();
    }
}
'@

function Is-Worktree($Row) {
    $content = $Row.Text -replace '\r?\nRefreshing$', ''
    return $content -match '\r?\nS\s+\+\d+/-\d+|\r?\nCounts unavailable' -or [IO.Path]::IsPathFullyQualified(($content -split '\r?\n')[0])
}

function Assert-WindowBounds($Window, $Original) {
    $bounds = [WtmSpacingProbe+Rect]::new()
    if (-not [WtmSpacingProbe]::GetWindowRect($Window, [ref]$bounds)) { throw 'Monitor bounds unavailable' }
    if ($bounds.Left -ne $Original.Left -or $bounds.Top -ne $Original.Top -or $bounds.Right -ne $Original.Right -or $bounds.Bottom -ne $Original.Bottom) {
        throw "Monitor bounds changed from ($($Original.Left), $($Original.Top), $($Original.Right), $($Original.Bottom)) to ($($bounds.Left), $($bounds.Top), $($bounds.Right), $($bounds.Bottom))"
    }
    return $bounds
}

$process = Get-Process -Id $ProcessId
$deadline = [DateTime]::UtcNow.AddSeconds(30)
do {
    $process.Refresh()
    $window = $process.MainWindowHandle
    if ($window -ne [IntPtr]::Zero) { break }
    Start-Sleep -Milliseconds 50
} while ([DateTime]::UtcNow -lt $deadline)
if ($window -eq [IntPtr]::Zero) { throw 'Monitor window unavailable' }
$previousDpi = [WtmSpacingProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
$original = [WtmSpacingProbe+Rect]::new()
if (-not [WtmSpacingProbe]::GetWindowRect($window, [ref]$original)) { throw 'Monitor bounds unavailable' }
$checks = [Collections.Generic.List[object]]::new()
try {
    $viewport = [WtmSpacingProbe]::GetDlgItem($window, 1001)
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        $null = Assert-WindowBounds $window $original
        $rows = [WtmSpacingProbe]::Rows($viewport)
        $pairs = 0
        for ($index = 1; $index -lt $rows.Count; $index++) {
            if ((Is-Worktree $rows[$index - 1]) -and (Is-Worktree $rows[$index])) { $pairs++ }
        }
        if ($pairs -gt 0) { break }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)
    $phases = if ($Startup) { @('startup', 'loaded') } else { @('current') }
    foreach ($phase in $phases) {
        if ($phase -eq 'loaded') {
            $deadline = [DateTime]::UtcNow.AddSeconds(90)
            do {
                $null = Assert-WindowBounds $window $original
                $rows = [WtmSpacingProbe]::Rows($viewport)
                $pending = @($rows | Where-Object {
                    $_.Text -match '\r?\nRefreshing$' -or ((Is-Worktree $_) -and $_.Text -notmatch '\r?\nS\s+\+\d+/-\d+|\r?\nCounts unavailable')
                })
                if ($pending.Count -eq 0 -and @($rows | Where-Object { Is-Worktree $_ }).Count -gt 0) { break }
                Start-Sleep -Milliseconds 100
            } while ([DateTime]::UtcNow -lt $deadline)
            if ($pending.Count -ne 0 -or @($rows | Where-Object { Is-Worktree $_ }).Count -eq 0) { throw 'Startup did not finish' }
        }
        $bounds = Assert-WindowBounds $window $original
        $width = $bounds.Right - $bounds.Left
        $height = $bounds.Bottom - $bounds.Top
        $bitmap = [Drawing.Bitmap]::new($width, $height)
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        $dc = $graphics.GetHdc()
        try {
            if (-not [WtmSpacingProbe]::PrintWindow($window, $dc, 0)) { throw 'Monitor capture failed' }
        } finally { $graphics.ReleaseHdc($dc); $graphics.Dispose() }
        try { $bitmap.Save((Join-Path $Output "$phase-width-$width-height-$height.png")) } finally { $bitmap.Dispose() }
        $null = Assert-WindowBounds $window $original
        $rows = [WtmSpacingProbe]::Rows($viewport)
        $pairs = 0
        for ($index = 1; $index -lt $rows.Count; $index++) {
            $previous = $rows[$index - 1]
            $current = $rows[$index]
            if ((Is-Worktree $previous) -and (Is-Worktree $current)) {
                $gap = $current.Bounds.Top - $previous.Bounds.Bottom
                $checks.Add(@{ phase = $phase; width = $width; height = $height; bounds = @{ left = $bounds.Left; top = $bounds.Top; right = $bounds.Right; bottom = $bounds.Bottom }; previous = $previous.Text; current = $current.Text; gap = $gap })
                if ($gap -ne 0) { throw "Adjacent worktrees have a $gap pixel gap at width $width" }
                $pairs++
            }
        }
        if ($pairs -eq 0) { throw 'No adjacent worktrees available to verify' }
        $null = Assert-WindowBounds $window $original
    }
    $checks | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $Output 'proof.json')
    Write-Host "PASS. Adjacent worktree bounds touch and monitor bounds are unchanged. Evidence: $Output"
} finally {
    [void][WtmSpacingProbe]::SetThreadDpiAwarenessContext($previousDpi)
}
