param([string]$Binary = "$PSScriptRoot/../target/debug/wtm-ui.exe")

$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path $Binary).Path
$source = Get-Content -LiteralPath "$PSScriptRoot/smoke-window.ps1" -Raw
$start = $source.IndexOf('using System;')
$end = $source.IndexOf("`n'@", $start)
$source = $source.Substring($start, $end - $start).Replace('WtmWindowProbe', 'WtmTrayDragProbe')
$source = $source.Replace('private static void Drag(', 'public static void Drag(')
$extra = @'
    public struct Message { public IntPtr Window; public uint Id; public UIntPtr WParam; public IntPtr LParam; public uint Time; public Point Position; public uint Private; }
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr CreateWindowExW(uint extended, string name, string title, uint style, int x, int y, int width, int height, IntPtr parent, IntPtr menu, IntPtr instance, IntPtr data);
    [DllImport("user32.dll")] public static extern bool PeekMessageW(out Message message, IntPtr window, uint first, uint last, uint remove);
    [DllImport("user32.dll")] public static extern IntPtr DispatchMessageW(ref Message message);
    public static void CheckOutsideDismissal(IntPtr window, int x, int y) {
        IntPtr backdrop = IntPtr.Zero;
        Exception failure = null;
        var ready = new System.Threading.ManualResetEvent(false);
        var worker = new System.Threading.Thread(() => {
            SetThreadDpiAwarenessContext(new IntPtr(-4));
            try {
                backdrop = CreateWindowExW(0x88, "STATIC", "WTM tray probe", 0x10cf0000, x, y, 160, 100, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero);
                if (backdrop == IntPtr.Zero) throw new InvalidOperationException("Cannot create owned backdrop");
                ready.Set();
                while (IsWindow(backdrop)) {
                    Message message;
                    while (PeekMessageW(out message, IntPtr.Zero, 0, 0, 1)) DispatchMessageW(ref message);
                    System.Threading.Thread.Sleep(10);
                }
            } catch (Exception error) { failure = error; ready.Set(); }
        });
        worker.IsBackground = true;
        worker.Start();
        try {
            if (!ready.WaitOne(3000)) throw new InvalidOperationException("Owned backdrop did not start");
            if (failure != null) throw failure;
            var rect = Bounds(window);
            PostMessage(window, 32811, Position(rect.Left + 40, rect.Top + 40), new IntPtr((1 << 16) | 123));
            WaitFor(() => HasMenu(window), "Hidden fixture tray menu did not open");
            var target = new Point { X = x + 50, Y = y + 50 };
            if (!SetCursorPos(target.X, target.Y) || WindowFromPoint(target) != backdrop) throw new InvalidOperationException("Outside click is not the owned backdrop");
            mouse_event(2, 0, 0, 0, UIntPtr.Zero);
            mouse_event(4, 0, 0, 0, UIntPtr.Zero);
            WaitFor(() => !HasMenu(window), "Hidden fixture tray menu stayed open after a real outside click");
        } finally {
            if (backdrop != IntPtr.Zero) PostMessage(backdrop, 0x10, IntPtr.Zero, IntPtr.Zero);
            if (!worker.Join(3000)) throw new InvalidOperationException("Owned backdrop did not close");
            ready.Dispose();
        }
    }
    public struct IconId { public uint Size; public IntPtr Window; public uint Id; public Guid Guid; }
    [DllImport("shell32.dll")] public static extern int Shell_NotifyIconGetRect(ref IconId id, out Rect rect);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern uint RegisterWindowMessage(string name);
    [DllImport("user32.dll")] public static extern bool GetMenuItemRect(IntPtr window, IntPtr menu, uint item, out Rect rect);
    [DllImport("user32.dll", EntryPoint="SystemParametersInfoW")] public static extern bool SystemParametersInfo(uint action, uint parameter, out Rect rect, uint flags);
    public static Rect Bounds(IntPtr handle) {
        Rect rect;
        if (handle == IntPtr.Zero || !GetWindowRect(handle, out rect)) throw new InvalidOperationException("Fixture bounds unavailable");
        return rect;
    }
    public static string Caption(IntPtr handle) {
        var text = new StringBuilder(1024);
        GetWindowText(handle, text, text.Capacity);
        return text.ToString();
    }
    public static bool HasIcon(IntPtr window, uint id) {
        var identity = new IconId { Size = (uint)Marshal.SizeOf(typeof(IconId)), Window = window, Id = id };
        Rect rect;
        return Shell_NotifyIconGetRect(ref identity, out rect) == 0 && rect.Right > rect.Left && rect.Bottom > rect.Top;
    }
    public static bool HasMenu(IntPtr window) {
        uint owner;
        uint thread = GetWindowThreadProcessId(window, out owner);
        var info = new GuiInfo { Size = (uint)Marshal.SizeOf(typeof(GuiInfo)) };
        return GetGUIThreadInfo(thread, ref info) && info.MenuOwner == window && (info.Flags & 4) != 0;
    }
    public static void Menu(IntPtr window, bool exit) {
        var rect = Bounds(window);
        if (!PostMessage(window, 32811, Position(rect.Left + 40, rect.Top + 40), new IntPtr((1 << 16) | 123))) throw new InvalidOperationException("Cannot open fixture tray menu");
        WaitFor(() => HasMenu(window), "Fixture tray menu did not open");
        IntPtr popup = IntPtr.Zero;
        uint pid;
        GetWindowThreadProcessId(window, out pid);
        WaitFor(() => {
            EnumWindows((handle, data) => {
                uint owner;
                GetWindowThreadProcessId(handle, out owner);
                var name = new StringBuilder(128);
                GetClassName(handle, name, name.Capacity);
                if (owner == pid && IsWindowVisible(handle) && name.ToString() == "#32768") { popup = handle; return false; }
                return true;
            }, IntPtr.Zero);
            return popup != IntPtr.Zero;
        }, "Fixture popup window unavailable");
        var menu = SendMessage(popup, 0x01e1, IntPtr.Zero, IntPtr.Zero);
        Rect item;
        if (!GetMenuItemRect(IntPtr.Zero, menu, exit ? 1u : 0u, out item)) throw new InvalidOperationException("Fixture menu item bounds unavailable");
        int x = (item.Left + item.Right) / 2, y = (item.Top + item.Bottom) / 2;
        if (!SetCursorPos(x, y) || WindowFromPoint(new Point { X = x, Y = y }) != popup) throw new InvalidOperationException("Menu click target is not the fixture popup");
        mouse_event(2, 0, 0, 0, UIntPtr.Zero);
        mouse_event(4, 0, 0, 0, UIntPtr.Zero);
    }

    public static void DragAndCheck(IntPtr window, int x, int y, bool resize) {
        var before = Bounds(window);
        Exception failure = null;
        var worker = new System.Threading.Thread(() => {
            var dpi = SetThreadDpiAwarenessContext(new IntPtr(-4));
            try { Drag(window, x, y, 40, 30); }
            catch (Exception error) { failure = error; }
            finally { if (dpi != IntPtr.Zero) SetThreadDpiAwarenessContext(dpi); }
        });
        worker.IsBackground = true;
        worker.Start();
        if (!worker.Join(6000)) {
            mouse_event(4, 0, 0, 0, UIntPtr.Zero);
            throw new InvalidOperationException("Real mouse drag exceeded six seconds");
        }
        if (failure != null) throw failure;
        var after = Bounds(window);
        if (resize ? after.Right - after.Left <= before.Right - before.Left || after.Bottom - after.Top <= before.Bottom - before.Top
            : Math.Abs(after.Left - before.Left - 40) > 2 || Math.Abs(after.Top - before.Top - 30) > 2) throw new InvalidOperationException("Real drag did not produce the expected window bounds. Before=" + before.Left + "," + before.Top + "," + before.Right + "," + before.Bottom + " After=" + after.Left + "," + after.Top + "," + after.Right + "," + after.Bottom);
    }
