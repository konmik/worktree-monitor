param(
    [string]$Binary = "$PSScriptRoot/../target/debug/wts-ui.exe",
    [string]$Output = "$env:LOCALAPPDATA/Temp/opencode/wts-dashboard.png",
    [switch]$Screen = $true,
    [int]$Height = 0,
    [switch]$Client,
    [switch]$ShowAll,
    [switch]$VerifyScrollbar,
    [int]$DirectoryCount = 0,
    [switch]$VerifyRefresh,
    [switch]$VerifySerialGit
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
    public struct GuiInfo { public uint Size, Flags; public IntPtr Active, Focus, Capture, MenuOwner, MoveSize, Caret; public Rect CaretRect; }
    public struct SCROLLINFO { public uint cbSize, fMask; public int nMin, nMax; public uint nPage; public int nPos, nTrackPos; }
    public delegate bool EnumProc(IntPtr handle, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr handle, out uint pid);
    [DllImport("user32.dll")] public static extern bool GetGUIThreadInfo(uint thread, ref GuiInfo info);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr handle);
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr parent, int id);
    [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr handle, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr handle, StringBuilder text, int count);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr SendMessageTimeout(IntPtr handle, uint message, UIntPtr wparam, StringBuilder text, uint flags, uint timeout, out UIntPtr result);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr handle, out Rect rect);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr handle, out Rect rect);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr handle, ref Point point);
    [DllImport("user32.dll")] public static extern bool GetScrollInfo(IntPtr handle, int bar, ref SCROLLINFO info);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out Point point);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint x, uint y, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr handle);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr handle, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] public static extern bool SystemParametersInfo(uint action, uint param, out Rect rect, uint flags);
    [DllImport("user32.dll")] public static extern bool RedrawWindow(IntPtr handle, IntPtr rect, IntPtr region, uint flags);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr handle, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr handle, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("dwmapi.dll")] public static extern int DwmFlush();
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
    foreach ($expected in $script:expectedRows) {
        $row = '(?m)^' + [regex]::Escape($expected.Branch) + '\r?\n' + $expected.Counts
        if ($Text -notmatch $row) { return $false }
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
        for ($index = 0; $index -lt $DirectoryCount; $index++) {
            [void][IO.Directory]::CreateDirectory((Join-Path $repo "folder-$index/source"))
        }
        $after = @($before | Select-Object -Skip $spec.Deleted) + @(1..$spec.Added | ForEach-Object { "new line $_" })
        [IO.File]::WriteAllLines((Join-Path $repo 'source.txt'), $after)
        if ($spec.Binaries -gt 0) { [IO.File]::WriteAllBytes((Join-Path $repo 'asset.bin'), [byte[]]@(0, 1, 2)) }
        $branch = (Invoke-FixtureGit $repo @('branch', '--show-current') | Out-String).Trim()
        $binaryCounts = if ($spec.Binaries -gt 0) { " · binaries: $($spec.Binaries)" } else { '' }
        $script:expectedRows += [pscustomobject]@{ Branch = $branch; Path = $repo; Added = $spec.Added; Counts = "S\s+\+$($spec.Added)/-$($spec.Deleted)$binaryCounts\r?$" }
        & $CliBinary add $repo
        if ($LASTEXITCODE -ne 0) { throw 'Visual fixture registration failed' }
    }
    $start = [Diagnostics.ProcessStartInfo]::new($Binary)
    $start.UseShellExecute = $false
    $start.WorkingDirectory = $fixtureHome
    $gitTrace = Join-Path $fixture 'git-trace.jsonl'
    if ($VerifySerialGit) { $start.Environment['GIT_TRACE2_EVENT'] = $gitTrace }
    $process = [Diagnostics.Process]::Start($start)
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
    if ($VerifyRefresh) {
        foreach ($expected in $script:expectedRows) {
            [IO.File]::AppendAllText((Join-Path $expected.Path 'source.txt'), "refresh line`n")
            $expected.Counts = $expected.Counts.Replace("+$($expected.Added)/", "+$($expected.Added + 1)/")
        }
        $deadline = [DateTime]::UtcNow.AddSeconds(30)
        do {
            $text = [WtsCapture]::Text($handle)
            if (Has-FixtureData $text) { break }
            Start-Sleep -Milliseconds 100
        } while ([DateTime]::UtcNow -lt $deadline)
        if (-not (Has-FixtureData $text)) { throw 'Dashboard did not refresh all three edited worktree counts' }
        Write-Output 'PASS. Initial counts and subsequent edits appear in the running dashboard.'
    }
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
    if ($ShowAll) {
        $toggle = [WtsCapture]::GetDlgItem($handle, 1004)
        if ($toggle -eq [IntPtr]::Zero) { throw 'Show all control is unavailable' }
        [void][WtsCapture]::SendMessageW($toggle, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero)
    }
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
        [void][WtsCapture]::DwmFlush()
        $graphics.CopyFromScreen($bounds.Left, $bounds.Top, 0, 0, $bitmap.Size)
    } else {
        $dc = $graphics.GetHdc()
        try {
            if (-not [WtsCapture]::PrintWindow($handle, $dc, 2)) { throw 'Native window capture failed' }
        } finally { $graphics.ReleaseHdc($dc) }
    }
    [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($Output)) | Out-Null
    $bitmap.Save($Output, [Drawing.Imaging.ImageFormat]::Png)
    if ($VerifyScrollbar) {
        $viewport = [WtsCapture]::GetDlgItem($handle, 1001)
        $scrollbar = [WtsCapture]::GetDlgItem($handle, 1003)
        if ($viewport -eq [IntPtr]::Zero) { throw 'Dashboard viewport control 1001 is missing' }
        if ($scrollbar -eq [IntPtr]::Zero) { throw 'Scrollbar control 1003 is missing' }
        if (-not [WtsCapture]::IsWindowVisible($scrollbar)) { throw 'Scrollbar control 1003 is not visible' }
        $windowBounds = New-Object WtsCapture+Rect
        $viewportBounds = New-Object WtsCapture+Rect
        $scrollbarBounds = New-Object WtsCapture+Rect
        if (-not [WtsCapture]::GetWindowRect($handle, [ref]$windowBounds)) { throw 'Scrollbar verification window bounds unavailable' }
        if (-not [WtsCapture]::GetWindowRect($viewport, [ref]$viewportBounds)) { throw 'Scrollbar verification viewport bounds unavailable' }
        if (-not [WtsCapture]::GetWindowRect($scrollbar, [ref]$scrollbarBounds)) { throw 'Scrollbar verification bounds unavailable' }
        $dpi = [Math]::Max(96, [int][WtsCapture]::GetDpiForWindow($handle))
        $scrollbarWidth = $scrollbarBounds.Right - $scrollbarBounds.Left
        $scrollbarLogicalWidth = $scrollbarWidth * 96.0 / $dpi
        if ($scrollbarBounds.Right -le $scrollbarBounds.Left -or $scrollbarBounds.Bottom -le $scrollbarBounds.Top) { throw 'Scrollbar has empty bounds' }
        if ($scrollbarBounds.Left -lt $viewportBounds.Left -or $scrollbarBounds.Right -gt $windowBounds.Right -or $scrollbarBounds.Top -lt $viewportBounds.Top -or $scrollbarBounds.Bottom -gt $viewportBounds.Bottom) { throw 'Scrollbar is outside the viewport gutter' }
        $rightGutter = $scrollbarBounds.Left - $viewportBounds.Right
        if ($rightGutter -lt 0) { throw 'Scrollbar overlaps the viewport' }
        $leftMargin = $viewportBounds.Left - $windowBounds.Left
        if ($leftMargin -lt 0) { throw 'Viewport extends past the window left edge' }
        if ($scrollbarLogicalWidth -gt 8) { throw "Scrollbar width is $scrollbarLogicalWidth logical pixels at $dpi DPI" }
        function Verify-ScrollbarPixels {
            param($Image, $CaptureBounds, $NativeScrollbarBounds, [string]$Label)
            $left = $NativeScrollbarBounds.Left - $CaptureBounds.Left
            $top = $NativeScrollbarBounds.Top - $CaptureBounds.Top
            $right = $NativeScrollbarBounds.Right - $CaptureBounds.Left
            $bottom = $NativeScrollbarBounds.Bottom - $CaptureBounds.Top
            if ($left -lt 0 -or $top -lt 0 -or $right -gt $Image.Width -or $bottom -gt $Image.Height) { throw "$Label scrollbar bounds are outside the captured bitmap" }
            $background = [Drawing.Color]::FromArgb(11, 16, 18).ToArgb()
            $border = [Drawing.Color]::FromArgb(40, 52, 58).ToArgb()
            $muted = [Drawing.Color]::FromArgb(132, 150, 156).ToArgb()
            $teal = [Drawing.Color]::FromArgb(84, 213, 197).ToArgb()
            $surface = [Drawing.Color]::FromArgb(23, 30, 34).ToArgb()
            $palette = @{}
            foreach ($color in @($background, $border, $muted, $teal, $surface)) { $palette[$color] = $true }
            $thumbPalette = @{}
            foreach ($color in @($border, $muted, $teal, $surface)) { $thumbPalette[$color] = $true }
            $hasBackground = $false
            $hasThumb = $false
            for ($y = $top; $y -lt $bottom; $y++) {
                for ($x = $left; $x -lt $right; $x++) {
                    $pixel = $Image.GetPixel($x, $y).ToArgb()
                    if (-not $palette.ContainsKey($pixel)) { throw "$Label scrollbar contains unexpected pixel color $pixel" }
                    if ($pixel -eq $background) { $hasBackground = $true }
                    if ($thumbPalette.ContainsKey($pixel)) { $hasThumb = $true }
                }
            }
            if (-not $hasBackground) { throw "$Label scrollbar has no background pixels" }
            if (-not $hasThumb) { throw "$Label scrollbar has no visible thumb color" }
            $center = [int](($left + $right) / 2)
            $runTop = $top
            $bestTop = $top
            $bestBottom = $top
            for ($y = $top; $y -le $bottom; $y++) {
                $colored = $false
                if ($y -lt $bottom) {
                    $pixel = $Image.GetPixel($center, $y).ToArgb()
                    $colored = $pixel -eq $muted -or $pixel -eq $teal
                }
                if (-not $colored) {
                    if ($y - $runTop -gt $bestBottom - $bestTop) { $bestTop = $runTop; $bestBottom = $y }
                    $runTop = $y + 1
                }
            }
            if ($bestBottom -le $bestTop) { throw "$Label scrollbar has no visible thumb at its center" }
            return @{ Top = $bestTop + $CaptureBounds.Top; Bottom = $bestBottom + $CaptureBounds.Top }
        }
        $captured = [Drawing.Bitmap]::FromFile($Output)
        try {
            $initialThumb = Verify-ScrollbarPixels $captured $bounds $scrollbarBounds 'Initial'
        } finally {
            $captured.Dispose()
        }
        $thumbTop = $initialThumb.Top
        $thumbBottom = $initialThumb.Bottom
        $trackTop = $scrollbarBounds.Top
        $trackBottom = $scrollbarBounds.Bottom
        $beforeInfo = New-Object WtsCapture+SCROLLINFO
        $beforeInfo.cbSize = [uint32][Runtime.InteropServices.Marshal]::SizeOf([type][WtsCapture+SCROLLINFO])
        $beforeInfo.fMask = 0x17
        if (-not [WtsCapture]::GetScrollInfo($scrollbar, 2, [ref]$beforeInfo)) { throw 'Native scrollbar state unavailable' }
        $draggingOutput = [IO.Path]::Combine([IO.Path]::GetDirectoryName($Output), [IO.Path]::GetFileNameWithoutExtension($Output) + '-dragging.png')
        $savedCursor = New-Object WtsCapture+Point
        if (-not [WtsCapture]::GetCursorPos([ref]$savedCursor)) { throw 'Cursor position unavailable' }
        $mouseDown = $false
        $draggingBitmap = $null
        $draggingGraphics = $null
        try {
            $thumbPoint = New-Object WtsCapture+Point
            $thumbPoint.X = [int](($scrollbarBounds.Left + $scrollbarBounds.Right) / 2)
            $thumbPoint.Y = [int](($thumbTop + $thumbBottom) / 2)
            if (-not [WtsCapture]::SetCursorPos($thumbPoint.X, $thumbPoint.Y) -or [WtsCapture]::WindowFromPoint($thumbPoint) -ne $scrollbar) { throw 'Native scrollbar thumb is not the mouse target' }
            [WtsCapture]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
            $mouseDown = $true
            [uint32]$owner = 0
            $thread = [WtsCapture]::GetWindowThreadProcessId($scrollbar, [ref]$owner)
            $deadline = [DateTime]::UtcNow.AddSeconds(3)
            do {
                $gui = New-Object WtsCapture+GuiInfo
                $gui.Size = [uint32][Runtime.InteropServices.Marshal]::SizeOf([type][WtsCapture+GuiInfo])
                if ([WtsCapture]::GetGUIThreadInfo($thread, [ref]$gui) -and $gui.Capture -eq $scrollbar) { break }
                Start-Sleep -Milliseconds 20
            } while ([DateTime]::UtcNow -lt $deadline)
            if ($gui.Capture -ne $scrollbar) { throw 'Native scrollbar did not capture the mouse for dragging' }
            $dragPoint = New-Object WtsCapture+Point
            $dragPoint.X = $thumbPoint.X
            $dragPoint.Y = [Math]::Min($trackBottom - 1, $thumbPoint.Y + 36)
            $dragPoint.Y = [Math]::Max($trackTop, $dragPoint.Y)
            if ($dragPoint.Y -le $thumbPoint.Y) { throw 'Native scrollbar thumb has no downward drag room' }
            if (-not [WtsCapture]::SetCursorPos($dragPoint.X, $dragPoint.Y)) { throw 'Native scrollbar drag failed' }
            $changed = $false
            $deadline = [DateTime]::UtcNow.AddSeconds(3)
            do {
                $duringInfo = New-Object WtsCapture+SCROLLINFO
                $duringInfo.cbSize = $beforeInfo.cbSize
                $duringInfo.fMask = 0x17
                if (-not [WtsCapture]::GetScrollInfo($scrollbar, 2, [ref]$duringInfo)) { throw 'Native scrollbar state polling failed' }
                if ($duringInfo.nPos -gt $beforeInfo.nPos) { $changed = $true; break }
                Start-Sleep -Milliseconds 20
            } while ([DateTime]::UtcNow -lt $deadline)
            if (-not $changed) { throw 'Native scrollbar thumb drag did not change the position' }
            $duringWindowBounds = New-Object WtsCapture+Rect
            if (-not [WtsCapture]::GetWindowRect($handle, [ref]$duringWindowBounds)) { throw 'Scrollbar dragging window bounds unavailable' }
            if ($duringWindowBounds.Left -ne $windowBounds.Left -or $duringWindowBounds.Top -ne $windowBounds.Top -or $duringWindowBounds.Right -ne $windowBounds.Right -or $duringWindowBounds.Bottom -ne $windowBounds.Bottom) { throw 'Scrollbar dragging changed the window bounds' }
            Start-Sleep -Milliseconds 50
            $draggingBitmap = [Drawing.Bitmap]::new($bounds.Right - $bounds.Left, $bounds.Bottom - $bounds.Top)
            $draggingGraphics = [Drawing.Graphics]::FromImage($draggingBitmap)
            if ($Screen) {
                [void][WtsCapture]::DwmFlush()
                $draggingGraphics.CopyFromScreen($bounds.Left, $bounds.Top, 0, 0, $draggingBitmap.Size)
            } else {
                $draggingDc = $draggingGraphics.GetHdc()
                try {
                    if (-not [WtsCapture]::PrintWindow($handle, $draggingDc, 2)) { throw 'Native dragging window capture failed' }
                } finally { $draggingGraphics.ReleaseHdc($draggingDc) }
            }
            $draggingBitmap.Save($draggingOutput, [Drawing.Imaging.ImageFormat]::Png)
            $duringThumb = Verify-ScrollbarPixels $draggingBitmap $bounds $scrollbarBounds 'Dragging'
            $gui = New-Object WtsCapture+GuiInfo
            $gui.Size = [uint32][Runtime.InteropServices.Marshal]::SizeOf([type][WtsCapture+GuiInfo])
            if (-not [WtsCapture]::GetGUIThreadInfo($thread, [ref]$gui) -or $gui.Capture -ne $scrollbar) { throw 'Scrollbar lost capture before the dragging screenshot' }
            if ($duringThumb.Top -le $initialThumb.Top) { throw 'The visible scrollbar thumb did not move down while dragging' }
        } finally {
            if ($mouseDown) { [WtsCapture]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero) }
            if (-not [WtsCapture]::SetCursorPos($savedCursor.X, $savedCursor.Y)) { throw 'Cursor restoration failed' }
            if ($null -ne $draggingGraphics) { $draggingGraphics.Dispose() }
            if ($null -ne $draggingBitmap) { $draggingBitmap.Dispose() }
        }
        Write-Output 'PASS. Slim themed scrollbar stays in the gutter and scrolls while dragged.'
    }
    Write-Output "Captured $Output"
    Write-Output "Real Git fixture and visible text at $fixture"
    if (-not [WtsCapture]::PostMessage($handle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) { throw 'Close failed' }
    if (-not $process.WaitForExit(10000)) { throw 'Captured window did not close' }
    if ($process.ExitCode -ne 0) { throw 'Captured app exited unsuccessfully' }
    if ($VerifySerialGit) {
        $activeGit = [Collections.Generic.HashSet[string]]::new()
        $maximumGit = 0
        $countScans = 0
        foreach ($line in [IO.File]::ReadLines($gitTrace)) {
            $event = $line | ConvertFrom-Json
            if ($event.event -eq 'start' -and $event.argv -contains 'core.fsmonitor=false') {
                [void]$activeGit.Add($event.sid)
                $maximumGit = [Math]::Max($maximumGit, $activeGit.Count)
                if ($event.argv -contains 'diff' -and $event.argv -contains '--numstat') { $countScans++ }
            } elseif ($event.event -eq 'exit') {
                [void]$activeGit.Remove($event.sid)
            }
        }
        if ($maximumGit -ne 1 -or $countScans -lt 3) { throw 'Git checks did not run sequentially or count scans are missing' }
        Write-Output 'PASS. Git trace confirms sequential worktree checks.'
    }
} finally {
    $env:HOME = $oldHome
    $env:USERPROFILE = $oldProfile
    if ($null -ne $process -and -not $process.HasExited) { $process.Kill($true); $process.WaitForExit() }
    if ($null -ne $graphics) { $graphics.Dispose() }
    if ($null -ne $bitmap) { $bitmap.Dispose() }
    if ($previousDpiContext -ne [IntPtr]::Zero) { [void][WtsCapture]::SetThreadDpiAwarenessContext($previousDpiContext) }
}
