param(
    [string]$Binary = "$PSScriptRoot/../target/debug/wts-ui.exe",
    [string]$Output = "$env:LOCALAPPDATA/Temp/opencode/wts-features-$([Guid]::NewGuid().ToString('N'))"
)

$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path $Binary).Path
$CliBinary = (Resolve-Path (Join-Path (Split-Path -Parent $Binary) 'wts.exe')).Path
$Output = [IO.Path]::GetFullPath($Output)
$repo = Join-Path $Output 'repo'
$fixtureHome = Join-Path $Output 'home'
New-Item -ItemType Directory -Path $repo, $fixtureHome -Force | Out-Null
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Text;
public static class WtsFeatureProbe {
    public struct Rect { public int Left, Top, Right, Bottom; }
    public struct Size { public int Width, Height; }
    public struct Point { public int X, Y; }
    public delegate bool EnumProc(IntPtr window, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr window, StringBuilder text, int count);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr window, StringBuilder text, int count);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern bool EnableWindow(IntPtr window, bool enabled);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out Point point);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsZoomed(IntPtr window);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr window, int command);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
    [DllImport("user32.dll", EntryPoint="SystemParametersInfoW")] public static extern bool SystemParametersInfo(uint action, uint parameter, out Rect rect, uint flags);
    [DllImport("user32.dll")] public static extern bool RedrawWindow(IntPtr window, IntPtr rect, IntPtr region, uint flags);
    [DllImport("user32.dll")] public static extern IntPtr GetDC(IntPtr window);
    [DllImport("user32.dll")] public static extern int ReleaseDC(IntPtr window, IntPtr dc);
    [DllImport("gdi32.dll")] public static extern IntPtr SelectObject(IntPtr dc, IntPtr handle);
    [DllImport("gdi32.dll", CharSet=CharSet.Unicode)] public static extern int GetTextFaceW(IntPtr dc, int count, StringBuilder face);
    [DllImport("gdi32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr CreateFontW(int height, int width, int escapement, int orientation, int weight, uint italic, uint underline, uint strikeout, uint charset, uint outputPrecision, uint clipPrecision, uint quality, uint pitchAndFamily, string face);
    [DllImport("gdi32.dll")] public static extern bool DeleteObject(IntPtr handle);
    [DllImport("gdi32.dll")] public static extern int SetBkMode(IntPtr dc, int mode);
    [DllImport("gdi32.dll")] public static extern uint SetTextColor(IntPtr dc, uint color);
    [DllImport("gdi32.dll")] public static extern IntPtr CreateSolidBrush(uint color);
    [DllImport("user32.dll")] public static extern int FillRect(IntPtr dc, ref Rect rect, IntPtr brush);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int DrawTextW(IntPtr dc, string text, int count, ref Rect rect, uint flags);
    [DllImport("gdi32.dll", CharSet=CharSet.Unicode)] public static extern bool GetTextExtentPoint32W(IntPtr dc, string text, int length, out Size size);
    [DllImport("dwmapi.dll")] public static extern int DwmFlush();
    public static string Text(IntPtr window) {
        var text = new StringBuilder(65536);
        GetWindowText(window, text, text.Capacity);
        return text.ToString();
    }
    public static IntPtr Find(uint pid) {
        var found = IntPtr.Zero;
        EnumWindows((window, data) => {
            uint owner;
            GetWindowThreadProcessId(window, out owner);
            if (owner != pid || !IsWindowVisible(window) || !Text(window).StartsWith("WTS")) return true;
            found = window;
            return false;
        }, IntPtr.Zero);
        return found;
    }
    public static IntPtr FindCount(IntPtr parent, string expected) {
        var found = IntPtr.Zero;
        EnumChildWindows(parent, (window, data) => {
            if (Text(window) != expected) return true;
            found = window;
            return false;
        }, IntPtr.Zero);
        return found;
    }
    public static int Measure(IntPtr window, string text) {
        var dc = GetDC(window);
        if (dc == IntPtr.Zero) throw new InvalidOperationException("Text DC unavailable");
        var font = TerminalFont(window, dc);
        var old = SelectObject(dc, font);
        try {
            Size size;
            if (!GetTextExtentPoint32W(dc, text, text.Length, out size)) throw new InvalidOperationException("Text measurement failed");
            return size.Width;
        } finally {
            SelectObject(dc, old);
            DeleteObject(font);
            ReleaseDC(window, dc);
        }
    }
    public static IntPtr TerminalFont(IntPtr window, IntPtr dc) {
        foreach (var face in new[] { "Berkeley Mono", "Berkeley Mono Variable", "Consolas", "Cascadia Mono", "Courier New" }) {
            var font = CreateFontW(-12 * (int)GetDpiForWindow(window) / 96, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 49, face);
            var old = SelectObject(dc, font);
            var actual = new StringBuilder(64);
            GetTextFaceW(dc, actual.Capacity, actual);
            SelectObject(dc, old);
            if (actual.ToString().Equals(face, StringComparison.OrdinalIgnoreCase)) return font;
            DeleteObject(font);
        }
        throw new InvalidOperationException("Terminal font unavailable");
    }
    public static string AssertFont(IntPtr root, IntPtr sample, Bitmap image, int left, int top, int secondLineOffset = 0) {
        var font = SendMessage(sample, 0x0031, IntPtr.Zero, IntPtr.Zero);
        if (font == IntPtr.Zero) throw new InvalidOperationException("Control font unavailable");
        EnumChildWindows(root, (window, data) => {
            var type = new StringBuilder(64);
            GetClassName(window, type, type.Capacity);
            if (IsWindowVisible(window) && (type.ToString() == "STATIC" || type.ToString() == "Button") &&
                SendMessage(window, 0x0031, IntPtr.Zero, IntPtr.Zero) != font)
                throw new InvalidOperationException("Visible text does not share one font");
            return true;
        }, IntPtr.Zero);
        var dc = GetDC(sample);
        var localFont = TerminalFont(sample, dc);
        var old = SelectObject(dc, localFont);
        try {
            var face = new StringBuilder(64);
            if (GetTextFaceW(dc, face.Capacity, face) == 0) throw new InvalidOperationException("Actual font unavailable");
            if (Measure(sample, "iiii") != Measure(sample, "WWWW")) throw new InvalidOperationException("Font is not monospace");
            var heading = Text(sample).Split('\n')[0];
            int advance = (int)Math.Round(12 * 1.2 * GetDpiForWindow(sample) / 96, MidpointRounding.AwayFromZero);
            for (int line = 0; line < 2; line++) {
                var label = line == 0 ? heading : "S ";
                label = label.Substring(0, Math.Min(8, label.Length));
                var width = Measure(sample, label);
                var height = advance;
                using (var expected = new Bitmap(width, height)) {
                    using (var graphics = Graphics.FromImage(expected)) {
                        var target = graphics.GetHdc();
                        var previous = SelectObject(target, localFont);
                        var background = CreateSolidBrush(11u | (16u << 8) | (18u << 16));
                        try {
                            var rect = new Rect { Right = width, Bottom = height };
                            FillRect(target, ref rect, background);
                            SetBkMode(target, 1);
                            SetTextColor(target, line == 0 ? 232u | (239u << 8) | (235u << 16) : 132u | (150u << 8) | (156u << 16));
                            DrawTextW(target, label, label.Length, ref rect, 0x0820);
                        } finally { SelectObject(target, previous); DeleteObject(background); graphics.ReleaseHdc(target); }
                    }
                    int ink = 0;
                    for (int x = 0; x < width; x++) {
                        for (int y = 0; y < height; y++) {
                            var actual = image.GetPixel(left + x, top + line * advance + (line == 1 ? secondLineOffset : 0) + y);
                            var wanted = expected.GetPixel(x, y);
                            if (wanted.R != 11 || wanted.G != 16 || wanted.B != 18) ink++;
                            if (actual.R != wanted.R || actual.G != wanted.G || actual.B != wanted.B)
                                throw new InvalidOperationException("Rendered application text differs from expected font or shared line origin on line " + line);
                        }
                    }
                    if (ink == 0) throw new InvalidOperationException("Rendered font sample is empty");
                }
            }
            return face.ToString();
        } finally { SelectObject(dc, old); DeleteObject(localFont); ReleaseDC(sample, dc); }
    }
    public static int[] Pixels(Bitmap image, int left, int right, int top, int bottom) {
        var counts = new int[3];
        for (int x = left; x < right; x++) {
            for (int y = top; y < bottom; y++) {
                var pixel = image.GetPixel(x, y);
                if (Math.Abs(pixel.R - 194) < 24 && Math.Abs(pixel.G - 249) < 24 && Math.Abs(pixel.B - 112) < 24) counts[0]++;
                if (Math.Abs(pixel.R - 255) < 24 && Math.Abs(pixel.G - 131) < 24 && Math.Abs(pixel.B - 137) < 24) counts[1]++;
                if (Math.Abs(pixel.R - 132) < 24 && Math.Abs(pixel.G - 150) < 24 && Math.Abs(pixel.B - 156) < 24) counts[2]++;
            }
        }
        return counts;
    }
    public static void ButtonStates(IntPtr button, string output) {
        Rect rect;
        if (!GetWindowRect(button, out rect)) throw new InvalidOperationException("Button bounds unavailable");
        Point cursor;
        GetCursorPos(out cursor);
        try {
            foreach (uint state in new uint[] { 0, 0x0040, 0x0001, 0x0004 }) {
                SetCursorPos(state == 0x0040 ? (rect.Left + rect.Right) / 2 : rect.Left - 10,
                    state == 0x0040 ? (rect.Top + rect.Bottom) / 2 : rect.Top - 10);
                EnableWindow(button, state != 4);
                SendMessage(button, 0x00f3, new IntPtr(state == 1 ? 1 : 0), IntPtr.Zero);
                System.Threading.Thread.Sleep(100);
                RedrawWindow(button, IntPtr.Zero, IntPtr.Zero, 0x0181);
                DwmFlush();
                using (var image = new Bitmap(rect.Right - rect.Left, rect.Bottom - rect.Top)) {
                    using (var graphics = Graphics.FromImage(image)) {
                        graphics.CopyFromScreen(rect.Left, rect.Top, 0, 0, image.Size);
                    }
                    image.Save(System.IO.Path.Combine(output, "button-state-" + state + ".png"));
                    var surface = Color.FromArgb(11, 16, 18).ToArgb();
                    if (image.GetPixel(0, 0).ToArgb() != surface || image.GetPixel(image.Width - 1, 0).ToArgb() != surface ||
                        image.GetPixel(0, image.Height - 1).ToArgb() != surface || image.GetPixel(image.Width - 1, image.Height - 1).ToArgb() != surface)
                        throw new InvalidOperationException("Button corners differ from terminal background in owner-draw state " + state);
                    var body = surface;
                    if (image.GetPixel(image.Width / 2, image.Height - 5).ToArgb() != body)
                        throw new InvalidOperationException("Button body color is wrong in owner-draw state " + state);
                }
            }
        } finally {
            EnableWindow(button, true);
            SendMessage(button, 0x00f3, IntPtr.Zero, IntPtr.Zero);
            SetCursorPos(cursor.X, cursor.Y);
        }
    }
}
'@ -ReferencedAssemblies System.Drawing.Common, System.Drawing.Primitives, System.Private.Windows.GdiPlus, System.Private.Windows.Core, System.Threading.Thread

function Wait-For([scriptblock]$Condition, [string]$Label) {
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        if (& $Condition) { return }
        Start-Sleep -Milliseconds 75
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for $Label"
}

function Run-Git([string[]]$Arguments) {
    & git -c core.fsmonitor=false -c core.autocrlf=false -c commit.gpgsign=false -c "core.hooksPath=$Output" -C $repo @Arguments
    if ($LASTEXITCODE -ne 0) { throw 'Fixture Git command failed' }
}

function Write-Lines([string]$Path, [string[]]$Lines) {
    [IO.File]::WriteAllText((Join-Path $repo $Path), ($Lines -join "`n") + "`n", [Text.UTF8Encoding]::new($false))
}

function Bounds([IntPtr]$Window) {
    $rect = [WtsFeatureProbe+Rect]::new()
    if (-not [WtsFeatureProbe]::GetWindowRect($Window, [ref]$rect)) { throw 'Window bounds unavailable' }
    return $rect
}

function Same-Bounds($Left, $Right) {
    return $Left.Left -eq $Right.Left -and $Left.Top -eq $Right.Top -and $Left.Right -eq $Right.Right -and $Left.Bottom -eq $Right.Bottom
}

function Bounds-Record($Rect) {
    return [ordered]@{ left = $Rect.Left; top = $Rect.Top; right = $Rect.Right; bottom = $Rect.Bottom }
}

function Fits($Rect) {
    return $Rect.Left -ge $area.Left -and $Rect.Top -ge $area.Top -and $Rect.Right -le $area.Right -and $Rect.Bottom -le $area.Bottom
}

function Start-Window([string]$Name) {
    $foreground = [WtsFeatureProbe]::GetForegroundWindow()
    $script:process = Start-Process -FilePath $Binary -WorkingDirectory $repo -NoNewWindow -PassThru
    $script:window = [IntPtr]::Zero
    Wait-For {
        if ($script:process.HasExited) { throw 'Feature window exited before showing counts' }
        $script:window = [WtsFeatureProbe]::Find([uint32]$script:process.Id)
        if ($script:window -eq [IntPtr]::Zero) { return $false }
        $script:card = [WtsFeatureProbe]::FindCount($script:window, $expectedCaption)
        return $script:window -ne [IntPtr]::Zero -and $script:card -ne [IntPtr]::Zero
    } "$Name window and exact S/T/X caption"
    if ([WtsFeatureProbe]::IsIconic($script:window)) { throw 'Startup restored a minimized window' }
    if ($foreground -ne [IntPtr]::Zero -and [WtsFeatureProbe]::GetForegroundWindow() -ne $foreground) { throw "$Name startup stole focus" }
    $proof.runs += [ordered]@{ name = $Name; pid = $script:process.Id; caption = [WtsFeatureProbe]::Text($script:card) }
}

function Close-Window {
    if (-not [WtsFeatureProbe]::PostMessage($script:window, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) { throw 'Window close failed' }
    if (-not $script:process.WaitForExit(10000) -or $script:process.ExitCode -ne 0) { throw 'Window did not close cleanly' }
    $script:process.Dispose()
    $script:process = $null
}

function Assert-Restored([string]$Name) {
    $actual = Bounds $script:window
    if (-not (Same-Bounds $target $actual)) { throw "$Name did not restore the saved bounds" }
    $proof.checks += [ordered]@{ name = $Name; restored = $true; actual = Bounds-Record $actual }
}

function Assert-Fallback([string]$Name) {
    $actual = Bounds $script:window
    if (-not (Fits $actual) -or (Same-Bounds $target $actual)) { throw "$Name did not use a fitting default window" }
    $proof.checks += [ordered]@{ name = $Name; fallback_fits = $true; actual = Bounds-Record $actual }
}

function Write-Placement($Value) {
    $Value | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $placementPath -Encoding utf8NoBOM
}

function Assert-Colors {
    $outer = Bounds $script:window
    $card = Bounds $script:card
    $dpi = [WtsFeatureProbe]::GetDpiForWindow($script:card)
    $x = $card.Left - $outer.Left + [int][Math]::Floor(14 * $dpi / 96)
    $top = $card.Top - $outer.Top + [int][Math]::Floor(8 * $dpi / 96) + [int][Math]::Round(12 * 1.2 * $dpi / 96)
    $bottom = $card.Bottom - $outer.Top - [int][Math]::Floor(8 * $dpi / 96)
    $image = [Drawing.Bitmap]::new($outer.Right - $outer.Left, $outer.Bottom - $outer.Top)
    $graphics = [Drawing.Graphics]::FromImage($image)
    try {
        [void][WtsFeatureProbe]::RedrawWindow($script:window, [IntPtr]::Zero, [IntPtr]::Zero, 0x0181)
        [void][WtsFeatureProbe]::DwmFlush()
        $graphics.CopyFromScreen($outer.Left, $outer.Top, 0, 0, $image.Size)
        $image.Save((Join-Path $Output 'counts.png'))
        $button = [WtsFeatureProbe]::FindCount($script:card, 'Hide')
        if ($button -eq [IntPtr]::Zero) { throw 'Hide button unavailable' }
        $buttonBounds = Bounds $button
        $corner = $image.GetPixel($buttonBounds.Left - $outer.Left, $buttonBounds.Top - $outer.Top)
        if ($corner.R -ne 11 -or $corner.G -ne 16 -or $corner.B -ne 18) { throw "Hide corner differs from terminal background" }
        $headingTop = $card.Top - $outer.Top + [int][Math]::Floor(8 * $dpi / 96)
        $proof.rendered_font = [WtsFeatureProbe]::AssertFont($script:window, $script:card, $image, $x, $headingTop)
        $rejectedShift = $false
        try { [void][WtsFeatureProbe]::AssertFont($script:window, $script:card, $image, $x, $headingTop, 1) }
        catch {
            if ($_.Exception.Message -notmatch 'shared line origin on line 1') { throw }
            $rejectedShift = $true
        }
        if (-not $rejectedShift) { throw 'Text position proof accepted a one-pixel second-line shift' }
        $proof.line_origin_shift_rejected = $true
        [WtsFeatureProbe]::ButtonStates($button, $Output)
        $proof.button_corners_match_card = $true
        $proof.button_states = @('normal', 'hover', 'pressed', 'disabled')
        $proof.colors = @()
        foreach ($part in @('S +4/-1', 'T +5/-1', 'X +6/-1')) {
            $width = [WtsFeatureProbe]::Measure($script:card, $part)
            $pixels = [WtsFeatureProbe]::Pixels($image, $x, $x + $width, $top, $bottom)
            $proof.colors += [ordered]@{ text = $part; left = $x; width = $width; top = $top; bottom = $bottom; green = $pixels[0]; red = $pixels[1]; muted = $pixels[2] }
            if ($part.StartsWith('S')) {
                if ($pixels[0] -eq 0 -or $pixels[1] -eq 0) { throw 'Source additions and deletions are not colored' }
            } elseif ($pixels[0] -ne 0 -or $pixels[1] -ne 0 -or $pixels[2] -eq 0) { throw "$part is not muted" }
            $x += $width + [WtsFeatureProbe]::Measure($script:card, ' · ')
        }
    } finally { $graphics.Dispose(); $image.Dispose() }
}

$names = @('HOME', 'USERPROFILE', 'GIT_DIR', 'GIT_WORK_TREE', 'GIT_COMMON_DIR', 'GIT_INDEX_FILE', 'GIT_CONFIG_GLOBAL', 'GIT_CONFIG_NOSYSTEM')
$previous = @{}
foreach ($name in $names) { $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
$dpiContext = [WtsFeatureProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
$script:process = $null
$proof = [ordered]@{ result = 'FAIL'; binary = $Binary; sha256 = (Get-FileHash $Binary).Hash; checks = @(); runs = @() }
try {
    $env:USERPROFILE = $fixtureHome
    $env:HOME = $fixtureHome
    $env:GIT_CONFIG_GLOBAL = Join-Path $Output 'empty-gitconfig'
    $env:GIT_CONFIG_NOSYSTEM = '1'
    foreach ($name in @('GIT_DIR', 'GIT_WORK_TREE', 'GIT_COMMON_DIR', 'GIT_INDEX_FILE')) { Remove-Item "Env:$name" -ErrorAction SilentlyContinue }
    New-Item -ItemType Directory -Path (Join-Path $repo 'tests'), (Join-Path $repo 'scripts') | Out-Null
    Run-Git @('init', '--quiet', '--initial-branch=km/wna-2067-document-the-agreement-and-usage')
    foreach ($path in @('source.txt', 'tests/unit.test.js', 'scripts/tool.ps1')) { Write-Lines $path @('keep', 'old') }
    Run-Git @('add', '.')
    Run-Git @('-c', 'user.name=Wts Test', '-c', 'user.email=wts@example.invalid', 'commit', '--quiet', '-m', 'Fixture')
    foreach ($path in @('source.txt', 'tests/unit.test.js', 'scripts/tool.ps1')) { Write-Lines $path @('keep', 'staged') }
    Run-Git @('add', '.')
    foreach ($path in @('source.txt', 'tests/unit.test.js', 'scripts/tool.ps1')) { Write-Lines $path @('keep', 'staged', 'unstaged') }
    Write-Lines 'new-source.txt' @('new1', 'new2')
    Write-Lines 'tests/new.test.js' @('new1', 'new2', 'new3')
    Write-Lines 'scripts/new.ps1' @('new1', 'new2', 'new3', 'new4')
    & $CliBinary add $repo
    if ($LASTEXITCODE -ne 0) { throw 'Fixture registration failed' }
    $expectedCaption = "km/wna-2067-document-the-agreement-and-usage`nS +4/-1 · T +5/-1 · X +6/-1"
    $placementPath = Join-Path $fixtureHome '.config/wts/config.placement.json'
    $area = [WtsFeatureProbe+Rect]::new()
    if (-not [WtsFeatureProbe]::SystemParametersInfo(0x0030, 0, [ref]$area, 0)) { throw 'Work area unavailable' }
    $target = [WtsFeatureProbe+Rect]::new()
    $target.Left = $area.Left + 37
    $target.Top = $area.Top + 43
    $target.Right = $target.Left + 760
    $target.Bottom = $target.Top + 540
    if (-not (Fits $target)) { throw 'The fixture window cannot fit on this screen' }
    $proof.work_area = Bounds-Record $area
    $proof.expected_bounds = Bounds-Record $target
    Start-Window 'initial'
    if (-not [WtsFeatureProbe]::SetWindowPos($script:window, [IntPtr]::Zero, $target.Left, $target.Top, 760, 540, 0x14)) { throw 'Fixture resize failed' }
    Wait-For { Same-Bounds $target (Bounds $script:window) } 'fixture bounds'
    [void][WtsFeatureProbe]::SendMessage($script:window, 0x0232, [IntPtr]::Zero, [IntPtr]::Zero)
    $savedMove = Get-Content $placementPath -Raw | ConvertFrom-Json
    if (-not (Same-Bounds $target $savedMove.normal)) { throw 'Completed move did not save its bounds' }
    Assert-Colors
    Close-Window
    Start-Window 'normal restart'
    Assert-Restored 'normal restart'
    [void][WtsFeatureProbe]::ShowWindow($script:window, 6)
    Wait-For { [WtsFeatureProbe]::IsIconic($script:window) } 'minimization'
    Close-Window
    Start-Window 'minimized restart'
    Assert-Restored 'minimized restart'
    Close-Window
    $original = Get-Content $placementPath -Raw
    $changed = $original | ConvertFrom-Json
    $changed.monitor.full_bounds.right++
    Write-Placement $changed
    Start-Window 'changed screen'
    Assert-Fallback 'changed screen'
    Close-Window
    $changed = $original | ConvertFrom-Json
    $changed.monitor.work_area.right--
    Write-Placement $changed
    Start-Window 'changed work area'
    Assert-Restored 'changed work area'
    Close-Window
    $changed = $original | ConvertFrom-Json
    $changed.normal.left = $area.Right + 20
    $changed.normal.right = $changed.normal.left + 760
    Write-Placement $changed
    Start-Window 'off-screen bounds'
    Assert-Fallback 'off-screen bounds'
    Close-Window
    Write-Placement ($original | ConvertFrom-Json)
    Start-Window 'maximize'
    [void][WtsFeatureProbe]::ShowWindow($script:window, 3)
    Wait-For { [WtsFeatureProbe]::IsZoomed($script:window) } 'maximization'
    if (-not (Same-Bounds $area (Bounds $script:window))) { throw 'Maximized frameless window does not fit the work area' }
    Close-Window
    Start-Window 'maximized restart'
    if (-not [WtsFeatureProbe]::IsZoomed($script:window)) { throw 'Maximized state was lost' }
    $proof.checks += [ordered]@{ name = 'maximized restart'; maximized = [WtsFeatureProbe]::IsZoomed($script:window) }
    [void][WtsFeatureProbe]::ShowWindow($script:window, 6)
    Wait-For { [WtsFeatureProbe]::IsIconic($script:window) } 'maximized minimization'
    Close-Window
    Start-Window 'maximized then minimized restart'
    if (-not [WtsFeatureProbe]::IsZoomed($script:window)) { throw 'Minimizing lost the maximized state' }
    $proof.checks += [ordered]@{ name = 'maximized then minimized restart'; maximized = [WtsFeatureProbe]::IsZoomed($script:window) }
    [void][WtsFeatureProbe]::ShowWindow($script:window, 9)
    Assert-Restored 'normal bounds after maximization'
    Close-Window
    $proof.result = 'PASS'
    Write-Host "PASS. Counts, source-only colors, and window placement. Evidence at $Output"
} catch { $proof.failure = $_.Exception.Message; throw } finally {
    if ($null -ne $script:process) {
        [void][WtsFeatureProbe]::PostMessage($script:window, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
        if (-not $script:process.WaitForExit(10000)) { $script:process.Kill($true); $script:process.WaitForExit() }
        $script:process.Dispose()
    }
    $proof | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $Output 'proof.json') -Encoding utf8NoBOM
    foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process') }
    [void][WtsFeatureProbe]::SetThreadDpiAwarenessContext($dpiContext)
}
