param(
    [string]$Binary = "$PSScriptRoot/../target/debug/wtm-gui.exe",
    [string]$Output = "$PSScriptRoot/../target/verification/smoke-features-$([Guid]::NewGuid().ToString('N'))"
)

. (Join-Path $PSScriptRoot 'verification-directories.ps1')
try {
$verificationFixture = New-VerificationDirectory 'smoke-features'
$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path $Binary).Path
$CliBinary = (Resolve-Path (Join-Path (Split-Path -Parent $Binary) 'wtm.exe')).Path
$Output = [IO.Path]::GetFullPath($Output)
New-Item -ItemType Directory -Path $Output -Force | Out-Null
$repo = Join-Path $verificationFixture 'repo'
$fixtureHome = Join-Path $verificationFixture 'home'
New-Item -ItemType Directory -Path $repo, $fixtureHome -Force | Out-Null
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition (@'
using System;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Text;
public static class WtmFeatureProbe {
    public struct Rect { public int Left, Top, Right, Bottom; }
    public struct Size { public int Width, Height; }
    public struct Point { public int X, Y; }
    public struct GuiInfo { public uint Size, Flags; public IntPtr Active, Focus, Capture, MenuOwner, MoveSize, Caret; public Rect CaretRect; }
    public struct MouseInput { public int X, Y; public uint Data, Flags, Time; public UIntPtr Extra; }
    public struct Input { public uint Type; public MouseInput Mouse; }
    public delegate bool EnumProc(IntPtr window, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr window, StringBuilder text, int count);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr window, StringBuilder text, int count);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr window, int id);
    [DllImport("user32.dll")] public static extern bool GetGUIThreadInfo(uint thread, ref GuiInfo info);
    [DllImport("user32.dll")] public static extern uint SendInput(uint count, Input[] inputs, int size);
    [DllImport("user32.dll")] public static extern int GetSystemMetrics(int index);
    [DllImport("user32.dll")] public static extern bool EnableWindow(IntPtr window, bool enabled);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out Point point);
    [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr window, uint flags);
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
    public static void Click(IntPtr window) {
        Rect rect;
        if (!GetWindowRect(window, out rect) || !IsWindowVisible(window)) throw new InvalidOperationException("Click target unavailable");
        int x = (rect.Left + rect.Right) / 2;
        int y = (rect.Top + rect.Bottom) / 2;
        var inputs = new[] {
            new Input { Mouse = new MouseInput {
                X = (int)Math.Round((x - GetSystemMetrics(76)) * 65535.0 / (GetSystemMetrics(78) - 1)),
                Y = (int)Math.Round((y - GetSystemMetrics(77)) * 65535.0 / (GetSystemMetrics(79) - 1)), Flags = 0xc001 } },
            new Input { Mouse = new MouseInput { Flags = 2 } },
            new Input { Mouse = new MouseInput { Flags = 4 } }
        };
        if (SendInput(3, inputs, Marshal.SizeOf(typeof(Input))) != 3) throw new InvalidOperationException("Pointer click failed");
    }
    public static void AssertRepeatedDpiKeepsBounds(IntPtr window) {
        Rect before;
        GetWindowRect(window, out before);
        var suggested = new Rect { Left = before.Left + 5, Top = before.Top + 5, Right = before.Right - 25, Bottom = before.Bottom - 25 };
        var data = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(Rect)));
        try {
            Marshal.StructureToPtr(suggested, data, false);
            uint dpi = GetDpiForWindow(window);
            SendMessage(window, 0x02e0, new IntPtr(dpi | (dpi << 16)), data);
            Rect after;
            GetWindowRect(window, out after);
            if (before.Left != after.Left || before.Top != after.Top || before.Right != after.Right || before.Bottom != after.Bottom)
                throw new InvalidOperationException("Repeated DPI notification resized the window after counts loaded");
        } finally { Marshal.FreeHGlobal(data); }
    }
    public static IntPtr Find(uint pid) {
        var found = IntPtr.Zero;
        EnumWindows((window, data) => {
            uint owner;
            GetWindowThreadProcessId(window, out owner);
            if (owner != pid || !IsWindowVisible(window) || !Text(window).StartsWith("WTM")) return true;
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
        var font = WtmNative.CloneFont(window);
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
    public static void AssertNameWidth(IntPtr root, IntPtr row, bool controlsVisible) {
        Rect bounds;
        GetWindowRect(row, out bounds);
        int dpi = (int)GetDpiForWindow(row);
        int left = WtmNative.Inset(row, bounds.Right - bounds.Left);
        int right = bounds.Right - bounds.Left - left;
        if (controlsVisible) {
            Rect button;
            GetWindowRect(FindCount(row, "Hide"), out button);
            right = button.Left - bounds.Left - 8 * dpi / 96;
        }
        string heading = Text(row).Split('\n')[0];
        int available = Math.Max(0, right - left);
        string fitted = heading;
        if (Measure(row, heading) > available) {
            fitted = "…";
            foreach (char character in heading) {
                string candidate = fitted.Substring(0, fitted.Length - 1) + character + "…";
                if (Measure(row, candidate) > available) break;
                fitted = candidate;
            }
        }
        int height = (int)Math.Ceiling(24.0 * dpi / 96);
        var dc = GetDC(row);
        var font = WtmNative.CloneFont(row);
        ReleaseDC(row, dc);
        try {
            RedrawWindow(root, IntPtr.Zero, IntPtr.Zero, 0x0181);
            DwmFlush();
            using (var expected = new Bitmap(available, height))
            using (var actual = new Bitmap(available, height)) {
                using (var graphics = Graphics.FromImage(expected)) {
                    var target = graphics.GetHdc();
                    var old = SelectObject(target, font);
                    var brush = CreateSolidBrush(11u | (16u << 8) | (18u << 16));
                    try {
                        var rect = new Rect { Right = available, Bottom = height };
                        FillRect(target, ref rect, brush);
                        SetBkMode(target, 1);
                        SetTextColor(target, 232u | (239u << 8) | (235u << 16));
                        DrawTextW(target, fitted, fitted.Length, ref rect, 0x0820);
                    } finally { SelectObject(target, old); DeleteObject(brush); graphics.ReleaseHdc(target); }
                }
                using (var graphics = Graphics.FromImage(actual)) {
                    graphics.CopyFromScreen(bounds.Left + left, bounds.Top, 0, 0, actual.Size);
                }
                int advance = (int)Math.Round(12 * 1.2 * dpi / 96, MidpointRounding.AwayFromZero);
                for (int y = 0; y < advance; y++) {
                    for (int x = 0; x < available; x++) {
                        if (actual.GetPixel(x, y) != expected.GetPixel(x, y))
                            throw new InvalidOperationException("Name does not use available width with controls " + (controlsVisible ? "visible" : "hidden"));
                    }
                }
            }
        } finally { DeleteObject(font); }
    }
    public static void AssertActionWidth(IntPtr root, string caption) {
        var button = FindCount(root, caption);
        Rect rect;
        if (button == IntPtr.Zero || !GetWindowRect(button, out rect)) throw new InvalidOperationException("Action unavailable");
        if (rect.Right - rect.Left != Measure(button, caption) + 16 * (int)GetDpiForWindow(button) / 96)
            throw new InvalidOperationException("Action width does not follow its current label: " + caption);
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
        var localFont = WtmNative.CloneFont(sample);
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
'@ + (Get-Content -LiteralPath "$PSScriptRoot/native-probe.cs" -Raw)) -ReferencedAssemblies System.Drawing.Common, System.Drawing.Primitives, System.Private.Windows.GdiPlus, System.Private.Windows.Core, System.Threading.Thread

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
    $rect = [WtmFeatureProbe+Rect]::new()
    if (-not [WtmFeatureProbe]::GetWindowRect($Window, [ref]$rect)) { throw 'Window bounds unavailable' }
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
    $foreground = [WtmFeatureProbe]::GetForegroundWindow()
    $script:process = Start-Process -FilePath $Binary -WorkingDirectory $repo -NoNewWindow -PassThru
    $script:window = [IntPtr]::Zero
    Wait-For {
        if ($script:process.HasExited) { throw 'Feature window exited before showing counts' }
        $script:window = [WtmFeatureProbe]::Find([uint32]$script:process.Id)
        if ($script:window -eq [IntPtr]::Zero) { return $false }
        $script:card = [WtmFeatureProbe]::FindCount($script:window, $expectedCaption)
        return $script:window -ne [IntPtr]::Zero -and $script:card -ne [IntPtr]::Zero
    } "$Name window and exact S/T/X caption"
    if ([WtmFeatureProbe]::IsIconic($script:window)) { throw 'Startup restored a minimized window' }
    if ($foreground -ne [IntPtr]::Zero -and [WtmFeatureProbe]::GetForegroundWindow() -eq $script:window) { throw "$Name startup stole focus" }
    $proof.runs += [ordered]@{ name = $Name; pid = $script:process.Id; caption = [WtmFeatureProbe]::Text($script:card) }
}

function Close-Window {
    if (-not [WtmFeatureProbe]::PostMessage($script:window, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) { throw 'Window close failed' }
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

function Move-Pointer([int]$X, [int]$Y) {
    Wait-For { [WtmFeatureProbe]::SetCursorPos($X, $Y) } 'pointer move'
}

function Click-Control([IntPtr]$Control) {
    $rect = Bounds $Control
    Move-Pointer ([int](($rect.Left + $rect.Right) / 2)) ([int](($rect.Top + $rect.Bottom) / 2))
    [WtmFeatureProbe]::Click($Control)
}

function Assert-Hover {
    $cursor = [WtmFeatureProbe+Point]::new()
    [void][WtmFeatureProbe]::GetCursorPos([ref]$cursor)
    $toggle = [WtmFeatureProbe]::GetDlgItem($script:window, 1004)
    try {
        foreach ($width in @(380, 760)) {
            [void][WtmFeatureProbe]::SetWindowPos($script:window, [IntPtr]::Zero, $target.Left, $target.Top, $width, 540, 20)
            $button = [WtmFeatureProbe]::FindCount($script:window, 'Hide')
            $cardBounds = Bounds $script:card
            Move-Pointer ($target.Left - 20) ($target.Top - 20)
            Wait-For { -not [WtmFeatureProbe]::IsWindowVisible($toggle) -and -not [WtmFeatureProbe]::IsWindowVisible($button) } 'controls hidden outside window'
            $status = [WtmFeatureProbe]::GetDlgItem($script:window, 1002)
            $outsideStatus = Bounds $status
            [WtmFeatureProbe]::AssertNameWidth($script:window, $script:card, $false)
            Move-Pointer ($target.Left + 20) ($target.Top + 400)
            Wait-For { [WtmFeatureProbe]::IsWindowVisible($toggle) -and [WtmFeatureProbe]::IsWindowVisible($button) } 'controls visible inside window'
            $insideStatus = Bounds $status
            if (-not (Same-Bounds $cardBounds (Bounds $script:card)) -or $outsideStatus.Top -ne $insideStatus.Top -or
                $outsideStatus.Bottom -ne $insideStatus.Bottom) { throw 'Hover changed vertical layout' }
            if ($outsideStatus.Right -le $insideStatus.Right) { throw 'Hidden Show all still reserves header width' }
            [WtmFeatureProbe]::AssertNameWidth($script:window, $script:card, $true)
            foreach ($control in @($script:card, $toggle, $button, [WtmFeatureProbe]::GetDlgItem($script:window, 1002))) {
                $rect = Bounds $control
                Move-Pointer ([int](($rect.Left + $rect.Right) / 2)) ([int](($rect.Top + $rect.Bottom) / 2))
                $sample = 0
                $deadline = [DateTime]::UtcNow.AddSeconds(20)
                while ($sample -lt 10 -and [DateTime]::UtcNow -lt $deadline) {
                    $position = [WtmFeatureProbe+Point]::new()
                    [void][WtmFeatureProbe]::GetCursorPos([ref]$position)
                    $under = [WtmFeatureProbe]::WindowFromPoint($position)
                    if ([WtmFeatureProbe]::GetAncestor($under, 2) -ne $script:window -or
                        $position.X -ne [int](($rect.Left + $rect.Right) / 2) -or $position.Y -ne [int](($rect.Top + $rect.Bottom) / 2)) {
                        Move-Pointer ([int](($rect.Left + $rect.Right) / 2)) ([int](($rect.Top + $rect.Bottom) / 2))
                        Wait-For {
                            if ([WtmFeatureProbe]::IsWindowVisible($toggle) -and [WtmFeatureProbe]::IsWindowVisible($button)) { return $true }
                            Move-Pointer ([int](($rect.Left + $rect.Right) / 2)) ([int](($rect.Top + $rect.Bottom) / 2))
                            return $false
                        } 'controls after pointer return'
                        continue
                    }
                    if (-not [WtmFeatureProbe]::IsWindowVisible($toggle) -or -not [WtmFeatureProbe]::IsWindowVisible($button)) {
                        $currentPosition = [WtmFeatureProbe+Point]::new()
                        [void][WtmFeatureProbe]::GetCursorPos([ref]$currentPosition)
                        if ($currentPosition.X -ne $position.X -or $currentPosition.Y -ne $position.Y) { continue }
                        throw 'Child transition hid controls while pointer remained inside window'
                    }
                    $sample++
                    Start-Sleep -Milliseconds 20
                }
                if ($sample -ne 10) { throw 'Pointer did not remain inside the test window' }
            }
            Click-Control $button
            Wait-For { [WtmFeatureProbe]::FindCount($script:window, $expectedCaption) -eq [IntPtr]::Zero } 'Hide click'
            Click-Control $toggle
            Wait-For {
                $script:card = [WtmFeatureProbe]::FindCount($script:window, $expectedCaption)
                $script:card -ne [IntPtr]::Zero
            } 'Show all click'
            Move-Pointer ($target.Left - 20) ($target.Top - 20)
            Wait-For { -not [WtmFeatureProbe]::IsWindowVisible($toggle) } 'checked toggle hidden'
            $thread = [WtmFeatureProbe]::GetWindowThreadProcessId($script:window, [ref]([uint32]0))
            $focus = [WtmFeatureProbe+GuiInfo]::new()
            $focus.Size = [Runtime.InteropServices.Marshal]::SizeOf($focus)
            if (-not [WtmFeatureProbe]::GetGUIThreadInfo($thread, [ref]$focus) -or
                $focus.Focus -eq $toggle -or -not [WtmFeatureProbe]::IsWindowVisible($focus.Focus)) { throw 'Focus remained on a hidden control' }
            if ([WtmFeatureProbe]::SendMessage($toggle, 0x00f0, [IntPtr]::Zero, [IntPtr]::Zero) -ne [IntPtr]1) { throw 'Hover reset checkbox state' }
            Move-Pointer ($target.Left + 20) ($target.Top + 400)
            Wait-For { [WtmFeatureProbe]::IsWindowVisible($toggle) } 'checked toggle restored'
            $unhide = [WtmFeatureProbe]::FindCount($script:window, 'Unhide')
            [WtmFeatureProbe]::AssertActionWidth($script:window, 'Unhide')
            Click-Control $unhide
            Wait-For { [WtmFeatureProbe]::FindCount($script:window, 'Hide') -ne [IntPtr]::Zero } 'Unhide click'
            [WtmFeatureProbe]::AssertActionWidth($script:window, 'Hide')
            Click-Control $toggle
            Wait-For { [WtmFeatureProbe]::SendMessage($toggle, 0x00f0, [IntPtr]::Zero, [IntPtr]::Zero) -eq [IntPtr]::Zero } 'Show all unchecked'
            [void][WtmFeatureProbe]::SetWindowPos($script:window, [IntPtr]::Zero, $target.Left, $target.Top, $width, 100, 20)
            $scrollbar = [WtmFeatureProbe]::GetDlgItem($script:window, 1003)
            Wait-For { [WtmFeatureProbe]::IsWindowVisible($scrollbar) } 'scrollbar visible'
            $rect = Bounds $scrollbar
            Move-Pointer ([int](($rect.Left + $rect.Right) / 2)) ([int](($rect.Top + $rect.Bottom) / 2))
            for ($sample = 0; $sample -lt 10; $sample++) {
                if (-not [WtmFeatureProbe]::IsWindowVisible($toggle)) { throw 'Scrollbar transition hid controls' }
                Start-Sleep -Milliseconds 20
            }
            $proof.checks += [ordered]@{ name = 'whole-window hover'; width = $width; stable_layout = $true; child_transitions = $true; clicks = $true; checkbox_preserved = $true }
        }
    } finally {
        [void][WtmFeatureProbe]::SetWindowPos($script:window, [IntPtr]::Zero, $target.Left, $target.Top, 760, 540, 20)
        [void][WtmFeatureProbe]::SetCursorPos($cursor.X, $cursor.Y)
    }
}

function Assert-Colors {
    [WtmNative]::Hover($script:window)
    $outer = Bounds $script:window
    $card = Bounds $script:card
    $dpi = [WtmFeatureProbe]::GetDpiForWindow($script:card)
    $x = $card.Left - $outer.Left + [WtmNative]::Inset($script:card, $card.Right - $card.Left)
    $top = $card.Top - $outer.Top + [int][Math]::Round(12 * 1.2 * $dpi / 96)
    $bottom = $card.Bottom - $outer.Top - [int][Math]::Floor(8 * $dpi / 96)
    $image = [Drawing.Bitmap]::new($outer.Right - $outer.Left, $outer.Bottom - $outer.Top)
    $graphics = [Drawing.Graphics]::FromImage($image)
    try {
        [void][WtmFeatureProbe]::RedrawWindow($script:window, [IntPtr]::Zero, [IntPtr]::Zero, 0x0181)
        [void][WtmFeatureProbe]::DwmFlush()
        $graphics.CopyFromScreen($outer.Left, $outer.Top, 0, 0, $image.Size)
        $image.Save((Join-Path $Output 'counts.png'))
        $button = [WtmFeatureProbe]::FindCount($script:card, 'Hide')
        if ($button -eq [IntPtr]::Zero) { throw 'Hide button unavailable' }
        $buttonBounds = Bounds $button
        $corner = $image.GetPixel($buttonBounds.Left - $outer.Left, $buttonBounds.Top - $outer.Top)
        if ($corner.R -ne 11 -or $corner.G -ne 16 -or $corner.B -ne 18) { throw "Hide corner differs from terminal background" }
        $headingTop = $card.Top - $outer.Top
        $proof.rendered_font = [WtmFeatureProbe]::AssertFont($script:window, $script:card, $image, $x, $headingTop)
        $rejectedShift = $false
        try { [void][WtmFeatureProbe]::AssertFont($script:window, $script:card, $image, $x, $headingTop, 1) }
        catch {
            if ($_.Exception.Message -notmatch 'shared line origin on line 1') { throw }
            $rejectedShift = $true
        }
        if (-not $rejectedShift) { throw 'Text position proof accepted a one-pixel second-line shift' }
        $proof.line_origin_shift_rejected = $true
        [WtmFeatureProbe]::ButtonStates($button, $Output)
        $proof.button_corners_match_card = $true
        $proof.button_states = @('normal', 'hover', 'pressed', 'disabled')
        $proof.colors = @()
        foreach ($part in @('S +4/-1', 'T +5/-1', 'X +6/-1')) {
            $width = [WtmFeatureProbe]::Measure($script:card, $part)
            $pixels = [WtmFeatureProbe]::Pixels($image, $x, $x + $width, $top, $bottom)
            $proof.colors += [ordered]@{ text = $part; left = $x; width = $width; top = $top; bottom = $bottom; green = $pixels[0]; red = $pixels[1]; muted = $pixels[2] }
            if ($part.StartsWith('S')) {
                if ($pixels[0] -eq 0 -or $pixels[1] -eq 0) { throw 'Source additions and deletions are not colored' }
            } elseif ($pixels[0] -ne 0 -or $pixels[1] -ne 0 -or $pixels[2] -eq 0) { throw "$part is not muted" }
            $x += $width + [WtmFeatureProbe]::Measure($script:card, ' · ')
        }
    } finally { $graphics.Dispose(); $image.Dispose() }
}

$names = @('HOME', 'USERPROFILE', 'GIT_DIR', 'GIT_WORK_TREE', 'GIT_COMMON_DIR', 'GIT_INDEX_FILE', 'GIT_CONFIG_GLOBAL', 'GIT_CONFIG_NOSYSTEM')
$previous = @{}
foreach ($name in $names) { $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
$dpiContext = [WtmFeatureProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
$script:process = $null
$proof = [ordered]@{ result = 'FAIL'; binary = $Binary; sha256 = (Get-FileHash $Binary).Hash; checks = @(); runs = @() }
try {
    $env:USERPROFILE = $fixtureHome
    [WtmNative]::Opaque($fixtureHome)
    $env:HOME = $fixtureHome
    $env:GIT_CONFIG_GLOBAL = Join-Path $verificationFixture 'empty-gitconfig'
    $env:GIT_CONFIG_NOSYSTEM = '1'
    foreach ($name in @('GIT_DIR', 'GIT_WORK_TREE', 'GIT_COMMON_DIR', 'GIT_INDEX_FILE')) { Remove-Item "Env:$name" -ErrorAction SilentlyContinue }
    New-Item -ItemType Directory -Path (Join-Path $repo 'tests'), (Join-Path $repo 'scripts') | Out-Null
    Run-Git @('init', '--quiet', '--initial-branch=km/wna-2067-document-the-agreement-and-usage')
    foreach ($path in @('source.txt', 'tests/unit.test.js', 'scripts/tool.ps1')) { Write-Lines $path @('keep', 'old') }
    Run-Git @('add', '.')
    Run-Git @('-c', 'user.name=Wtm Test', '-c', 'user.email=wtm@example.invalid', 'commit', '--quiet', '-m', 'Fixture')
    foreach ($path in @('source.txt', 'tests/unit.test.js', 'scripts/tool.ps1')) { Write-Lines $path @('keep', 'staged') }
    Run-Git @('add', '.')
    foreach ($path in @('source.txt', 'tests/unit.test.js', 'scripts/tool.ps1')) { Write-Lines $path @('keep', 'staged', 'unstaged') }
    Write-Lines 'new-source.txt' @('new1', 'new2')
    Write-Lines 'tests/new.test.js' @('new1', 'new2', 'new3')
    Write-Lines 'scripts/new.ps1' @('new1', 'new2', 'new3', 'new4')
    & $CliBinary add $repo
    if ($LASTEXITCODE -ne 0) { throw 'Fixture registration failed' }
    $expectedCaption = "km/wna-2067-document-the-agreement-and-usage`nS +4/-1 · T +5/-1 · X +6/-1"
    $placementPath = Join-Path $fixtureHome '.config/wtm/config.placement.json'
    $area = [WtmFeatureProbe+Rect]::new()
    if (-not [WtmFeatureProbe]::SystemParametersInfo(0x0030, 0, [ref]$area, 0)) { throw 'Work area unavailable' }
    $target = [WtmFeatureProbe+Rect]::new()
    $target.Left = $area.Left + 37
    $target.Top = $area.Top + 43
    $target.Right = $target.Left + 760
    $target.Bottom = $target.Top + 540
    if (-not (Fits $target)) { throw 'The fixture window cannot fit on this screen' }
    $proof.work_area = Bounds-Record $area
    $proof.expected_bounds = Bounds-Record $target
    Start-Window 'initial'
    if (-not [WtmFeatureProbe]::SetWindowPos($script:window, [IntPtr]::Zero, $target.Left, $target.Top, 760, 540, 0x14)) { throw 'Fixture resize failed' }
    Wait-For { Same-Bounds $target (Bounds $script:window) } 'fixture bounds'
    [void][WtmFeatureProbe]::SendMessage($script:window, 0x0232, [IntPtr]::Zero, [IntPtr]::Zero)
    $savedMove = Get-Content $placementPath -Raw | ConvertFrom-Json
    if (-not (Same-Bounds $target $savedMove.normal)) { throw 'Completed move did not save its bounds' }
    [WtmFeatureProbe]::AssertRepeatedDpiKeepsBounds($script:window)
    $proof.checks += [ordered]@{ name = 'repeated DPI notification'; bounds_unchanged = $true }
    Assert-Hover
    Assert-Colors
    Close-Window
    Start-Window 'normal restart'
    Assert-Restored 'normal restart'
    [void][WtmFeatureProbe]::ShowWindow($script:window, 6)
    Wait-For { [WtmFeatureProbe]::IsIconic($script:window) } 'minimization'
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
    [void][WtmFeatureProbe]::ShowWindow($script:window, 3)
    Wait-For { [WtmFeatureProbe]::IsZoomed($script:window) } 'maximization'
    if (-not (Same-Bounds $area (Bounds $script:window))) { throw 'Maximized frameless window does not fit the work area' }
    Close-Window
    Start-Window 'maximized restart'
    if (-not [WtmFeatureProbe]::IsZoomed($script:window)) { throw 'Maximized state was lost' }
    $proof.checks += [ordered]@{ name = 'maximized restart'; maximized = [WtmFeatureProbe]::IsZoomed($script:window) }
    [void][WtmFeatureProbe]::ShowWindow($script:window, 6)
    Wait-For { [WtmFeatureProbe]::IsIconic($script:window) } 'maximized minimization'
    Close-Window
    Start-Window 'maximized then minimized restart'
    if (-not [WtmFeatureProbe]::IsZoomed($script:window)) { throw 'Minimizing lost the maximized state' }
    $proof.checks += [ordered]@{ name = 'maximized then minimized restart'; maximized = [WtmFeatureProbe]::IsZoomed($script:window) }
    [void][WtmFeatureProbe]::ShowWindow($script:window, 9)
    Assert-Restored 'normal bounds after maximization'
    Close-Window
    $proof.result = 'PASS'
    Write-Host "PASS. Counts, source-only colors, and window placement. Evidence at $Output"
} catch { $proof.failure = $_.Exception.Message; throw } finally {
    if ($null -ne $script:process) {
        [void][WtmFeatureProbe]::PostMessage($script:window, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
        if (-not $script:process.WaitForExit(10000)) { $script:process.Kill($true); $script:process.WaitForExit() }
        $script:process.Dispose()
    }
    $proof | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $Output 'proof.json') -Encoding utf8NoBOM
    foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process') }
    [void][WtmFeatureProbe]::SetThreadDpiAwarenessContext($dpiContext)
}

} finally { Remove-VerificationDirectories }
