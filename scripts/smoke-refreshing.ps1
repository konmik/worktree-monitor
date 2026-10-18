param(
    [string]$Binary = "$PSScriptRoot/../target/debug/wtm-gui.exe",
    [string]$Output = "$env:LOCALAPPDATA/Temp/opencode/wtm-refreshing-$([Guid]::NewGuid().ToString('N'))",
    [switch]$SavedOrder,
    [switch]$MainOnly,
    [string]$Placement,
    [int]$PlacementWidth = 0
)

$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path $Binary).Path
$CliBinary = (Resolve-Path (Join-Path (Split-Path -Parent $Binary) 'wtm.exe')).Path
$Output = [IO.Path]::GetFullPath($Output)
New-Item -ItemType Directory -Path $Output -Force | Out-Null
$fixture = Join-Path $Output "fixture-$([Guid]::NewGuid().ToString('N'))"
$fixtureHome = Join-Path $fixture 'home'
$gates = Join-Path $fixture 'gates'
$shim = Join-Path $fixture 'bin'
New-Item -ItemType Directory -Path $fixtureHome, $gates, $shim | Out-Null
$git = (Get-Command git.exe -CommandType Application | Select-Object -First 1).Source
Add-Type -AssemblyName System.Drawing
$compiler = Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
if (-not (Test-Path -LiteralPath $compiler)) { throw 'The isolated Git delay fixture requires the Windows .NET Framework C# compiler.' }
if ($null -eq ('WtmRefreshingProbe' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class WtmRefreshingProbe {
    public struct Rect { public int Left, Top, Right, Bottom; }
    public struct Point { public int X, Y; }
    public struct MonitorInfo { public uint Size; public Rect Monitor, Work; public uint Flags; }
    public delegate bool EnumProc(IntPtr window, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr window, StringBuilder text, int count);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr window, StringBuilder text, int count);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr window, ref Point point);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("dwmapi.dll")] public static extern int DwmFlush();
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern IntPtr MonitorFromWindow(IntPtr window, uint flags);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern bool GetMonitorInfoW(IntPtr monitor, ref MonitorInfo info);
    [DllImport("user32.dll")] public static extern bool RedrawWindow(IntPtr window, IntPtr rect, IntPtr region, uint flags);
    [DllImport("user32.dll")] public static extern IntPtr GetDC(IntPtr window);
    [DllImport("user32.dll")] public static extern int ReleaseDC(IntPtr window, IntPtr dc);
    [DllImport("gdi32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr CreateFontW(int height, int width, int escapement, int orientation, int weight, uint italic, uint underline, uint strikeout, uint charset, uint precision, uint clipping, uint quality, uint family, string face);
    [DllImport("gdi32.dll", CharSet=CharSet.Unicode)] public static extern int GetTextFaceW(IntPtr dc, int count, StringBuilder face);
    [DllImport("gdi32.dll")] public static extern IntPtr SelectObject(IntPtr dc, IntPtr handle);
    [DllImport("gdi32.dll")] public static extern bool DeleteObject(IntPtr handle);
    [DllImport("gdi32.dll")] public static extern uint GetPixel(IntPtr dc, int x, int y);
    [DllImport("gdi32.dll", CharSet=CharSet.Unicode)] public static extern bool GetTextExtentPoint32W(IntPtr dc, string text, int count, out Point size);
    [DllImport("gdi32.dll")] public static extern IntPtr CreateCompatibleDC(IntPtr dc);
    [DllImport("gdi32.dll")] public static extern IntPtr CreateCompatibleBitmap(IntPtr dc, int width, int height);
    [DllImport("gdi32.dll")] public static extern bool DeleteDC(IntPtr dc);
    [DllImport("gdi32.dll")] public static extern IntPtr CreateSolidBrush(uint color);
    [DllImport("user32.dll")] public static extern int FillRect(IntPtr dc, ref Rect rect, IntPtr brush);
    [DllImport("gdi32.dll")] public static extern int SetBkMode(IntPtr dc, int mode);
    [DllImport("gdi32.dll")] public static extern uint SetTextColor(IntPtr dc, uint color);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int DrawTextW(IntPtr dc, string text, int count, ref Rect rect, uint flags);
    public static void VerifyMarker(IntPtr root, string name, bool repository, bool refreshing) {
        IntPtr row = IntPtr.Zero;
        EnumChildWindows(root, (window, data) => {
            if (Text(window).Split('\n')[0] == name && Text(window).Contains("Refreshing") == refreshing) row = window;
            return true;
        }, IntPtr.Zero);
        if (row == IntPtr.Zero) throw new InvalidOperationException("Expected refresh state unavailable: " + name);
        int dpi = (int)GetDpiForWindow(row);
        var dc = GetDC(row);
        var font = IntPtr.Zero;
        foreach (var face in new[] { "Berkeley Mono", "Berkeley Mono Variable", "Consolas", "Cascadia Mono", "Courier New" }) {
            var candidate = CreateFontW(-12 * dpi / 96, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 49, face);
            var saved = SelectObject(dc, candidate);
            var actual = new StringBuilder(64);
            GetTextFaceW(dc, actual.Capacity, actual);
            SelectObject(dc, saved);
            if (actual.ToString().Equals(face, StringComparison.OrdinalIgnoreCase)) { font = candidate; break; }
            DeleteObject(candidate);
        }
        if (font == IntPtr.Zero) throw new InvalidOperationException("Terminal font unavailable");
        var old = SelectObject(dc, font);
        Point extent, marker, space;
        GetTextExtentPoint32W(dc, name, name.Length, out extent);
        GetTextExtentPoint32W(dc, "*", 1, out marker);
        GetTextExtentPoint32W(dc, " ", 1, out space);
        var expected = CreateCompatibleDC(dc);
        var bitmap = CreateCompatibleBitmap(dc, marker.X, marker.Y);
        var previous = SelectObject(expected, bitmap);
        var oldFont = SelectObject(expected, font);
        uint background = 11u | (16u << 8) | (18u << 16);
        var brush = CreateSolidBrush(background);
        try {
            var rect = new Rect { Right = marker.X, Bottom = marker.Y };
            FillRect(expected, ref rect, brush);
            SetBkMode(expected, 1);
            SetTextColor(expected, 84u | (213u << 8) | (197u << 16));
            if (refreshing) DrawTextW(expected, "*", 1, ref rect, 0x0820);
            int left = Math.Max(0, 14 * dpi / 96 - space.X) + extent.X + space.X;
            int top = repository ? 8 * dpi / 96 : 0;
            int ink = 0;
            for (int y = 0; y < marker.Y; y++) {
                for (int gap = 1; gap <= space.X; gap++) {
                    if (GetPixel(dc, left - gap, top + y) != background) throw new InvalidOperationException("Missing one-space gap for " + name);
                }
                for (int x = 0; x < marker.X; x++) {
                    uint wanted = GetPixel(expected, x, y);
                    if (wanted != background) ink++;
                    if (GetPixel(dc, left + x, top + y) != wanted) throw new InvalidOperationException("Marker differs from measured name width plus one space: " + name);
                }
            }
            if (refreshing && ink == 0) throw new InvalidOperationException("Marker proof is empty");
        } finally {
            SelectObject(expected, oldFont); SelectObject(expected, previous);
            DeleteObject(brush); DeleteObject(bitmap); DeleteDC(expected);
            SelectObject(dc, old); DeleteObject(font); ReleaseDC(row, dc);
        }
    }
    public static string Text(IntPtr window) {
        var text = new StringBuilder(65536);
        GetWindowText(window, text, text.Capacity);
        return text.ToString();
    }
    public static string Bounds(IntPtr window) {
        Rect rect;
        if (!GetWindowRect(window, out rect)) throw new InvalidOperationException("Window bounds unavailable");
        return rect.Left + "," + rect.Top + "," + rect.Right + "," + rect.Bottom;
    }
    public static void AssertDefaultSize(IntPtr window) {
        var monitor = new MonitorInfo { Size = (uint)Marshal.SizeOf(typeof(MonitorInfo)) };
        if (!GetMonitorInfoW(MonitorFromWindow(window, 2), ref monitor)) throw new InvalidOperationException("Monitor unavailable");
        Rect rect;
        GetWindowRect(window, out rect);
        int dpi = (int)GetDpiForWindow(window);
        if (rect.Right - rect.Left != Math.Min(640 * dpi / 96, monitor.Work.Right - monitor.Work.Left) ||
            rect.Bottom - rect.Top != Math.Min(540 * dpi / 96, monitor.Work.Bottom - monitor.Work.Top))
            throw new InvalidOperationException("First visible window does not have its intended default size");
    }
    public static int RowHeight(IntPtr root, string firstLine) {
        int height = 0;
        EnumChildWindows(root, (window, data) => {
            if (Text(window).Split('\n')[0] == firstLine) {
                Rect rect;
                if (GetWindowRect(window, out rect)) height = rect.Bottom - rect.Top;
            }
            return true;
        }, IntPtr.Zero);
        if (height == 0) throw new InvalidOperationException("Row height unavailable: " + firstLine);
        return height;
    }
    public static IntPtr Find(uint pid) {
        var found = IntPtr.Zero;
        EnumWindows((window, data) => {
            uint owner;
            GetWindowThreadProcessId(window, out owner);
            if (owner != pid || !IsWindowVisible(window) || !Text(window).StartsWith("WTM")) return true;
            found = window;
            return false;
        }, IntPtr.Zero);
        return found;
    }
    public static string[] Captions(IntPtr parent) {
        var captions = new List<Tuple<int, string>>();
        EnumChildWindows(parent, (window, data) => {
            var name = new StringBuilder(128);
            GetClassName(window, name, name.Capacity);
            if (IsWindowVisible(window) && String.Equals(name.ToString(), "STATIC", StringComparison.OrdinalIgnoreCase)) {
                var text = Text(window);
                Rect rect;
                if (text.Length != 0 && GetWindowRect(window, out rect)) captions.Add(Tuple.Create(rect.Top, text));
            }
            return true;
        }, IntPtr.Zero);
        captions.Sort((left, right) => left.Item1.CompareTo(right.Item1));
        return captions.ConvertAll(entry => entry.Item2).ToArray();
    }
}
'@
}

