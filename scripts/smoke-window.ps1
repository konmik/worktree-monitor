param([string]$Binary = "$PSScriptRoot/../target/debug/wts.exe")

$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path $Binary).Path
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class WtsWindowProbe {
    public struct Rect { public int Left, Top, Right, Bottom; }
    public struct Point { public int X, Y; }
    public struct GuiInfo { public uint Size, Flags; public IntPtr Active, Focus, Capture, MenuOwner, MoveSize, Caret; public Rect CaretRect; }
    public struct ScrollInfo { public uint Size, Mask; public int Min, Max; public uint Page; public int Position, TrackPosition; }
    public delegate bool EnumProc(IntPtr handle, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr handle, out uint processId);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr handle, StringBuilder text, int count);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr SendMessageTimeout(IntPtr handle, uint message, UIntPtr wparam, StringBuilder text, uint flags, uint timeout, out UIntPtr result);
    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW")] public static extern IntPtr GetWindowLongPtr(IntPtr handle, int index);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr handle);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr handle);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr handle, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr handle, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr handle, out Rect rect);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr handle, out Rect rect);
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr handle, int id);
    [DllImport("user32.dll")] public static extern IntPtr GetParent(IntPtr handle);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr handle, StringBuilder text, int count);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr handle, ref Point point);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out Point point);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr handle, uint flags);
    [DllImport("user32.dll")] public static extern bool GetGUIThreadInfo(uint thread, ref GuiInfo info);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint x, uint y, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] public static extern bool GetScrollInfo(IntPtr handle, int bar, ref ScrollInfo info);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr handle, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("kernel32.dll")] public static extern uint GetConsoleProcessList(uint[] processIds, uint count);
    public static bool SharesConsole(uint processId) {
        var ids = new uint[256];
        var count = GetConsoleProcessList(ids, (uint)ids.Length);
        if (count == 0 || count > ids.Length) { throw new InvalidOperationException("Console process list unavailable"); }
        return Array.IndexOf(ids, processId, 0, (int)count) >= 0;
    }
    public static IntPtr Find(uint pid) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((handle, data) => {
            uint owner;
            GetWindowThreadProcessId(handle, out owner);
            var title = new StringBuilder(256);
            GetWindowText(handle, title, title.Capacity);
            if (owner == pid && IsWindowVisible(handle) && title.ToString().StartsWith("WTS", StringComparison.OrdinalIgnoreCase)) { found = handle; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }
    public static string Text(IntPtr parent) {
        var all = new StringBuilder();
        EnumChildWindows(parent, (handle, data) => {
            if (!IsWindowVisible(handle)) { return true; }
            var text = new StringBuilder(65536);
            UIntPtr result;
            SendMessageTimeout(handle, 0x000D, (UIntPtr)text.Capacity, text, 2, 2000, out result);
            all.AppendLine(text.ToString());
            return true;
        }, IntPtr.Zero);
        return all.ToString();
    }
    public static string[] Captions(IntPtr parent) {
        var captions = new System.Collections.Generic.List<string>();
        EnumChildWindows(parent, (handle, data) => {
            if (GetParent(handle) != parent || !IsWindowVisible(handle)) { return true; }
            var text = new StringBuilder(65536);
            GetWindowText(handle, text, text.Capacity);
            captions.Add(text.ToString());
            return true;
        }, IntPtr.Zero);
        return captions.ToArray();
    }
    public static IntPtr FirstCard(IntPtr parent) {
        IntPtr found = IntPtr.Zero;
        EnumChildWindows(parent, (handle, data) => {
            var text = new StringBuilder(65536);
            GetWindowText(handle, text, text.Capacity);
            if (GetParent(handle) == parent && IsWindowVisible(handle) && text.Length != 0) { found = handle; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }
    public static int ScrollPosition(IntPtr handle) {
        handle = Scrollbar(GetParent(handle));
        var info = new ScrollInfo { Size = (uint)Marshal.SizeOf(typeof(ScrollInfo)), Mask = 7 };
        if (!GetScrollInfo(handle, 2, ref info)) { throw new InvalidOperationException("Scrollbar state unavailable"); }
        return info.Position;
    }
    public static IntPtr Scrollbar(IntPtr parent) {
        IntPtr found = IntPtr.Zero;
        EnumChildWindows(parent, (handle, data) => {
            var name = new StringBuilder(128);
            GetClassName(handle, name, name.Capacity);
            if (GetParent(handle) == parent && name.ToString().Equals("SCROLLBAR", StringComparison.OrdinalIgnoreCase)) { found = handle; return false; }
            return true;
        }, IntPtr.Zero);
        if (found == IntPtr.Zero) throw new InvalidOperationException("Native scrollbar is missing");
        return found;
    }
    public static IntPtr Position(int x, int y) {
        return new IntPtr(unchecked((int)(((uint)(ushort)y << 16) | (ushort)x)));
    }
    public static void CheckFrame(IntPtr window, IntPtr viewport) {
        long style = GetWindowLongPtr(window, -16).ToInt64();
        if ((style & 0x00C00000) != 0 || (style & 0x00040000) == 0) throw new InvalidOperationException("Window is not frameless and resizable");
        Rect bounds, client, view;
        GetWindowRect(window, out bounds);
        GetClientRect(window, out client);
        GetWindowRect(viewport, out view);
        var origin = new Point();
        ClientToScreen(window, ref origin);
        if (bounds.Left != origin.X || bounds.Top != origin.Y || bounds.Right != origin.X + client.Right || bounds.Bottom != origin.Y + client.Bottom) throw new InvalidOperationException("Nonclient frame remains visible");
        if (view.Bottom != origin.Y + client.Bottom) throw new InvalidOperationException("Bottom padding remains");
        int middleX = (bounds.Left + bounds.Right) / 2, middleY = (bounds.Top + bounds.Bottom) / 2;
        int[,] points = { {bounds.Left + 1, bounds.Top + 1, 13}, {middleX, bounds.Top + 1, 12}, {bounds.Right - 2, bounds.Top + 1, 14}, {bounds.Left + 1, middleY, 10}, {bounds.Right - 2, middleY, 11}, {bounds.Left + 1, bounds.Bottom - 2, 16}, {middleX, bounds.Bottom - 2, 15}, {bounds.Right - 2, bounds.Bottom - 2, 17} };
        for (int i = 0; i < points.GetLength(0); i++) {
            if (SendMessage(window, 0x0084, IntPtr.Zero, Position(points[i, 0], points[i, 1])).ToInt64() != points[i, 2]) throw new InvalidOperationException("Resize hit test failed at edge " + i);
        }
    }
    public static void DragEmptySpace(IntPtr window, IntPtr viewport) {
        Rect before, after, view;
        GetWindowRect(window, out before);
        GetWindowRect(viewport, out view);
        int x = (view.Left + view.Right) / 2, y = view.Bottom - 8 * (int)GetDpiForWindow(window) / 96;
        var target = WindowFromPoint(new Point { X = x, Y = y });
        if (y < view.Top || y >= view.Bottom || (target != viewport && target != window)) throw new InvalidOperationException("Empty-gap drag point is not the fixture window");
        Drag(window, x, y, 40, 30);
        WaitFor(() => {
            GetWindowRect(window, out after);
            return after.Left != before.Left || after.Top != before.Top;
        }, "Dragging empty space did not move the window");
    }
    public static void DragCorner(IntPtr window) {
        Rect before, after;
        GetWindowRect(window, out before);
        Drag(window, before.Right - 2, before.Bottom - 2, 40, 30);
        WaitFor(() => {
            GetWindowRect(window, out after);
            return after.Right - after.Left > before.Right - before.Left && after.Bottom - after.Top > before.Bottom - before.Top;
        }, "Dragging the corner did not resize the window");
    }
    public static void CancelEmptyMove(IntPtr window, IntPtr viewport) {
        Rect view;
        GetWindowRect(viewport, out view);
        Drag(window, (view.Left + view.Right) / 2, view.Bottom - 8 * (int)GetDpiForWindow(window) / 96, 0, 0, true);
    }
    public static void ClickScrollbar(IntPtr window, IntPtr scrollbar) {
        Rect bar;
        GetWindowRect(scrollbar, out bar);
        var point = new Point { X = (bar.Left + bar.Right) / 2, Y = bar.Top + (bar.Right - bar.Left) / 2 };
        Point saved;
        if (!GetCursorPos(out saved)) throw new InvalidOperationException("Cursor position unavailable");
        bool pressed = false;
        try {
            if (!SetCursorPos(point.X, point.Y) || WindowFromPoint(point) != scrollbar) throw new InvalidOperationException("Scrollbar click target is not the fixture control");
            mouse_event(2, 0, 0, 0, UIntPtr.Zero);
            pressed = true;
            WaitFor(() => ScrollPosition(GetDlgItem(window, 1001)) == 0, "Native scrollbar arrow did not scroll to the top");
            mouse_event(4, 0, 0, 0, UIntPtr.Zero);
            pressed = false;
        } finally {
            if (pressed) mouse_event(4, 0, 0, 0, UIntPtr.Zero);
            if (!SetCursorPos(saved.X, saved.Y)) throw new InvalidOperationException("Cursor restoration failed");
        }
    }
    private static void WaitFor(Func<bool> condition, string error) {
        var deadline = DateTime.UtcNow.AddSeconds(3);
        do {
            if (condition()) return;
            System.Threading.Thread.Sleep(20);
        } while (DateTime.UtcNow < deadline);
        throw new InvalidOperationException(error);
    }
    private static void Drag(IntPtr window, int x, int y, int dx, int dy, bool cancel = false) {
        Point saved;
        if (!GetCursorPos(out saved)) throw new InvalidOperationException("Cursor position unavailable");
        bool pressed = false;
        try {
            if (!SetCursorPos(x, y) || GetAncestor(WindowFromPoint(new Point { X = x, Y = y }), 2) != window) throw new InvalidOperationException("Mouse target is not the fixture window");
            mouse_event(2, 0, 0, 0, UIntPtr.Zero);
            pressed = true;
            uint owner;
            uint thread = GetWindowThreadProcessId(window, out owner);
            WaitFor(() => {
                var info = new GuiInfo { Size = (uint)Marshal.SizeOf(typeof(GuiInfo)) };
                return GetGUIThreadInfo(thread, ref info) && GetAncestor(info.Capture, 2) == window;
            }, "Fixture did not capture the mouse for dragging");
            if (cancel) {
                var info = new GuiInfo { Size = (uint)Marshal.SizeOf(typeof(GuiInfo)) };
                GetGUIThreadInfo(thread, ref info);
                if (!PostMessage(info.Capture, 0x0100, new IntPtr(0x1b), IntPtr.Zero)) throw new InvalidOperationException("Escape could not be sent to the fixture");
                WaitFor(() => GetGUIThreadInfo(thread, ref info) && info.Capture == IntPtr.Zero, "Escape left the window move holding mouse capture");
                return;
            }
            Rect before;
            GetWindowRect(window, out before);
            if (!SetCursorPos(x + dx, y + dy)) throw new InvalidOperationException("Mouse drag failed");
            WaitFor(() => {
                Rect changed;
                return GetWindowRect(window, out changed) && (changed.Left != before.Left || changed.Top != before.Top || changed.Right != before.Right || changed.Bottom != before.Bottom);
            }, "Fixture drag did not update window bounds");
            mouse_event(4, 0, 0, 0, UIntPtr.Zero);
            pressed = false;
        } finally {
            if (pressed) mouse_event(4, 0, 0, 0, UIntPtr.Zero);
            if (!SetCursorPos(saved.X, saved.Y)) throw new InvalidOperationException("Cursor restoration failed");
        }
    }
    public static int Top(IntPtr handle) {
        Rect rect;
        if (!GetWindowRect(handle, out rect)) { throw new InvalidOperationException("Card bounds unavailable"); }
        return rect.Top;
    }
    public static int Height(IntPtr handle) {
        Rect rect;
        if (!GetClientRect(handle, out rect)) { throw new InvalidOperationException("Control bounds unavailable"); }
        return rect.Bottom - rect.Top;
    }
}
'@

function Wait-For([scriptblock]$Condition, [string]$Label, [int]$Seconds = 40) {
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        if (& $Condition) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for $Label"
}

function Wait-ScrollbarHidden([IntPtr]$Scrollbar, [Diagnostics.Stopwatch]$SinceActivity) {
    Wait-For { -not [WtsWindowProbe]::IsWindowVisible($Scrollbar) } 'idle scrollbar to hide' 5
    if ($SinceActivity.Elapsed.TotalSeconds -lt 2.9) { throw 'Scrollbar hid before three seconds' }
}

function Run-Git([string[]]$Arguments) {
    & git -c core.fsmonitor=false -c commit.gpgsign=false -c "core.hooksPath=$fixture" -C $repo @Arguments
    if ($LASTEXITCODE -ne 0) { throw "Git failed with $LASTEXITCODE" }
}

function Has-Counts([int]$Added, [int]$Deleted) {
    $viewport = [WtsWindowProbe]::GetDlgItem($script:handle, 1001)
    $rows = @([WtsWindowProbe]::Captions($viewport) | Where-Object {
        $lines = @($_ -split "`r?`n")
        $lines.Count -ge 2 -and $lines[0] -eq $script:branch -and $lines[1] -match 'S\s+\+\d+/-\d+\b'
    })
    return $rows.Count -eq 1 -and $rows[0] -match "S\s+\+$Added/-$Deleted\b"
}

$fixture = Join-Path $env:LOCALAPPDATA "Temp/opencode/wts-window-$([Guid]::NewGuid().ToString('N'))"
$repo = Join-Path $fixture 'repo'
$fixtureHome = Join-Path $fixture 'home'
New-Item -ItemType Directory -Path $repo, $fixtureHome | Out-Null
$oldHome = $env:HOME
$oldProfile = $env:USERPROFILE
$process = $null
$launcher = $null
$previousDpiContext = [WtsWindowProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
try {
    $env:HOME = Join-Path $fixture 'unix-home'
    $env:USERPROFILE = $fixtureHome
    Run-Git @('init', '--quiet')
    [IO.File]::WriteAllText((Join-Path $repo 'lines.txt'), "before`n")
    Run-Git @('add', '.')
    Run-Git @('-c', 'user.name=Wts Test', '-c', 'user.email=wts@example.invalid', 'commit', '--quiet', '-m', 'Fixture')
    $script:branch = (& git -c core.fsmonitor=false -c core.autocrlf=false -c commit.gpgsign=false -c "core.hooksPath=$fixture" -C $repo branch --show-current | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Git branch lookup failed' }
    [IO.File]::WriteAllText((Join-Path $repo 'lines.txt'), "before`nadded`n")
    Run-Git @('config', 'filter.wts-metric.clean', 'sleep 1; cat')
    [IO.File]::WriteAllText((Join-Path $repo '.git/info/attributes'), "lines.txt filter=wts-metric`n")
    & $Binary add $repo
    if ($LASTEXITCODE -ne 0) { throw 'Registration failed' }
    $config = Join-Path $fixtureHome '.config/wts/config.json'
    if (-not (Test-Path -LiteralPath $config)) { throw 'Registry was not written at the required home path' }
    $start = [Diagnostics.ProcessStartInfo]::new($Binary)
    $start.ArgumentList.Add('--live')
    $start.WorkingDirectory = $fixtureHome
    $start.UseShellExecute = $false
    $start.RedirectStandardInput = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $launcher = [Diagnostics.Process]::Start($start)
    $stdout = $launcher.StandardOutput.ReadToEndAsync()
    $stderr = $launcher.StandardError.ReadToEndAsync()
    $launcher.StandardInput.Close()
    $script:handle = [IntPtr]::Zero
    Wait-For {
        foreach ($child in @(Get-CimInstance Win32_Process -Filter "ParentProcessId = $($launcher.Id)")) {
            $script:handle = [WtsWindowProbe]::Find([uint32]$child.ProcessId)
            if ($script:handle -ne [IntPtr]::Zero) {
                $script:process = [Diagnostics.Process]::GetProcessById([int]$child.ProcessId)
                [void]$script:process.Handle
                return $true
            }
        }
        return $false
    } 'visible child window'
    if (-not $launcher.WaitForExit(3000) -or $launcher.ExitCode -ne 0) { throw 'Live launcher did not exit successfully' }
    if (-not $stdout.Wait(1000) -or -not $stderr.Wait(1000)) { throw 'Live launcher did not release stdout and stderr' }
    if ([WtsWindowProbe]::SharesConsole([uint32]$process.Id)) { throw 'Live window is still attached to the launching terminal' }
    $uiBinary = $process.MainModule.FileName
    $uiHash = (Get-FileHash -LiteralPath $uiBinary -Algorithm SHA256).Hash
    Write-Host 'PASS. Live launcher exited, stdout and stderr reached EOF, and the child window is detached from the launching terminal.'
    $initialStyle = [WtsWindowProbe]::GetWindowLongPtr($script:handle, -20).ToInt64()
    $foregroundOnShow = [WtsWindowProbe]::GetForegroundWindow()
    if (($initialStyle -band 8) -eq 0) { throw 'Window is not always on top' }
    if ($foregroundOnShow -eq $script:handle) { throw 'Window took foreground focus' }
    Wait-For { Has-Counts 1 0 } 'initial source count'
    $initial = [WtsWindowProbe]::Text($script:handle)
    if ($initial -cmatch 'REPOSITORIES|WORKTREES|LINE CHANGES') { throw 'Dashboard still shows summary totals' }
    $initialRead = [regex]::Match($initial, '\[read: (100|[0-9]{1,2})%\]')
    if (-not $initialRead.Success -or $initial -match 'Polling interval') { throw 'Read percentage is missing or still shows the interval' }
    $initialReadPercent = [int]$initialRead.Groups[1].Value
    [IO.File]::WriteAllText((Join-Path $fixture 'initial.txt'), $initial)
    $viewport = [WtsWindowProbe]::GetDlgItem($script:handle, 1001)
    if ($viewport -eq [IntPtr]::Zero) { throw 'Dashboard viewport is missing' }
    if (@([WtsWindowProbe]::Captions($viewport) | Where-Object { $_ -match '^repo(?:\r?\nRefreshing)?$' }).Count -ne 1) { throw 'Repository name is missing or still has a branch counter' }
    $scrollbar = [WtsWindowProbe]::Scrollbar($script:handle)
    $sinceResize = [Diagnostics.Stopwatch]::StartNew()
    if (-not [WtsWindowProbe]::SetWindowPos($script:handle, [IntPtr]::Zero, 0, 0, 600, 90, 22)) { throw 'Window resize failed' }
    [WtsWindowProbe]::CheckFrame($script:handle, $viewport)
    $card = [WtsWindowProbe]::FirstCard($viewport)
    if ($card -eq [IntPtr]::Zero) { throw 'Visible dashboard cards are missing' }
    $visibleBounds = [WtsWindowProbe+Rect]::new()
    [void][WtsWindowProbe]::GetWindowRect($card, [ref]$visibleBounds)
    if (-not [WtsWindowProbe]::IsWindowVisible($scrollbar)) { throw 'Resizing did not show the scrollbar' }
    $viewBounds = [WtsWindowProbe+Rect]::new()
    $barBounds = [WtsWindowProbe+Rect]::new()
    $windowBounds = [WtsWindowProbe+Rect]::new()
    [void][WtsWindowProbe]::GetWindowRect($viewport, [ref]$viewBounds)
    [void][WtsWindowProbe]::GetWindowRect($scrollbar, [ref]$barBounds)
    [void][WtsWindowProbe]::GetWindowRect($script:handle, [ref]$windowBounds)
    if ($barBounds.Left -lt $viewBounds.Right -or $barBounds.Right -gt $windowBounds.Right -or $barBounds.Top -lt $viewBounds.Top -or $barBounds.Bottom -gt $viewBounds.Bottom -or $barBounds.Right -le $barBounds.Left -or $barBounds.Bottom -le $barBounds.Top) { throw 'Scrollbar does not fit the right gutter' }
    Start-Sleep -Milliseconds 1500
    $sinceResize.Restart()
    foreach ($height in @(91, 90)) {
        if (-not [WtsWindowProbe]::SetWindowPos($script:handle, [IntPtr]::Zero, 0, 0, 600, $height, 22)) { throw 'Repeated window resize failed' }
    }
    Wait-ScrollbarHidden $scrollbar $sinceResize
    $hiddenBounds = [WtsWindowProbe+Rect]::new()
    [void][WtsWindowProbe]::GetWindowRect($card, [ref]$hiddenBounds)
    if ($visibleBounds.Left -ne $hiddenBounds.Left -or $visibleBounds.Right -ne $hiddenBounds.Right) { throw 'Scrollbar visibility shifted cards' }
    $cardBefore = [WtsWindowProbe]::Top($card) - [WtsWindowProbe]::Top($viewport)
    $sinceScroll = [Diagnostics.Stopwatch]::StartNew()
    [void][WtsWindowProbe]::SendMessage($script:handle, 0x0115, [IntPtr]1, $scrollbar)
    if (-not [WtsWindowProbe]::IsWindowVisible($scrollbar)) { throw 'Scrolling did not reveal the scrollbar' }
    $beforeScroll = [WtsWindowProbe]::ScrollPosition($viewport)
    $cardScrolled = [WtsWindowProbe]::Top($card) - [WtsWindowProbe]::Top($viewport)
    if ($beforeScroll -le 0 -or $cardScrolled -ge $cardBefore) { throw 'Viewport scroll did not move the real cards' }
    Start-Sleep -Milliseconds 1500
    $sinceScroll.Restart()
    [void][WtsWindowProbe]::SendMessage($viewport, 0x0115, [IntPtr]0, [IntPtr]::Zero)
    [void][WtsWindowProbe]::SendMessage($viewport, 0x0115, [IntPtr]1, [IntPtr]::Zero)
    if ([WtsWindowProbe]::ScrollPosition($viewport) -ne $beforeScroll) { throw 'Scroll activity did not restore the card position' }
    Wait-ScrollbarHidden $scrollbar $sinceScroll
    $script:nextMetricChange = [DateTime]::MinValue
    Wait-For {
        $text = [WtsWindowProbe]::Text($script:handle)
        $line = [WtsWindowProbe]::ScrollPosition($viewport)
        $height = [WtsWindowProbe]::Height($viewport)
        $focus = [WtsWindowProbe]::GetForegroundWindow().ToInt64()
        [IO.File]::AppendAllText((Join-Path $fixture 'scroll-trace.txt'), "$line`t$height`t$focus`t$($text.Replace("`r", '').Replace("`n", '\n'))`n")
        $polling = [regex]::Match($text, '\[read: (100|[0-9]{1,2})%\]')
        if ($polling.Success -and [int]$polling.Groups[1].Value -gt $initialReadPercent) { return $true }
        if ([DateTime]::UtcNow -ge $script:nextMetricChange) {
            [IO.File]::SetLastWriteTimeUtc((Join-Path $repo 'lines.txt'), [DateTime]::UtcNow)
            $script:nextMetricChange = [DateTime]::UtcNow.AddMilliseconds(600)
        }
        return $false
    } 'read percentage to include scheduled scans' 20
    Wait-For { [WtsWindowProbe]::Text($script:handle) -notmatch '(?m)^Refreshing\r?$' } 'metric scan completion' 20
    $afterScroll = [WtsWindowProbe]::ScrollPosition($viewport)
    [IO.File]::WriteAllText((Join-Path $fixture 'scroll.txt'), "before=$beforeScroll`nafter=$afterScroll`n")
    if ($afterScroll -ne $beforeScroll) { throw "Unchanged refresh reset the scroll position from $beforeScroll to $afterScroll" }
    $unchangedAnchor = [WtsWindowProbe]::Top($card) - [WtsWindowProbe]::Top($viewport)
    if ($unchangedAnchor -ne $cardScrolled) { throw "Unchanged refresh moved the card anchor from $cardScrolled to $unchangedAnchor" }
    [IO.File]::WriteAllText((Join-Path $fixture 'polling-time.txt'), [WtsWindowProbe]::Text($script:handle))
    [IO.File]::WriteAllText((Join-Path $repo 'lines.txt'), "before`nadded`nanother`n")
    Wait-For { Has-Counts 2 0 } 'live source count'
    $updated = [WtsWindowProbe]::Text($script:handle)
    if ([WtsWindowProbe]::ScrollPosition($viewport) -ne $beforeScroll) { throw 'Count-only refresh reset viewport scroll' }
    $updatedAnchor = [WtsWindowProbe]::Top($card) - [WtsWindowProbe]::Top($viewport)
    if ($updatedAnchor -ne $cardScrolled) { throw "Count-only refresh moved the card anchor from $cardScrolled to $updatedAnchor" }
    if ($updated -notmatch '\[read: (100|[0-9]{1,2})%\]' -or $updated -match 'Polling interval') { throw 'Count refresh lost the read percentage' }
    $foregroundOnRefresh = [WtsWindowProbe]::GetForegroundWindow()
    if ($foregroundOnRefresh -eq $script:handle) { throw 'Refresh took foreground focus' }
    if (([WtsWindowProbe]::GetWindowLongPtr($script:handle, -20).ToInt64() -band 0x08000000) -ne 0) { throw 'Window cannot be activated by clicking' }
    [IO.File]::WriteAllText((Join-Path $fixture 'updated.txt'), $updated)
    [void][WtsWindowProbe]::SendMessage($script:handle, 0x0115, [IntPtr]1, $scrollbar)
    [WtsWindowProbe]::ClickScrollbar($script:handle, $scrollbar)
    if (-not [WtsWindowProbe]::SetWindowPos($script:handle, [IntPtr]::Zero, 0, 0, 600, 400, 22)) { throw 'Window resize restoration failed' }
    [void][WtsWindowProbe]::SetWindowPos($script:handle, [IntPtr]::Zero, 80, 80, 600, 400, 20)
    [WtsWindowProbe]::DragEmptySpace($script:handle, $viewport)
    [WtsWindowProbe]::DragCorner($script:handle)
    [WtsWindowProbe]::CancelEmptyMove($script:handle, $viewport)
    Write-Host 'PASS. Frameless edge hit testing, empty-space dragging, corner resizing, flush bottom, and idle gutter scrollbars.'
    $validConfig = [IO.File]::ReadAllText($config)
    [IO.File]::WriteAllText($config, '{broken')
    Wait-For { [WtsWindowProbe]::Text($script:handle) -match '(?i)error' -and (Has-Counts 2 0) } 'invalid config with last-good rows retained'
    [IO.File]::WriteAllText((Join-Path $fixture 'invalid-config.txt'), [WtsWindowProbe]::Text($script:handle))
    [IO.File]::WriteAllText($config, $validConfig)
    Wait-For { [WtsWindowProbe]::Text($script:handle) -notmatch '(?i)error' -and (Has-Counts 2 0) } 'valid config recovery'
    & $Binary remove $repo
    if ($LASTEXITCODE -ne 0) { throw 'Removal failed' }
    Wait-For { [WtsWindowProbe]::Text($script:handle) -match '(?i)no (registered )?repo' } 'live registry reload'
    [IO.File]::WriteAllText((Join-Path $fixture 'empty.txt'), [WtsWindowProbe]::Text($script:handle))
    if ($process.HasExited -or -not [WtsWindowProbe]::IsWindow($script:handle)) { throw 'Window exited before close' }
    if (-not [WtsWindowProbe]::PostMessage($script:handle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) { throw 'Close message failed' }
    Wait-For { -not [WtsWindowProbe]::IsWindow($script:handle) } 'window destruction' 10
    if (-not $process.WaitForExit(10000)) { throw 'Window did not close within 10 seconds' }
    if ($process.ExitCode -ne 0) { throw "Window exited with $($process.ExitCode)" }
    [ordered]@{
        binary = $Binary
        sha256 = (Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash
        ui_binary = $uiBinary
        ui_sha256 = $uiHash
        initial_read_percent = $initialReadPercent
        read_percentage = $true
        frameless = $true
        gutter_scrollbar = $true
        idle_scrollbar_hidden = $true
        empty_space_drag = $true
        corner_resize_drag = $true
        bottom_padding = 0
        topmost = ($initialStyle -band 8) -ne 0
        activated_on_show = $foregroundOnShow -eq $script:handle
        activated_on_refresh = $foregroundOnRefresh -eq $script:handle
        console_detached = $true
        launcher_exited = $launcher.HasExited
        launcher_exit_code = $launcher.ExitCode
        stdout_eof = $stdout.IsCompletedSuccessfully
        stderr_eof = $stderr.IsCompletedSuccessfully
        launcher_pid = $launcher.Id
        window_pid = $process.Id
        registry_empty = [IO.File]::ReadAllText((Join-Path $fixture 'empty.txt')) -match '(?i)no (registered )?repo'
        window_destroyed = -not [WtsWindowProbe]::IsWindow($script:handle)
        scroll_before = $beforeScroll
        scroll_after = $afterScroll
        exit_code = $process.ExitCode
        result = 'PASS'
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $fixture 'proof.json') -Encoding utf8NoBOM
    Write-Host "PASS. Terminal detached, always on top, live counts, scroll preservation, read percentage, config error/recovery, registry reload, and clean close. Evidence at $fixture"
} finally {
    $env:HOME = $oldHome
    $env:USERPROFILE = $oldProfile
    if ($null -ne $process -and -not $process.HasExited) { $process.Kill($true); $process.WaitForExit() }
    if ($null -ne $launcher) {
        foreach ($child in @(Get-CimInstance Win32_Process -Filter "ParentProcessId = $($launcher.Id)")) {
            try {
                $owned = [Diagnostics.Process]::GetProcessById([int]$child.ProcessId)
                if (-not $owned.HasExited) { $owned.Kill($true); $owned.WaitForExit() }
            } catch [ArgumentException] {} catch [InvalidOperationException] {}
        }
        if (-not $launcher.HasExited) { $launcher.Kill($true); $launcher.WaitForExit() }
    }
    if ($previousDpiContext -ne [IntPtr]::Zero) { [void][WtsWindowProbe]::SetThreadDpiAwarenessContext($previousDpiContext) }
}
