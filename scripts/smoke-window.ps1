param([string]$Binary = "$PSScriptRoot/../target/debug/wts.exe")

$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path $Binary).Path
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class WtsWindowProbe {
    public struct Rect { public int Left, Top, Right, Bottom; }
    public struct ScrollInfo { public uint Size, Mask; public int Min, Max; public uint Page; public int Position, TrackPosition; }
    public delegate bool EnumProc(IntPtr handle, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr handle, out uint processId);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr handle, StringBuilder text, int count);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr SendMessageTimeout(IntPtr handle, uint message, UIntPtr wparam, StringBuilder text, uint flags, uint timeout, out UIntPtr result);
    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW")] public static extern IntPtr GetWindowLongPtr(IntPtr handle, int index);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr handle);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr handle);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr handle, StringBuilder text, int count);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr handle, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr handle, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr handle, out Rect rect);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr handle, out Rect rect);
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr handle, int id);
    [DllImport("user32.dll")] public static extern bool GetScrollInfo(IntPtr handle, int bar, ref ScrollInfo info);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr handle, uint message, IntPtr wparam, IntPtr lparam);
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
    public static IntPtr FirstCard(IntPtr parent) {
        IntPtr found = IntPtr.Zero;
        EnumChildWindows(parent, (handle, data) => {
            var name = new StringBuilder(128);
            GetClassName(handle, name, name.Capacity);
            if (name.ToString().Equals("STATIC", StringComparison.OrdinalIgnoreCase)) { found = handle; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }
    public static int ScrollPosition(IntPtr handle) {
        var info = new ScrollInfo { Size = (uint)Marshal.SizeOf(typeof(ScrollInfo)), Mask = 7 };
        if (!GetScrollInfo(handle, 1, ref info)) { throw new InvalidOperationException("Viewport scroll state unavailable"); }
        return info.Position;
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

function Run-Git([string[]]$Arguments) {
    & git -c core.fsmonitor=false -c commit.gpgsign=false -c "core.hooksPath=$fixture" -C $repo @Arguments
    if ($LASTEXITCODE -ne 0) { throw "Git failed with $LASTEXITCODE" }
}

function Has-Counts([int]$Added, [int]$Deleted) {
    $text = [WtsWindowProbe]::Text($script:handle)
    $counts = [regex]::Matches($text, '\+(\d+)\s+-(\d+)')
    if ($counts.Count -eq 0) { return $false }
    foreach ($count in $counts) {
        if ([int]$count.Groups[1].Value -ne $Added -or [int]$count.Groups[2].Value -ne $Deleted) { return $false }
    }
    return $text -match 'repo'
}

$fixture = Join-Path $env:LOCALAPPDATA "Temp/opencode/wts-window-$([Guid]::NewGuid().ToString('N'))"
$repo = Join-Path $fixture 'repo'
$fixtureHome = Join-Path $fixture 'home'
New-Item -ItemType Directory -Path $repo, $fixtureHome | Out-Null
$oldHome = $env:HOME
$oldProfile = $env:USERPROFILE
$process = $null
try {
    $env:HOME = Join-Path $fixture 'unix-home'
    $env:USERPROFILE = $fixtureHome
    Run-Git @('init', '--quiet')
    [IO.File]::WriteAllText((Join-Path $repo 'lines.txt'), "before`n")
    Run-Git @('add', '.')
    Run-Git @('-c', 'user.name=Wts Test', '-c', 'user.email=wts@example.invalid', 'commit', '--quiet', '-m', 'Fixture')
    [IO.File]::WriteAllText((Join-Path $repo 'lines.txt'), "before`nadded`n")
    & $Binary add $repo
    if ($LASTEXITCODE -ne 0) { throw 'Registration failed' }
    $config = Join-Path $fixtureHome '.config/wts/config.json'
    if (-not (Test-Path -LiteralPath $config)) { throw 'Registry was not written at the required home path' }
    $process = Start-Process -FilePath $Binary -ArgumentList '--live' -NoNewWindow -PassThru
    $script:handle = [IntPtr]::Zero
    Wait-For { $script:handle = [WtsWindowProbe]::Find([uint32]$process.Id); $script:handle -ne [IntPtr]::Zero } 'visible window'
    $initialStyle = [WtsWindowProbe]::GetWindowLongPtr($script:handle, -20).ToInt64()
    $foregroundOnShow = [WtsWindowProbe]::GetForegroundWindow()
    if (($initialStyle -band 8) -eq 0) { throw 'Window is not always on top' }
    if ($foregroundOnShow -eq $script:handle) { throw 'Window took foreground focus' }
    Wait-For { Has-Counts 1 0 } 'initial +1 -0 count'
    $initial = [WtsWindowProbe]::Text($script:handle)
    if ($initial -notmatch '5s\b') { throw 'Initial polling interval is not 5s' }
    [IO.File]::WriteAllText((Join-Path $fixture 'initial.txt'), $initial)
    $viewport = [WtsWindowProbe]::GetDlgItem($script:handle, 1001)
    if ($viewport -eq [IntPtr]::Zero) { throw 'Dashboard viewport is missing' }
    if (-not [WtsWindowProbe]::SetWindowPos($script:handle, [IntPtr]::Zero, 0, 0, 600, 130, 22)) { throw 'Window resize failed' }
    $card = [WtsWindowProbe]::FirstCard($viewport)
    if ($card -eq [IntPtr]::Zero) { throw 'Visible dashboard cards are missing' }
    $cardBefore = [WtsWindowProbe]::Top($card) - [WtsWindowProbe]::Top($viewport)
    [void][WtsWindowProbe]::SendMessage($viewport, 0x0115, [IntPtr]1, [IntPtr]::Zero)
    $beforeScroll = [WtsWindowProbe]::ScrollPosition($viewport)
    $cardScrolled = [WtsWindowProbe]::Top($card) - [WtsWindowProbe]::Top($viewport)
    if ($beforeScroll -le 0 -or $cardScrolled -ge $cardBefore) { throw 'Viewport scroll did not move the real cards' }
    Wait-For {
        $text = [WtsWindowProbe]::Text($script:handle)
        $line = [WtsWindowProbe]::ScrollPosition($viewport)
        $height = [WtsWindowProbe]::Height($viewport)
        $focus = [WtsWindowProbe]::GetForegroundWindow().ToInt64()
        [IO.File]::AppendAllText((Join-Path $fixture 'scroll-trace.txt'), "$line`t$height`t$focus`t$($text.Replace("`r", '').Replace("`n", '\n'))`n")
        $text -match '10s\b'
    } 'unchanged polling backoff to 10s' 15
    $afterScroll = [WtsWindowProbe]::ScrollPosition($viewport)
    [IO.File]::WriteAllText((Join-Path $fixture 'scroll.txt'), "before=$beforeScroll`nafter=$afterScroll`n")
    if ($afterScroll -ne $beforeScroll) { throw "Unchanged refresh reset the scroll position from $beforeScroll to $afterScroll" }
    $unchangedAnchor = [WtsWindowProbe]::Top($card) - [WtsWindowProbe]::Top($viewport)
    if ($unchangedAnchor -ne $cardScrolled) { throw "Unchanged refresh moved the card anchor from $cardScrolled to $unchangedAnchor" }
    Wait-For { [WtsWindowProbe]::Text($script:handle) -match '20s\b' } 'unchanged polling backoff to 20s' 20
    Wait-For { [WtsWindowProbe]::Text($script:handle) -match '30s\b' } 'unchanged polling backoff to 30s' 30
    [IO.File]::WriteAllText((Join-Path $fixture 'backoff.txt'), [WtsWindowProbe]::Text($script:handle))
    [IO.File]::WriteAllText((Join-Path $repo 'lines.txt'), "before`nadded`nanother`n")
    Wait-For { Has-Counts 2 0 } 'live +2 -0 count'
    $updated = [WtsWindowProbe]::Text($script:handle)
    if ([WtsWindowProbe]::ScrollPosition($viewport) -ne $beforeScroll) { throw 'Count-only refresh reset viewport scroll' }
    $updatedAnchor = [WtsWindowProbe]::Top($card) - [WtsWindowProbe]::Top($viewport)
    if ($updatedAnchor -ne $cardScrolled) { throw "Count-only refresh moved the card anchor from $cardScrolled to $updatedAnchor" }
    if ($updated -notmatch '5s\b') { throw 'Polling did not reset to 5s after a change' }
    $foregroundOnRefresh = [WtsWindowProbe]::GetForegroundWindow()
    if ($foregroundOnRefresh -eq $script:handle) { throw 'Refresh took foreground focus' }
    if (([WtsWindowProbe]::GetWindowLongPtr($script:handle, -20).ToInt64() -band 0x08000000) -ne 0) { throw 'Window cannot be activated by clicking' }
    [IO.File]::WriteAllText((Join-Path $fixture 'updated.txt'), $updated)
    if (-not [WtsWindowProbe]::SetWindowPos($script:handle, [IntPtr]::Zero, 0, 0, 600, 400, 22)) { throw 'Window resize restoration failed' }
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
        initial_interval = 5
        maximum_interval = 30
        topmost = ($initialStyle -band 8) -ne 0
        activated_on_show = $foregroundOnShow -eq $script:handle
        activated_on_refresh = $foregroundOnRefresh -eq $script:handle
        registry_empty = [IO.File]::ReadAllText((Join-Path $fixture 'empty.txt')) -match '(?i)no (registered )?repo'
        window_destroyed = -not [WtsWindowProbe]::IsWindow($script:handle)
        scroll_before = $beforeScroll
        scroll_after = $afterScroll
        exit_code = $process.ExitCode
        result = 'PASS'
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $fixture 'proof.json') -Encoding utf8NoBOM
    Write-Host "PASS. Always on top, live counts, scroll preservation, polling backoff/reset, config error/recovery, registry reload, and clean close. Evidence at $fixture"
} finally {
    $env:HOME = $oldHome
    $env:USERPROFILE = $oldProfile
    if ($null -ne $process -and -not $process.HasExited) { $process.Kill($true); $process.WaitForExit() }
}