$source = @'
using System;
using System.Diagnostics;
using System.IO;
using System.Text;
using System.Threading;
public static class GitDelay {
    static string Quote(string value) {
        var result = new StringBuilder("\"");
        int slashes = 0;
        foreach (char c in value) {
            if (c == '\\') { slashes++; continue; }
            if (c == '"') { result.Append('\\', slashes * 2 + 1); result.Append(c); slashes = 0; continue; }
            result.Append('\\', slashes); slashes = 0; result.Append(c);
        }
        result.Append('\\', slashes * 2); result.Append('"');
        return result.ToString();
    }
    public static int Main(string[] args) {
        string cwd = Environment.CurrentDirectory;
        bool scan = false;
        bool repository = false;
        for (int i = 0; i < args.Length; i++) {
            if (args[i] == "-C" && i + 1 < args.Length) cwd = Path.GetFullPath(Path.Combine(cwd, args[++i]));
            else if (args[i] == "--git-dir" && i + 1 < args.Length) { cwd = Path.GetDirectoryName(args[++i]); repository = true; }
            else if (args[i] == "status" || args[i] == "diff") scan = true;
        }
        string gate = Path.Combine(Environment.GetEnvironmentVariable("WTM_REFRESH_GATES"), new DirectoryInfo(cwd).Name + (repository ? ".repository" : ""));
        bool claimed = false;
        if (scan || repository) {
            try { File.Move(gate + ".gate", gate + ".active"); claimed = true; }
            catch (IOException) { }
        }
        try {
            if (claimed) Thread.Sleep(1800);
            var command = new StringBuilder();
            foreach (string arg in args) { if (command.Length != 0) command.Append(' '); command.Append(Quote(arg)); }
            var start = new ProcessStartInfo(Environment.GetEnvironmentVariable("WTM_REFRESH_REAL_GIT"), command.ToString());
            start.UseShellExecute = false;
            using (var child = Process.Start(start)) { child.WaitForExit(); return child.ExitCode; }
        } finally {
            if (claimed) File.Move(gate + ".active", gate + ".done");
        }
    }
}
'@
[IO.File]::WriteAllText((Join-Path $shim 'git-delay.cs'), $source)
& $compiler /nologo /target:exe "/out:$(Join-Path $shim 'git.exe')" (Join-Path $shim 'git-delay.cs')
if ($LASTEXITCODE -ne 0) { throw 'Isolated Git delay fixture compilation failed.' }

