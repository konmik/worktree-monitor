param(
    [string]$Binary = "$PSScriptRoot/../target/debug/wts-ui.exe",
    [string]$Output = "$env:LOCALAPPDATA/Temp/opencode/wts-visibility-$([Guid]::NewGuid().ToString('N'))",
    [switch]$Baseline,
    [switch]$VisibilityOnly
)

$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path $Binary).Path
$CliBinary = (Resolve-Path (Join-Path (Split-Path -Parent $Binary) 'wts.exe')).Path
$Output = [IO.Path]::GetFullPath($Output)
New-Item -ItemType Directory -Path $Output -Force | Out-Null

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

public static class WtsVisibilityNativeProbe {
    public struct Point { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] public struct MouseInput {
        public int X, Y;
        public uint Data, Flags, Time;
        public UIntPtr Extra;
    }
    [StructLayout(LayoutKind.Explicit)] public struct InputData {
        [FieldOffset(0)] public MouseInput Mouse;
    }
    [StructLayout(LayoutKind.Sequential)] public struct Input {
        public uint Type;
        public InputData Data;
    }
    public struct Rect { public int Left, Top, Right, Bottom; }
    public struct GuiInfo {
        public uint Size, Flags;
        public IntPtr Active, Focus, Capture, MenuOwner, MoveSize, Caret;
        public Rect CaretRect;
    }
    public struct ScrollInfo {
        public uint Size, Mask;
        public int Min, Max;
        public uint Page;
        public int Position, TrackPosition;
    }
    public sealed class WindowInfo {
        public IntPtr Handle;
        public IntPtr Parent;
        public string Text;
        public string Class;
        public Rect Bounds;
    }
    public sealed class UpdateInfo {
        public bool Pending;
        public Rect Bounds;
        public Rect[] Regions;
    }
    public delegate bool EnumProc(IntPtr handle, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr handle, out uint processId);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr handle, StringBuilder text, int count);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr handle, StringBuilder text, int count);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr handle);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr handle);
    [DllImport("user32.dll")] public static extern bool IsWindowEnabled(IntPtr handle);
    [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out Point point);
    [DllImport("user32.dll")] public static extern uint SendInput(uint count, Input[] inputs, int size);
    [DllImport("user32.dll", EntryPoint = "GetWindowLongW")] public static extern int GetWindowLong(IntPtr handle, int index);
    [DllImport("user32.dll")] public static extern IntPtr GetParent(IntPtr handle);
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr handle, int id);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr handle, out Rect rect);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr handle, out Rect rect);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr handle, ref Point point);
    [DllImport("user32.dll")] public static extern bool GetScrollInfo(IntPtr handle, int bar, ref ScrollInfo info);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr handle, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr handle);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] public static extern int GetUpdateRgn(IntPtr handle, IntPtr region, bool erase);
    [DllImport("gdi32.dll")] public static extern IntPtr CreateRectRgn(int left, int top, int right, int bottom);
    [DllImport("gdi32.dll")] public static extern uint GetRegionData(IntPtr region, uint size, IntPtr data);
    [DllImport("gdi32.dll")] public static extern bool DeleteObject(IntPtr handle);
    [DllImport("user32.dll")] public static extern bool UpdateWindow(IntPtr handle);
    [DllImport("user32.dll")] public static extern bool ValidateRect(IntPtr handle, IntPtr rect);
    [DllImport("user32.dll")] public static extern IntPtr GetDC(IntPtr handle);
    [DllImport("user32.dll")] public static extern int ReleaseDC(IntPtr handle, IntPtr dc);
    [DllImport("user32.dll", EntryPoint = "SendMessageTimeoutW")] public static extern IntPtr SendMessageTimeout(IntPtr handle, uint message, IntPtr wparam, IntPtr lparam, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr handle, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr handle, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] public static extern int MapWindowPoints(IntPtr from, IntPtr to, ref Point points, uint count);
    [DllImport("user32.dll")] public static extern IntPtr SetFocus(IntPtr handle);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr handle);
    [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint sourceThread, uint targetThread, bool attach);
    [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
    [DllImport("gdi32.dll")] public static extern uint GetPixel(IntPtr dc, int x, int y);

    public static Rect ClientBounds(IntPtr handle) {
        Rect rect;
        if (!GetClientRect(handle, out rect)) throw new InvalidOperationException("Client bounds unavailable");
        return rect;
    }

    public static Rect WindowBounds(IntPtr handle) {
        Rect rect;
        if (!GetWindowRect(handle, out rect)) throw new InvalidOperationException("Window bounds unavailable");
        return rect;
    }

    public static ScrollInfo Scroll(IntPtr handle) {
        var info = new ScrollInfo { Size = (uint)Marshal.SizeOf<ScrollInfo>(), Mask = 7 };
        var scrollbar = GetDlgItem(GetParent(handle), 1003);
        if (!GetScrollInfo(scrollbar, 2, ref info)) throw new InvalidOperationException("Scroll state unavailable");
        return info;
    }

    public static void MoveDragPointer(IntPtr viewport, int x, int y) {
        var point = new Point { X = x, Y = y };
        if (!ClientToScreen(viewport, ref point) || !SetCursorPos(point.X, point.Y)) throw new InvalidOperationException("Drag pointer could not be positioned");
        Send(viewport, 0x0200, new IntPtr(1), Position(x, y));
    }

    public static string Text(IntPtr handle) {
        var text = new StringBuilder(65536);
        GetWindowText(handle, text, text.Capacity);
        return text.ToString();
    }

    public static string Class(IntPtr handle) {
        var text = new StringBuilder(256);
        GetClassName(handle, text, text.Capacity);
        return text.ToString();
    }

    public static WindowInfo[] Children(IntPtr parent) {
        var windows = new List<WindowInfo>();
        EnumChildWindows(parent, (handle, data) => {
            Rect bounds;
            GetWindowRect(handle, out bounds);
            windows.Add(new WindowInfo { Handle = handle, Parent = GetParent(handle), Text = Text(handle), Class = Class(handle), Bounds = bounds });
            return true;
        }, IntPtr.Zero);
        return windows.ToArray();
    }

    public static IntPtr Find(uint processId) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((handle, data) => {
            uint owner;
            GetWindowThreadProcessId(handle, out owner);
            if (owner == processId && IsWindowVisible(handle) && Text(handle).StartsWith("WTS", StringComparison.OrdinalIgnoreCase)) {
                found = handle;
                return false;
            }
            return true;
        }, IntPtr.Zero);
        return found;
    }

    public static UpdateInfo Update(IntPtr handle) {
        var region = CreateRectRgn(0, 0, 0, 0);
        if (region == IntPtr.Zero) throw new InvalidOperationException("Update region unavailable");
        try {
            var kind = GetUpdateRgn(handle, region, false);
            if (kind == 0) throw new InvalidOperationException("Update region query failed");
            if (kind == 1) return new UpdateInfo { Regions = Array.Empty<Rect>() };
            var size = GetRegionData(region, 0, IntPtr.Zero);
            var data = Marshal.AllocHGlobal(checked((int)size));
            try {
                if (GetRegionData(region, size, data) == 0) throw new InvalidOperationException("Update region data unavailable");
                var count = Marshal.ReadInt32(data, 8);
                var rectangles = new Rect[count];
                for (var index = 0; index < count; index++) rectangles[index] = Marshal.PtrToStructure<Rect>(IntPtr.Add(data, 32 + index * 16));
                return new UpdateInfo { Pending = true, Bounds = Marshal.PtrToStructure<Rect>(IntPtr.Add(data, 16)), Regions = rectangles };
            } finally { Marshal.FreeHGlobal(data); }
        } finally { DeleteObject(region); }
    }

    public static GuiInfo ThreadInfo(IntPtr window) {
        uint owner;
        uint thread = GetWindowThreadProcessId(window, out owner);
        var info = new GuiInfo { Size = (uint)Marshal.SizeOf(typeof(GuiInfo)) };
        if (!GetGUIThreadInfo(thread, ref info)) throw new InvalidOperationException("GUI thread state unavailable");
        return info;
    }

    [DllImport("user32.dll")] public static extern bool GetGUIThreadInfo(uint thread, ref GuiInfo info);

    public static IntPtr Capture(IntPtr window) { return ThreadInfo(window).Capture; }
    public static IntPtr Focused(IntPtr window) { return ThreadInfo(window).Focus; }

    public static bool FocusWindow(IntPtr target, IntPtr root) {
        if (target == IntPtr.Zero) return false;
        uint owner;
        uint targetThread = GetWindowThreadProcessId(target, out owner);
        uint currentThread = GetCurrentThreadId();
        bool attached = targetThread != 0 && targetThread != currentThread && AttachThreadInput(currentThread, targetThread, true);
        if (root != IntPtr.Zero) SetForegroundWindow(root);
        SetFocus(target);
        bool focused = false;
        try { focused = ThreadInfo(target).Focus == target; } catch { }
        if (attached) AttachThreadInput(currentThread, targetThread, false);
        return focused;
    }

    public static IntPtr ClientPoint(IntPtr window, int screenX, int screenY) {
        var point = new Point { X = screenX, Y = screenY };
        MapWindowPoints(IntPtr.Zero, window, ref point, 1);
        return new IntPtr(unchecked((int)(((uint)(ushort)point.Y << 16) | (ushort)point.X)));
    }

    public static IntPtr Position(int x, int y) {
        return new IntPtr(unchecked((int)(((uint)(ushort)y << 16) | (ushort)x)));
    }

    public static IntPtr Send(IntPtr handle, uint message, IntPtr wparam, IntPtr lparam) {
        IntPtr result;
        if (SendMessageTimeout(handle, message, wparam, lparam, 2, 3000, out result) == IntPtr.Zero) throw new InvalidOperationException("Native message timed out or failed");
        return result;
    }

    public sealed class ScreenSample {
        public int X, Y, IntervalMilliseconds, Frames, BrightFrames, InvalidFrames, FirstBrightFrame;
        public uint FirstColor, LastColor;
        public bool DcReleased;
    }

    public sealed class ScreenSampler : IDisposable {
        private readonly int x, y, intervalMilliseconds;
        private readonly Thread thread;
        private readonly ManualResetEventSlim ready = new ManualResetEventSlim(false);
        private readonly ManualResetEventSlim stop = new ManualResetEventSlim(false);
        private Exception failure;
        private int frames, brightFrames, invalidFrames, firstBrightFrame = -1;
        private uint firstColor, lastColor;
        private bool hasFirstColor, dcReleased, started, disposed;

        public ScreenSampler(int x, int y, int intervalMilliseconds) {
            if (intervalMilliseconds < 1) throw new ArgumentOutOfRangeException("intervalMilliseconds");
            this.x = x;
            this.y = y;
            this.intervalMilliseconds = intervalMilliseconds;
            thread = new Thread(SampleLoop) { IsBackground = true };
        }

        public void Start() {
            ThrowIfDisposed();
            if (started) throw new InvalidOperationException("Screen sampler already started");
            started = true;
            thread.Start();
            if (!ready.Wait(3000)) {
                stop.Set();
                if (!thread.Join(3000)) throw new InvalidOperationException("Screen sampler did not stop");
                throw new InvalidOperationException("Screen sampler did not start");
            }
            ThrowFailure();
        }

        public ScreenSample Stop() {
            ThrowIfDisposed();
            if (!started) throw new InvalidOperationException("Screen sampler was not started");
            stop.Set();
            if (!thread.Join(3000)) throw new InvalidOperationException("Screen sampler did not stop");
            ThrowFailure();
            return new ScreenSample {
                X = x,
                Y = y,
                IntervalMilliseconds = intervalMilliseconds,
                Frames = frames,
                BrightFrames = brightFrames,
                InvalidFrames = invalidFrames,
                FirstBrightFrame = firstBrightFrame,
                FirstColor = firstColor,
                LastColor = lastColor,
                DcReleased = dcReleased
            };
        }

        public void Dispose() {
            if (disposed) return;
            try {
                if (started && thread.IsAlive) {
                    stop.Set();
                    if (!thread.Join(3000)) throw new InvalidOperationException("Screen sampler did not stop");
                }
            } finally {
                ready.Dispose();
                stop.Dispose();
                disposed = true;
            }
        }

        private void SampleLoop() {
            var screen = IntPtr.Zero;
            try {
                screen = GetDC(IntPtr.Zero);
                if (screen == IntPtr.Zero) throw new InvalidOperationException("Screen DC unavailable");
                ready.Set();
                while (!stop.IsSet) {
                    var color = GetPixel(screen, x, y);
                    frames++;
                    lastColor = color;
                    if (!hasFirstColor) {
                        firstColor = color;
                        hasFirstColor = true;
                    }
                    if (color == 0xffffffffu) {
                        invalidFrames++;
                    } else {
                        var red = color & 0xffu;
                        var green = (color >> 8) & 0xffu;
                        var blue = (color >> 16) & 0xffu;
                        if (red > 180u && green > 180u && blue > 180u) {
                            brightFrames++;
                            if (firstBrightFrame < 0) firstBrightFrame = frames;
                        }
                    }
                    if (stop.Wait(intervalMilliseconds)) break;
                }
            } catch (Exception error) {
                failure = error;
                ready.Set();
            } finally {
                if (screen != IntPtr.Zero) ReleaseDC(IntPtr.Zero, screen);
                dcReleased = true;
            }
        }

        private void ThrowFailure() {
            if (failure != null) throw new InvalidOperationException("Screen sampler failed", failure);
        }

        private void ThrowIfDisposed() {
            if (disposed) throw new ObjectDisposedException("ScreenSampler");
        }
    }

    public static void Click() {
        var inputs = new[] {
            new Input { Data = new InputData { Mouse = new MouseInput { Flags = 2 } } },
            new Input { Data = new InputData { Mouse = new MouseInput { Flags = 4 } } }
        };
        if (SendInput(2, inputs, Marshal.SizeOf<Input>()) != 2) throw new InvalidOperationException("Mouse click input was not delivered");
    }
}
'@

