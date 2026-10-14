param([string]$Binary = "$PSScriptRoot/../target/debug/wts.exe")

$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path $Binary).Path
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class WtsDragProbe {
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
            if (owner == pid && IsWindowVisible(handle) && Text(handle).StartsWith("WTS")) {
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
            if (GetParent(handle) == parent && IsWindowVisible(handle) && Text(handle).Length != 0) {
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
        DragAt(source, target, cancel, false, 0x0100);
    }
    public static void DragBelow(IntPtr source, IntPtr target) {
        DragAt(source, target, false, true, 0);
    }
    public static void CancelDrag(IntPtr source, IntPtr target, uint message) {
        DragAt(source, target, true, false, message);
    }
    private static void DragAt(IntPtr source, IntPtr target, bool cancel, bool below, uint cancelMessage) {
        var root = GetAncestor(source, 2);
        if (root == IntPtr.Zero) { throw new InvalidOperationException("Dashboard root unavailable"); }
        int scale = (int)GetDpiForWindow(source);
        var start = new Point { X = 12 * scale / 96, Y = 20 * scale / 96 };
        var hit = start;
        var parent = GetParent(source);
        if (!ClientToScreen(source, ref hit) || !ScreenToClient(parent, ref hit)) { throw new InvalidOperationException("Handle coordinates unavailable"); }
        var handle = ChildWindowFromPointEx(parent, hit, 7);
        if (handle == IntPtr.Zero || handle == parent) { throw new InvalidOperationException("Drag handle unavailable"); }
        var screenStart = start;
        if (!ClientToScreen(source, ref screenStart)) { throw new InvalidOperationException("Handle hit-test coordinates unavailable"); }
        if (Send(handle, 0x0084, IntPtr.Zero, Position(screenStart)).ToInt64() != 1) { throw new InvalidOperationException("Drag handle does not accept real mouse hit testing"); }
        if (!ClientToScreen(parent, ref hit) || !ScreenToClient(handle, ref hit)) { throw new InvalidOperationException("Handle coordinates unavailable"); }
        Send(handle, 0x0201, new IntPtr(1), Position(hit));
        var capture = Captured(root);
        if (capture == IntPtr.Zero) { throw new InvalidOperationException("Drag handle did not capture the mouse"); }
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
'@

function Wait-For([scriptblock]$Condition, [string]$Label, [int]$Seconds = 25) {
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        if (& $Condition) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($null -ne $script:viewport -and $script:viewport -ne [IntPtr]::Zero) {
        $cards = @([WtsDragProbe]::Cards($script:viewport))
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
    Run-Git $repo @('-c', 'user.name=Wts Test', '-c', 'user.email=wts@example.invalid', 'commit', '--quiet', '-m', 'Fixture')
    if ($Linked) { Run-Git $repo @('worktree', 'add', '--quiet', '-b', 'topic', (Join-Path $fixture "$Name-tree")) }
    return $repo
}

function Start-Dashboard([int]$ExpectedCards = 8) {
    $start = [Diagnostics.ProcessStartInfo]::new($Binary)
    $start.ArgumentList.Add('--live')
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
            $script:handle = [WtsDragProbe]::Find([uint32]$child.ProcessId)
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
    $dpi = [WtsDragProbe]::GetDpiForWindow($script:handle)
    if (-not [WtsDragProbe]::SetWindowPos($script:handle, [IntPtr]::Zero, 0, 0, (800 * $dpi / 96), (1400 * $dpi / 96), 20)) { throw 'Dashboard resize failed' }
    $script:viewport = [WtsDragProbe]::GetDlgItem($script:handle, 1001)
    Wait-For { @([WtsDragProbe]::Cards($script:viewport)).Count -eq $ExpectedCards } "$ExpectedCards repository and worktree cards"
}

function Close-Dashboard {
    if (-not [WtsDragProbe]::PostMessage($script:handle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) { throw 'Window close failed' }
    if (-not $script:process.WaitForExit(10000) -or $script:process.ExitCode -ne 0) { throw 'Dashboard did not close cleanly' }
}

function Find-Card([string]$Name, [switch]$Repository) {
    $cards = @([WtsDragProbe]::Cards($script:viewport) | Where-Object {
        if ($Repository) { $_.Text.StartsWith("$Name ·") }
        else {
            $lines = @($_.Text -split "`r?`n")
            $lines.Count -ge 2 -and (Path-Key $lines[1]) -eq (Path-Key $Name)
        }
    })
    if ($cards.Count -ne 1) { throw "Expected one card for $Name, found $($cards.Count)" }
    return $cards[0]
}

function Path-Key([string]$Path) {
    [IO.Path]::GetFullPath($Path).TrimEnd('\', '/').Replace('\', '/').ToLowerInvariant()
}

function Card-Order {
    return @([WtsDragProbe]::Cards($script:viewport) | ForEach-Object {
        $lines = $_.Text -split "`r?`n"
        if ($lines[0] -match '^(.+) · (\d+) worktrees?$') {
            $name = $Matches[1]
            $count = [int]$Matches[2]
            $expected = @{ alpha = 2; beta = 2; current = 1 }
            if (-not $expected.ContainsKey($name) -or $count -ne $expected[$name]) { throw "Unexpected worktree count for $name" }
            "repo:$name"
        }
        elseif (-not $_.Text.StartsWith('Presentation order warning')) { "tree:$(Path-Key $lines[1])" }
    }) -join ','
}

function Assert-Order([string]$Expected, [string]$Label) {
    Wait-For { (Card-Order) -eq $Expected } $Label
    Write-Host "PASS. $Label."
}

$fixture = Join-Path $env:LOCALAPPDATA "Temp/opencode/wts-drag-$([Guid]::NewGuid().ToString('N'))"
$fixtureHome = Join-Path $fixture 'home'
New-Item -ItemType Directory -Path $fixtureHome | Out-Null
$oldHome = $env:HOME
$oldProfile = $env:USERPROFILE
$oldPath = $env:PATH
$process = $null
$launcher = $null
$previousDpiContext = [WtsDragProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
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
    foreach ($repo in @($alpha, $beta)) {
        & $Binary add $repo
        if ($LASTEXITCODE -ne 0) { throw 'Fixture registration failed' }
    }
    $config = Join-Path $fixtureHome '.config/wts/config.json'
    $registrations = [IO.File]::ReadAllText($config)
    $preferences = Join-Path $fixtureHome '.config/wts/config.order.json'
    $initialOrder = @("repo:alpha", "tree:$(Path-Key $alpha)", "tree:$(Path-Key $alphaTree)", "repo:beta", "tree:$(Path-Key $beta)", "tree:$(Path-Key $betaTree)", "repo:current", "tree:$(Path-Key $current)") -join ','
    $repositoryOrder = @("repo:beta", "tree:$(Path-Key $beta)", "tree:$(Path-Key $betaTree)", "repo:alpha", "tree:$(Path-Key $alpha)", "tree:$(Path-Key $alphaTree)", "repo:current", "tree:$(Path-Key $current)") -join ','
    $worktreeOrder = @("repo:beta", "tree:$(Path-Key $beta)", "tree:$(Path-Key $betaTree)", "repo:alpha", "tree:$(Path-Key $alphaTree)", "tree:$(Path-Key $alpha)", "repo:current", "tree:$(Path-Key $current)") -join ','
    Start-Dashboard
    $text = [WtsDragProbe]::AllText($script:handle)
    if ($text -cmatch 'REPOSITORIES|WORKTREES|LINE CHANGES') { throw 'Dashboard still shows summary totals' }
    $currentCard = Find-Card $current
    $currentPaths = @($currentCard.Text -split "`r?`n" | Where-Object { [IO.Path]::IsPathFullyQualified($_) })
    if ($currentPaths.Count -ne 1 -or [IO.Path]::GetFullPath($currentPaths[0]) -ne $current) { throw 'Launch context did not resolve the actual worktree root' }
    if ([IO.File]::ReadAllText($config) -ne $registrations) { throw 'Launching registered the current repository' }
    Write-Host 'PASS. Unregistered launch repository appears as a worktree without changing registrations.'
    Assert-Order $initialOrder 'Unregistered launch repository is appended to registered groups'
    $initial = Card-Order
    [IO.File]::WriteAllText((Join-Path $fixture 'initial-order.txt'), $initial)
    $source = Find-Card 'beta' -Repository
    $target = Find-Card 'alpha' -Repository
    [WtsDragProbe]::Drag($source.Handle, $target.Handle, $false)
    Assert-Order $repositoryOrder 'Repository handle moves its whole group'
    $source = Find-Card 'beta' -Repository
    $target = Find-Card $alphaTree
    [WtsDragProbe]::DragBelow($source.Handle, $target.Handle)
    Assert-Order $initial 'Repository handle can move a group down past another group'
    $source = Find-Card 'beta' -Repository
    $target = Find-Card 'alpha' -Repository
    [WtsDragProbe]::Drag($source.Handle, $target.Handle, $false)
    Assert-Order $repositoryOrder 'Repository handle can move a group back up'
    $source = Find-Card $alphaTree
    $target = Find-Card $alpha
    [WtsDragProbe]::Drag($source.Handle, $target.Handle, $false)
    Assert-Order $worktreeOrder 'Worktree handle reorders within its repository'
    $source = Find-Card $alphaTree
    $target = Find-Card $alpha
    [WtsDragProbe]::DragBelow($source.Handle, $target.Handle)
    Assert-Order $repositoryOrder 'Worktree handle can move a row down'
    $source = Find-Card $alphaTree
    $target = Find-Card $alpha
    [WtsDragProbe]::Drag($source.Handle, $target.Handle, $false)
    Assert-Order $worktreeOrder 'Worktree handle can move a row back up'
    $source = Find-Card $alpha
    $target = Find-Card $beta
    [WtsDragProbe]::Drag($source.Handle, $target.Handle, $false)
    Assert-Order $worktreeOrder 'Cross-repository worktree drop leaves groups unchanged'
    $source = Find-Card 'alpha' -Repository
    $target = Find-Card 'beta' -Repository
    [WtsDragProbe]::Drag($source.Handle, $target.Handle, $true)
    Assert-Order $worktreeOrder 'Escape cancels a drag'
    $savedBeforeCancel = [IO.File]::ReadAllText($preferences)
    foreach ($message in @(0x001f, 0x0215)) {
        $source = Find-Card 'alpha' -Repository
        $target = Find-Card 'beta' -Repository
        [WtsDragProbe]::CancelDrag($source.Handle, $target.Handle, [uint32]$message)
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
    & $Binary add $current
    if ($LASTEXITCODE -ne 0) { throw 'Explicit registration of launch repository failed' }
    [IO.File]::WriteAllText((Join-Path $current 'lines.txt'), "before`nfirst`nsecond`n")
    Wait-For { (Find-Card $current).Text -match 'S\s+\+2/-0\b' } 'registered launch repository refresh' 40
    Wait-For { @([WtsDragProbe]::Cards($script:viewport)).Count -eq 8 } 'deduplicated current repository'
    Assert-Order $worktreeOrder 'Registering the launch repository does not duplicate its worktree'
    [IO.File]::WriteAllText((Join-Path $fixture 'final-order.txt'), (Card-Order))
    & $Binary remove $current
    if ($LASTEXITCODE -ne 0) { throw 'Removing launch registration failed' }
    [IO.File]::WriteAllText((Join-Path $current 'lines.txt'), "before`nfirst`nsecond`nthird`n")
    Wait-For { (Find-Card $current).Text -match 'S\s+\+3/-0\b' } 'unregistered launch repository refresh' 40
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
        [WtsDragProbe]::Drag($source.Handle, $target.Handle, $false)
        Wait-For { [WtsDragProbe]::AllText($script:handle) -match 'Order was not saved' } 'visible failed-save warning'
        Assert-Order $outsideOrder 'Failed save keeps the previous visible order'
        if ([IO.File]::ReadAllText($preferences) -ne $savedOrder) { throw 'Failed save changed stored order' }
    } finally { [IO.File]::SetAttributes($preferences, $attributes) }
    [IO.File]::WriteAllText($preferences, 'broken')
    Wait-For { [WtsDragProbe]::AllText($script:handle) -match 'Cannot load presentation order' } 'visible malformed-preference warning'
    $source = Find-Card 'alpha' -Repository
    $target = Find-Card 'beta' -Repository
    [WtsDragProbe]::Drag($source.Handle, $target.Handle, $false)
    Assert-Order $outsideOrder 'Malformed preferences keep the last good visible order'
    if ([IO.File]::ReadAllText($preferences) -ne 'broken') { throw 'Drag overwrote malformed preferences' }
    [IO.File]::WriteAllText($preferences, $savedOrder)
    $source = Find-Card 'alpha' -Repository
    $target = Find-Card 'beta' -Repository
    [WtsDragProbe]::Drag($source.Handle, $target.Handle, $false)
    Assert-Order (@("repo:alpha", "tree:$(Path-Key $alphaTree)", "tree:$(Path-Key $alpha)", "repo:beta", "tree:$(Path-Key $beta)", "tree:$(Path-Key $betaTree)") -join ',') 'Saving recovers after preferences are repaired'
    Wait-For { [WtsDragProbe]::AllText($script:handle) -notmatch 'Presentation order warning' } 'cleared save warning'
    $source = Find-Card 'beta' -Repository
    $target = Find-Card 'alpha' -Repository
    [WtsDragProbe]::Drag($source.Handle, $target.Handle, $false)
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
    $failureText = [WtsDragProbe]::AllText($script:handle)
    if ($failureText -notmatch '(?m)^ERROR\b' -or $failureText -notmatch 'Could not start Git') { throw 'Launch discovery failure is not visible as an error' }
    if (Test-Path (Join-Path $missingGitHome '.config/wts/config.json')) { throw 'Failed launch discovery wrote registrations' }
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
    if ($previousDpiContext -ne [IntPtr]::Zero) { [void][WtsDragProbe]::SetThreadDpiAwarenessContext($previousDpiContext) }
}