'@
$source = $source.Insert($source.LastIndexOf('}'), $extra)
Add-Type -TypeDefinition $source

function Wait-For([scriptblock]$Condition, [string]$Label) {
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        if (& $Condition) { return }
        if ($null -ne $process -and $process.HasExited) { throw "Fixture exited waiting for $Label" }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for $Label"
}

$fixture = Join-Path $env:LOCALAPPDATA "Temp/opencode/wtm-tray-drag-$([Guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $fixture | Out-Null
$process = $null
$window = [IntPtr]::Zero
$savedCursor = [WtmTrayDragProbe+Point]::new()
if (-not [WtmTrayDragProbe]::GetCursorPos([ref]$savedCursor)) { throw 'Cannot save cursor position' }
$previousDpi = [WtmTrayDragProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
try {
    $start = [Diagnostics.ProcessStartInfo]::new($Binary)
    $start.WorkingDirectory = $fixture
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.Environment['HOME'] = $fixture
    $start.Environment['USERPROFILE'] = $fixture
    $process = [Diagnostics.Process]::Start($start)
    Wait-For { $script:window = [WtmTrayDragProbe]::Find([uint32]$process.Id); $window -ne [IntPtr]::Zero } 'visible fixture'
    $style = [WtmTrayDragProbe]::GetWindowLongPtr($window, -20).ToInt64()
    if (($style -band 0x80) -eq 0 -or ($style -band 0x40000) -ne 0) { throw 'Window is not excluded from the taskbar' }
    Wait-For { [WtmTrayDragProbe]::HasIcon($window, 1) } 'real Shell notification icon'
    foreach ($id in @(0, 2, 3)) {
        if ([WtmTrayDragProbe]::HasIcon($window, $id)) { throw "Unexpected extra notification icon $id" }
    }
    $taskbarCreated = [WtmTrayDragProbe]::RegisterWindowMessage('TaskbarCreated')
    if ($taskbarCreated -eq 0) { throw 'TaskbarCreated message unavailable' }
    1..3 | ForEach-Object { [void][WtmTrayDragProbe]::SendMessage($window, $taskbarCreated, [IntPtr]::Zero, [IntPtr]::Zero) }
    if (-not [WtmTrayDragProbe]::HasIcon($window, 1)) { throw 'Notification icon disappeared after repeated registration' }
    Write-Host 'PASS. Tool window has a real notification icon and no taskbar app style. Repeated registration preserves its identity.'
    $work = [WtmTrayDragProbe+Rect]::new()
    if (-not [WtmTrayDragProbe]::SystemParametersInfo(0x30, 0, [ref]$work, 0)) { throw 'Work area unavailable' }
    if (-not [WtmTrayDragProbe]::SetWindowPos($window, [IntPtr](-1), $work.Left + 80, $work.Top + 80, 640, 300, 0x10)) { throw 'Cannot position fixture' }
    $status = [WtmTrayDragProbe]::GetDlgItem($window, 1002)
    Wait-For { [WtmTrayDragProbe]::Caption($status) -match '\[read: \d+%\]' } 'read indicator'
    $bounds = [WtmTrayDragProbe]::Bounds($status)
    [WtmTrayDragProbe]::DragAndCheck($window, $bounds.Left + 25, [int](($bounds.Top + $bounds.Bottom) / 2), $false)
    $bounds = [WtmTrayDragProbe]::Bounds($status)
    [WtmTrayDragProbe]::DragAndCheck($window, $bounds.Right - 30, [int](($bounds.Top + $bounds.Bottom) / 2), $false)
    $bounds = [WtmTrayDragProbe]::Bounds($window)
    if ([WtmTrayDragProbe]::SendMessage($window, 0x84, [IntPtr]::Zero, [WtmTrayDragProbe]::Position($bounds.Right - 2, $bounds.Bottom - 2)).ToInt64() -ne 17) { throw 'Resize corner no longer has its resize hit test' }
    [WtmTrayDragProbe]::DragAndCheck($window, $bounds.Right - 2, $bounds.Bottom - 2, $true)
    $toggle = [WtmTrayDragProbe]::GetDlgItem($window, 1004)
    $bounds = [WtmTrayDragProbe]::Bounds($status)
    [void][WtmTrayDragProbe]::SetCursorPos($bounds.Left + 25, [int](($bounds.Top + $bounds.Bottom) / 2))
    Wait-For { [WtmTrayDragProbe]::IsWindowVisible($toggle) } 'Show all on hover'
    $bounds = [WtmTrayDragProbe]::Bounds($toggle)
    $point = [WtmTrayDragProbe]::Position([int](($bounds.Left + $bounds.Right) / 2), [int](($bounds.Top + $bounds.Bottom) / 2))
    if ([WtmTrayDragProbe]::SendMessage($window, 0x84, [IntPtr]::Zero, $point).ToInt64() -ne 1) { throw 'Show all is treated as a window drag region' }
    Write-Host 'PASS. Real mouse drags on the read text and empty status row move the window. Real corner drag still resizes it.'
    [WtmTrayDragProbe]::Menu($window, $false)
    Wait-For { -not [WtmTrayDragProbe]::IsWindowVisible($window) } 'tray Hide action'
    if (-not [WtmTrayDragProbe]::HasIcon($window, 1)) { throw 'Hiding window removed its notification icon' }
    [WtmTrayDragProbe]::CheckOutsideDismissal($window, $work.Left + 10, $work.Top + 10)
    Write-Host 'PASS. A real outside click on an owned backdrop dismisses the tray menu while the app is hidden.'
    [void][WtmTrayDragProbe]::PostMessage($window, 32811, [IntPtr]::Zero, [IntPtr]((1 -shl 16) -bor 1024))
    Wait-For { [WtmTrayDragProbe]::IsWindowVisible($window) } 'tray selection to restore window'
    [WtmTrayDragProbe]::Menu($window, $false)
    Wait-For { -not [WtmTrayDragProbe]::IsWindowVisible($window) } 'second tray Hide action'
    [WtmTrayDragProbe]::Menu($window, $false)
    Wait-For { [WtmTrayDragProbe]::IsWindowVisible($window) } 'tray Show action'
    [WtmTrayDragProbe]::Menu($window, $true)
    if (-not $process.WaitForExit(5000) -or $process.ExitCode -ne 0) { throw 'Tray Exit did not close cleanly' }
    if ([WtmTrayDragProbe]::HasIcon($window, 1)) { throw 'Notification icon remains after Exit' }
    Write-Host 'PASS. Tray callback opens a real mouse-operated menu. Hide, Show, selection restore, and Exit work. Exit removes the icon.'
} finally {
    if ($null -ne $process -and -not $process.HasExited) {
        if ($window -ne [IntPtr]::Zero) { [void][WtmTrayDragProbe]::PostMessage($window, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) }
        if (-not $process.WaitForExit(5000)) { $process.Kill(); $process.WaitForExit() }
    }
    [void][WtmTrayDragProbe]::SetCursorPos($savedCursor.X, $savedCursor.Y)
    if ($previousDpi -ne [IntPtr]::Zero) { [void][WtmTrayDragProbe]::SetThreadDpiAwarenessContext($previousDpi) }
    if ($null -ne $process) { $process.Dispose() }
}
