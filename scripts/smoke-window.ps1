param([string]$Binary = "$PSScriptRoot/../target/debug/wts.exe")

$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path $Binary).Path
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class WtsWindowProbe {
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
    public static IntPtr Edit(IntPtr parent) {
        IntPtr found = IntPtr.Zero;
        EnumChildWindows(parent, (handle, data) => {
            var name = new StringBuilder(128);
            GetClassName(handle, name, name.Capacity);
            if (name.ToString().Equals("EDIT", StringComparison.OrdinalIgnoreCase)) { found = handle; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
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
    if (([WtsWindowProbe]::GetWindowLongPtr($script:handle, -20).ToInt64() -band 8) -eq 0) { throw 'Window is not always on top' }
    if ([WtsWindowProbe]::GetForegroundWindow() -eq $script:handle) { throw 'Window took foreground focus' }
    Wait-For { Has-Counts 1 0 } 'initial +1 -0 count'
    $initial = [WtsWindowProbe]::Text($script:handle)
    if ($initial -notmatch '5s\b') { throw 'Initial polling interval is not 5s' }
    [IO.File]::WriteAllText((Join-Path $fixture 'initial.txt'), $initial)
    $edit = [WtsWindowProbe]::Edit($script:handle)
    if ($edit -eq [IntPtr]::Zero) { throw 'Status text control is missing' }
    if (-not [WtsWindowProbe]::SetWindowPos($script:handle, [IntPtr]::Zero, 0, 0, 600, 130, 22)) { throw 'Window resize failed' }
    [void][WtsWindowProbe]::SendMessage($edit, 0x00B6, [IntPtr]::Zero, [IntPtr]100)
    $beforeScroll = [WtsWindowProbe]::SendMessage($edit, 0x00CE, [IntPtr]::Zero, [IntPtr]::Zero).ToInt64()
    if ($beforeScroll -le 0) { throw 'Status text did not scroll in the resized window' }
    Wait-For { [WtsWindowProbe]::Text($script:handle) -match '10s\b' } 'unchanged polling backoff to 10s' 15
    $afterScroll = [WtsWindowProbe]::SendMessage($edit, 0x00CE, [IntPtr]::Zero, [IntPtr]::Zero).ToInt64()
    [IO.File]::WriteAllText((Join-Path $fixture 'scroll.txt'), "before=$beforeScroll`nafter=$afterScroll`n")
    if ($afterScroll -ne $beforeScroll) { throw "Unchanged refresh reset the scroll position from $beforeScroll to $afterScroll" }
    if (-not [WtsWindowProbe]::SetWindowPos($script:handle, [IntPtr]::Zero, 0, 0, 600, 400, 22)) { throw 'Window resize restoration failed' }
    Wait-For { [WtsWindowProbe]::Text($script:handle) -match '20s\b' } 'unchanged polling backoff to 20s' 20
    Wait-For { [WtsWindowProbe]::Text($script:handle) -match '30s\b' } 'unchanged polling backoff to 30s' 30
    [IO.File]::WriteAllText((Join-Path $fixture 'backoff.txt'), [WtsWindowProbe]::Text($script:handle))
    [IO.File]::WriteAllText((Join-Path $repo 'lines.txt'), "before`nadded`nanother`n")
    Wait-For { Has-Counts 2 0 } 'live +2 -0 count'
    $updated = [WtsWindowProbe]::Text($script:handle)
    if ($updated -notmatch '5s\b') { throw 'Polling did not reset to 5s after a change' }
    if ([WtsWindowProbe]::GetForegroundWindow() -eq $script:handle) { throw 'Refresh took foreground focus' }
    if (([WtsWindowProbe]::GetWindowLongPtr($script:handle, -20).ToInt64() -band 0x08000000) -ne 0) { throw 'Window cannot be activated by clicking' }
    [IO.File]::WriteAllText((Join-Path $fixture 'updated.txt'), $updated)
    $validConfig = [IO.File]::ReadAllText($config)
    [IO.File]::WriteAllText($config, '{broken')
    Wait-For { [WtsWindowProbe]::Text($script:handle) -match '(?i)error' -and (Has-Counts 2 0) } 'invalid config with last-good rows retained'
    [IO.File]::WriteAllText((Join-Path $fixture 'invalid-config.txt'), [WtsWindowProbe]::Text($script:handle))
    [IO.File]::WriteAllText($config, $validConfig)
    Wait-For { [WtsWindowProbe]::Text($script:handle) -notmatch '(?i)error' -and (Has-Counts 2 0) } 'valid config recovery'
    & $Binary remove $repo
    if ($LASTEXITCODE -ne 0) { throw 'Removal failed' }
    Wait-For { [WtsWindowProbe]::Text($script:handle) -match '(?i)no (registered )?repo' } 'live registry reload'
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
