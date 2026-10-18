param([string]$Binary = "$PSScriptRoot/../target/debug/wtm-gui.exe", [switch]$ResetOnly)

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
    [DllImport("user32.dll")] public static extern int GetMenuItemCount(IntPtr menu);
    [DllImport("user32.dll")] public static extern IntPtr GetSubMenu(IntPtr menu, int index);
    [DllImport("user32.dll")] public static extern uint GetMenuState(IntPtr menu, uint item, uint flags);
    [DllImport("user32.dll")] public static extern bool GetLayeredWindowAttributes(IntPtr window, out uint color, out byte alpha, out uint flags);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetMenuStringW(IntPtr menu, uint item, StringBuilder text, int count, uint flags);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr window, int command);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsZoomed(IntPtr window);
    public static string ReadPlacement(string path) {
        using (var file = new System.IO.FileStream(path, System.IO.FileMode.Open, System.IO.FileAccess.Read, System.IO.FileShare.ReadWrite | System.IO.FileShare.Delete))
        using (var reader = new System.IO.StreamReader(file)) return reader.ReadToEnd();
    }
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
    public static byte Alpha(IntPtr window) {
        uint color, flags;
        byte alpha;
        if (!GetLayeredWindowAttributes(window, out color, out alpha, out flags)) throw new InvalidOperationException("Fixture opacity unavailable");
        return alpha;
    }
    public static IntPtr FindPopup(uint pid, IntPtr menu) {
        IntPtr popup = IntPtr.Zero;
        EnumWindows((handle, data) => {
            uint owner;
            GetWindowThreadProcessId(handle, out owner);
            var name = new StringBuilder(128);
            GetClassName(handle, name, name.Capacity);
            if (owner == pid && IsWindowVisible(handle) && name.ToString() == "#32768" &&
                (menu == IntPtr.Zero || SendMessage(handle, 0x01e1, IntPtr.Zero, IntPtr.Zero) == menu)) { popup = handle; return false; }
            return true;
        }, IntPtr.Zero);
        return popup;
    }
    public static void ClickMenuItem(IntPtr popup, IntPtr menu, uint index) {
        Rect item;
        if (!GetMenuItemRect(IntPtr.Zero, menu, index, out item)) throw new InvalidOperationException("Fixture menu item bounds unavailable");
        int x = (item.Left + item.Right) / 2, y = (item.Top + item.Bottom) / 2;
        if (!SetCursorPos(x, y)) throw new InvalidOperationException("Cannot move to fixture menu item");
        WaitFor(() => WindowFromPoint(new Point { X = x, Y = y }) == popup, "Menu click target is not the fixture popup");
        mouse_event(2, 0, 0, 0, UIntPtr.Zero);
        mouse_event(4, 0, 0, 0, UIntPtr.Zero);
    }
    public static void Menu(IntPtr window, string caption, int opacity = 0, int expectedOpacity = 0) {
        Menu(window, caption, opacity, expectedOpacity, IntPtr.Zero);
    }
    public static void Menu(IntPtr window, string caption, int opacity, int expectedOpacity, IntPtr target) {
        var rect = Bounds(target == IntPtr.Zero ? window : target);
        if (target != IntPtr.Zero) {
            var point = new Point { X = (rect.Left + rect.Right) / 2, Y = (rect.Top + rect.Bottom) / 2 };
            if (!SetCursorPos(point.X, point.Y) || GetAncestor(WindowFromPoint(point), 2) != window) throw new InvalidOperationException("Right-click target is not the fixture window");
            mouse_event(8, 0, 0, 0, UIntPtr.Zero);
            mouse_event(16, 0, 0, 0, UIntPtr.Zero);
        } else if (!PostMessage(window, 32811, Position(rect.Left + 40, rect.Top + 40), new IntPtr((1 << 16) | 123))) throw new InvalidOperationException("Cannot open fixture tray menu");
        WaitFor(() => HasMenu(window), "Fixture context menu did not open");
        IntPtr popup = IntPtr.Zero;
        uint pid;
        GetWindowThreadProcessId(window, out pid);
        WaitFor(() => {
            popup = FindPopup(pid, IntPtr.Zero);
            return popup != IntPtr.Zero;
        }, "Fixture popup window unavailable");
        var menu = SendMessage(popup, 0x01e1, IntPtr.Zero, IntPtr.Zero);
        int index = -1;
        for (int i = 0; i < GetMenuItemCount(menu); i++) {
            var text = new StringBuilder(128);
            GetMenuStringW(menu, (uint)i, text, text.Capacity, 0x400);
            if (text.ToString() == caption) { index = i; break; }
        }
        if (index < 0) {
            PostMessage(window, 0x001f, IntPtr.Zero, IntPtr.Zero);
            throw new InvalidOperationException("Fixture menu item unavailable: " + caption);
        }
        if (opacity != 0) {
            var submenu = GetSubMenu(menu, index);
            if (submenu == IntPtr.Zero || GetMenuItemCount(submenu) != 20) throw new InvalidOperationException("Opacity submenu must have twenty 5% steps");
            for (uint i = 0; i < 20; i++) {
                var text = new StringBuilder(128);
                GetMenuStringW(submenu, i, text, text.Capacity, 0x400);
                if (text.ToString() != ((i + 1) * 5) + "%") throw new InvalidOperationException("Opacity submenu step is incorrect");
                if (((GetMenuState(submenu, i, 0x400) & 8) != 0) != ((i + 1) * 5 == expectedOpacity)) throw new InvalidOperationException("Current opacity is not the sole checked item");
            }
            ClickMenuItem(popup, menu, (uint)index);
            IntPtr child = IntPtr.Zero;
            WaitFor(() => { child = FindPopup(pid, submenu); return child != IntPtr.Zero; }, "Opacity submenu did not open");
            ClickMenuItem(child, submenu, (uint)(opacity / 5 - 1));
        } else {
            ClickMenuItem(popup, menu, (uint)index);
        }
        WaitFor(() => !IsWindow(popup), "Fixture popup did not close after selection");
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
    if (-not $ResetOnly) {
        $status = [WtmTrayDragProbe]::GetDlgItem($window, 1002)
        Wait-For { [WtmTrayDragProbe]::Caption($status) -match '\[idle: \d+%\]' } 'read indicator'
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
    }
    foreach ($target in @($window, [WtmTrayDragProbe]::GetDlgItem($window, 1002))) {
        [WtmTrayDragProbe]::Menu($window, 'Reset position', 0, 0, $target)
        if (-not [WtmTrayDragProbe]::SetWindowPos($window, [IntPtr](-1), $work.Left + 80, $work.Top + 80, 640, 300, 0x10)) { throw 'Cannot restore fixture position' }
    }
    Write-Host 'PASS. Real right-clicks on the window and status open the menu with Reset position.'
    [WtmTrayDragProbe]::Menu($window, 'Hide')
    Wait-For { -not [WtmTrayDragProbe]::IsWindowVisible($window) } 'tray Hide action'
    if (-not [WtmTrayDragProbe]::HasIcon($window, 1)) { throw 'Hiding window removed its notification icon' }
    [WtmTrayDragProbe]::CheckOutsideDismissal($window, $work.Left + 10, $work.Top + 10)
    Write-Host 'PASS. A real outside click on an owned backdrop dismisses the tray menu while the app is hidden.'
    [void][WtmTrayDragProbe]::PostMessage($window, 32811, [IntPtr]::Zero, [IntPtr]((1 -shl 16) -bor 1024))
    Wait-For { [WtmTrayDragProbe]::IsWindowVisible($window) } 'tray selection to restore window'
    [WtmTrayDragProbe]::Menu($window, 'Hide')
    Wait-For { -not [WtmTrayDragProbe]::IsWindowVisible($window) } 'second tray Hide action'
    [WtmTrayDragProbe]::Menu($window, 'Show')
    Wait-For { [WtmTrayDragProbe]::IsWindowVisible($window) } 'tray Show action'
    $selectedOpacity = 20
    foreach ($percent in @(5, 35, 100, 40)) {
        [WtmTrayDragProbe]::Menu($window, 'Opacity', $percent, $selectedOpacity)
        $selectedOpacity = $percent
        [void][WtmTrayDragProbe]::SetCursorPos($work.Right - 10, $work.Top + 10)
        Wait-For { [WtmTrayDragProbe]::Alpha($window) -eq [Math]::Floor(($percent * 255 + 50) / 100) } "idle opacity $percent%"
        $bounds = [WtmTrayDragProbe]::Bounds($window)
        [void][WtmTrayDragProbe]::SetCursorPos($bounds.Left + 30, $bounds.Top + 30)
        Wait-For { [WtmTrayDragProbe]::Alpha($window) -eq 255 } 'full opacity on hover'
        Write-Host "PASS. Opacity submenu highlights the current value, selects $percent%, and keeps full opacity on hover."
    }
    $defaultBounds = $null
    foreach ($mode in @('normal', 'hidden', 'minimized', 'maximized')) {
        if (-not [WtmTrayDragProbe]::SetWindowPos($window, [IntPtr](-1), $work.Left + 80, $work.Top + 80, 640, 300, 0x10)) { throw 'Cannot move fixture before Reset' }
        switch ($mode) {
            'hidden' { [void][WtmTrayDragProbe]::ShowWindow($window, 0) }
            'minimized' { [void][WtmTrayDragProbe]::ShowWindow($window, 6) }
            'maximized' { [void][WtmTrayDragProbe]::ShowWindow($window, 3) }
        }
        [WtmTrayDragProbe]::Menu($window, 'Reset position')
        Wait-For {
            $rect = [WtmTrayDragProbe]::Bounds($window)
            $dpi = [WtmTrayDragProbe]::GetDpiForWindow($window)
            [WtmTrayDragProbe]::IsWindowVisible($window) -and -not [WtmTrayDragProbe]::IsIconic($window) -and -not [WtmTrayDragProbe]::IsZoomed($window) -and
                $rect.Left -eq $work.Left -and $rect.Top -eq $work.Top -and
                ($rect.Right - $rect.Left) -eq [Math]::Min(640 * $dpi / 96, $work.Right - $work.Left) -and
                ($rect.Bottom - $rect.Top) -eq [Math]::Min(540 * $dpi / 96, $work.Bottom - $work.Top)
        } "Reset from $mode"
        $defaultBounds = [WtmTrayDragProbe]::Bounds($window)
        Wait-For {
            $saved = [WtmTrayDragProbe]::ReadPlacement((Join-Path $fixture '.config/wtm/config.placement.json')) | ConvertFrom-Json
            $saved.state -eq 'normal' -and $saved.normal.left -eq $defaultBounds.Left -and $saved.normal.top -eq $defaultBounds.Top -and
                $saved.normal.right -eq $defaultBounds.Right -and $saved.normal.bottom -eq $defaultBounds.Bottom
        } 'saved reset placement'
        Write-Host "PASS. Tray Reset from $mode shows the window at default bounds and saves them."
    }
    [WtmTrayDragProbe]::Menu($window, 'Exit')
    if (-not $process.WaitForExit(5000) -or $process.ExitCode -ne 0) { throw 'Tray Exit did not close cleanly' }
    if ([WtmTrayDragProbe]::HasIcon($window, 1)) { throw 'Notification icon remains after Exit' }
    Write-Host 'PASS. Tray callback opens a real mouse-operated menu. Hide, Show, selection restore, and Exit work. Exit removes the icon.'
    $process.Dispose()
    $process = [Diagnostics.Process]::Start($start)
    Wait-For { $script:window = [WtmTrayDragProbe]::Find([uint32]$process.Id); $window -ne [IntPtr]::Zero } 'restarted fixture'
    $restored = [WtmTrayDragProbe]::Bounds($window)
    if ($restored.Left -ne $defaultBounds.Left -or $restored.Top -ne $defaultBounds.Top -or $restored.Right -ne $defaultBounds.Right -or $restored.Bottom -ne $defaultBounds.Bottom) { throw 'Restart did not retain reset bounds' }
    Write-Host 'PASS. Reset bounds survive app restart.'
    [void][WtmTrayDragProbe]::SetCursorPos($work.Right - 10, $work.Top + 10)
    Wait-For { [WtmTrayDragProbe]::Alpha($window) -eq 102 } 'saved 40% opacity after restart'
    [WtmTrayDragProbe]::Menu($window, 'Opacity', 40, 40)
    Write-Host 'PASS. Selected opacity survives Reset and app restart and remains highlighted.'
} finally {
    if ($null -ne $process -and -not $process.HasExited) {
        if ($window -ne [IntPtr]::Zero) { [void][WtmTrayDragProbe]::PostMessage($window, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) }
        if (-not $process.WaitForExit(5000)) { $process.Kill(); $process.WaitForExit() }
    }
    [void][WtmTrayDragProbe]::SetCursorPos($savedCursor.X, $savedCursor.Y)
    if ($previousDpi -ne [IntPtr]::Zero) { [void][WtmTrayDragProbe]::SetThreadDpiAwarenessContext($previousDpi) }
    if ($null -ne $process) { $process.Dispose() }
}
