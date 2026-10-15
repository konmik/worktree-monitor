param(
    [string]$Binary = "$env:USERPROFILE/.cargo/bin/wts-ui.exe",
    [int]$DurationSeconds = 35,
    [string]$Output = "$env:LOCALAPPDATA/Temp/wts-window-trace-$([Guid]::NewGuid().ToString('N'))",
    [switch]$Restart
)

$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path $Binary).Path
New-Item -ItemType Directory -Path $Output -Force | Out-Null
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
public static class WtsBoundsTrace {
    public struct Rect { public int Left, Top, Right, Bottom; public override string ToString() { return Left + "," + Top + "," + Right + "," + Bottom; } }
    public struct Point { public int X, Y; }
    public struct Message { public IntPtr Window; public uint Id; public UIntPtr Wparam; public IntPtr Lparam; public uint Time; public Point Point; public uint Private; }
    public sealed class Entry {
        public double Milliseconds; public DateTime ObservedUtc; public string Source; public uint ProcessId; public long Window; public bool Visible;
        public string Bounds, Client, Viewport, Frame, Status; public uint Dpi; public int PendingRows, CountRows, RefreshingRows;
    }
    public delegate void EventProc(IntPtr hook, uint kind, IntPtr window, int objectId, int childId, uint thread, uint time);
    public delegate bool EnumProc(IntPtr window, IntPtr data);
    [DllImport("user32.dll")] static extern IntPtr SetWinEventHook(uint first, uint last, IntPtr module, EventProc callback, uint process, uint thread, uint flags);
    [DllImport("user32.dll")] static extern bool UnhookWinEvent(IntPtr hook);
    [DllImport("user32.dll")] static extern int GetMessage(out Message message, IntPtr window, uint first, uint last);
    [DllImport("user32.dll")] static extern bool PostThreadMessage(uint thread, uint message, UIntPtr wparam, IntPtr lparam);
    [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr parent, EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] static extern IntPtr GetParent(IntPtr window);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] static extern bool GetClientRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll")] static extern uint GetDpiForWindow(IntPtr window);
    [DllImport("user32.dll")] static extern IntPtr GetDlgItem(IntPtr window, int id);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr window, StringBuilder text, int count);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window, StringBuilder text, int count);
    [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("dwmapi.dll")] static extern int DwmGetWindowAttribute(IntPtr window, uint attribute, out Rect rect, int size);
    static readonly object Gate = new object();
    static readonly List<Entry> Entries = new List<Entry>();
    static readonly Dictionary<IntPtr, string> Previous = new Dictionary<IntPtr, string>();
    static readonly Stopwatch Clock = new Stopwatch();
    static readonly ManualResetEvent Ready = new ManualResetEvent(false);
    static Thread Worker;
    static uint WorkerId;
    static Exception Failure;
    static string Text(IntPtr window) { var text = new StringBuilder(65536); GetWindowText(window, text, text.Capacity); return text.ToString(); }
    static void Record(IntPtr window, string source) {
        if (window == IntPtr.Zero) return;
        var type = new StringBuilder(64);
        GetClassName(window, type, type.Capacity);
        if (type.ToString() != "WtsStatusWindow") return;
        Rect bounds, client, viewport, frame;
        if (!GetWindowRect(window, out bounds)) return;
        GetClientRect(window, out client);
        GetWindowRect(GetDlgItem(window, 1001), out viewport);
        DwmGetWindowAttribute(window, 9, out frame, Marshal.SizeOf(typeof(Rect)));
        uint process;
        GetWindowThreadProcessId(window, out process);
        var entry = new Entry { Milliseconds = Clock.Elapsed.TotalMilliseconds, ObservedUtc = DateTime.UtcNow, Source = source, ProcessId = process,
            Window = window.ToInt64(), Visible = IsWindowVisible(window), Bounds = bounds.ToString(), Client = client.ToString(),
            Viewport = viewport.ToString(), Frame = frame.ToString(), Status = Text(GetDlgItem(window, 1002)), Dpi = GetDpiForWindow(window) };
        var parent = GetDlgItem(window, 1001);
        if (parent != IntPtr.Zero) EnumChildWindows(parent, (child, data) => {
            if (GetParent(child) != parent) return true;
            string text = Text(child);
            if (text.Contains("\nS +")) entry.CountRows++;
            else if (text.Contains("\nCounts unavailable")) { }
            else if (System.IO.Path.IsPathRooted(text.Split('\n')[0])) entry.PendingRows++;
            if (text.Contains("Refreshing")) entry.RefreshingRows++;
            return true;
        }, IntPtr.Zero);
        string signature = entry.Visible + "|" + entry.Bounds + "|" + entry.Client + "|" + entry.Viewport + "|" + entry.Frame + "|" + entry.Status + "|" + entry.Dpi + "|" + entry.PendingRows + "|" + entry.CountRows + "|" + entry.RefreshingRows;
        lock (Gate) {
            string previous;
            if (source != "poll" || !Previous.TryGetValue(window, out previous) || previous != signature) Entries.Add(entry);
            Previous[window] = signature;
        }
    }
    public static void Start() {
        Clock.Start();
        Worker = new Thread(() => {
            SetThreadDpiAwarenessContext(new IntPtr(-4));
            WorkerId = GetCurrentThreadId();
            EventProc callback = (hook, kind, window, objectId, childId, thread, time) => {
                if (objectId == 0 && childId == 0) Record(window, "event-" + kind.ToString("x"));
            };
            var handle = SetWinEventHook(0x8000, 0x800b, IntPtr.Zero, callback, 0, 0, 0);
            if (handle == IntPtr.Zero) { Failure = new InvalidOperationException("Window event hook failed"); Ready.Set(); return; }
            Ready.Set();
            Message message;
            while (GetMessage(out message, IntPtr.Zero, 0, 0) > 0) { }
            UnhookWinEvent(handle);
            GC.KeepAlive(callback);
        });
        Worker.IsBackground = true;
        Worker.Start();
        Ready.WaitOne();
        if (Failure != null) throw Failure;
    }
    public static void Poll() {
        SetThreadDpiAwarenessContext(new IntPtr(-4));
        EnumWindows((window, data) => { Record(window, "poll"); return true; }, IntPtr.Zero);
    }
    public static Entry[] Stop() {
        PostThreadMessage(WorkerId, 0x0012, UIntPtr.Zero, IntPtr.Zero);
        Worker.Join(5000);
        lock (Gate) return Entries.ToArray();
    }
}
'@

