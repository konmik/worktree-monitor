param(
    [string]$Binary = "$PSScriptRoot/../target/debug/wtm-gui.exe",
    [string]$CliBinary = "$PSScriptRoot/../target/debug/wtm.exe"
)

. (Join-Path $PSScriptRoot 'verification-directories.ps1')
try {
$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path $Binary).Path
$CliBinary = (Resolve-Path $CliBinary).Path
$verificationEvidence = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot "../target/verification/smoke-indicator-opacity-evidence-$([Guid]::NewGuid().ToString('N'))"))
$fixture = New-VerificationDirectory 'smoke-indicator-opacity' $verificationEvidence
[void][IO.Directory]::CreateDirectory($fixture)
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class WtmIndicatorProbe {
    public struct Rect { public int Left, Top, Right, Bottom; }
    public struct Point { public int X, Y; }
    public struct Size { public int Width, Height; }
    public delegate bool EnumProc(IntPtr window, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr window, EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr window, uint command);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr window, StringBuilder text, int capacity);
    [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] public static extern bool GetLayeredWindowAttributes(IntPtr window, out uint color, out byte alpha, out uint flags);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out Point point);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll", EntryPoint="SystemParametersInfoW")] public static extern bool SystemParametersInfo(uint action, uint parameter, out Rect rect, uint flags);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr CreateWindowExW(uint exStyle, string className, string title, uint style, int x, int y, int width, int height, IntPtr parent, IntPtr menu, IntPtr instance, IntPtr parameter);
    [DllImport("user32.dll")] public static extern bool UpdateWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool DestroyWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern IntPtr GetDC(IntPtr window);
    [DllImport("user32.dll")] public static extern int ReleaseDC(IntPtr window, IntPtr dc);
    [DllImport("gdi32.dll")] public static extern IntPtr SelectObject(IntPtr dc, IntPtr value);
    [DllImport("gdi32.dll")] public static extern bool DeleteObject(IntPtr value);
    [DllImport("gdi32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr CreateFontW(int height, int width, int escapement, int orientation, int weight, uint italic, uint underline, uint strikeout, uint charset, uint output, uint clip, uint quality, uint pitch, string face);
    [DllImport("gdi32.dll", CharSet=CharSet.Unicode)] public static extern int GetTextFaceW(IntPtr dc, int capacity, StringBuilder face);
    [DllImport("gdi32.dll", CharSet=CharSet.Unicode)] public static extern bool GetTextExtentPoint32W(IntPtr dc, string text, int length, out Size size);
    [DllImport("dwmapi.dll")] public static extern int DwmFlush();
    public static string Text(IntPtr window) {
        var text = new StringBuilder(4096);
        GetWindowTextW(window, text, text.Capacity);
        return text.ToString();
    }
    public static IntPtr[] Windows(uint process) {
        var windows = new List<IntPtr>();
        EnumWindows((window, data) => {
            uint owner;
            GetWindowThreadProcessId(window, out owner);
            if (owner == process) windows.Add(window);
            return true;
        }, IntPtr.Zero);
        return windows.ToArray();
    }
    public static IntPtr[] Children(IntPtr parent) {
        var children = new List<IntPtr>();
        EnumChildWindows(parent, (window, data) => {
            if (IsWindowVisible(window)) children.Add(window);
            return true;
        }, IntPtr.Zero);
        return children.ToArray();
    }
    public static Size Measure(IntPtr window, string text) {
        var dc = GetDC(window);
        var font = IntPtr.Zero;
        var old = IntPtr.Zero;
        try {
            foreach (var face in new[] { "Berkeley Mono", "Berkeley Mono Variable", "Consolas", "Cascadia Mono", "Courier New" }) {
                    font = CreateFontW(-(12 * (int)GetDpiForWindow(window) / 96), 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 49, face);
                    if (font == IntPtr.Zero) continue;
                    var previous = SelectObject(dc, font);
                    var actual = new StringBuilder(64);
                    GetTextFaceW(dc, actual.Capacity, actual);
                    SelectObject(dc, previous);
                    if (String.Equals(actual.ToString(), face, StringComparison.OrdinalIgnoreCase)) break;
                    DeleteObject(font);
                    font = IntPtr.Zero;
            }
            if (font == IntPtr.Zero) throw new Exception("Dashboard font unavailable");
            old = SelectObject(dc, font);
            Size size;
            if (!GetTextExtentPoint32W(dc, text, text.Length, out size)) throw new Exception("Font measurement failed");
            return size;
        } finally {
            SelectObject(dc, old);
            if (font != IntPtr.Zero) DeleteObject(font);
            ReleaseDC(window, dc);
        }
    }
}
'@

function Start-FixtureProcess([string]$File, [string[]]$Arguments, [string]$Directory, [switch]$Wait) {
    $start = [Diagnostics.ProcessStartInfo]::new($File)
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.WorkingDirectory = $Directory
    $start.Environment['HOME'] = $fixture
    $start.Environment['USERPROFILE'] = $fixture
    $start.Environment['XDG_CONFIG_HOME'] = Join-Path $fixture 'xdg'
    foreach ($argument in $Arguments) { $start.ArgumentList.Add($argument) }
    if ($Wait) { $start.RedirectStandardOutput = $true; $start.RedirectStandardError = $true }
    $child = [Diagnostics.Process]::Start($start)
    if (-not $Wait) { return $child }
    try {
        $stdout = $child.StandardOutput.ReadToEnd()
        $stderr = $child.StandardError.ReadToEnd()
        $child.WaitForExit()
        if ($child.ExitCode -ne 0) { throw "$File failed. $stdout $stderr" }
    } finally { $child.Dispose() }
}

function Get-Bounds([IntPtr]$Handle) {
    $bounds = [WtmIndicatorProbe+Rect]::new()
    if (-not [WtmIndicatorProbe]::GetWindowRect($Handle, [ref]$bounds)) { throw 'Window bounds unavailable' }
    return $bounds
}

function Assert-Alpha([byte]$Expected) {
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        [uint32]$color = 0
        [byte]$alpha = 0
        [uint32]$flags = 0
        if (-not [WtmIndicatorProbe]::GetLayeredWindowAttributes($window, [ref]$color, [ref]$alpha, [ref]$flags)) { throw 'Root alpha unavailable' }
        if ($alpha -eq $Expected -and ($flags -band 2) -ne 0) { Start-Sleep -Milliseconds 150; return }
        Start-Sleep -Milliseconds 30
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Expected root alpha $Expected, observed $alpha"
}

function Capture-Screen([string]$Name) {
    $bounds = Get-Bounds $window
    $bitmap = [Drawing.Bitmap]::new($bounds.Right - $bounds.Left, $bounds.Bottom - $bounds.Top)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        [void][WtmIndicatorProbe]::DwmFlush()
        $graphics.CopyFromScreen($bounds.Left, $bounds.Top, 0, 0, $bitmap.Size)
        $bitmap.Save((Join-Path $fixture "$Name.png"), [Drawing.Imaging.ImageFormat]::Png)
    } finally { $graphics.Dispose() }
    return ,$bitmap
}

