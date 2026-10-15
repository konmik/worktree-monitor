param(
    [string]$Binary = "$PSScriptRoot/../target/debug/wts-ui.exe",
    [string]$Output = "$env:LOCALAPPDATA/Temp/opencode/wts-dashboard.png",
    [switch]$Screen = $true,
    [int]$Height = 0,
    [switch]$Client
)

$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path $Binary).Path
$CliBinary = (Resolve-Path (Join-Path (Split-Path -Parent $Binary) 'wts.exe')).Path
$Output = [IO.Path]::GetFullPath($Output)
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class WtsCapture {
    public struct Rect { public int Left, Top, Right, Bottom; }
    public struct Point { public int X, Y; }
    public delegate bool EnumProc(IntPtr handle, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr handle, out uint pid);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr handle);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr handle, StringBuilder text, int count);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr SendMessageTimeout(IntPtr handle, uint message, UIntPtr wparam, StringBuilder text, uint flags, uint timeout, out UIntPtr result);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr handle, out Rect rect);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr handle, out Rect rect);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr handle, ref Point point);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr handle, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] public static extern bool SystemParametersInfo(uint action, uint param, out Rect rect, uint flags);
    [DllImport("user32.dll")] public static extern bool RedrawWindow(IntPtr handle, IntPtr rect, IntPtr region, uint flags);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr handle, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr handle, uint message, IntPtr wparam, IntPtr lparam);
    public static IntPtr Find(uint pid) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((handle, data) => {
            uint owner;
            GetWindowThreadProcessId(handle, out owner);
            if (owner != pid || !IsWindowVisible(handle)) { return true; }
            var text = new StringBuilder(256);
            GetWindowText(handle, text, text.Capacity);
            if (!text.ToString().StartsWith("WTS", StringComparison.OrdinalIgnoreCase)) { return true; }
            found = handle;
            return false;
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
}
'@

function Invoke-FixtureGit([string]$Repo, [string[]]$Arguments) {
    & git -c core.fsmonitor=false -c core.autocrlf=false -c commit.gpgsign=false -c "core.hooksPath=$fixture" -C $Repo @Arguments
    if ($LASTEXITCODE -ne 0) { throw 'Git fixture command failed' }
}

function Has-FixtureData([string]$Text) {
    $normalized = $Text.Replace('\', '/')
    foreach ($expected in $script:expectedRows) {
        $path = $expected.Path.Replace('\', '/')
        $row = '(?m)^' + [regex]::Escape($expected.Branch) + '\r?\n' + [regex]::Escape($path) + '\r?\n' + $expected.Counts
        if ($normalized -notmatch $row) { return $false }
    }
    return $true
}

$fixture = Join-Path $env:LOCALAPPDATA "Temp/opencode/wts-dashboard-$([Guid]::NewGuid().ToString('N'))"
$fixtureHome = Join-Path $fixture 'home'
New-Item -ItemType Directory -Path $fixtureHome -Force | Out-Null
$oldHome = $env:HOME
$oldProfile = $env:USERPROFILE
$process = $null
$bitmap = $null
$graphics = $null
$script:expectedRows = @()
$previousDpiContext = [WtsCapture]::SetThreadDpiAwarenessContext([IntPtr](-4))
try {
    $env:HOME = Join-Path $fixture 'unix-home'
    $env:USERPROFILE = $fixtureHome
    foreach ($spec in @(
        @{ Name = 'signal-core'; Added = 42; Deleted = 8; Binaries = 1 },
        @{ Name = 'edge-gateway'; Added = 18; Deleted = 12; Binaries = 0 },
        @{ Name = 'infra-tools'; Added = 8; Deleted = 0; Binaries = 0 }
    )) {
        $repo = Join-Path $fixture $spec.Name
        New-Item -ItemType Directory -Path $repo | Out-Null
        Invoke-FixtureGit $repo @('init', '--quiet')
        $before = @(1..100 | ForEach-Object { "original line $_" })
        [IO.File]::WriteAllLines((Join-Path $repo 'source.txt'), $before)
        Invoke-FixtureGit $repo @('add', '.')
        Invoke-FixtureGit $repo @('-c', 'user.name=Wts Visual Test', '-c', 'user.email=wts@example.invalid', 'commit', '--quiet', '-m', 'Fixture')
        $after = @($before | Select-Object -Skip $spec.Deleted) + @(1..$spec.Added | ForEach-Object { "new line $_" })
        [IO.File]::WriteAllLines((Join-Path $repo 'source.txt'), $after)
        if ($spec.Binaries -gt 0) { [IO.File]::WriteAllBytes((Join-Path $repo 'asset.bin'), [byte[]]@(0, 1, 2)) }
        $branch = (Invoke-FixtureGit $repo @('branch', '--show-current') | Out-String).Trim()
        $binaryCounts = if ($spec.Binaries -gt 0) { " · binaries: $($spec.Binaries)" } else { '' }
        $script:expectedRows += [pscustomobject]@{ Branch = $branch; Path = $repo; Counts = "\+$($spec.Added)\s+-$($spec.Deleted)$binaryCounts\r?$" }
        & $CliBinary add $repo
        if ($LASTEXITCODE -ne 0) { throw 'Visual fixture registration failed' }
    }
    $process = Start-Process -FilePath $Binary -WorkingDirectory $fixtureHome -NoNewWindow -PassThru
    $handle = [IntPtr]::Zero
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        $handle = [WtsCapture]::Find([uint32]$process.Id)
        if ($handle -ne [IntPtr]::Zero) {
            $text = [WtsCapture]::Text($handle)
            if (Has-FixtureData $text) { break }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($handle -eq [IntPtr]::Zero -or -not (Has-FixtureData $text)) { throw 'Dashboard did not show all three real fixture counts' }
    if ($text -cmatch 'REPOSITORIES|WORKTREES|LINE CHANGES') { throw 'Dashboard still shows summary totals' }
    [IO.File]::WriteAllText((Join-Path $fixture 'visible-text.txt'), $text)
    $automation = @"
Add-Type -AssemblyName UIAutomationClient
`$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$($handle.ToInt64()))
`$items = `$root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
foreach (`$item in `$items) { `$item.Current.Name }
"@
    $names = (& "$env:WINDIR/System32/WindowsPowerShell/v1.0/powershell.exe" -NoProfile -NonInteractive -Command $automation | Out-String)
    if ($LASTEXITCODE -ne 0) { throw 'Native accessibility inspection failed' }
    [IO.File]::WriteAllText((Join-Path $fixture 'accessibility.txt'), $names)
    if (-not (Has-FixtureData $names)) { throw 'Dashboard data has no accessible names' }
    $bounds = New-Object WtsCapture+Rect
    if (-not [WtsCapture]::GetWindowRect($handle, [ref]$bounds)) { throw 'Window bounds unavailable' }
    if ($Height -gt 0) {
        $area = New-Object WtsCapture+Rect
        if (-not [WtsCapture]::SystemParametersInfo(0x0030, 0, [ref]$area, 0)) { throw 'Monitor work area unavailable' }
        $height = [Math]::Min($Height, $area.Bottom - $area.Top - 32)
        if (-not [WtsCapture]::SetWindowPos($handle, [IntPtr]::Zero, $area.Left + 16, $area.Top + 16, $bounds.Right - $bounds.Left, $height, 20)) { throw 'Capture layout resize failed' }
        if (-not [WtsCapture]::GetWindowRect($handle, [ref]$bounds)) { throw 'Resized window bounds unavailable' }
    }
    if ($Client) {
        if (-not $Screen) { throw 'Client capture requires screen capture' }
        $clientBounds = New-Object WtsCapture+Rect
        $origin = New-Object WtsCapture+Point
        if (-not [WtsCapture]::GetClientRect($handle, [ref]$clientBounds) -or -not [WtsCapture]::ClientToScreen($handle, [ref]$origin)) { throw 'Client capture bounds unavailable' }
        $bounds.Left = $origin.X
        $bounds.Top = $origin.Y
        $bounds.Right = $origin.X + $clientBounds.Right
        $bounds.Bottom = $origin.Y + $clientBounds.Bottom
    }
    $bitmap = [Drawing.Bitmap]::new($bounds.Right - $bounds.Left, $bounds.Bottom - $bounds.Top)
    if (-not [WtsCapture]::RedrawWindow($handle, [IntPtr]::Zero, [IntPtr]::Zero, 0x0181)) { throw 'Capture repaint failed' }
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    if ($Screen) {
        $graphics.CopyFromScreen($bounds.Left, $bounds.Top, 0, 0, $bitmap.Size)
    } else {
        $dc = $graphics.GetHdc()
        try {
            if (-not [WtsCapture]::PrintWindow($handle, $dc, 2)) { throw 'Native window capture failed' }
        } finally { $graphics.ReleaseHdc($dc) }
    }
    [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($Output)) | Out-Null
    $bitmap.Save($Output, [Drawing.Imaging.ImageFormat]::Png)
    Write-Output "Captured $Output"
    Write-Output "Real Git fixture and visible text at $fixture"
    if (-not [WtsCapture]::PostMessage($handle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) { throw 'Close failed' }
    if (-not $process.WaitForExit(10000)) { throw 'Captured window did not close' }
    if ($process.ExitCode -ne 0) { throw 'Captured app exited unsuccessfully' }
} finally {
    $env:HOME = $oldHome
    $env:USERPROFILE = $oldProfile
    if ($null -ne $process -and -not $process.HasExited) { $process.Kill($true); $process.WaitForExit() }
    if ($null -ne $graphics) { $graphics.Dispose() }
    if ($null -ne $bitmap) { $bitmap.Dispose() }
    if ($previousDpiContext -ne [IntPtr]::Zero) { [void][WtsCapture]::SetThreadDpiAwarenessContext($previousDpiContext) }
}