$existing = @(Get-Process wts-ui -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $Binary })
$existing | Select-Object Id, Path, StartTime | ConvertTo-Json | Set-Content (Join-Path $Output 'initial-processes.json')
$config = Join-Path $env:USERPROFILE '.config/wts'
Copy-Item -LiteralPath (Join-Path $config 'config.json'), (Join-Path $config 'config.placement.json') -Destination $Output
[WtsBoundsTrace]::Start()
try {
    [WtsBoundsTrace]::Poll()
    if ($Restart) {
        if ($existing.Count -gt 1) { throw 'Multiple installed monitor instances are running' }
        foreach ($app in $existing) {
            [void]$app.CloseMainWindow()
            if (-not $app.WaitForExit(5000)) { throw 'Installed monitor did not close' }
        }
        $app = Start-Process -FilePath $Binary -WorkingDirectory (Resolve-Path "$PSScriptRoot/..") -WindowStyle Hidden -PassThru
        Write-Host "Tracing installed process $($app.Id) without moving or resizing it."
    }
    $clock = [Diagnostics.Stopwatch]::StartNew()
    while ($clock.Elapsed.TotalSeconds -lt $DurationSeconds) {
        [WtsBoundsTrace]::Poll()
        Start-Sleep -Milliseconds 10
    }
} finally {
    $entries = [WtsBoundsTrace]::Stop()
    $entries | Sort-Object Milliseconds | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $Output 'events.json')
    $visible = @($entries | Where-Object Visible)
    $visible | Group-Object ProcessId, Bounds | Select-Object Name, Count | Format-Table | Out-String | Write-Host
    Write-Host "Native bounds evidence at $Output"
}