function Get-Regions {
    $regions = @()
    $root = Get-Bounds $window
    foreach ($child in [WtmIndicatorProbe]::Children($window)) {
        $caption = [WtmIndicatorProbe]::Text($child)
        if ($caption -match '(?m)^S \+([01])/-0 · T \+1/-0 · X \+1/-0 · binaries: 1\r?$') {
            $added = [int]$Matches[1]
            $changed = $added -gt 0
            $parts = @(
                @{ Text = 'S '; Name = 'S label'; Color = @(132, 150, 156); Active = $changed },
                @{ Text = "+$added"; Name = 'S additions'; Color = $(if ($changed) { @(194, 249, 112) } else { @(132, 150, 156) }); Active = $changed },
                @{ Text = '/'; Name = 'S slash'; Color = @(132, 150, 156); Active = $changed },
                @{ Text = '-0'; Name = 'S deletions'; Color = $(if ($changed) { @(255, 131, 137) } else { @(132, 150, 156) }); Active = $changed },
                @{ Text = ' · '; Name = 'S separator'; Color = @(132, 150, 156); Active = $false },
                @{ Text = 'T +1/-0'; Name = 'T'; Color = @(132, 150, 156); Active = $true },
                @{ Text = ' · '; Name = 'T separator'; Color = @(132, 150, 156); Active = $false },
                @{ Text = 'X +1/-0'; Name = 'X'; Color = @(132, 150, 156); Active = $true },
                @{ Text = ' · binaries: 1'; Name = 'Binaries'; Color = @(132, 150, 156); Active = $false }
            )
        } elseif ($caption -match '(?m)^S \+0/-0\r?$') {
            $parts = @(@{ Text = 'S +0/-0'; Name = 'Zero S'; Color = @(132, 150, 156); Active = $false })
        } elseif ($caption -match '^\[idle: \d+%\]$') {
            $bounds = Get-Bounds $child
            $size = [WtmIndicatorProbe]::Measure($child, $caption)
            $regions += @{ Name = 'Read'; X = $bounds.Left - $root.Left; Y = $bounds.Top - $root.Top; Width = $size.Width; Height = $bounds.Bottom - $bounds.Top; Color = @(194, 249, 112); Active = $false }
            continue
        } else { continue }
        $bounds = Get-Bounds $child
        $dpi = [Math]::Max(96, [int][WtmIndicatorProbe]::GetDpiForWindow($child))
        $space = [WtmIndicatorProbe]::Measure($child, ' ')
        $x = $bounds.Left - $root.Left + [Math]::Max(0, [int][Math]::Floor(14 * $dpi / 96) - $space.Width)
        $y = $bounds.Top - $root.Top + [int][Math]::Round(12 * 1.2 * $dpi / 96, [MidpointRounding]::AwayFromZero)
        foreach ($part in $parts) {
            $size = [WtmIndicatorProbe]::Measure($child, $part.Text)
            $regions += @{ Name = $part.Name; X = $x; Y = $y; Width = $size.Width; Height = $size.Height; Color = $part.Color; Active = $part.Active }
            $x += $size.Width
        }
    }
    if ($regions.Count -ne 11) { throw "Expected 11 indicator regions, found $($regions.Count)" }
    return $regions
}