function Wait-For([scriptblock]$Condition, [string]$Label, [int]$Seconds = 30) {
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        if (& $Condition) { return }
        Start-Sleep -Milliseconds 75
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for $Label"
}

function Write-Utf8([string]$Path, [string]$Value) {
    [IO.File]::WriteAllText($Path, $Value, [Text.UTF8Encoding]::new($false))
}

function Run-Git([string]$Repo, [string[]]$Arguments) {
    & git -c core.fsmonitor=false -c core.autocrlf=false -c commit.gpgsign=false -c "core.hooksPath=$Output" -C $Repo @Arguments | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Git fixture failed with $LASTEXITCODE" }
}

function Get-GitText([string]$Repo, [string[]]$Arguments) {
    $value = (& git -c core.fsmonitor=false -c core.autocrlf=false -c commit.gpgsign=false -c "core.hooksPath=$Output" -C $Repo @Arguments | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) { throw "Git fixture lookup failed with $LASTEXITCODE" }
    $value
}

function New-Repository([string]$Name) {
    $repo = Join-Path $Output $Name
    $tree = Join-Path $Output "${Name}-tree"
    New-Item -ItemType Directory -Path $repo, $tree | Out-Null
    Run-Git $repo @('init', '--quiet')
    Write-Utf8 (Join-Path $repo 'lines.txt') "before`n"
    Run-Git $repo @('add', '.')
    Run-Git $repo @('-c', 'user.name=Wts Visibility Test', '-c', 'user.email=wts-visibility@example.invalid', 'commit', '--quiet', '-m', 'Fixture')
    $commit = Get-GitText $repo @('rev-parse', 'HEAD')
    if ($commit.Length -eq 0) { throw 'Fixture commit lookup failed' }
    $branch = "${Name}-topic"
    Run-Git $repo @('update-ref', "refs/heads/$branch", $commit)
    $rootBranch = Get-GitText $repo @('symbolic-ref', '--short', 'HEAD')
    Write-Utf8 (Join-Path $repo 'lines.txt') "before`nchanged-$Name`n"
    [IO.File]::Copy((Join-Path $repo 'lines.txt'), (Join-Path $tree 'lines.txt'), $true)
    $common = Join-Path $repo '.git'
    $metadata = Join-Path $common "worktrees/${Name}-tree"
    New-Item -ItemType Directory -Path $metadata | Out-Null
    $treeGit = Join-Path $tree '.git'
    Write-Utf8 $treeGit ("gitdir: " + ((Join-Path $metadata '').Replace('\', '/') + '/'))
    Write-Utf8 (Join-Path $metadata 'gitdir') ($treeGit.Replace('\', '/'))
    Write-Utf8 (Join-Path $metadata 'HEAD') "ref: refs/heads/$branch"
    Write-Utf8 (Join-Path $metadata 'commondir') '../..'
    [IO.File]::Copy((Join-Path $common 'index'), (Join-Path $metadata 'index'), $true)
    [pscustomobject]@{
        Name = $Name
        Repository = $repo
        Worktree = $tree
        CommonDir = $common
        RootBranch = $rootBranch
        TreeBranch = $branch
    }
}

function Rect-Record($Rect) {
    [ordered]@{ left = $Rect.Left; top = $Rect.Top; right = $Rect.Right; bottom = $Rect.Bottom }
}

function Rect-Same($Left, $Right) {
    $Left.Left -eq $Right.Left -and $Left.Top -eq $Right.Top -and $Left.Right -eq $Right.Right -and $Left.Bottom -eq $Right.Bottom
}

function Rect-Overlaps($Left, $Right) {
    $Left.Left -lt $Right.Right -and $Right.Left -lt $Left.Right -and $Left.Top -lt $Right.Bottom -and $Right.Top -lt $Left.Bottom
}

function Normalize-Identity([string]$Path) {
    $text = $Path.Replace('\', '/').ToLowerInvariant()
    if ($text.StartsWith('//?/unc/')) { return '//' + $text.Substring(8) }
    if ($text.StartsWith('//?/')) { return $text.Substring(4) }
    $text
}

function New-RowKey([string]$Repository, [string]$Worktree) {
    "$(Normalize-Identity $Repository)|$(Normalize-Identity $Worktree)"
}

function Get-Cards {
    $children = @([WtsVisibilityNativeProbe]::Children($script:viewport) | Where-Object {
        $_.Parent -eq $script:viewport -and $_.Class -ieq 'STATIC' -and $_.Text.Length -gt 0 -and [WtsVisibilityNativeProbe]::IsWindowVisible($_.Handle)
    })
    @($children | ForEach-Object {
        [pscustomobject]@{ Handle = $_.Handle; Text = $_.Text -replace '\r?\nRefreshing$', ''; Class = $_.Class; Bounds = $_.Bounds; Top = $_.Bounds.Top }
    } | Sort-Object Top)
}

function New-ExpectedRows([array]$Repositories) {
    $rows = [Collections.Generic.List[object]]::new()
    foreach ($repository in $Repositories) {
        $repositoryKey = Normalize-Identity $repository.CommonDir
        [void]$rows.Add([pscustomobject]@{
            Kind = 'repository'
            Key = $repositoryKey
            RepositoryKey = $repositoryKey
            WorktreeKey = $null
            Name = $repository.Name
            PathText = $null
            Branch = $null
            Caption = $repository.Name
        })
        [void]$rows.Add([pscustomobject]@{
            Kind = 'worktree'
            Key = New-RowKey $repository.CommonDir $repository.Repository
            RepositoryKey = $repositoryKey
            WorktreeKey = Normalize-Identity $repository.Repository
            Name = $repository.Name
            PathText = $repository.Repository
            Branch = $repository.RootBranch
            CountsCaption = 'S +1/-0'
            Caption = $null
        })
        [void]$rows.Add([pscustomobject]@{
            Kind = 'worktree'
            Key = New-RowKey $repository.CommonDir $repository.Worktree
            RepositoryKey = $repositoryKey
            WorktreeKey = Normalize-Identity $repository.Worktree
            Name = $repository.Name
            PathText = $repository.Worktree
            Branch = $repository.TreeBranch
            CountsCaption = 'S +1/-0'
            Caption = $null
        })
    }
    $rows.ToArray()
}

function Get-RowSnapshot([array]$Cards) {
    $rows = [Collections.Generic.List[object]]::new()
    $repositoryName = $null
    foreach ($card in @($Cards)) {
        $lines = @(([string]$card.Text) -split "`r?`n")
        $repositoryMatches = @($script:expectedRows | Where-Object { $_.Kind -eq 'repository' -and $_.Caption -ceq [string]$card.Text })
        if ($repositoryMatches.Count -eq 1) {
            $expected = $repositoryMatches[0]
            $repositoryName = $expected.Name
            [void]$rows.Add([pscustomobject]@{
                Kind = 'repository'
                Key = $expected.Key
                RepositoryKey = $expected.RepositoryKey
                WorktreeKey = $null
                Caption = [string]$card.Text
                Handle = $card.Handle
                Card = $card
                Branch = $null
                PathText = $null
            })
            continue
        }
        if ($lines.Count -ge 3 -and [IO.Path]::IsPathFullyQualified([string]$lines[0])) {
            $worktreeKey = Normalize-Identity ([string]$lines[0])
            $matches = @($script:expectedRows | Where-Object { $_.Kind -eq 'worktree' -and $_.WorktreeKey -eq $worktreeKey })
            if ($matches.Count -ne 1) { throw "Unexpected worktree row key $($lines[0])" }
            $expected = $matches[0]
            [void]$rows.Add([pscustomobject]@{
                Kind = 'worktree'
                Key = $expected.Key
                RepositoryKey = $expected.RepositoryKey
                WorktreeKey = $expected.WorktreeKey
                Caption = [string]$card.Text
                Handle = $card.Handle
                Card = $card
                Branch = $null
                PathText = [string]$lines[0]
            })
            continue
        }
        $matches = @($script:expectedRows | Where-Object {
            $_.Kind -eq 'worktree' -and $_.Name -ceq $repositoryName -and $_.Branch -ceq [string]$lines[0] -and $_.CountsCaption -ceq [string]$lines[1]
        })
        if ($matches.Count -ne 1) { throw "Unexpected successful worktree caption: $($card.Text)" }
        $expected = $matches[0]
        if ($lines.Count -ne 2) { throw "Unexpected successful worktree caption shape for $($expected.Key)" }
        [void]$rows.Add([pscustomobject]@{
            Kind = 'worktree'
            Key = $expected.Key
            RepositoryKey = $expected.RepositoryKey
            WorktreeKey = $expected.WorktreeKey
            Caption = [string]$card.Text
            Handle = $card.Handle
            Card = $card
            Branch = [string]$lines[0]
            PathText = $null
        })
    }
    $rows.ToArray()
}

function Get-ExpectedVisibleRows([string]$HiddenKey) {
    @($script:expectedRows | Where-Object { $_.Kind -ne 'worktree' -or $_.Key -cne $HiddenKey })
}

function Assert-RowSnapshot([array]$Actual, [array]$Expected, [string]$Label) {
    $actual = @($Actual)
    $expected = @($Expected)
    if ($actual.Count -ne $expected.Count) { throw "$Label has $($actual.Count) rows; expected $($expected.Count)" }
    for ($index = 0; $index -lt $expected.Count; $index++) {
        if ([string]$actual[$index].Key -cne [string]$expected[$index].Key) { throw "$Label row $index has key $($actual[$index].Key); expected $($expected[$index].Key)" }
        if ($expected[$index].Kind -eq 'repository') {
            if ([string]$actual[$index].Caption -cne [string]$expected[$index].Caption) { throw "$Label repository caption changed for $($expected[$index].Key)" }
        } elseif ($script:baselineCaptions.ContainsKey([string]$expected[$index].Key) -and [string]$actual[$index].Caption -cne [string]$script:baselineCaptions[[string]$expected[$index].Key]) {
            throw "$Label worktree caption changed for $($expected[$index].Key)"
        }
    }
}

function Set-BaselineCaptions([array]$Rows) {
    $script:baselineCaptions = @{}
    foreach ($row in @($Rows)) { $script:baselineCaptions[[string]$row.Key] = [string]$row.Caption }
}

function Get-PublicRows([array]$Rows) {
    @($Rows | ForEach-Object {
        [ordered]@{
            kind = $_.Kind
            key = $_.Key
            repository = $_.RepositoryKey
            worktree = $_.WorktreeKey
            caption = $_.Caption
        }
    })
}

function Get-CurrentRows {
    @(Get-RowSnapshot @(Get-Cards))
}

function Wait-For-ExpectedRows([array]$Expected, [string]$Label, [int]$Seconds = 20) {
    $script:waitRows = $null
    Wait-For {
        try {
            $candidate = @(Get-CurrentRows)
            Assert-RowSnapshot $candidate $Expected $Label
            $script:waitRows = $candidate
            $true
        } catch {
            $false
        }
    } $Label $Seconds
    @($script:waitRows)
}

function Get-NativeButtons {
    @([WtsVisibilityNativeProbe]::Children($script:handle) | Where-Object {
        $_.Class -ieq 'Button' -and [WtsVisibilityNativeProbe]::IsWindowVisible($_.Handle)
    } | ForEach-Object {
        [pscustomobject]@{ Handle = $_.Handle; Parent = $_.Parent; Text = $_.Text; Class = $_.Class; Bounds = $_.Bounds }
    })
}

function Find-VisibilityButton($Card, [string]$Caption) {
    @((Get-NativeButtons) | Where-Object {
        $_.Text -ceq $Caption -and $_.Parent -eq $Card.Handle
    })
}

function Find-VisibilityTopButton([string]$Caption) {
    $viewportBounds = [WtsVisibilityNativeProbe]::WindowBounds($script:viewport)
    @((Get-NativeButtons) | Where-Object {
        $_.Parent -eq $script:handle -and $_.Text -ceq $Caption -and $_.Bounds.Top -lt $viewportBounds.Top
    })
}

function Assert-NativeVisibilitySurface([array]$Rows, [string]$HiddenKey, [bool]$Checked = $false) {
    $buttons = @(Get-NativeButtons)
    if ($buttons.Count -eq 0) { throw 'Native visibility buttons are not present' }
    if (@($buttons | Where-Object { $_.Class -ine 'Button' }).Count -ne 0) { throw 'Visibility controls are not native Button windows' }
    $top = @(Find-VisibilityTopButton 'Show all')
    if ($top.Count -ne 1) { throw "Expected one Show all checkbox, found $($top.Count)" }
    if (@(Find-VisibilityTopButton 'Hide hidden').Count -ne 0) { throw 'Show all label changed when toggled' }
    $actualChecked = [WtsVisibilityNativeProbe]::Send($top[0].Handle, 0x00F0, [IntPtr]::Zero, [IntPtr]::Zero).ToInt64()
    if ($actualChecked -ne [int]$Checked) { throw "Expected Show all checked state $Checked, found $actualChecked" }
    if (([WtsVisibilityNativeProbe]::GetWindowLong($top[0].Handle, -16) -band 15) -ne 3) {
        throw 'Show all is not a native automatic checkbox'
    }
    $dpi = [WtsVisibilityNativeProbe]::GetDpiForWindow($script:handle)
    if (($top[0].Bounds.Bottom - $top[0].Bounds.Top) -gt [int](28 * $dpi / 96)) {
        throw 'Show all is taller than a compact toggle'
    }
    $worktrees = @($Rows | Where-Object { $_.Kind -eq 'worktree' })
    if ($worktrees.Count -eq 0) { throw 'Visibility controls appeared without any worktree rows' }
    foreach ($row in $worktrees) {
        $caption = if ([string]$row.Key -ceq [string]$HiddenKey) { 'Unhide' } else { 'Hide' }
        $matches = @(Find-VisibilityButton $row.Card $caption)
        if ($matches.Count -ne 1) { throw "Expected one native $caption button for $($row.Key), found $($matches.Count)" }
        if ($matches[0].Class -ine 'Button') { throw "Visibility control for $($row.Key) is not a native Button window" }
    }
    $visibilityButtons = @($buttons | Where-Object { $_.Text -match '^(Hide|Unhide|Show all|Hide hidden)$' })
    if ($visibilityButtons.Count -ne ($worktrees.Count + 1)) { throw "Expected $($worktrees.Count + 1) native visibility buttons, found $($visibilityButtons.Count)" }
    foreach ($button in @($visibilityButtons | Where-Object { $_.Handle -ne $top[0].Handle })) {
        if (([WtsVisibilityNativeProbe]::GetWindowLong($button.Handle, -16) -band 15) -ne 11) {
            throw "Visibility button $($button.Text) does not use the app's shared owner-drawn style"
        }
    }
    [ordered]@{
        top_handle = $top[0].Handle.ToInt64()
        top_caption = 'Show all'
        top_checked = $actualChecked
        worktree_count = $worktrees.Count
        native_button_count = $visibilityButtons.Count
        all_native_button_class = 'Button'
        action_buttons_owner_drawn = $true
    }
}

function Get-FileSnapshot([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return [pscustomobject]@{ Exists = $false; Content = $null }
    }
    [pscustomobject]@{ Exists = $true; Content = [Convert]::ToBase64String([IO.File]::ReadAllBytes($Path)) }
}

function Assert-FileSnapshot($Expected, [string]$Path, [string]$Label) {
    $actual = Get-FileSnapshot $Path
    if ($actual.Exists -ne $Expected.Exists -or $actual.Content -cne $Expected.Content) { throw "$Label changed" }
}

function Read-JsonFile([string]$Path, [string]$Label) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "$Label does not exist" }
    try {
        Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
    } catch {
        throw "$Label is not valid JSON: $($_.Exception.Message)"
    }
}

function Assert-VisibilityFile([string]$Repository, [string]$Worktree, [bool]$Hidden) {
    $document = Read-JsonFile $script:visibilityPath 'Visibility configuration'
    $properties = @($document.PSObject.Properties | ForEach-Object { $_.Name } | Sort-Object)
    if (($properties -join ',') -cne 'hidden,version') { throw 'Visibility configuration has unexpected fields' }
    if ([int]$document.version -ne 1) { throw 'Visibility configuration version is not 1' }
    if ($null -eq $document.hidden) { throw 'Visibility configuration has no hidden array' }
    $entries = @($document.hidden)
    $seen = @{}
    foreach ($entry in $entries) {
        $entryProperties = @($entry.PSObject.Properties | ForEach-Object { $_.Name } | Sort-Object)
        if (($entryProperties -join ',') -cne 'repository,worktree') { throw 'Visibility entry has unexpected fields' }
        $key = "$(Normalize-Identity ([string]$entry.repository))|$(Normalize-Identity ([string]$entry.worktree))"
        if ($seen.ContainsKey($key)) { throw 'Visibility configuration contains a duplicate key' }
        $seen[$key] = $true
    }
    $expectedKey = "$(Normalize-Identity $Repository)|$(Normalize-Identity $Worktree)"
    if ($Hidden) {
        if ($entries.Count -ne 1 -or -not $seen.ContainsKey($expectedKey)) { throw 'Saved visibility does not contain exactly the hidden worktree identity pair' }
    } elseif ($entries.Count -ne 0) {
        throw 'Unhide did not clear the saved visibility identity pair'
    }
    [ordered]@{
        version = [int]$document.version
        hidden = @($entries | ForEach-Object {
            [ordered]@{ repository = Normalize-Identity ([string]$_.repository); worktree = Normalize-Identity ([string]$_.worktree) }
        })
    }
}

function Assert-StableConfiguration {
    Assert-FileSnapshot $script:configBefore $script:configPath 'Registry configuration'
    Assert-FileSnapshot $script:orderBefore $script:orderPath 'Presentation order configuration'
}

function Focus-Button($Button) {
    if (-not [WtsVisibilityNativeProbe]::IsWindow($Button.Handle) -or -not [WtsVisibilityNativeProbe]::IsWindowEnabled($Button.Handle)) { throw "Button $($Button.Text) is not focusable" }
    if (-not [WtsVisibilityNativeProbe]::FocusWindow($Button.Handle, $script:handle)) { throw "Could not focus native button $($Button.Text)" }
    Wait-For { [WtsVisibilityNativeProbe]::Focused($script:handle) -eq $Button.Handle } "focus on $($Button.Text)" 5
    [WtsVisibilityNativeProbe]::Focused($script:handle)
}

function Invoke-KeyboardButton($Button) {
    try { $before = Focus-Button $Button } catch {
        return [ordered]@{
            result = 'NOT_FEASIBLE'
            handle = $Button.Handle.ToInt64()
            caption = $Button.Text
            reason = 'Could not observe focus on the native button'
            key = 'SPACE'
        }
    }
    [void][WtsVisibilityNativeProbe]::Send($Button.Handle, 0x0100, [IntPtr]0x20, [IntPtr]1)
    [void][WtsVisibilityNativeProbe]::Send($Button.Handle, 0x0101, [IntPtr]0x20, [IntPtr](-1073741823))
    [ordered]@{
        result = 'PASS'
        handle = $Button.Handle.ToInt64()
        caption = $Button.Text
        focused_before = $before.ToInt64()
        key = 'SPACE'
    }
}

function Invoke-TabProbe($Button) {
    try { $before = Focus-Button $Button } catch {
        return [ordered]@{
            result = 'NOT_FEASIBLE'
            focused_before = 0
            focused_after = 0
            message = 'TAB'
            reason = 'Could not observe focus on the native button'
        }
    }
    if (-not [WtsVisibilityNativeProbe]::PostMessage($Button.Handle, 0x0100, [IntPtr]9, [IntPtr]1)) { throw 'Tab key could not be posted' }
    if (-not [WtsVisibilityNativeProbe]::PostMessage($Button.Handle, 0x0101, [IntPtr]9, [IntPtr](-1073741815))) { throw 'Tab release could not be posted' }
    Start-Sleep -Milliseconds 100
    $after = [WtsVisibilityNativeProbe]::Focused($script:handle)
    $native = @(Get-NativeButtons | Where-Object { $_.Handle -eq $after })
    [ordered]@{
        result = if ($after -ne [IntPtr]::Zero -and $after -ne $before -and $native.Count -eq 1) { 'PASS' } else { 'NOT_FEASIBLE' }
        focused_before = $before.ToInt64()
        focused_after = $after.ToInt64()
        message = 'TAB'
    }
}

function Invoke-ButtonClick($Button) {
    $bounds = [WtsVisibilityNativeProbe]::WindowBounds($Button.Handle)
    $point = [WtsVisibilityNativeProbe+Point]::new()
    $point.X = [int](($bounds.Left + $bounds.Right) / 2)
    $point.Y = [int](($bounds.Top + $bounds.Bottom) / 2)
    [void][WtsVisibilityNativeProbe]::SetForegroundWindow($script:handle)
    Start-Sleep -Milliseconds 50
    if (-not [WtsVisibilityNativeProbe]::SetCursorPos($point.X, $point.Y)) { throw 'Could not position the mouse over the button' }
    $hit = [WtsVisibilityNativeProbe]::WindowFromPoint($point)
    [uint32]$owner = 0
    [void][WtsVisibilityNativeProbe]::GetWindowThreadProcessId($hit, [ref]$owner)
    if ($owner -ne $script:process.Id) { throw 'Mouse target is outside the test window' }
    [WtsVisibilityNativeProbe]::Click()
    if ($hit -ne $Button.Handle) {
        throw "Mouse click on $($Button.Text) reaches $([WtsVisibilityNativeProbe]::Class($hit)) instead of the button"
    }
}

function Invoke-VisibilityActivation($Button, [bool]$Keyboard) {
    $caption = [WtsVisibilityNativeProbe]::Text($Button.Handle)
    $checked = [WtsVisibilityNativeProbe]::Send($Button.Handle, 0x00F0, [IntPtr]::Zero, [IntPtr]::Zero).ToInt64()
    if ($Keyboard) {
        $result = Invoke-KeyboardButton $Button
        if ($result.result -cne 'PASS') { throw "Could not verify keyboard activation of $($Button.Text)" }
    } else { Invoke-ButtonClick $Button }
    Wait-For {
        if ($caption -ceq 'Show all') {
            [WtsVisibilityNativeProbe]::Send($Button.Handle, 0x00F0, [IntPtr]::Zero, [IntPtr]::Zero).ToInt64() -ne $checked
        } else {
            -not [WtsVisibilityNativeProbe]::IsWindow($Button.Handle) -or [WtsVisibilityNativeProbe]::Text($Button.Handle) -cne $caption
        }
    } "activation of $caption" 10
}

function Get-ButtonScreenSamplePoint($Button) {
    $bounds = [WtsVisibilityNativeProbe]::WindowBounds($Button.Handle)
    $dpi = [Math]::Max(96, [int][WtsVisibilityNativeProbe]::GetDpiForWindow($script:handle))
    $inset = [Math]::Max(1, [int][Math]::Ceiling(4 * $dpi / 96))
    $x = $bounds.Left + $inset
    $y = [int](($bounds.Top + $bounds.Bottom) / 2)
    if ($x -ge $bounds.Right -or $y -le $bounds.Top -or $y -ge $bounds.Bottom) { throw 'Visibility button has no usable background sample point' }
    [pscustomobject]@{
        X = $x
        Y = $y
        Dpi = $dpi
        InsetLogical = 4
        InsetPixels = $inset
        Bounds = $bounds
    }
}

function Invoke-VisibilityFrameProbe($Button, [bool]$BaselineMode) {
    $handle = $Button.Handle
    $initialCaption = [WtsVisibilityNativeProbe]::Text($handle)
    if ($initialCaption -cne 'Unhide') { throw "Live frame probe started with $initialCaption instead of Unhide" }
    $initialBounds = [WtsVisibilityNativeProbe]::WindowBounds($handle)
    $point = Get-ButtonScreenSamplePoint $Button
    [void][WtsVisibilityNativeProbe]::SetForegroundWindow($script:handle)
    Start-Sleep -Milliseconds 100
    $screenPoint = [WtsVisibilityNativeProbe+Point]::new()
    $screenPoint.X = $point.X
    $screenPoint.Y = $point.Y
    $hit = [WtsVisibilityNativeProbe]::WindowFromPoint($screenPoint)
    if ($hit -ne $handle) { throw 'Visibility button background sample point is not visible on screen' }
    $sampler = $null
    $measurement = $null
    $completed = 0
    $cycles = 8
    $captions = [Collections.Generic.List[string]]::new()
    $cleanupError = $null
    try {
        $sampler = [WtsVisibilityNativeProbe+ScreenSampler]::new($point.X, $point.Y, 1)
        $sampler.Start()
        foreach ($index in 1..$cycles) {
            $expected = if (($index % 2) -eq 1) { 'Hide' } else { 'Unhide' }
            [void][WtsVisibilityNativeProbe]::Send($handle, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero)
            Wait-For {
                [WtsVisibilityNativeProbe]::IsWindow($handle) -and [WtsVisibilityNativeProbe]::Text($handle) -ceq $expected
            } "BM_CLICK caption $expected" 10
            if (-not (Rect-Same $initialBounds ([WtsVisibilityNativeProbe]::WindowBounds($handle)))) {
                throw 'Visibility button bounds changed while Show all was enabled'
            }
            [void]$captions.Add($expected)
            $completed++
            Start-Sleep -Milliseconds 40
        }
        $measurement = $sampler.Stop()
    } finally {
        try {
            if ($null -ne $sampler) { $sampler.Dispose() }
        } catch {
            $cleanupError = $_
        }
        try {
            if ([WtsVisibilityNativeProbe]::IsWindow($handle) -and [WtsVisibilityNativeProbe]::Text($handle) -cne $initialCaption) {
                [void][WtsVisibilityNativeProbe]::Send($handle, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero)
                Wait-For {
                    [WtsVisibilityNativeProbe]::IsWindow($handle) -and [WtsVisibilityNativeProbe]::Text($handle) -ceq $initialCaption
                } 'restoration of Unhide after live frame probe' 10
            }
        } catch {
            if ($null -eq $cleanupError) { $cleanupError = $_ }
        }
        if ($null -ne $cleanupError) { throw $cleanupError }
    }
    if ($null -eq $measurement -or -not $measurement.DcReleased) { throw 'Live frame sampler did not release its screen DC' }
    if ($measurement.Frames -le 0) { throw 'Live frame sampler captured no screen frames' }
    if ($measurement.InvalidFrames -ne 0) { throw 'Live frame sampler could not read every screen sample' }
    if (-not [WtsVisibilityNativeProbe]::IsWindow($handle) -or [WtsVisibilityNativeProbe]::Text($handle) -cne $initialCaption) {
        throw 'Live frame probe did not leave the original Unhide caption'
    }
    $brightFrames = [int]$measurement.BrightFrames
    $result = if ($brightFrames -gt 0) {
        if ($BaselineMode) { 'REPRODUCED' } else { 'FAIL' }
    } elseif ($BaselineMode) {
        'INCONCLUSIVE'
    } else {
        'PASS'
    }
    [ordered]@{
        result = $result
        activation = 'BM_CLICK'
        cycles = $cycles
        completed_cycles = $completed
        initial_caption = $initialCaption
        final_caption = [WtsVisibilityNativeProbe]::Text($handle)
        captions = @($captions.ToArray())
        handle = $handle.ToInt64()
        handle_stable = [WtsVisibilityNativeProbe]::IsWindow($handle)
        bounds = Rect-Record $initialBounds
        sample_point = [ordered]@{
            x = $point.X
            y = $point.Y
            dpi = $point.Dpi
            inset_logical = $point.InsetLogical
            inset_pixels = $point.InsetPixels
        }
        sample_interval_ms = $measurement.IntervalMilliseconds
        bright_threshold_rgb = 180
        frames = $measurement.Frames
        bright_frames = $brightFrames
        invalid_frames = $measurement.InvalidFrames
        first_bright_frame = $measurement.FirstBrightFrame
        first_color = $measurement.FirstColor
        last_color = $measurement.LastColor
        dc_released = [bool]$measurement.DcReleased
        limitation = if ($BaselineMode -and $brightFrames -eq 0) { 'No bright frame was observed in this bounded run; baseline reproduction is inconclusive.' } else { $null }
    }
}

function Get-PointsForCard($Card) {
    $viewportBounds = [WtsVisibilityNativeProbe]::ClientBounds($script:viewport)
    $scale = [Math]::Max(96, [int][WtsVisibilityNativeProbe]::GetDpiForWindow($script:handle))
    $x = $Card.Bounds.Left + [Math]::Max(8, [int](12 * $scale / 96))
    $offset = [Math]::Max(12, [int](14 * $scale / 96))
    $ys = @(
        [int](($Card.Bounds.Top + $Card.Bounds.Bottom) / 2)
        [int]($Card.Bounds.Top + $offset)
        [int]($Card.Bounds.Bottom - $offset)
    )
    $margin = [Math]::Max(24, [int](28 * $scale / 96))
    $points = @()
    foreach ($screenY in $ys) {
        $point = [WtsVisibilityNativeProbe]::ClientPoint($script:viewport, $x, $screenY)
        $clientX = [int]($point.ToInt64() -band 0xffff)
        $clientY = [int](($point.ToInt64() -shr 16) -band 0xffff)
        if ($clientY -lt $margin -or $clientY -gt ($viewportBounds.Bottom - $margin)) { throw "Card $($Card.Text) is too close to a viewport scroll edge" }
        $points += [pscustomobject]@{ X = $clientX; Y = $clientY }
    }
    $points
}

function Get-PaintWindows([array]$Cards) {
    @($script:handle, $script:viewport) + @($Cards | ForEach-Object { $_.Handle }) + @(Get-NativeButtons | ForEach-Object { $_.Handle }) | Where-Object { $_ -ne [IntPtr]::Zero }
}

function Clear-Paint([array]$Cards) {
    $windows = @(Get-PaintWindows $Cards)
    foreach ($window in $windows) { [void][WtsVisibilityNativeProbe]::ValidateRect($window, [IntPtr]::Zero) }
    foreach ($window in $windows) { [void][WtsVisibilityNativeProbe]::UpdateWindow($window) }
    foreach ($window in $windows) { [void][WtsVisibilityNativeProbe]::ValidateRect($window, [IntPtr]::Zero) }
}

function Settle-Paint([array]$Cards) {
    foreach ($index in 1..4) {
        Clear-Paint $Cards
        Start-Sleep -Milliseconds 75
    }
    Clear-Paint $Cards
}

function Is-NarrowStrip($Update, $Client, [int]$Scale) {
    if (-not $Update.Pending) { return $false }
    $limit = [Math]::Max(8, [int][Math]::Ceiling(8 * $Scale / 96))
    $rectangles = @($Update.Regions)
    $rectangles.Count -gt 0 -and @($rectangles | Where-Object { $_.Bottom - $_.Top -gt $limit * 2 }).Count -eq 0
}

function Is-NarrowEdgeStrip($Update, $Client, [int]$Scale) {
    if (-not (Is-NarrowStrip $Update $Client $Scale)) { return $false }
    $limit = [Math]::Max(8, [int][Math]::Ceiling(8 * $Scale / 96))
    @($Update.Regions | Where-Object { $_.Top -gt $limit -and $_.Bottom -lt ($Client.Bottom - $limit) }).Count -eq 0
}

function Is-BroadCardUpdate($Update, $Client, [int]$Scale) {
    $Update.Pending -and -not (Is-NarrowEdgeStrip $Update $Client $Scale)
}

function Start-Drag($Card) {
    $scale = [Math]::Max(96, [int][WtsVisibilityNativeProbe]::GetDpiForWindow($Card.Handle))
    $x = [Math]::Max(2, [int](12 * $scale / 96))
    $bounds = [WtsVisibilityNativeProbe]::ClientBounds($Card.Handle)
    $y = [int][Math]::Floor(($bounds.Bottom - $bounds.Top) / 2)
    [void][WtsVisibilityNativeProbe]::Send($Card.Handle, 0x0201, [IntPtr]1, [WtsVisibilityNativeProbe]::Position($x, $y))
    Wait-For { [WtsVisibilityNativeProbe]::Capture($script:handle) -eq $script:viewport } 'drag capture' 5
}

function Stop-Drag {
    if ([WtsVisibilityNativeProbe]::Capture($script:handle) -ne [IntPtr]::Zero) {
        [void][WtsVisibilityNativeProbe]::Send($script:viewport, 0x0100, [IntPtr]0x1b, [IntPtr]::Zero)
        [void][WtsVisibilityNativeProbe]::Send($script:viewport, 0x0101, [IntPtr]0x1b, [IntPtr]::Zero)
        Wait-For { [WtsVisibilityNativeProbe]::Capture($script:handle) -eq [IntPtr]::Zero } 'drag cancellation' 5
    }
}

function Measure-DragMove([string]$Name, $Point, $Cards, $Source, $Target) {
    [void][WtsVisibilityNativeProbe]::Send($script:viewport, 0x0200, [IntPtr]1, [WtsVisibilityNativeProbe]::Position($Point.X, $Point.Y))
    $viewportUpdate = [WtsVisibilityNativeProbe]::Update($script:viewport)
    $scale = [Math]::Max(96, [int][WtsVisibilityNativeProbe]::GetDpiForWindow($script:handle))
    $records = [Collections.Generic.List[object]]::new()
    foreach ($Card in @($Cards)) {
        $update = [WtsVisibilityNativeProbe]::Update($Card.Handle)
        $client = [WtsVisibilityNativeProbe]::ClientBounds($Card.Handle)
        $marker = Is-NarrowEdgeStrip $update $client $scale
        $record = [ordered]@{
            handle = $Card.Handle.ToInt64()
            text = $Card.Text
            pending = [bool]$update.Pending
            bounds = Rect-Record $update.Bounds
            regions = @($update.Regions | ForEach-Object { Rect-Record $_ })
            full_client = [bool]($update.Pending -and (Rect-Same $update.Bounds $client))
            marker_strip = [bool]$marker
            broad = [bool](Is-BroadCardUpdate $update $client $scale)
            is_source = $Card.Handle -eq $Source.Handle
            is_target = $Card.Handle -eq $Target.Handle
            flush_return = $false
            paint_cleared = $false
        }
        [void]$records.Add([pscustomobject]$record)
    }
    $viewportClient = [WtsVisibilityNativeProbe]::ClientBounds($script:viewport)
    $viewportNarrow = Is-NarrowStrip $viewportUpdate $viewportClient $scale
    $viewportRecord = [pscustomobject][ordered]@{
        pending = [bool]$viewportUpdate.Pending
        bounds = Rect-Record $viewportUpdate.Bounds
        regions = @($viewportUpdate.Regions | ForEach-Object { Rect-Record $_ })
        full_client = [bool]($viewportUpdate.Pending -and (Rect-Same $viewportUpdate.Bounds $viewportClient))
        narrow_strip = [bool]$viewportNarrow
        broad = [bool]($viewportUpdate.Pending -and -not $viewportNarrow)
        flush_return = $false
        paint_cleared = $false
    }
    foreach ($record in @($records | Where-Object pending)) {
        $recordWindow = [IntPtr]::new([long]$record.handle)
        $record.flush_return = [bool][WtsVisibilityNativeProbe]::UpdateWindow($recordWindow)
        $after = [WtsVisibilityNativeProbe]::Update($recordWindow)
        $record.paint_cleared = -not $after.Pending
    }
    if ($viewportUpdate.Pending) {
        $viewportRecord.flush_return = [bool][WtsVisibilityNativeProbe]::UpdateWindow($script:viewport)
        $afterViewport = [WtsVisibilityNativeProbe]::Update($script:viewport)
        $viewportRecord.paint_cleared = -not $afterViewport.Pending
    }
    $sourceHandle = $Source.Handle.ToInt64()
    $targetHandle = $Target.Handle.ToInt64()
    $invalidated = @($records | Where-Object pending)
    $painted = @($records | Where-Object { $_.pending -and $_.paint_cleared })
    $unrelatedInvalidated = @($invalidated | Where-Object { $_.handle -ne $sourceHandle -and $_.handle -ne $targetHandle })
    $unrelatedPainted = @($painted | Where-Object { $_.handle -ne $sourceHandle -and $_.handle -ne $targetHandle })
    $unrelatedBroadInvalidated = @($unrelatedInvalidated | Where-Object broad)
    $unrelatedBroadPainted = @($unrelatedPainted | Where-Object broad)
    $allCardsInvalidated = $Cards.Count -gt 0 -and $invalidated.Count -eq $Cards.Count
    $allInvalidatedCardsPainted = $allCardsInvalidated -and $painted.Count -eq $invalidated.Count
    $allCardsBroadInvalidated = $allCardsInvalidated -and @($invalidated | Where-Object { -not $_.broad }).Count -eq 0
    [ordered]@{
        name = $Name
        point = [ordered]@{ x = $Point.X; y = $Point.Y }
        source = $sourceHandle
        target = $targetHandle
        viewport = $viewportRecord
        cards = $records.ToArray()
        card_count = $Cards.Count
        invalidated_card_count = $invalidated.Count
        painted_card_count = $painted.Count
        unrelated_invalidated_count = $unrelatedInvalidated.Count
        unrelated_painted_count = $unrelatedPainted.Count
        unrelated_broad_invalidated_count = $unrelatedBroadInvalidated.Count
        unrelated_broad_painted_count = $unrelatedBroadPainted.Count
        all_cards_invalidated = $allCardsInvalidated
        all_invalidated_cards_painted = $allInvalidatedCardsPainted
        all_cards_broad_invalidated = $allCardsBroadInvalidated
    }
}

function Invoke-DragProbe([string]$Name, $Source, $Target) {
    $Cards = @(Get-Cards)
    $Points = @(Get-PointsForCard $Target)
    Clear-Paint $Cards
    Start-Drag $Source
    try {
        Clear-Paint $Cards
        [void]$script:actions.Add((Measure-DragMove "$Name-moving-1" $Points[0] $Cards $Source $Target))
        foreach ($index in 1..2) {
            Clear-Paint $Cards
            [void]$script:actions.Add((Measure-DragMove "$Name-stationary-$index" $Points[0] $Cards $Source $Target))
        }
        foreach ($index in 1..2) {
            Clear-Paint $Cards
            [void]$script:actions.Add((Measure-DragMove "$Name-moving-$($index + 1)" $Points[$index] $Cards $Source $Target))
        }
    } finally {
        Stop-Drag
        Clear-Paint $Cards
    }
}

function Invoke-EdgeScrollTest {
    $original = [WtsVisibilityNativeProbe]::WindowBounds($script:handle)
    $edgeTrace = [Collections.Generic.List[object]]::new()
    $scale = [int][WtsVisibilityNativeProbe]::GetDpiForWindow($script:handle)
    if (-not [WtsVisibilityNativeProbe]::SetWindowPos($script:handle, [IntPtr]::Zero, 0, 0, [int](760 * $scale / 96), [int](400 * $scale / 96), 0x16)) { throw 'Edge-scroll fixture resize failed' }
    try {
        [void][WtsVisibilityNativeProbe]::Send($script:viewport, 0x0115, [IntPtr]6, [IntPtr]::Zero)
        $cards = @(Get-Cards)
        $source = @((Get-RowSnapshot $cards) | Where-Object Kind -eq 'worktree')[0].Card
        Start-Drag $source
        $viewportBounds = [WtsVisibilityNativeProbe]::ClientBounds($script:viewport)
        $x = [int](12 * $scale / 96)
        $bottom = [pscustomobject]@{ X = $x; Y = $viewportBounds.Bottom - [int](8 * $scale / 96) }
        [WtsVisibilityNativeProbe]::MoveDragPointer($script:viewport, $bottom.X, $bottom.Y)
        Wait-For {
            $scroll = [WtsVisibilityNativeProbe]::Scroll($script:viewport)
            $edgeTrace.Add([ordered]@{ position = $scroll.Position; max = $scroll.Max; page = $scroll.Page; capture = [WtsVisibilityNativeProbe]::Capture($script:handle).ToInt64(); viewport = $script:viewport.ToInt64(); point_x = $bottom.X; point_y = $bottom.Y; card_top = [WtsVisibilityNativeProbe]::WindowBounds($source.Handle).Top; viewport_top = [WtsVisibilityNativeProbe]::WindowBounds($script:viewport).Top })
            $scroll.Position -gt 0 -and $scroll.Position -eq ($scroll.Max - [int]$scroll.Page + 1)
        } 'drag auto-scroll to the bottom' 10
        $bottomPosition = [WtsVisibilityNativeProbe]::Scroll($script:viewport).Position
        if ([WtsVisibilityNativeProbe]::Capture($script:handle) -ne $script:viewport) { throw 'Edge scrolling lost drag capture' }
        $cards = @(Get-Cards)
        $target = @((Get-RowSnapshot $cards) | Where-Object Kind -eq 'worktree')[-1].Card
        Settle-Paint $cards
        $stationary = Measure-DragMove 'edge-bottom-stationary' $bottom $cards $source $target
        if ($stationary.viewport.broad -or $stationary.unrelated_broad_invalidated_count -gt 0) { throw 'A clamped edge drag redrew unrelated rows' }
        [WtsVisibilityNativeProbe]::MoveDragPointer($script:viewport, $x, [int](8 * $scale / 96))
        Wait-For { [WtsVisibilityNativeProbe]::Scroll($script:viewport).Position -eq 0 } 'drag auto-scroll to the top' 10
        if ([WtsVisibilityNativeProbe]::Capture($script:handle) -ne $script:viewport) { throw 'Reverse edge scrolling lost drag capture' }
        [ordered]@{ result = 'PASS'; bottom_position = $bottomPosition; returned_to_top = $true; capture_preserved = $true; stationary_edge_broad_repaints = 0 }
    } finally {
        $edgeTrace | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $Output 'edge-scroll.json') -Encoding utf8NoBOM
        Stop-Drag
        if (-not [WtsVisibilityNativeProbe]::SetWindowPos($script:handle, [IntPtr]::Zero, 0, 0,
            $original.Right - $original.Left, $original.Bottom - $original.Top, 0x16)) { throw 'Edge-scroll fixture restoration failed' }
        [void][WtsVisibilityNativeProbe]::Send($script:viewport, 0x0115, [IntPtr]6, [IntPtr]::Zero)
    }
}

function Get-StatusText {
    $status = @([WtsVisibilityNativeProbe]::Children($script:handle) | Where-Object {
        $_.Parent -eq $script:handle -and $_.Class -ieq 'STATIC' -and $_.Text -match '^\[read: (100|[0-9]{1,2})%\]$'
    })
    if ($status.Count -ne 1) { throw "Expected one read indicator, found $($status.Count)" }
    [string]$status[0].Text
}

function Invoke-MetricPaintTest([array]$Cards, [string]$Path) {
    $repo = Split-Path -Parent $Path
    $attributes = Join-Path $repo '.git/info/attributes'
    Run-Git $repo @('config', 'filter.wts-metric.clean', 'sleep 1; cat')
    [IO.File]::WriteAllText($attributes, "lines.txt filter=wts-metric`n")
    try {
    Clear-Paint $Cards
    $statusBefore = Get-StatusText
    $initialMetric = [regex]::Match($statusBefore, '\[read: (100|[0-9]{1,2})%\]')
    if (-not $initialMetric.Success) { throw 'Initial read percentage is missing' }
    $initialPercent = [int]$initialMetric.Groups[1].Value
    $metricAdvanced = $false
    $lastStatus = $statusBefore
    $statusChanges = 0
    $samples = 0
    $badViewport = 0
    $badCards = 0
    $minimumDeadline = [DateTime]::UtcNow.AddMilliseconds(1400)
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    $nextChange = [DateTime]::UtcNow
    while ([DateTime]::UtcNow -lt $deadline -and (-not $metricAdvanced -or [DateTime]::UtcNow -lt $minimumDeadline)) {
        if (-not $metricAdvanced -and [DateTime]::UtcNow -ge $nextChange) {
            [IO.File]::SetLastWriteTimeUtc($Path, [DateTime]::UtcNow)
            $nextChange = [DateTime]::UtcNow.AddMilliseconds(600)
            $minimumDeadline = [DateTime]::UtcNow.AddMilliseconds(1400)
        }
        $scale = [Math]::Max(96, [int][WtsVisibilityNativeProbe]::GetDpiForWindow($script:handle))
        $viewportUpdate = [WtsVisibilityNativeProbe]::Update($script:viewport)
        $viewportClient = [WtsVisibilityNativeProbe]::ClientBounds($script:viewport)
        if ($viewportUpdate.Pending -and -not (Is-NarrowStrip $viewportUpdate $viewportClient $scale)) { $badViewport++ }
        if ($viewportUpdate.Pending) { [void][WtsVisibilityNativeProbe]::UpdateWindow($script:viewport) }
        foreach ($Card in @($Cards)) {
            $update = [WtsVisibilityNativeProbe]::Update($Card.Handle)
            $client = [WtsVisibilityNativeProbe]::ClientBounds($Card.Handle)
            if (Is-BroadCardUpdate $update $client $scale) { $badCards++ }
            if ($update.Pending) { [void][WtsVisibilityNativeProbe]::UpdateWindow($Card.Handle) }
        }
        $status = Get-StatusText
        $metric = [regex]::Match($status, '\[read: (100|[0-9]{1,2})%\]')
        if ($metric.Success -and [int]$metric.Groups[1].Value -gt $initialPercent) { $metricAdvanced = $true }
        if ($status -cne $lastStatus) { $statusChanges++ }
        $lastStatus = $status
        $samples++
        Start-Sleep -Milliseconds 50
    }
    Clear-Paint $Cards
    if ($badViewport -gt 0 -or $badCards -gt 0) { throw "Metric-only updates produced broad paint invalidation (viewport=$badViewport cards=$badCards)" }
    if (-not $metricAdvanced) { throw 'No increase in read percentage was observed during the paint check' }
    [ordered]@{
        result = 'PASS'
        maximum_window_ms = 15000
        samples = $samples
        status_before = $statusBefore
        status_after = $lastStatus
        status_changes = $statusChanges
        observed_read_percentage_update = $true
        broad_viewport_updates = $badViewport
        broad_card_updates = $badCards
    }
    } finally {
        Remove-Item -LiteralPath $attributes
        Run-Git $repo @('config', '--unset', 'filter.wts-metric.clean')
    }
}

function Start-Ui {
    $script:process = $null
    $script:handle = [IntPtr]::Zero
    $script:viewport = [IntPtr]::Zero
    $start = [Diagnostics.ProcessStartInfo]::new($Binary)
    $start.WorkingDirectory = $script:fixtureHome
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.Environment['HOME'] = $env:HOME
    $start.Environment['USERPROFILE'] = $env:USERPROFILE
    $start.Environment['GIT_CONFIG_GLOBAL'] = $env:GIT_CONFIG_GLOBAL
    $start.Environment['GIT_CONFIG_NOSYSTEM'] = $env:GIT_CONFIG_NOSYSTEM
    $script:process = [Diagnostics.Process]::Start($start)
    if ($null -eq $script:process) { throw 'Could not start visibility UI' }
    Wait-For {
        if ($script:process.HasExited) { throw "UI exited before showing a window with code $($script:process.ExitCode)" }
        $script:handle = [WtsVisibilityNativeProbe]::Find([uint32]$script:process.Id)
        $script:handle -ne [IntPtr]::Zero
    } 'visibility window' 30
    $viewportWindows = @([WtsVisibilityNativeProbe]::Children($script:handle) | Where-Object { $_.Class -eq 'WtsDashboardViewport' })
    if ($viewportWindows.Count -ne 1) { throw "Expected one dashboard viewport, found $($viewportWindows.Count)" }
    $script:viewport = $viewportWindows[0].Handle
    $dpi = [Math]::Max(96, [int][WtsVisibilityNativeProbe]::GetDpiForWindow($script:handle))
    if (-not [WtsVisibilityNativeProbe]::SetWindowPos($script:handle, [IntPtr]::Zero, 0, 0, [int](760 * $dpi / 96), [int](860 * $dpi / 96), 0x14)) { throw 'Visibility fixture resize failed' }
    Start-Sleep -Milliseconds 150
}

function Stop-Ui {
    if ($null -eq $script:process) { return }
    $process = $script:process
    try {
        if (-not $process.HasExited) {
            if ($script:handle -ne [IntPtr]::Zero) { [void][WtsVisibilityNativeProbe]::PostMessage($script:handle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) }
            if (-not $process.WaitForExit(10000) -and -not $process.HasExited) {
                $process.Kill()
                [void]$process.WaitForExit()
            }
        }
    } finally {
        $process.Dispose()
        $script:process = $null
        $script:handle = [IntPtr]::Zero
        $script:viewport = [IntPtr]::Zero
    }
}

function Select-SafeTarget([array]$Candidates) {
    foreach ($candidate in @($Candidates)) {
        try {
            [void]@(Get-PointsForCard $candidate.Card)
            return $candidate.Card
        } catch { }
    }
    throw 'No drag target stayed away from the viewport scroll edges'
}

function Invoke-VisibilityTest([array]$InitialRows, [switch]$Keyboard, [bool]$BaselineMode = $false) {
    $fullExpected = @(Get-ExpectedVisibleRows '')
    Assert-RowSnapshot $InitialRows $fullExpected 'Initial exact rows'
    $initialSurface = Assert-NativeVisibilitySurface $InitialRows ''
    $worktrees = @($InitialRows | Where-Object { $_.Kind -eq 'worktree' })
    if ($worktrees.Count -ne 6) { throw "Expected six fixture worktree rows, found $($worktrees.Count)" }
    $target = $worktrees[0]
    $unrelated = $worktrees[1]
    $targetButton = @(Find-VisibilityButton $target.Card 'Hide')
    if ($targetButton.Count -ne 1) { throw 'Keyboard visibility probe lacks a native Hide button' }
    $tab = Invoke-TabProbe $targetButton[0]
    if ($tab.result -cne 'PASS') { throw 'Could not verify Tab navigation from Hide' }
    Invoke-VisibilityActivation $targetButton[0] $Keyboard
    $hiddenExpected = @(Get-ExpectedVisibleRows $target.Key)
    $afterHide = Wait-For-ExpectedRows $hiddenExpected 'Hide exact rows' 10
    $afterHideSurface = Assert-NativeVisibilitySurface $afterHide $target.Key
    $unrelatedAfterHide = @($afterHide | Where-Object { $_.Key -ceq $unrelated.Key })
    if ($unrelatedAfterHide.Count -ne 1 -or $unrelatedAfterHide[0].Handle -ne $unrelated.Handle) { throw 'Unrelated card HWND was not stable across hide' }
    $visibilityAfterHideData = Assert-VisibilityFile $target.RepositoryKey $target.WorktreeKey $true
    $visibilityAfterHide = Get-FileSnapshot $script:visibilityPath
    Assert-StableConfiguration

    Stop-Ui
    Start-Ui
    $afterRestartHidden = Wait-For-ExpectedRows $hiddenExpected 'Saved hide after restart' 20
    $afterRestartHiddenSurface = Assert-NativeVisibilitySurface $afterRestartHidden $target.Key
    if (@($afterRestartHidden | Where-Object { $_.Key -ceq $target.Key }).Count -ne 0) { throw 'Hidden worktree returned after restart without Show all' }
    Assert-FileSnapshot $visibilityAfterHide $script:visibilityPath 'Saved visibility after restart'
    Assert-StableConfiguration
    $unrelatedAfterRestart = @($afterRestartHidden | Where-Object { $_.Key -ceq $unrelated.Key })
    if ($unrelatedAfterRestart.Count -ne 1) { throw 'Unrelated card disappeared after restart' }

    $showAllButton = @(Find-VisibilityTopButton 'Show all')
    if ($showAllButton.Count -ne 1) { throw 'Show all button was not present after restart' }
    Invoke-VisibilityActivation $showAllButton[0] $Keyboard
    $afterShowAll = Wait-For-ExpectedRows $fullExpected 'Show all exact rows' 10
    $afterShowAllSurface = Assert-NativeVisibilitySurface $afterShowAll $target.Key $true
    Assert-FileSnapshot $visibilityAfterHide $script:visibilityPath 'Show all visibility configuration'
    Assert-StableConfiguration
    $unrelatedAfterShowAll = @($afterShowAll | Where-Object { $_.Key -ceq $unrelated.Key })
    if ($unrelatedAfterShowAll.Count -ne 1 -or $unrelatedAfterShowAll[0].Handle -ne $unrelatedAfterRestart[0].Handle) { throw 'Unrelated card HWND was not stable across Show all' }

    Stop-Ui
    Start-Ui
    $afterShowAllRestartHidden = Wait-For-ExpectedRows $hiddenExpected 'Transient Show all after restart' 20
    $afterShowAllRestartHiddenSurface = Assert-NativeVisibilitySurface $afterShowAllRestartHidden $target.Key
    Assert-FileSnapshot $visibilityAfterHide $script:visibilityPath 'Transient Show all visibility configuration'
    Assert-StableConfiguration
    $unrelatedAfterShowAllRestart = @($afterShowAllRestartHidden | Where-Object { $_.Key -ceq $unrelated.Key })
    if ($unrelatedAfterShowAllRestart.Count -ne 1) { throw 'Unrelated card disappeared after transient Show all restart' }
    $showAllAgainButton = @(Find-VisibilityTopButton 'Show all')
    if ($showAllAgainButton.Count -ne 1) { throw 'Show all button was not restored after transient restart' }
    Invoke-VisibilityActivation $showAllAgainButton[0] $Keyboard
    $afterShowAllAgain = Wait-For-ExpectedRows $fullExpected 'Show all exact rows after transient restart' 10
    $afterShowAllAgainSurface = Assert-NativeVisibilitySurface $afterShowAllAgain $target.Key $true
    Assert-FileSnapshot $visibilityAfterHide $script:visibilityPath 'Show all visibility configuration after transient restart'
    Assert-StableConfiguration

    $unhideRow = @($afterShowAllAgain | Where-Object { $_.Key -ceq $target.Key })
    if ($unhideRow.Count -ne 1) { throw 'Hidden worktree was not shown by Show all' }
    $unhideButton = @(Find-VisibilityButton $unhideRow[0].Card 'Unhide')
    if ($unhideButton.Count -ne 1) { throw 'Show all did not expose one native Unhide button' }
    $flicker = Invoke-VisibilityFrameProbe $unhideButton[0] $BaselineMode
    Invoke-VisibilityActivation $unhideButton[0] $Keyboard
    $afterUnhide = Wait-For-ExpectedRows $fullExpected 'Unhide exact rows' 10
    $afterUnhideSurface = Assert-NativeVisibilitySurface $afterUnhide '' $true
    $visibilityAfterUnhideData = Assert-VisibilityFile $target.RepositoryKey $target.WorktreeKey $false
    $visibilityAfterUnhide = Get-FileSnapshot $script:visibilityPath
    Assert-StableConfiguration
    $unrelatedAfterUnhide = @($afterUnhide | Where-Object { $_.Key -ceq $unrelated.Key })
    if ($unrelatedAfterUnhide.Count -ne 1 -or $unrelatedAfterUnhide[0].Handle -ne $unrelatedAfterShowAllRestart[0].Handle) { throw 'Unrelated card HWND was not stable across Unhide' }

    $hideHiddenButton = @(Find-VisibilityTopButton 'Show all')
    if ($hideHiddenButton.Count -ne 1) { throw 'Show all checkbox was not present after Unhide' }
    Invoke-VisibilityActivation $hideHiddenButton[0] $Keyboard
    $afterReturn = Wait-For-ExpectedRows $fullExpected 'Return from Show all exact rows' 10
    $afterReturnSurface = Assert-NativeVisibilitySurface $afterReturn ''
    Assert-FileSnapshot $visibilityAfterUnhide $script:visibilityPath 'Hide hidden visibility configuration'
    Assert-StableConfiguration

    Stop-Ui
    Start-Ui
    $afterFinalRestart = Wait-For-ExpectedRows $fullExpected 'Permanent Unhide after restart' 20
    $afterFinalRestartSurface = Assert-NativeVisibilitySurface $afterFinalRestart ''
    Assert-FileSnapshot $visibilityAfterUnhide $script:visibilityPath 'Permanent Unhide visibility configuration'
    Assert-StableConfiguration
    [ordered]@{
        result = if ($flicker.result -eq 'FAIL') { 'FAIL' } elseif ($flicker.result -eq 'INCONCLUSIVE') { 'INCONCLUSIVE' } else { 'PASS' }
        top_button_class = 'Button'
        initial_surface = $initialSurface
        after_hide_surface = $afterHideSurface
        after_restart_hidden_surface = $afterRestartHiddenSurface
        after_show_all_surface = $afterShowAllSurface
        after_show_all_restart_hidden_surface = $afterShowAllRestartHiddenSurface
        after_show_all_again_surface = $afterShowAllAgainSurface
        flicker = $flicker
        after_unhide_surface = $afterUnhideSurface
        after_return_surface = $afterReturnSurface
        after_final_restart_surface = $afterFinalRestartSurface
        activation = if ($Keyboard) { 'keyboard' } else { 'mouse' }
        tab_navigation = $tab
        hidden_key = [ordered]@{ repository = $target.RepositoryKey; worktree = $target.WorktreeKey }
        saved_hidden = $visibilityAfterHideData
        saved_empty_after_unhide = $visibilityAfterUnhideData
        rows = [ordered]@{
            initial = Get-PublicRows $InitialRows
            after_hide = Get-PublicRows $afterHide
            after_restart_hidden = Get-PublicRows $afterRestartHidden
            after_show_all = Get-PublicRows $afterShowAll
            after_show_all_restart_hidden = Get-PublicRows $afterShowAllRestartHidden
            after_show_all_again = Get-PublicRows $afterShowAllAgain
            after_unhide = Get-PublicRows $afterUnhide
            after_return = Get-PublicRows $afterReturn
            after_final_restart = Get-PublicRows $afterFinalRestart
        }
        show_all_file_unchanged = $true
        config_unchanged = $true
        order_unchanged = $true
        unrelated_card_hwnd_stable = $true
    }
}

$names = @('HOME', 'USERPROFILE', 'GIT_DIR', 'GIT_WORK_TREE', 'GIT_COMMON_DIR', 'GIT_INDEX_FILE', 'GIT_CONFIG_GLOBAL', 'GIT_CONFIG_NOSYSTEM')
$previous = @{}
foreach ($name in $names) { $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
$fixtureHome = Join-Path $Output 'home'
New-Item -ItemType Directory -Path $fixtureHome | Out-Null
$script:fixtureHome = $fixtureHome
$script:process = $null
$script:handle = [IntPtr]::Zero
$script:viewport = [IntPtr]::Zero
$script:actions = [Collections.Generic.List[object]]::new()
$script:expectedRows = @()
$script:baselineCaptions = @{}
$script:configPath = Join-Path $fixtureHome '.config/wts/config.json'
$script:visibilityPath = Join-Path $fixtureHome '.config/wts/config.visibility.json'
$script:orderPath = Join-Path $fixtureHome '.config/wts/config.order.json'
$proof = [ordered]@{
    result = 'FAIL'
    mode = if ($Baseline) { 'baseline' } else { 'regression' }
    binary = $Binary
    sha256 = (Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash
    output = $Output
    actions = @()
    paint = $null
    visibility = $null
    assumptions = @('Fixture uses direct linked-worktree metadata and does not run git worktree add')
}
$dpiContext = [IntPtr]::Zero
$cursor = [WtsVisibilityNativeProbe+Point]::new()
$restoreCursor = [WtsVisibilityNativeProbe]::GetCursorPos([ref]$cursor)
try {
    $env:HOME = Join-Path $Output 'unix-home'
    $env:USERPROFILE = $fixtureHome
    $env:GIT_CONFIG_GLOBAL = Join-Path $Output 'empty-gitconfig'
    $env:GIT_CONFIG_NOSYSTEM = '1'
    foreach ($name in @('GIT_DIR', 'GIT_WORK_TREE', 'GIT_COMMON_DIR', 'GIT_INDEX_FILE')) { Remove-Item "Env:$name" -ErrorAction SilentlyContinue }
    $dpiContext = [WtsVisibilityNativeProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
    $repositories = @('alpha', 'beta', 'gamma') | ForEach-Object { New-Repository $_ }
    $script:expectedRows = @(New-ExpectedRows $repositories)
    foreach ($entry in $repositories) {
        & $CliBinary add $entry.Repository | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "Fixture registration failed for $($entry.Name)" }
    }
    Wait-For { Test-Path -LiteralPath $script:configPath -PathType Leaf } 'fixture registry configuration' 10
    Start-Ui
    $initialRows = Wait-For-ExpectedRows (Get-ExpectedVisibleRows '') 'initial exact rows' 40
    Set-BaselineCaptions $initialRows
    $script:configBefore = Get-FileSnapshot $script:configPath
    $script:orderBefore = Get-FileSnapshot $script:orderPath
    $cards = @(Get-Cards)
    Settle-Paint $cards
    if (-not $Baseline) {
        [void](Assert-NativeVisibilitySurface $initialRows '')
    }
    if ($VisibilityOnly) {
        $proof.visibility = Invoke-VisibilityTest $initialRows -BaselineMode ([bool]$Baseline)
        if ($proof.visibility.result -eq 'FAIL') { throw 'Live visibility frame regression found bright screen frames' }
        $keyboardRows = Wait-For-ExpectedRows (Get-ExpectedVisibleRows '') 'rows before keyboard visibility checks' 10
        $proof.keyboard_visibility = Invoke-VisibilityTest $keyboardRows -Keyboard -BaselineMode ([bool]$Baseline)
        if ($proof.keyboard_visibility.result -eq 'FAIL') { throw 'Live visibility frame regression found bright screen frames' }
        $visibilityRuns = @($proof.visibility, $proof.keyboard_visibility)
        $proof.result = if ($Baseline -and @($visibilityRuns | Where-Object { $_.result -eq 'INCONCLUSIVE' }).Count -gt 0) { 'INCONCLUSIVE' } else { 'PASS' }
        Write-Host "$($proof.result). Visibility control evidence at $Output"
        return
    }
    $worktreeRows = @($initialRows | Where-Object { $_.Kind -eq 'worktree' })
    $worktreeSource = $worktreeRows[0]
    $worktreeCandidates = @($worktreeRows | Where-Object { $_.Key -cne $worktreeSource.Key -and $_.Branch -match 'topic' })
    if ($worktreeCandidates.Count -eq 0) { $worktreeCandidates = @($worktreeRows | Where-Object { $_.Key -cne $worktreeSource.Key }) }
    $worktreeTarget = Select-SafeTarget $worktreeCandidates
    $repositoryRows = @($initialRows | Where-Object { $_.Kind -eq 'repository' })
    $repositorySource = $repositoryRows[0]
    $repositoryTarget = Select-SafeTarget @($repositoryRows | Where-Object { $_.Key -cne $repositorySource.Key })
    Invoke-DragProbe 'worktree' $worktreeSource.Card $worktreeTarget
    Invoke-DragProbe 'repository' $repositorySource.Card $repositoryTarget
    $postPaintRows = Wait-For-ExpectedRows (Get-ExpectedVisibleRows '') 'rows after paint probes' 10
    Assert-StableConfiguration
    $reproduced = @($script:actions | Where-Object { $_.unrelated_broad_invalidated_count -gt 0 -and $_.unrelated_broad_painted_count -gt 0 })
    $stationary = @($script:actions | Where-Object { $_.name -match 'stationary' })
    $moving = @($script:actions | Where-Object { $_.name -match 'moving' })
    $stationaryEvidence = @($stationary | Where-Object { $_.unrelated_broad_invalidated_count -gt 0 -and $_.unrelated_broad_painted_count -gt 0 })
    $movingEvidence = @($moving | Where-Object { $_.unrelated_broad_invalidated_count -gt 0 -and $_.unrelated_broad_painted_count -gt 0 })
    $wholeListActions = @($script:actions | Where-Object { $_.all_cards_invalidated -and $_.all_invalidated_cards_painted })
    if ($Baseline) {
        if ($reproduced.Count -lt 4) { throw 'Baseline did not show unrelated cards being invalidated and painted during drag movement' }
        if ($stationaryEvidence.Count -lt 4) { throw 'Stationary repeated drag moves did not repaint unrelated cards' }
        if ($movingEvidence.Count -lt 4) { throw 'Moving drag moves did not repaint unrelated cards' }
        if ($wholeListActions.Count -lt 4) { throw 'Drag movement did not produce repeated whole-list paint invalidations' }
        $proof.paint = [ordered]@{
            result = 'REPRODUCED'
            card_count = $cards.Count
            stationary_actions = $stationary.Count
            moving_actions = $moving.Count
            unrelated_repaints_observed = $reproduced.Count
            whole_list_repaint_actions = $wholeListActions.Count
            whole_list_repainted = $wholeListActions.Count -gt 0
        }
        $proof.visibility = [ordered]@{ result = 'SKIPPED'; reason = 'Baseline mode retains the pre-visibility executable contract' }
        $proof.result = 'PASS'
        Write-Host "PASS. Baseline repaint reproduction retained. Evidence at $Output"
    } else {
        $badPaint = @($script:actions | Where-Object {
            $_.unrelated_broad_invalidated_count -gt 0 -or $_.unrelated_broad_painted_count -gt 0 -or $_.viewport.broad -or $_.all_cards_broad_invalidated
        })
        if ($badPaint.Count -ne 0) { throw "Regression paint check found broad unrelated or whole-list invalidation in $($badPaint.Count) drag probes" }
        $metric = Invoke-MetricPaintTest $cards (Join-Path $repositories[0].Repository 'lines.txt')
        $edgeScroll = Invoke-EdgeScrollTest
        $proof.paint = [ordered]@{
            result = 'PASS'
            card_count = $cards.Count
            stationary_actions = $stationary.Count
            moving_actions = $moving.Count
            broad_unrelated_repaints = 0
            whole_list_repaint_actions = $wholeListActions.Count
            metric = $metric
            edge_scroll = $edgeScroll
        }
        $proof.visibility = Invoke-VisibilityTest $postPaintRows -BaselineMode ([bool]$Baseline)
        if ($proof.visibility.result -eq 'FAIL') { throw 'Live visibility frame regression found bright screen frames' }
        $proof.result = 'PASS'
        Write-Host "PASS. Visibility regression and paint checks passed. Evidence at $Output"
    }
} catch {
    $proof.failure = $_.Exception.Message
    throw
} finally {
    try { Stop-Ui } catch { }
    if ($restoreCursor) { [void][WtsVisibilityNativeProbe]::SetCursorPos($cursor.X, $cursor.Y) }
    $proof.actions = $script:actions.ToArray()
    $proof.actions | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $Output 'paint-trace.json') -Encoding utf8NoBOM
    $proof | ConvertTo-Json -Depth 16 | Set-Content -LiteralPath (Join-Path $Output 'proof.json') -Encoding utf8NoBOM
    foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process') }
    if ($dpiContext -ne [IntPtr]::Zero) { [void][WtsVisibilityNativeProbe]::SetThreadDpiAwarenessContext($dpiContext) }
}
