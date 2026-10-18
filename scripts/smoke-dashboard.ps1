param([string]$Binary = "$PSScriptRoot/../target/debug/wtm-gui.exe")

$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path $Binary).Path
$CliBinary = (Resolve-Path (Join-Path (Split-Path -Parent $Binary) 'wtm.exe')).Path
Add-Type -TypeDefinition (@'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class WtmDragProbe {
    public struct Point { public int X, Y; }
    public struct Rect { public int Left, Top, Right, Bottom; }
    public struct GuiInfo {
        public uint Size, Flags;
        public IntPtr Active, Focus, Capture, MenuOwner, MoveSize, Caret;
        public Rect CaretRect;
    }
    public sealed class Card {
        public IntPtr Handle;
        public string Text;
        public string Class;
        public int Top;
    }
    public delegate bool EnumProc(IntPtr handle, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr handle, out uint processId);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr handle, StringBuilder text, int count);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr handle, StringBuilder text, int count);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr handle);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr handle);
    [DllImport("user32.dll")] public static extern IntPtr GetParent(IntPtr handle);
    [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr handle, uint flags);
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr handle, int id);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr handle);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr handle, out Rect rect);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr handle, out Rect rect);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr handle, ref Point point);
    [DllImport("user32.dll")] public static extern bool ScreenToClient(IntPtr handle, ref Point point);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out Point point);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint x, uint y, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern IntPtr ChildWindowFromPointEx(IntPtr parent, Point point, uint flags);
    [DllImport("user32.dll")] public static extern bool GetGUIThreadInfo(uint thread, ref GuiInfo info);
    [DllImport("user32.dll", EntryPoint = "SendMessageTimeoutW")] public static extern IntPtr SendMessageTimeout(IntPtr handle, uint message, IntPtr wparam, IntPtr lparam, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr handle, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr handle, IntPtr after, int x, int y, int width, int height, uint flags);
    public static string Text(IntPtr handle) {
        var text = new StringBuilder(65536);
        GetWindowText(handle, text, text.Capacity);
        return text.ToString();
    }
    public static string AllText(IntPtr parent) {
        var text = new StringBuilder();
        EnumChildWindows(parent, (handle, data) => {
            if (IsWindowVisible(handle)) { text.AppendLine(Text(handle)); }
            return true;
        }, IntPtr.Zero);
        return text.ToString();
    }
    public static IntPtr Find(uint pid) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((handle, data) => {
            uint owner;
            GetWindowThreadProcessId(handle, out owner);
            if (owner == pid && IsWindowVisible(handle) && Text(handle).StartsWith("WTM")) {
                found = handle;
                return false;
            }
            return true;
        }, IntPtr.Zero);
        return found;
    }
    public static Card[] Cards(IntPtr parent) {
        var cards = new List<Card>();
        EnumChildWindows(parent, (handle, data) => {
            var name = new StringBuilder(128);
            GetClassName(handle, name, name.Capacity);
            if (GetParent(handle) == parent && IsWindowVisible(handle) && string.Equals(name.ToString(), "STATIC", StringComparison.OrdinalIgnoreCase) && Text(handle).Length != 0) {
                Rect rect;
                if (!GetWindowRect(handle, out rect)) { throw new InvalidOperationException("Card bounds unavailable"); }
                cards.Add(new Card { Handle = handle, Text = Text(handle), Class = name.ToString(), Top = rect.Top });
            }
            return true;
        }, IntPtr.Zero);
        cards.Sort((left, right) => left.Top.CompareTo(right.Top));
        return cards.ToArray();
    }
    public static IntPtr Captured(IntPtr window) {
        return ThreadInfo(window).Capture;
    }
    public static GuiInfo ThreadInfo(IntPtr window) {
        uint owner;
        uint thread = GetWindowThreadProcessId(window, out owner);
        var info = new GuiInfo { Size = (uint)Marshal.SizeOf(typeof(GuiInfo)) };
        if (!GetGUIThreadInfo(thread, ref info)) { throw new InvalidOperationException("Capture state unavailable"); }
        return info;
    }
    public static IntPtr Send(IntPtr handle, uint message, IntPtr wparam, IntPtr lparam) {
        IntPtr result;
        if (SendMessageTimeout(handle, message, wparam, lparam, 2, 3000, out result) == IntPtr.Zero) {
            throw new InvalidOperationException("Native message timed out or failed");
        }
        return result;
    }
    public static IntPtr Position(Point point) {
        return new IntPtr(unchecked((int)(((uint)(ushort)point.Y << 16) | (ushort)point.X)));
    }
    public static void Drag(IntPtr source, IntPtr target, bool cancel) {
        DragAt(source, target, cancel, false, 0x0100, "heading");
    }
    public static void DragBelow(IntPtr source, IntPtr target) {
        DragAt(source, target, false, true, 0, "right");
    }
    public static void CancelDrag(IntPtr source, IntPtr target, uint message) {
        DragAt(source, target, true, false, message, "heading");
    }
    public static void DragFrom(IntPtr source, IntPtr target, string region) {
        DragAt(source, target, false, false, 0, region);
    }
    private static Point StartPoint(IntPtr source, string region) {
        Rect rect;
        if (!GetClientRect(source, out rect)) { throw new InvalidOperationException("Card bounds unavailable"); }
        int scale = (int)GetDpiForWindow(source);
        int cell = WtmNative.Measure(source, "Mg").Height;
        int advance = (int)Math.Round(12 * 1.2 * scale / 96, MidpointRounding.AwayFromZero);
        bool worktree = Text(source).Contains("\nS ");
        return new Point { X = region == "right" ? rect.Right - 20 * scale / 96 : Math.Min(rect.Right / 2, 50 * scale / 96),
            Y = Math.Min(rect.Bottom - 1, worktree && (region == "counts" || region == "right") ? advance + cell / 2 : cell / 2) };
    }
    private static IntPtr Begin(IntPtr source, Point start) {
        var screen = start;
        if (!ClientToScreen(source, ref screen)) { throw new InvalidOperationException("Card coordinates unavailable"); }
        if (Send(source, 0x0084, IntPtr.Zero, Position(screen)).ToInt64() != 1) { throw new InvalidOperationException("Card body rejects mouse hit testing"); }
        var parentPoint = screen;
        var parent = GetParent(source);
        if (!ScreenToClient(parent, ref parentPoint) || ChildWindowFromPointEx(parent, parentPoint, 7) != source ||
            ChildWindowFromPointEx(source, start, 7) != source) { throw new InvalidOperationException("Card body mouse target is not the card"); }
        Send(source, 0x0201, new IntPtr(1), Position(start));
        if (Captured(source) != parent) { throw new InvalidOperationException("Card body did not capture to viewport"); }
        return parent;
    }
    public static void ClickBody(IntPtr source, bool move) {
        var point = StartPoint(source, "heading");
        var capture = Begin(source, point);
        if (!ClientToScreen(source, ref point) || !ScreenToClient(capture, ref point)) { throw new InvalidOperationException("Click coordinates unavailable"); }
        if (move) {
            point.X++;
            point.Y++;
            Send(capture, 0x0200, new IntPtr(1), Position(point));
        }
        Send(capture, 0x0202, IntPtr.Zero, Position(point));
        if (Captured(source) != IntPtr.Zero) { throw new InvalidOperationException("Click left capture active"); }
    }
    public static void ClickAction(IntPtr source) {
        WtmNative.Hover(GetAncestor(source, 2));
        IntPtr button = IntPtr.Zero;
        EnumChildWindows(source, (handle, data) => {
            if (GetParent(handle) == source && (Text(handle) == "Hide" || Text(handle) == "Unhide")) { button = handle; return false; }
            return true;
        }, IntPtr.Zero);
        Rect rect;
        if (button == IntPtr.Zero || !GetClientRect(button, out rect)) { throw new InvalidOperationException("Native action button unavailable"); }
        var point = new Point { X = rect.Right / 2, Y = rect.Bottom / 2 };
        var bodyPoint = point;
        if (!ClientToScreen(button, ref bodyPoint) || !ScreenToClient(source, ref bodyPoint) ||
            ChildWindowFromPointEx(source, bodyPoint, 7) != button) { throw new InvalidOperationException("Card intercepts native action button"); }
        var root = GetAncestor(source, 2);
        var screen = point;
        Point saved;
        if (!ClientToScreen(button, ref screen) || !GetCursorPos(out saved)) { throw new InvalidOperationException("Native button screen coordinates unavailable"); }
        bool pressed = false;
        try {
            if (!SetCursorPos(screen.X, screen.Y) || WindowFromPoint(screen) != button) { throw new InvalidOperationException("Native button pointer target is not the fixture control"); }
            mouse_event(2, 0, 0, 0, UIntPtr.Zero);
            pressed = true;
            var deadline = DateTime.UtcNow.AddSeconds(3);
            while (Captured(root) != button && DateTime.UtcNow < deadline) { System.Threading.Thread.Sleep(10); }
            if (Captured(root) != button) { throw new InvalidOperationException("Native button did not own capture"); }
            mouse_event(4, 0, 0, 0, UIntPtr.Zero);
            pressed = false;
            deadline = DateTime.UtcNow.AddSeconds(3);
            while (Captured(root) != IntPtr.Zero && DateTime.UtcNow < deadline) { System.Threading.Thread.Sleep(10); }
            if (Captured(root) != IntPtr.Zero) { throw new InvalidOperationException("Native button left capture active"); }
        } finally {
            if (pressed) { mouse_event(4, 0, 0, 0, UIntPtr.Zero); }
            if (!SetCursorPos(saved.X, saved.Y)) { throw new InvalidOperationException("Cursor restoration failed"); }
        }
    }
    private static void DragAt(IntPtr source, IntPtr target, bool cancel, bool below, uint cancelMessage, string region) {
        var root = GetAncestor(source, 2);
        if (root == IntPtr.Zero) { throw new InvalidOperationException("Dashboard root unavailable"); }
        int scale = (int)GetDpiForWindow(source);
        var start = StartPoint(source, region);
        var capture = Begin(source, start);
        Rect targetRect;
        if (!GetClientRect(target, out targetRect)) { throw new InvalidOperationException("Drop target bounds unavailable"); }
        var end = new Point { X = 12 * scale / 96, Y = Math.Max(2, targetRect.Bottom * (below ? 3 : 1) / 4) };
        if (!ClientToScreen(target, ref end) || !ScreenToClient(capture, ref end)) { throw new InvalidOperationException("Drop coordinates unavailable"); }
        Send(capture, 0x0200, new IntPtr(1), Position(end));
        if (cancel) {
            if (cancelMessage == 0x0100) {
                var focus = ThreadInfo(root).Focus;
                if (focus == IntPtr.Zero) { throw new InvalidOperationException("Drag has no keyboard focus for Escape"); }
                Send(focus, 0x0100, new IntPtr(0x1b), IntPtr.Zero);
                Send(focus, 0x0101, new IntPtr(0x1b), IntPtr.Zero);
            } else { Send(capture, cancelMessage, IntPtr.Zero, IntPtr.Zero); }
        } else {
            if (Captured(root) != capture) { throw new InvalidOperationException("Drag unexpectedly lost capture"); }
            Send(capture, 0x0202, IntPtr.Zero, Position(end));
        }
        if (Captured(root) != IntPtr.Zero) { throw new InvalidOperationException("Drag left the mouse captured"); }
    }
}
'@ + (Get-Content -LiteralPath "$PSScriptRoot/native-probe.cs" -Raw))