function Assert-Pixels($Image, $Regions, [bool]$Hover, [string]$Name) {
    foreach ($region in $Regions) {
        $full = 0
        $ink = 0
        for ($y = $region.Y; $y -lt $region.Y + $region.Height; $y++) {
            for ($x = $region.X; $x -lt $region.X + $region.Width; $x++) {
                $pixel = $Image.GetPixel($x, $y)
                $distance = [Math]::Max([Math]::Abs($pixel.R - $region.Color[0]), [Math]::Max([Math]::Abs($pixel.G - $region.Color[1]), [Math]::Abs($pixel.B - $region.Color[2])))
                if ($distance -le 12) { $full++ }
                $background = if ($Hover) { @(11, 16, 18) } else { @(206, 207, 208) }
                if ([Math]::Max([Math]::Abs($pixel.R - $background[0]), [Math]::Max([Math]::Abs($pixel.G - $background[1]), [Math]::Abs($pixel.B - $background[2]))) -gt 5) { $ink++ }
            }
        }
        if ($ink -eq 0) { throw "$Name $($region.Name) has no visible text" }
        if (($Hover -or $region.Active) -and $full -eq 0) { throw "$Name $($region.Name) has no fully opaque foreground pixels" }
        if (-not $Hover -and -not $region.Active -and $full -gt 0) { throw "$Name $($region.Name) incorrectly remains fully opaque" }
        Write-Host "PASS. $Name $($region.Name) full foreground pixels=$full"
    }
}