function Run-Git([string]$Repo, [string[]]$Arguments) {
    & $git -c core.autocrlf=false -c commit.gpgsign=false -c "core.hooksPath=$fixture" -C $Repo @Arguments | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Fixture Git command failed: $Arguments" }
}

function Counts([string]$Caption) {
    return (([regex]::Matches($Caption, '[STX]\s+\+\d+/-\d+') | ForEach-Object { $_.Value -replace '\s+', ' ' }) -join ' | ')
}

function Is-Refreshing([string]$Caption) {
    return $Caption -match '\bRefreshing\b'
}

function Capture-Refreshing([string]$Name = 'refreshing') {
    $bounds = [WtmRefreshingProbe+Rect]::new()
    $origin = [WtmRefreshingProbe+Point]::new()
    if (-not [WtmRefreshingProbe]::GetClientRect($script:window, [ref]$bounds) -or
        -not [WtmRefreshingProbe]::ClientToScreen($script:window, [ref]$origin)) { throw 'Refreshing capture bounds unavailable.' }
    $image = [Drawing.Bitmap]::new($bounds.Right, $bounds.Bottom)
    $graphics = [Drawing.Graphics]::FromImage($image)
    try {
        [void][WtmRefreshingProbe]::DwmFlush()
        $graphics.CopyFromScreen($origin.X, $origin.Y, 0, 0, $image.Size)
        $image.Save((Join-Path $Output "$Name.png"))
    } finally { $graphics.Dispose(); $image.Dispose() }
}