function Wait-For([scriptblock]$Condition, [string]$Label, [int]$Seconds = 25) {
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        if (& $Condition) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($null -ne $script:viewport -and $script:viewport -ne [IntPtr]::Zero) {
        $cards = @([WtmDragProbe]::Cards($script:viewport))
        $cards | Select-Object Top, Class, Text | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $fixture 'failed-cards.json') -Encoding utf8NoBOM
        Write-Host "Observed $($cards.Count) cards. Evidence at $fixture"
    }
    throw "Timed out waiting for $Label"
}

function Run-Git([string]$Repo, [string[]]$Arguments) {
    & git -c core.fsmonitor=false -c core.autocrlf=false -c commit.gpgsign=false -c "core.hooksPath=$fixture" -C $Repo @Arguments
    if ($LASTEXITCODE -ne 0) { throw "Git fixture failed with $LASTEXITCODE" }
}

function New-Repository([string]$Name, [switch]$Linked) {
    $repo = Join-Path $fixture $Name
    New-Item -ItemType Directory -Path $repo | Out-Null
    Run-Git $repo @('init', '--quiet')
    [IO.File]::WriteAllText((Join-Path $repo 'lines.txt'), "before`n")
    Run-Git $repo @('add', '.')
    Run-Git $repo @('-c', 'user.name=Wtm Test', '-c', 'user.email=wtm@example.invalid', 'commit', '--quiet', '-m', 'Fixture')
    if ($Linked) { Run-Git $repo @('worktree', 'add', '--quiet', '-b', 'topic', (Join-Path $fixture "$Name-tree")) }
    return $repo
}