$process = $null
$window = [IntPtr]::Zero
$backdrop = [IntPtr]::Zero
$savedCursor = [WtmIndicatorProbe+Point]::new()
if (-not [WtmIndicatorProbe]::GetCursorPos([ref]$savedCursor)) { throw 'Cursor unavailable' }
$previousDpi = [WtmIndicatorProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
try {
    foreach ($name in @('dirty', 'clean')) {
        $repo = Join-Path $fixture $name
        [void][IO.Directory]::CreateDirectory($repo)
        Start-FixtureProcess 'git' @('init', '--quiet') $repo -Wait
        foreach ($file in @('source.rs', 'source_test.rs', 'script.ps1')) { [IO.File]::WriteAllText((Join-Path $repo $file), "initial`n") }
        [IO.File]::WriteAllBytes((Join-Path $repo 'asset.bin'), [byte[]]@(0, 1, 2))
        Start-FixtureProcess 'git' @('add', '.') $repo -Wait
        Start-FixtureProcess 'git' @('-c', 'user.name=Opacity Probe', '-c', 'user.email=opacity@example.invalid', 'commit', '--quiet', '-m', 'Fixture') $repo -Wait
        if ($name -eq 'dirty') {
            foreach ($file in @('source.rs', 'source_test.rs', 'script.ps1')) { [IO.File]::AppendAllText((Join-Path $repo $file), "added`n") }
            [IO.File]::WriteAllBytes((Join-Path $repo 'asset.bin'), [byte[]]@(0, 3, 4))
        }
        Start-FixtureProcess $CliBinary @('add', $repo) $fixture -Wait
    }
    $work = [WtmIndicatorProbe+Rect]::new()
    if (-not [WtmIndicatorProbe]::SystemParametersInfo(0x30, 0, [ref]$work, 0)) { throw 'Work area unavailable' }
    $width = [Math]::Min(800, $work.Right - $work.Left - 100)
    $height = [Math]::Min(550, $work.Bottom - $work.Top - 100)
    if ($width -lt 600 -or $height -lt 300) { throw 'Screen too small for indicator fixture' }
    $backdrop = [WtmIndicatorProbe]::CreateWindowExW(0x08000008, 'STATIC', 'WTM opacity backing', 2415919110, $work.Left, $work.Top, $work.Right - $work.Left, $work.Bottom - $work.Top, [IntPtr]::Zero, [IntPtr]::Zero, [IntPtr]::Zero, [IntPtr]::Zero)
    if ($backdrop -eq [IntPtr]::Zero) { throw 'White backing window creation failed' }
    [void][WtmIndicatorProbe]::UpdateWindow($backdrop)
    $process = Start-FixtureProcess $Binary @() $fixture
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        foreach ($candidate in [WtmIndicatorProbe]::Windows([uint32]$process.Id)) {
            if ([WtmIndicatorProbe]::GetWindow($candidate, 4) -eq [IntPtr]::Zero -and [WtmIndicatorProbe]::IsWindowVisible($candidate)) { $window = $candidate; break }
        }
        if ($window -ne [IntPtr]::Zero) {
            $captions = @([WtmIndicatorProbe]::Children($window) | ForEach-Object { [WtmIndicatorProbe]::Text($_) }) -join "`n"
            if ($captions -match 'S \+1/-0 · T \+1/-0 · X \+1/-0 · binaries: 1' -and $captions -match 'S \+0/-0') { break }
        }
        if ($process.HasExited) { throw 'Fixture exited before showing counts' }
        Start-Sleep -Milliseconds 80
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($window -eq [IntPtr]::Zero -or $captions -notmatch 'binaries: 1') { throw 'Fixture counts unavailable' }
    if (-not [WtmIndicatorProbe]::SetWindowPos($window, [IntPtr](-1), $work.Left + 40, $work.Top + 40, $width, $height, 0x10)) { throw 'Fixture positioning failed' }
    [void][WtmIndicatorProbe]::SetCursorPos($work.Right - 2, $work.Bottom - 2)
    Assert-Alpha 51
    $regions = Get-Regions
    $regions | ConvertTo-Json | Set-Content (Join-Path $fixture 'regions.json')
    $image = Capture-Screen 'outside'
    try {
        Assert-Pixels $image $regions $false 'Outside'
        $region = $regions | Where-Object { $_.Name -eq 'S additions' }
        $hoverPoint = $null
        for ($y = $region.Y; $y -lt $region.Y + $region.Height -and $null -eq $hoverPoint; $y++) {
            for ($x = $region.X; $x -lt $region.X + $region.Width; $x++) {
                $pixel = $image.GetPixel($x, $y)
                if ([Math]::Abs($pixel.R - 194) -le 12 -and [Math]::Abs($pixel.G - 249) -le 12 -and [Math]::Abs($pixel.B - 112) -le 12) {
                    $hoverPoint = @($x, $y)
                    break
                }
            }
        }
        if ($null -eq $hoverPoint) { throw 'Opaque indicator hover point unavailable' }
    } finally { $image.Dispose() }
    $bounds = Get-Bounds $window
    [void][WtmIndicatorProbe]::SetCursorPos($bounds.Left + $hoverPoint[0], $bounds.Top + $hoverPoint[1])
    Assert-Alpha 255
    $image = Capture-Screen 'hover'
    try { Assert-Pixels $image (Get-Regions) $true 'Hover' } finally { $image.Dispose() }
    [void][WtmIndicatorProbe]::SetCursorPos($work.Right - 2, $work.Bottom - 2)
    Assert-Alpha 51
    $image = Capture-Screen 'leave'
    try { Assert-Pixels $image (Get-Regions) $false 'Leave' } finally { $image.Dispose() }
    if (-not [WtmIndicatorProbe]::SetWindowPos($window, [IntPtr](-1), $bounds.Left + 30, $bounds.Top + 30, $width, $height, 0x10)) { throw 'Fixture move failed' }
    Assert-Alpha 51
    $image = Capture-Screen 'moved'
    try { Assert-Pixels $image (Get-Regions) $false 'Moved' } finally { $image.Dispose() }
    $vacated = [Drawing.Bitmap]::new(30, $height)
    $graphics = [Drawing.Graphics]::FromImage($vacated)
    try {
        [void][WtmIndicatorProbe]::DwmFlush()
        $graphics.CopyFromScreen($bounds.Left, $bounds.Top, 0, 0, $vacated.Size)
        $vacated.Save((Join-Path $fixture 'vacated.png'), [Drawing.Imaging.ImageFormat]::Png)
        for ($y = 0; $y -lt $vacated.Height; $y++) {
            for ($x = 0; $x -lt $vacated.Width; $x++) {
                $pixel = $vacated.GetPixel($x, $y)
                if ($pixel.R -lt 250 -or $pixel.G -lt 250 -or $pixel.B -lt 250) { throw 'Indicator pixels remain at the old window position' }
            }
        }
    } finally { $graphics.Dispose(); $vacated.Dispose() }
    Start-FixtureProcess 'git' @('restore', '--source=HEAD', '--', 'source.rs') (Join-Path $fixture 'dirty') -Wait
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        $captions = @([WtmIndicatorProbe]::Children($window) | ForEach-Object { [WtmIndicatorProbe]::Text($_) }) -join "`n"
        if ($captions -match 'S \+0/-0 · T \+1/-0 · X \+1/-0 · binaries: 1') { break }
        Start-Sleep -Milliseconds 80
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($captions -notmatch 'S \+0/-0 · T \+1/-0 · X \+1/-0 · binaries: 1') { throw 'Source reset did not refresh' }
    Assert-Alpha 51
    $image = Capture-Screen 'source-reset'
    try { Assert-Pixels $image (Get-Regions) $false 'Source reset' } finally { $image.Dispose() }
    $popups = @([WtmIndicatorProbe]::Windows([uint32]$process.Id) | Where-Object { [WtmIndicatorProbe]::GetWindow($_, 4) -eq $window })
    [void][WtmIndicatorProbe]::PostMessageW($window, 0x10, [IntPtr]::Zero, [IntPtr]::Zero)
    if (-not $process.WaitForExit(10000) -or $process.ExitCode -ne 0) { throw 'Fixture did not close successfully' }
    foreach ($popup in $popups) { if ([WtmIndicatorProbe]::IsWindow($popup)) { throw 'Owned indicator popup survived the main window' } }
    Write-Host "PASS. Screen pixels verify selective opacity across hover, leave, movement, count reset, and close. Captures at $verificationEvidence"
} finally {
    if ($null -ne $process -and -not $process.HasExited) {
        if ($window -ne [IntPtr]::Zero) { [void][WtmIndicatorProbe]::PostMessageW($window, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) }
        if (-not $process.WaitForExit(5000)) { $process.Kill(); $process.WaitForExit() }
    }
    if ($backdrop -ne [IntPtr]::Zero) { [void][WtmIndicatorProbe]::DestroyWindow($backdrop) }
    [void][WtmIndicatorProbe]::SetCursorPos($savedCursor.X, $savedCursor.Y)
    if ($previousDpi -ne [IntPtr]::Zero) { [void][WtmIndicatorProbe]::SetThreadDpiAwarenessContext($previousDpi) }
    if ($null -ne $process) { $process.Dispose() }
}

} finally { Remove-VerificationDirectories }