function Set-FixtureSize([int]$Width) {
    if (-not [WtmRefreshingProbe]::SetWindowPos($script:window, [IntPtr]::Zero, 32, 32, $Width, 720, 20)) { throw 'Fixture resize failed' }
    $script:expectedBounds = "32,32,$(32 + $Width),752"
    if ([WtmRefreshingProbe]::Bounds($script:window) -ne $script:expectedBounds) { throw 'Manual resize did not apply the requested bounds' }
    [void][WtmRefreshingProbe]::RedrawWindow($script:window, [IntPtr]::Zero, [IntPtr]::Zero, 0x0180)
    [void][WtmRefreshingProbe]::DwmFlush()
}

function Sample {
    if ($script:process.HasExited) { throw 'The isolated dashboard exited during observation.' }
    if ($script:window -eq [IntPtr]::Zero) { $script:window = [WtmRefreshingProbe]::Find([uint32]$script:process.Id) }
    $bounds = $null
    if ($script:window -ne [IntPtr]::Zero) {
        $bounds = [WtmRefreshingProbe]::Bounds($script:window)
        if ($null -eq $script:expectedBounds) {
            if ($Placement) {
                if ($bounds -ne $script:savedBounds) { throw "First visible window has bounds $bounds instead of saved bounds $script:savedBounds" }
            } else { [WtmRefreshingProbe]::AssertDefaultSize($script:window) }
            $script:expectedBounds = $bounds
            $proof.first_visible_bounds = $bounds
        }
        if ($bounds -ne $script:expectedBounds) { throw "Window resized without a manual resize: $script:expectedBounds to $bounds" }
    }
    $start = $clock.Elapsed.TotalMilliseconds
    $activeGates = @{}
    foreach ($row in $expected) {
        if ($row.gate -and (Test-Path -LiteralPath (Join-Path $gates "$($row.gate).active"))) { $activeGates[$row.gate] = $true }
    }
    $captions = if ($script:window -ne [IntPtr]::Zero) { @([WtmRefreshingProbe]::Captions($script:window)) } else { @() }
    $rows = @{}
    foreach ($row in $expected) {
        if ($row.gate) {
            $found = @($captions | Where-Object { ($_ -split '\r?\n', 2)[0] -ceq $row.branch })
            if ($found.Count -eq 0) {
                $found = @($captions | Where-Object {
                    $first = (($_ -split '\r?\n', 2)[0]) -replace '[\\/]+$', ''
                    [IO.Path]::IsPathFullyQualified($first) -and ($first -ieq $row.path -or $first -imatch ('(?:^|[\\/])' + [regex]::Escape($row.gate) + '$'))
                })
            }
        } else {
            $found = @($captions | Where-Object { ($_ -split '\r?\n', 2)[0] -match $row.pattern })
        }
        if ($found.Count -gt 1) { throw "Ambiguous native STATIC caption for $($row.key)." }
        if ($found.Count -eq 1) { $rows[$row.key] = $found[0] }
    }
    foreach ($gate in @($activeGates.Keys)) {
        if (-not (Test-Path -LiteralPath (Join-Path $gates "$gate.active"))) { $activeGates.Remove($gate) }
    }
    $sample = [pscustomobject]@{ start_ms = $start; end_ms = $clock.Elapsed.TotalMilliseconds; rows = $rows; captions = $captions; active_gates = $activeGates; window_bounds = $bounds }
    $samples.Add($sample)
    if ($SavedOrder) {
        $actual = @(foreach ($caption in $captions) {
            foreach ($key in $startupOrder) { if ($rows.ContainsKey($key) -and $rows[$key] -ceq $caption) { $key } }
        })
        $wanted = @($startupOrder | Where-Object { $rows.ContainsKey($_) })
        if (($actual -join ',') -ne ($wanted -join ',')) {
            throw "Startup order changed while scanning. Expected $($wanted -join ','); got $($actual -join ',')."
        }
    }
    return $sample
}

function Track($Sample, [hashtable]$Tracks, [string[]]$Keys) {
    foreach ($key in $Keys) {
        $refreshing = $Sample.rows.ContainsKey($key) -and (Is-Refreshing $Sample.rows[$key])
        if (-not $Tracks.ContainsKey($key)) {
            $Tracks[$key] = @{ first = $null; last = $null; absent = $null; previous = $null; appearance_after = $null; completed = [Collections.Generic.List[object]]::new() }
        }
        $track = $Tracks[$key]
        if ($refreshing) {
            if ($null -ne $track.absent) {
                $track.completed.Add(@{ first = $track.first; last = $track.last; absent = $track.absent; appearance_after = $track.appearance_after })
                $track.first = $null
                $track.last = $null
                $track.absent = $null
            }
            if ($null -eq $track.first) { $track.first = $Sample.end_ms; $track.appearance_after = $track.previous }
            $track.last = $Sample.start_ms
        } elseif ($null -ne $track.first -and $null -eq $track.absent) {
            $track.absent = $Sample.end_ms
        }
        $track.previous = $Sample.start_ms
    }
}