function Start-Dashboard([int]$ExpectedCards = 8) {
    $start = [Diagnostics.ProcessStartInfo]::new($Binary)
    $start.WorkingDirectory = $launchDirectory
    $start.UseShellExecute = $false
    $start.RedirectStandardInput = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $script:launcher = [Diagnostics.Process]::Start($start)
    $stdout = $script:launcher.StandardOutput.ReadToEndAsync()
    $stderr = $script:launcher.StandardError.ReadToEndAsync()
    $script:launcher.StandardInput.Close()
    $script:process = $null
    $script:handle = [IntPtr]::Zero
    Wait-For {
        foreach ($child in @(Get-CimInstance Win32_Process -Filter "ParentProcessId = $($script:launcher.Id)")) {
            $script:handle = [WtmDragProbe]::Find([uint32]$child.ProcessId)
            if ($script:handle -ne [IntPtr]::Zero) {
                $script:process = [Diagnostics.Process]::GetProcessById([int]$child.ProcessId)
                [void]$script:process.Handle
                return $true
            }
        }
        return $false
    } 'dashboard child window'
    if (-not $script:launcher.WaitForExit(3000) -or $script:launcher.ExitCode -ne 0) { throw 'Dashboard launcher did not exit successfully' }
    if (-not $stdout.Wait(1000) -or -not $stderr.Wait(1000)) { throw 'Dashboard launcher did not release stdout and stderr' }
    $dpi = [WtmDragProbe]::GetDpiForWindow($script:handle)
    if (-not [WtmDragProbe]::SetWindowPos($script:handle, [IntPtr]::Zero, 0, 0, (800 * $dpi / 96), (1400 * $dpi / 96), 20)) { throw 'Dashboard resize failed' }
    $script:viewport = [WtmDragProbe]::GetDlgItem($script:handle, 1001)
    Wait-For {
        $cards = @([WtmDragProbe]::Cards($script:viewport))
        $cards.Count -eq $ExpectedCards -and ($cards.Text -join "`n") -notmatch 'counts pending|Refreshing'
    } "$ExpectedCards repository and worktree cards"
}