function Assert-Holds([hashtable]$Tracks, [string[]]$Keys) {
    foreach ($key in $Keys) {
        $track = $Tracks[$key]
        if ($null -eq $track -or $null -eq $track.first -or $null -eq $track.absent) { throw "Missing complete Refreshing presentation for $key." }
        foreach ($episode in @($track.completed.ToArray()) + @($track)) {
            $episode.conservative_visible_ms = $episode.last - $episode.first
            $observed = $episode.absent - $episode.first
            if ($observed -lt (1000 - $pollMs)) {
                throw "Refreshing for $key lasted only $([Math]::Round($observed)) ms from first present to first absent observation. Require >= $((1000 - $pollMs)) ms with one $pollMs ms polling interval of tolerance; scan start is not UI presentation. Conservative first-to-last-present lower bound: $($episode.conservative_visible_ms) ms. Appearance window: ($($episode.appearance_after), $($episode.first)] ms."
            }
        }
    }
}

$names = @('HOME', 'USERPROFILE', 'PATH', 'GIT_DIR', 'GIT_WORK_TREE', 'GIT_COMMON_DIR', 'GIT_INDEX_FILE', 'GIT_CONFIG_GLOBAL', 'GIT_CONFIG_NOSYSTEM', 'WTM_REFRESH_GATES', 'WTM_REFRESH_REAL_GIT')
$previous = @{}
foreach ($name in $names) { $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
$script:process = $null
$script:window = [IntPtr]::Zero
$script:expectedBounds = $null
$samples = [Collections.Generic.List[object]]::new()
$expected = [Collections.Generic.List[object]]::new()
$pollMs = 50
$clock = [Diagnostics.Stopwatch]::new()
$previousDpi = [WtmRefreshingProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
$proof = [ordered]@{ result = 'FAIL'; binary = $Binary; sha256 = (Get-FileHash $Binary).Hash; fixture = $fixture; poll_ms = $pollMs; timing = 'Require first-present to first-absent >= 1000 ms minus one polling interval. First/last present samples also give a conservative visible lower bound. Appearance is bracketed by previous absent and first present observations.'; initial = $null; refresh = $null; fast_refresh = $null }
$proof.marker_placement = [Collections.Generic.List[object]]::new()
try {
    $env:HOME = $fixtureHome
    $env:USERPROFILE = $fixtureHome
    $env:GIT_CONFIG_GLOBAL = Join-Path $fixture 'empty-gitconfig'
    $env:GIT_CONFIG_NOSYSTEM = '1'
    [IO.File]::WriteAllText($env:GIT_CONFIG_GLOBAL, '')
    foreach ($name in @('GIT_DIR', 'GIT_WORK_TREE', 'GIT_COMMON_DIR', 'GIT_INDEX_FILE')) { Remove-Item "Env:$name" -ErrorAction SilentlyContinue }
    foreach ($name in @('alpha', 'beta')) {
        $repo = Join-Path $fixture $name
        $tree = Join-Path $fixture "$name-tree"
        New-Item -ItemType Directory -Path $repo | Out-Null
        Run-Git $repo @('init', '--quiet', '-b', "$name-main")
        [IO.File]::WriteAllText((Join-Path $repo 'source.rs'), "before`n")
        Run-Git $repo @('add', '.')
        Run-Git $repo @('-c', 'user.name=Wtm Test', '-c', 'user.email=wtm@example.invalid', 'commit', '--quiet', '-m', 'Fixture')
        if (-not $MainOnly) { Run-Git $repo @('worktree', 'add', '--quiet', '-b', "$name-topic", $tree) }
        $expected.Add(@{ key = "repo:$name"; pattern = '^' + [regex]::Escape($name) + '$' })
        if ($MainOnly) { [IO.File]::WriteAllText((Join-Path $gates "$name.repository.gate"), '') }
        $entries = @(@{ path = $repo; branch = "$name-main" })
        if (-not $MainOnly) { $entries += @{ path = $tree; branch = "$name-topic" } }
        foreach ($entry in $entries) {
            [IO.File]::WriteAllText((Join-Path $entry.path 'source.rs'), "before`nold`n")
            $expected.Add(@{ key = "tree:$($entry.branch)"; branch = $entry.branch; path = $entry.path; gate = Split-Path -Leaf $entry.path; parent = "repo:$name" })
            [IO.File]::WriteAllText((Join-Path $gates "$(Split-Path -Leaf $entry.path).gate"), '')
        }
        & $CliBinary add $repo | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Isolated repository registration failed.' }
    }
    $startupOrder = @('repo:beta', 'tree:beta-topic', 'tree:beta-main', 'repo:alpha', 'tree:alpha-topic', 'tree:alpha-main')
    if ($SavedOrder) {
        $preferences = [ordered]@{ version = 1; repositories = @((Join-Path $fixture 'beta/.git'), (Join-Path $fixture 'alpha/.git')); worktrees = @(
            @{ repository = Join-Path $fixture 'beta/.git'; paths = @((Join-Path $fixture 'beta-tree'), (Join-Path $fixture 'beta')) },
            @{ repository = Join-Path $fixture 'alpha/.git'; paths = @((Join-Path $fixture 'alpha-tree'), (Join-Path $fixture 'alpha')) }
        ) }
        $preferences | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $fixtureHome '.config/wtm/config.order.json') -Encoding utf8NoBOM
    }
    if ($Placement) {
        $saved = Get-Content $Placement -Raw | ConvertFrom-Json
        if ($PlacementWidth -gt 0) { $saved.normal.right = $saved.normal.left + $PlacementWidth }
        $saved.state = 'normal'
        $script:savedBounds = "$($saved.normal.left),$($saved.normal.top),$($saved.normal.right),$($saved.normal.bottom)"
        $saved | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $fixtureHome '.config/wtm/config.placement.json') -Encoding utf8NoBOM
    }
    $env:WTM_REFRESH_GATES = $gates
    $env:WTM_REFRESH_REAL_GIT = $git
    $env:PATH = "$shim;$($previous.PATH)"
    $start = [Diagnostics.ProcessStartInfo]::new($Binary)
    $start.WorkingDirectory = $fixtureHome
    $start.UseShellExecute = $false
    $start.Environment['HOME'] = $fixtureHome
    $start.Environment['USERPROFILE'] = $fixtureHome
    $clock.Start()
    $script:process = [Diagnostics.Process]::Start($start)
    $proof.pid = $script:process.Id
    $initial = @{}
    $proof.initial = $initial
    $keys = @($expected | ForEach-Object { $_.key })
    $pendingSeen = @{}
    $pendingHeights = @{}
    $initialActiveSeen = @{}
    $repositoryMarkers = @{}
    $deadline = $clock.Elapsed.TotalSeconds + 55
    do {
        $sample = Sample
        Track $sample $initial $keys
        if ($MainOnly) {
            foreach ($name in @('alpha', 'beta')) {
                if (-not $repositoryMarkers.ContainsKey($name) -and (Test-Path -LiteralPath (Join-Path $gates "$name.repository.active")) -and
                    $sample.rows.ContainsKey("repo:$name") -and (Is-Refreshing $sample.rows["repo:$name"])) {
                    foreach ($width in @(800, 380)) {
                        Set-FixtureSize $width
                        [WtmRefreshingProbe]::VerifyMarker($script:window, $name, $true, $true)
                        $proof.marker_placement.Add(@{ name = $name; width = $width; dpi = [WtmRefreshingProbe]::GetDpiForWindow($script:window); character_space_gap = 1 })
                        Capture-Refreshing "repository-$name-$width"
                    }
                    $repositoryMarkers[$name] = $true
                }
            }
        }
        foreach ($row in $expected) {
            if (-not $row.gate -or -not $sample.rows.ContainsKey($row.key)) { continue }
            $base = Join-Path $gates $row.gate
            if (-not (Test-Path -LiteralPath "$base.done")) {
                $caption = $sample.rows[$row.key]
                if ($caption -match '(?i)\bpending\b|Waiting for the first snapshot|Scanning repositories') { throw "Startup placeholder displayed for $($row.key)." }
                $content = $caption -replace '\r?\nRefreshing$', ''
                if ($content.Contains("`n")) { throw "Startup worktree contains an extra line: $content" }
                if ($caption -match '\bS\s+\+0/-0\b') { throw "Initial pending $($row.key) fabricated zero source counts." }
                if ((Counts $caption) -ne '') { throw "Initial pending $($row.key) published counts before its first delayed Git scan completed." }
                $pendingHeights[$row.key] = [WtmRefreshingProbe]::RowHeight($script:window, ($caption -split '\r?\n')[0])
                $pendingSeen[$row.key] = $true
            }
        }
        foreach ($row in $expected | Where-Object { $_.gate -and $sample.active_gates.ContainsKey($_.gate) }) {
            $indicated = $true
            foreach ($key in @($row.key)) {
                if (-not $sample.rows.ContainsKey($key) -or -not (Is-Refreshing $sample.rows[$key])) { $indicated = $false }
            }
            if ($indicated) { $initialActiveSeen[$row.key] = $true }
            if ($MainOnly -and $indicated -and -not $pendingSeen.ContainsKey("captured:$($row.key)")) {
                foreach ($width in @(800, 380)) {
                    Set-FixtureSize $width
                    Capture-Refreshing "startup-$($row.gate)-$width"
                }
                $pendingSeen["captured:$($row.key)"] = $true
            }
        }
        foreach ($name in @('alpha', 'beta')) {
            $key = "repo:$name"
            $pending = -not (Test-Path -LiteralPath (Join-Path $gates "$name.done")) -or (-not $MainOnly -and -not (Test-Path -LiteralPath (Join-Path $gates "$name-tree.done")))
            if ($pending -and $sample.rows.ContainsKey($key)) {
                if ($sample.rows[$key] -match '\bS\s+\+0/-0\b') { throw "Initial pending $key fabricated zero source counts." }
            }
        }
        $ready = $sample.rows.Count -eq $keys.Count
        foreach ($row in $expected) {
            if (-not $sample.rows.ContainsKey($row.key) -or (Is-Refreshing $sample.rows[$row.key])) { $ready = $false; continue }
            if ($row.gate -and $sample.rows[$row.key] -notmatch '\bS\s+\+1/-0\b') { $ready = $false }
        }
        if ($ready) { break }
        Start-Sleep -Milliseconds $pollMs
    } while ($clock.Elapsed.TotalSeconds -lt $deadline)
    if (-not $ready) { throw 'Initial Refreshing contract did not settle within 55 seconds; native integration may be pending.' }
    foreach ($row in $expected | Where-Object { $_.gate }) {
        if (-not $pendingSeen.ContainsKey($row.key)) { throw "No initial pending observation for $($row.key). Native integration must expose pending rows before the delayed first scan completes; integration may be pending." }
        if (-not $initialActiveSeen.ContainsKey($row.key)) { throw "No initial active-scan observation for $($row.key)." }
        $loadedHeight = [WtmRefreshingProbe]::RowHeight($script:window, ($sample.rows[$row.key] -split '\r?\n')[0])
        if ($loadedHeight -ne $pendingHeights[$row.key]) { throw "Count arrival changed the height of $($row.key) from $($pendingHeights[$row.key]) to $loadedHeight" }
    }
    Assert-Holds $initial $keys
    if ($MainOnly -and $repositoryMarkers.Count -ne 2) { throw 'Repository discovery markers were not verified' }
    $proof.initial = $initial
    $baseline = $sample.rows.Clone()
    $target = if ($MainOnly) { 'tree:alpha-main' } else { 'tree:alpha-topic' }
    $affected = @($target)
    $targetDirectory = if ($MainOnly) { 'alpha' } else { 'alpha-tree' }
    $gate = Join-Path $gates $targetDirectory
    Remove-Item -LiteralPath "$gate.done"
    [IO.File]::WriteAllText("$gate.gate", '')
    [IO.File]::WriteAllText((Join-Path $fixture "$targetDirectory/source.rs"), "before`nold`nnew-one`nnew-two`n")
    $edited = $clock.Elapsed.TotalMilliseconds
    $refresh = @{}
    $retained = @{}
    $activeSeen = $false
    $deadline = $clock.Elapsed.TotalSeconds + 35
    do {
        $sample = Sample
        Track $sample $refresh $affected
        foreach ($key in $keys | Where-Object { $_ -notin $affected }) {
            if (-not $sample.rows.ContainsKey($key)) { throw "Unaffected row $key disappeared." }
            if (Is-Refreshing $sample.rows[$key]) { throw "Unaffected row $key refreshed after only $target changed." }
            if ((Counts $sample.rows[$key]) -ne (Counts $baseline[$key])) { throw "Unaffected row $key changed counts." }
        }
        if ($sample.active_gates.ContainsKey($targetDirectory)) {
            $indicated = $true
            foreach ($key in $affected) {
                if (-not $sample.rows.ContainsKey($key)) { throw "Actively scanned $key disappeared." }
                if (-not (Is-Refreshing $sample.rows[$key])) { $indicated = $false }
                if ((Counts $sample.rows[$key]) -ne (Counts $baseline[$key])) { throw "Actively scanned $key did not preserve its last counts." }
                if (Is-Refreshing $sample.rows[$key]) { $retained[$key] = $true }
            }
            if ($indicated -and -not $activeSeen) {
                if ($MainOnly) {
                    foreach ($width in @(800, 380)) {
                        Set-FixtureSize $width
                        [WtmRefreshingProbe]::VerifyMarker($script:window, 'alpha-main', $false, $true)
                        $proof.marker_placement.Add(@{ name = 'alpha-main'; width = $width; dpi = [WtmRefreshingProbe]::GetDpiForWindow($script:window); character_space_gap = 1 })
                        Capture-Refreshing "worktree-$width"
                    }
                } else { Capture-Refreshing }
            }
            if ($indicated) { $activeSeen = $true }
        }
        $ready = $activeSeen -and $sample.rows.ContainsKey($target) -and $sample.rows[$target] -match '\bS\s+\+3/-0\b'
        foreach ($key in $affected) {
            if (-not $refresh.ContainsKey($key) -or $null -eq $refresh[$key].absent) { $ready = $false }
        }
        if ($ready) { break }
        Start-Sleep -Milliseconds $pollMs
    } while ($clock.Elapsed.TotalSeconds -lt $deadline)
    if (-not $ready) { throw 'The scheduled 10-second refresh did not expose its worktree indicator and updated counts within 35 seconds.' }
    Assert-Holds $refresh $affected
    foreach ($key in $affected) { if (-not $retained.ContainsKey($key)) { throw "No retained-count observation for $key." } }
    $proof.refresh = [ordered]@{ edited_ms = $edited; tracks = $refresh; retained = $retained; baseline = $baseline; updated = $sample.rows; delayed_git_observed = $activeSeen; unaffected_siblings_stable = $true }
    $fast = @{}
    $proof.fast_refresh = $fast
    [IO.File]::WriteAllText((Join-Path $fixture "$targetDirectory/source.rs"), "before`nold`nnew-one`nnew-two`nfast`n")
    $deadline = $clock.Elapsed.TotalSeconds + 35
    do {
        $sample = Sample
        Track $sample $fast $affected
        foreach ($key in $keys | Where-Object { $_ -notin $affected }) {
            if (-not $sample.rows.ContainsKey($key) -or (Is-Refreshing $sample.rows[$key])) { throw "Unaffected sibling $key refreshed or disappeared during the fast scan." }
            if ((Counts $sample.rows[$key]) -ne (Counts $baseline[$key])) { throw "Unaffected sibling $key changed counts during the fast scan." }
        }
        $ready = $sample.rows.ContainsKey($target) -and $sample.rows[$target] -match '\bS\s+\+4/-0\b'
        foreach ($key in $affected) {
            if (-not $fast.ContainsKey($key) -or $null -eq $fast[$key].absent) { $ready = $false }
        }
        if ($ready) { break }
        Start-Sleep -Milliseconds $pollMs
    } while ($clock.Elapsed.TotalSeconds -lt $deadline)
    if (-not $ready) { throw 'Undelayed scan did not expose latched Refreshing captions and updated counts within 35 seconds.' }
    Assert-Holds $fast $affected
    if ($MainOnly) {
        [void][WtmRefreshingProbe]::RedrawWindow($script:window, [IntPtr]::Zero, [IntPtr]::Zero, 0x0180)
        [void][WtmRefreshingProbe]::DwmFlush()
        foreach ($name in @('alpha', 'beta')) {
            [WtmRefreshingProbe]::VerifyMarker($script:window, $name, $true, $false)
            [WtmRefreshingProbe]::VerifyMarker($script:window, "$name-main", $false, $false)
        }
    }
    if (-not [WtmRefreshingProbe]::PostMessage($script:window, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) { throw 'Isolated dashboard close failed.' }
    if (-not $script:process.WaitForExit(10000) -or $script:process.ExitCode -ne 0) { throw 'Isolated dashboard did not exit cleanly.' }
    $proof.result = 'PASS'
    $proof.window_bounds_stable = $true
    Write-Host "PASS. Refreshing captions, pending counts, sibling isolation, and minimum visibility. Evidence: $Output"
} catch {
    $proof.error = $_.Exception.Message
    throw
} finally {
    $ownedChildren = [Collections.Generic.List[Diagnostics.Process]]::new()
    if ($null -ne $script:process -and -not $script:process.HasExited) {
        $parents = [Collections.Generic.Queue[int]]::new()
        $parents.Enqueue($script:process.Id)
        while ($parents.Count -gt 0) {
            $owner = $parents.Dequeue()
            foreach ($child in @(Get-CimInstance Win32_Process -Filter "ParentProcessId = $owner")) {
                try {
                    $owned = [Diagnostics.Process]::GetProcessById([int]$child.ProcessId)
                    [void]$owned.Handle
                    $ownedChildren.Add($owned)
                    $parents.Enqueue($owned.Id)
                } catch [ArgumentException] {} catch [InvalidOperationException] {}
            }
        }
    }
    if ($null -ne $script:process) {
        try {
            if (-not $script:process.HasExited) {
                if ($script:window -ne [IntPtr]::Zero) { [void][WtmRefreshingProbe]::PostMessage($script:window, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) }
                if (-not $script:process.WaitForExit(3000)) { $script:process.Kill($true); $script:process.WaitForExit() }
            }
        } catch [InvalidOperationException] {
        } finally { $script:process.Dispose() }
    }
    foreach ($owned in $ownedChildren) {
        try {
            if (-not $owned.HasExited) { $owned.Kill($true); $owned.WaitForExit() }
        } catch [InvalidOperationException] {
        } finally { $owned.Dispose() }
    }
    foreach ($name in $names) {
        if ($null -eq $previous[$name]) { Remove-Item "Env:$name" -ErrorAction SilentlyContinue }
        else { [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process') }
    }
    if ($previousDpi -ne [IntPtr]::Zero) { [void][WtmRefreshingProbe]::SetThreadDpiAwarenessContext($previousDpi) }
    $proof | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $Output 'proof.json') -Encoding utf8NoBOM
    $samples | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $Output 'samples.json') -Encoding utf8NoBOM
}