function Close-Dashboard {
    if (-not [WtmDragProbe]::PostMessage($script:handle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) { throw 'Window close failed' }
    if (-not $script:process.WaitForExit(10000) -or $script:process.ExitCode -ne 0) { throw 'Dashboard did not close cleanly' }
}

function Find-Card([string]$Name, [switch]$Repository, [switch]$Optional) {
    $cards = @([WtmDragProbe]::Cards($script:viewport))
    if ($Repository) {
        $cards = @($cards | Where-Object { ($_.Text -split "`r?`n")[0] -eq $Name -and $_.Text -notmatch 'S\s+\+' })
    } else {
        $entry = @($script:worktreePaths.GetEnumerator() | Where-Object { (Path-Key $_.Value) -eq (Path-Key $Name) })
        if ($entry.Count -ne 1) { throw "Expected one fixture mapping for $Name, found $($entry.Count)" }
        $separator = $entry[0].Key.IndexOf('|')
        $repositoryName = $entry[0].Key.Substring(0, $separator)
        $branch = $entry[0].Key.Substring($separator + 1)
        $current = $null
        $cardMatches = [Collections.Generic.List[object]]::new()
        foreach ($card in $cards) {
            $lines = @($card.Text -split "`r?`n")
            if ($lines[0] -in @('alpha', 'beta', 'current') -and $card.Text -notmatch 'S\s+\+') { $current = $lines[0]; continue }
            if ($current -eq $repositoryName -and $lines.Count -ge 2 -and $lines[0] -eq $branch) { [void]$cardMatches.Add($card) }
        }
        $cards = $cardMatches.ToArray()
    }
    if ($Optional -and $cards.Count -eq 0) { return $null }
    if ($cards.Count -ne 1) { throw "Expected one card for $Name, found $($cards.Count)" }
    return $cards[0]
}

function Path-Key([string]$Path) {
    [IO.Path]::GetFullPath($Path).TrimEnd('\', '/').Replace('\', '/').ToLowerInvariant()
}

function Card-Order {
    $repository = $null
    $order = [Collections.Generic.List[string]]::new()
    foreach ($card in @([WtmDragProbe]::Cards($script:viewport))) {
        $lines = $card.Text -split "`r?`n"
        if ($lines[0] -in @('alpha', 'beta', 'current') -and $card.Text -notmatch 'S\s+\+') {
            $name = $lines[0]
            $repository = $name
            [void]$order.Add("repo:$name")
        }
        elseif (-not $card.Text.StartsWith('Presentation order warning')) {
            $key = "$repository|$($lines[0])"
            if (-not $script:worktreePaths.ContainsKey($key)) { throw "Unexpected worktree branch $($lines[0]) in $repository" }
            [void]$order.Add("tree:$(Path-Key $script:worktreePaths[$key])")
        }
    }
    $order.ToArray() -join ','
}

function Assert-Order([string]$Expected, [string]$Label) {
    Wait-For { (Card-Order) -eq $Expected } $Label
    Write-Host "PASS. $Label."
}

$fixture = Join-Path $env:LOCALAPPDATA "Temp/opencode/wtm-drag-$([Guid]::NewGuid().ToString('N'))"
$fixtureHome = Join-Path $fixture 'home'
New-Item -ItemType Directory -Path $fixtureHome | Out-Null
$oldHome = $env:HOME
$oldProfile = $env:USERPROFILE
$oldPath = $env:PATH
$process = $null
$launcher = $null
$previousDpiContext = [WtmDragProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
try {
    $env:HOME = Join-Path $fixture 'unix-home'
    $env:USERPROFILE = $fixtureHome
    $alpha = New-Repository 'alpha' -Linked
    $beta = New-Repository 'beta' -Linked
    $current = New-Repository 'current'
    $alphaTree = Join-Path $fixture 'alpha-tree'
    $betaTree = Join-Path $fixture 'beta-tree'
    $launchDirectory = Join-Path $current 'nested'
    New-Item -ItemType Directory -Path $launchDirectory | Out-Null
    $script:worktreePaths = @{}
    foreach ($entry in @(
        @{ Name = 'alpha'; Paths = @($alpha, $alphaTree) }
        @{ Name = 'beta'; Paths = @($beta, $betaTree) }
        @{ Name = 'current'; Paths = @($current) }
    )) {
        foreach ($path in $entry.Paths) {
            $branch = (Run-Git $path @('branch', '--show-current') | Out-String).Trim()
            $script:worktreePaths["$($entry.Name)|$branch"] = $path
        }
    }
    foreach ($repo in @($alpha, $beta)) {
        & $CliBinary add $repo
        if ($LASTEXITCODE -ne 0) { throw 'Fixture registration failed' }
    }
    $config = Join-Path $fixtureHome '.config/wtm/config.json'
    $registrations = [IO.File]::ReadAllText($config)
    $preferences = Join-Path $fixtureHome '.config/wtm/config.order.json'
    $initialOrder = @("repo:alpha", "tree:$(Path-Key $alpha)", "tree:$(Path-Key $alphaTree)", "repo:beta", "tree:$(Path-Key $beta)", "tree:$(Path-Key $betaTree)", "repo:current", "tree:$(Path-Key $current)") -join ','
    $repositoryOrder = @("repo:beta", "tree:$(Path-Key $beta)", "tree:$(Path-Key $betaTree)", "repo:alpha", "tree:$(Path-Key $alpha)", "tree:$(Path-Key $alphaTree)", "repo:current", "tree:$(Path-Key $current)") -join ','
    $worktreeOrder = @("repo:beta", "tree:$(Path-Key $beta)", "tree:$(Path-Key $betaTree)", "repo:alpha", "tree:$(Path-Key $alphaTree)", "tree:$(Path-Key $alpha)", "repo:current", "tree:$(Path-Key $current)") -join ','
    Start-Dashboard
    $text = [WtmDragProbe]::AllText($script:handle)
    if ($text -cmatch 'REPOSITORIES|WORKTREES|LINE CHANGES') { throw 'Dashboard still shows summary totals' }
    $currentCard = Find-Card $current
    $currentLines = @($currentCard.Text -split "`r?`n")
    if ($currentLines.Count -ne 2 -or $currentLines[1] -notmatch '^S\s+\+') { throw 'Launch context did not resolve the actual worktree root' }
    if ([IO.File]::ReadAllText($config) -ne $registrations) { throw 'Launching registered the current repository' }
    Write-Host 'PASS. Unregistered launch repository appears as a worktree without changing registrations.'
    Assert-Order $initialOrder 'Unregistered launch repository is appended to registered groups'
    $initial = Card-Order
    [IO.File]::WriteAllText((Join-Path $fixture 'initial-order.txt'), $initial)
    foreach ($repository in @($true, $false)) {
        $card = if ($repository) { Find-Card 'alpha' -Repository } else { Find-Card $alpha }
        foreach ($move in @($false, $true)) {
            [WtmDragProbe]::ClickBody($card.Handle, $move)
            Assert-Order $initial 'Click and below-threshold motion preserve order'
            if (Test-Path $preferences) { throw 'Click or below-threshold motion wrote presentation order' }
        }
    }
    $source = Find-Card 'beta' -Repository
    $target = Find-Card 'alpha' -Repository
    [WtmDragProbe]::DragFrom($source.Handle, $target.Handle, 'right')
    Assert-Order $repositoryOrder 'Repository right-side space moves its whole group'
    $source = Find-Card 'beta' -Repository
    $target = Find-Card $alphaTree
    [WtmDragProbe]::DragBelow($source.Handle, $target.Handle)
    Assert-Order $initial 'Repository body can move a group down past another group'
    $source = Find-Card 'beta' -Repository
    $target = Find-Card 'alpha' -Repository
    [WtmDragProbe]::Drag($source.Handle, $target.Handle, $false)
    Assert-Order $repositoryOrder 'Repository heading can move a group back up'
    $source = Find-Card $alphaTree
    $target = Find-Card $alpha
    [WtmDragProbe]::DragFrom($source.Handle, $target.Handle, 'counts')
    Assert-Order $worktreeOrder 'Worktree counts reorder within its repository'
    $source = Find-Card $alphaTree
    $target = Find-Card $alpha
    [WtmDragProbe]::DragBelow($source.Handle, $target.Handle)
    Assert-Order $repositoryOrder 'Worktree right-side space can move a row down'
    $source = Find-Card $alphaTree
    $target = Find-Card $alpha
    [WtmDragProbe]::Drag($source.Handle, $target.Handle, $false)
    Assert-Order $worktreeOrder 'Worktree heading can move a row back up'
    $source = Find-Card $alpha
    $target = Find-Card $beta
    [WtmDragProbe]::Drag($source.Handle, $target.Handle, $false)
    Assert-Order $worktreeOrder 'Cross-repository worktree drop leaves groups unchanged'
    $source = Find-Card 'alpha' -Repository
    $target = Find-Card 'beta' -Repository
    [WtmDragProbe]::Drag($source.Handle, $target.Handle, $true)
    Assert-Order $worktreeOrder 'Escape cancels a drag'
    $savedBeforeCancel = [IO.File]::ReadAllText($preferences)
    $showAll = [WtmDragProbe]::GetDlgItem($script:handle, 1004)
    [void][WtmDragProbe]::Send($showAll, 0x00f5, [IntPtr]::Zero, [IntPtr]::Zero)
    [WtmDragProbe]::ClickAction((Find-Card $alpha).Handle)
    Wait-For { [WtmDragProbe]::AllText($script:handle) -match 'Unhide' } 'native Hide action'
    Assert-Order $worktreeOrder 'Hide button preserves order with Show all enabled'
    [void][WtmDragProbe]::Send($showAll, 0x00f5, [IntPtr]::Zero, [IntPtr]::Zero)
    Wait-For { $null -eq (Find-Card $alpha -Optional) } 'hidden row removed'
    [void][WtmDragProbe]::Send($showAll, 0x00f5, [IntPtr]::Zero, [IntPtr]::Zero)
    Wait-For { $null -ne (Find-Card $alpha -Optional) } 'hidden row revealed'
    if ((Find-Card $alpha).Text -match '\bRefreshing\b') { throw 'Revealing an idle row replayed its completed scan' }
    [WtmDragProbe]::ClickAction((Find-Card $alpha).Handle)
    Wait-For { [WtmDragProbe]::AllText($script:handle) -notmatch 'Unhide' } 'native Unhide action'
    [void][WtmDragProbe]::Send($showAll, 0x00f5, [IntPtr]::Zero, [IntPtr]::Zero)
    if ([IO.File]::ReadAllText($preferences) -ne $savedBeforeCancel) { throw 'Native buttons changed saved order' }
    foreach ($message in @(0x001f, 0x0215)) {
        $source = Find-Card 'alpha' -Repository
        $target = Find-Card 'beta' -Repository
        [WtmDragProbe]::CancelDrag($source.Handle, $target.Handle, [uint32]$message)
        Assert-Order $worktreeOrder "Cancellation message $message preserves order"
        if ([IO.File]::ReadAllText($preferences) -ne $savedBeforeCancel) { throw 'Cancelled drag changed saved order' }
    }
    [IO.File]::WriteAllText((Join-Path $alpha 'lines.txt'), "before`nchanged`n")
    Wait-For { (Find-Card $alpha).Text -match 'S\s+\+1/-0\b' } 'live count refresh' 40
    Assert-Order $worktreeOrder 'Refresh preserves repository and worktree order'
    $reorderedOrder = Card-Order
    if ([IO.File]::ReadAllText($config) -ne $registrations) { throw 'Reordering changed repository registrations' }
    Close-Dashboard
    Start-Dashboard
    Assert-Order $worktreeOrder 'Restart preserves repository and worktree order'
    & $CliBinary add $current
    if ($LASTEXITCODE -ne 0) { throw 'Explicit registration of launch repository failed' }
    [IO.File]::WriteAllText((Join-Path $current 'lines.txt'), "before`nfirst`nsecond`n")
    Wait-For { (Find-Card $current).Text -match 'S\s+\+2/-0\b' } 'registered launch repository refresh' 40
    Wait-For { @([WtmDragProbe]::Cards($script:viewport)).Count -eq 8 } 'deduplicated current repository'
    Assert-Order $worktreeOrder 'Registering the launch repository does not duplicate its worktree'
    [IO.File]::WriteAllText((Join-Path $fixture 'final-order.txt'), (Card-Order))
    & $CliBinary remove $current
    if ($LASTEXITCODE -ne 0) { throw 'Removing launch registration failed' }
    [IO.File]::WriteAllText((Join-Path $current 'lines.txt'), "before`nfirst`nsecond`nthird`n")
    Wait-For { (Find-Card $current -Optional).Text -match 'S\s+\+3/-0\b' } 'unregistered launch repository refresh' 40
    Assert-Order $worktreeOrder 'Removing a registration keeps the current repository visible'
    Close-Dashboard
    $launchDirectory = $fixtureHome
    Start-Dashboard 6
    Assert-Order (@("repo:beta", "tree:$(Path-Key $beta)", "tree:$(Path-Key $betaTree)", "repo:alpha", "tree:$(Path-Key $alphaTree)", "tree:$(Path-Key $alpha)") -join ',') 'Launching outside Git does not resurrect an unregistered repository'
    $outsideOrder = Card-Order
    $savedOrder = [IO.File]::ReadAllText($preferences)
    $attributes = [IO.File]::GetAttributes($preferences)
    try {
        [IO.File]::SetAttributes($preferences, ($attributes -bor [IO.FileAttributes]::ReadOnly))
        $source = Find-Card 'alpha' -Repository
        $target = Find-Card 'beta' -Repository
        [WtmDragProbe]::Drag($source.Handle, $target.Handle, $false)
        Wait-For { [WtmDragProbe]::AllText($script:handle) -match 'Order was not saved' } 'visible failed-save warning'
        Assert-Order $outsideOrder 'Failed save keeps the previous visible order'
        if ([IO.File]::ReadAllText($preferences) -ne $savedOrder) { throw 'Failed save changed stored order' }
    } finally { [IO.File]::SetAttributes($preferences, $attributes) }
    [IO.File]::WriteAllText($preferences, 'broken')
    Wait-For { [WtmDragProbe]::AllText($script:handle) -match 'Cannot load presentation order' } 'visible malformed-preference warning'
    $source = Find-Card 'alpha' -Repository
    $target = Find-Card 'beta' -Repository
    [WtmDragProbe]::Drag($source.Handle, $target.Handle, $false)
    Assert-Order $outsideOrder 'Malformed preferences keep the last good visible order'
    if ([IO.File]::ReadAllText($preferences) -ne 'broken') { throw 'Drag overwrote malformed preferences' }
    [IO.File]::WriteAllText($preferences, $savedOrder)
    $source = Find-Card 'alpha' -Repository
    $target = Find-Card 'beta' -Repository
    [WtmDragProbe]::Drag($source.Handle, $target.Handle, $false)
    Assert-Order (@("repo:alpha", "tree:$(Path-Key $alphaTree)", "tree:$(Path-Key $alpha)", "repo:beta", "tree:$(Path-Key $beta)", "tree:$(Path-Key $betaTree)") -join ',') 'Saving recovers after preferences are repaired'
    Wait-For { [WtmDragProbe]::AllText($script:handle) -notmatch 'Presentation order warning' } 'cleared save warning'
    $source = Find-Card 'beta' -Repository
    $target = Find-Card 'alpha' -Repository
    [WtmDragProbe]::Drag($source.Handle, $target.Handle, $false)
    Assert-Order $outsideOrder 'Recovered preferences retain the selected final order'
    if ([IO.File]::ReadAllText($config) -ne $registrations) { throw 'Dashboard actions changed the final registration set' }
    Copy-Item -LiteralPath $config -Destination (Join-Path $fixture 'registrations.json')
    Copy-Item -LiteralPath $preferences -Destination (Join-Path $fixture 'saved-order.json')
    Close-Dashboard
    $missingGitHome = Join-Path $fixture 'missing-git-home'
    New-Item -ItemType Directory -Path $missingGitHome | Out-Null
    $env:USERPROFILE = $missingGitHome
    $launchDirectory = $current
    $env:PATH = ''
    Start-Dashboard 2
    $failureText = [WtmDragProbe]::AllText($script:handle)
    [IO.File]::WriteAllText((Join-Path $fixture 'missing-git-error.txt'), $failureText)
    if ($failureText -notmatch '(?m)^Launch repository discovery error\r?$' -or $failureText -notmatch 'Could not start Git') { throw 'Launch discovery failure is not visible as an error' }
    if (Test-Path (Join-Path $missingGitHome '.config/wtm/config.json')) { throw 'Failed launch discovery wrote registrations' }
    Close-Dashboard
    $env:PATH = $oldPath
    Write-Host 'PASS. Missing Git is a visible launch discovery error without writing registrations.'
    [ordered]@{
        binary = $Binary
        sha256 = (Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash
        initial_order = $initial
        reordered = $reorderedOrder
        outside_git_order = $outsideOrder
        registrations_unchanged = $true
        saved_order_sha256 = (Get-FileHash -LiteralPath $preferences -Algorithm SHA256).Hash
        summary_totals_absent = $true
        missing_git_error_visible = $true
        result = 'PASS'
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $fixture 'proof.json') -Encoding utf8NoBOM
    Write-Host "PASS. Dashboard drag and launch-folder checks. Evidence at $fixture"
} finally {
    $env:HOME = $oldHome
    $env:USERPROFILE = $oldProfile
    $env:PATH = $oldPath
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
    if ($previousDpiContext -ne [IntPtr]::Zero) { [void][WtmDragProbe]::SetThreadDpiAwarenessContext($previousDpiContext) }
}
