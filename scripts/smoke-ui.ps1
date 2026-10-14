param(
    [string]$Binary = "$PSScriptRoot/../target/debug/wts-ui.exe",
    [string]$Output = "$env:LOCALAPPDATA/Temp/opencode/wts-ui-$([Guid]::NewGuid().ToString('N'))"
)

$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path $Binary).Path
$Output = [IO.Path]::GetFullPath($Output)
New-Item -ItemType Directory -Path $Output -Force | Out-Null

function Get-PeSubsystem([string]$Path) {
    $reader = [IO.BinaryReader]::new([IO.File]::OpenRead($Path))
    try {
        if ($reader.ReadUInt16() -ne 0x5A4D) { throw 'Binary has no DOS header' }
        $reader.BaseStream.Position = 0x3C
        $offset = $reader.ReadInt32()
        if ($offset -lt 0 -or $offset + 94 -gt $reader.BaseStream.Length) { throw 'Binary has an invalid PE header offset' }
        $reader.BaseStream.Position = $offset
        if ($reader.ReadUInt32() -ne 0x4550) { throw 'Binary has no PE signature' }
        $reader.BaseStream.Position = $offset + 24
        $magic = $reader.ReadUInt16()
        if ($magic -ne 0x10B -and $magic -ne 0x20B) { throw 'Binary has an invalid PE optional header' }
        $reader.BaseStream.Position = $offset + 24 + 68
        return $reader.ReadUInt16()
    } finally { $reader.Dispose() }
}

$proof = [ordered]@{
    result = 'FAIL'
    instance = [ordered]@{
        pid = $null
        hwnd = $null
        binary = $Binary
        sha256 = (Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash
        subsystem = $null
    }
    arguments = @()
    working_directory = Join-Path $Output 'work'
    home = Join-Path $Output 'home'
    userprofile = Join-Path $Output 'profile'
    launcher_pid = $PID
    create_no_window = $false
    use_shell_execute = $false
}
$process = $null
$handle = [IntPtr]::Zero
try {
    $proof.instance.subsystem = Get-PeSubsystem $Binary
    if ($proof.instance.subsystem -ne 2) { throw "Expected Windows GUI PE subsystem 2, got $($proof.instance.subsystem)" }

    Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class WtsUiProbe {
    public delegate bool EnumProc(IntPtr handle, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr handle, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr handle, StringBuilder text, int count);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr handle);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr handle);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr handle, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("kernel32.dll")] public static extern IntPtr GetConsoleWindow();
    [DllImport("kernel32.dll")] public static extern uint GetConsoleProcessList(uint[] processIds, uint count);
    public static bool SharesConsole(uint processId) {
        var ids = new uint[256];
        var count = GetConsoleProcessList(ids, (uint)ids.Length);
        if (count == 0 || count > ids.Length) { throw new InvalidOperationException("Console process list unavailable"); }
        return Array.IndexOf(ids, processId, 0, (int)count) >= 0;
    }
    public static string Text(IntPtr handle) {
        var text = new StringBuilder(256);
        GetWindowText(handle, text, text.Capacity);
        return text.ToString();
    }
    public static IntPtr Find(uint pid) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((handle, data) => {
            uint owner;
            GetWindowThreadProcessId(handle, out owner);
            if (owner == pid && IsWindowVisible(handle) && Text(handle).StartsWith("WTS", StringComparison.OrdinalIgnoreCase)) {
                found = handle;
                return false;
            }
            return true;
        }, IntPtr.Zero);
        return found;
    }
    public static string ChildText(IntPtr parent) {
        var text = new StringBuilder();
        EnumChildWindows(parent, (handle, data) => {
            text.AppendLine(Text(handle));
            return true;
        }, IntPtr.Zero);
        return text.ToString();
    }
}
'@

    foreach ($directory in @($proof.working_directory, $proof.home, $proof.userprofile)) {
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
        $ancestor = [IO.DirectoryInfo]::new($directory)
        while ($null -ne $ancestor) {
            if (Test-Path -LiteralPath (Join-Path $ancestor.FullName '.git')) { throw 'UI fixture must be outside Git' }
            $ancestor = $ancestor.Parent
        }
    }
    $proof['launcher_in_console'] = [WtsUiProbe]::SharesConsole([uint32]$PID)
    if (-not $proof.launcher_in_console) { throw 'Launching process is not in the console process list' }
    $proof['launcher_console_hwnd'] = [WtsUiProbe]::GetConsoleWindow().ToInt64()
    $start = [Diagnostics.ProcessStartInfo]::new($Binary)
    $start.WorkingDirectory = $proof.working_directory
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $false
    $start.Environment['HOME'] = $proof.home
    $start.Environment['USERPROFILE'] = $proof.userprofile
    foreach ($name in @('GIT_DIR', 'GIT_WORK_TREE', 'GIT_COMMON_DIR', 'GIT_INDEX_FILE')) { [void]$start.Environment.Remove($name) }
    $process = [Diagnostics.Process]::Start($start)
    $proof.instance.pid = $process.Id
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        if ($process.HasExited) { throw "UI exited before showing a window with code $($process.ExitCode)" }
        $handle = [WtsUiProbe]::Find([uint32]$process.Id)
        if ($handle -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($handle -eq [IntPtr]::Zero) { throw 'No visible WTS window belonging to the directly launched UI process' }
    $proof.instance.hwnd = $handle.ToInt64()
    $proof['window_title'] = [WtsUiProbe]::Text($handle)
    $proof['window_visible'] = [WtsUiProbe]::IsWindowVisible($handle)
    [uint32]$owner = 0
    [void][WtsUiProbe]::GetWindowThreadProcessId($handle, [ref]$owner)
    $proof['window_owner_pid'] = $owner
    $proof['process_binary'] = $process.MainModule.FileName
    $proof['console_detached'] = -not [WtsUiProbe]::SharesConsole([uint32]$process.Id)
    $proof['distinct_from_launcher'] = $process.Id -ne $PID -and $handle.ToInt64() -ne $proof.launcher_console_hwnd
    if ($owner -ne $process.Id -or -not $proof.window_visible -or -not $proof.console_detached -or -not $proof.distinct_from_launcher) { throw 'UI is not a separate visible window outside the launching console' }
    if ($proof.process_binary -ne $Binary) { throw 'UI process does not match the tested binary path' }
    if ((Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash -ne $proof.instance.sha256) { throw 'UI binary changed during the check' }
    if (-not [WtsUiProbe]::PostMessage($handle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) { throw 'UI window close failed' }
    if (-not $process.WaitForExit(10000)) { throw 'UI did not close within 10 seconds' }
    $proof['window_exit_code'] = $process.ExitCode
    $proof['window_destroyed'] = -not [WtsUiProbe]::IsWindow($handle)
    if ($process.ExitCode -ne 0 -or -not $proof.window_destroyed) { throw 'UI did not close cleanly' }
    $process.Dispose()
    $process = $null
    $handle = [IntPtr]::Zero
    [void]$start.Environment.Remove('USERPROFILE')
    $process = [Diagnostics.Process]::Start($start)
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        if ($process.HasExited) { throw 'UI startup failure exited without a visible error dialog' }
        $handle = [WtsUiProbe]::Find([uint32]$process.Id)
        if ($handle -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($handle -eq [IntPtr]::Zero -or [WtsUiProbe]::Text($handle) -ne 'wts-ui') { throw 'UI startup failure did not show its error dialog' }
    $proof['startup_error_text'] = [WtsUiProbe]::ChildText($handle)
    if ($proof.startup_error_text -notmatch 'USERPROFILE is not set') { throw 'UI error dialog did not explain the startup failure' }
    if (-not [WtsUiProbe]::PostMessage($handle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) { throw 'UI error dialog dismissal failed' }
    $closed = $process.WaitForExit(10000)
    $proof['startup_error_exit_code'] = if ($closed) { $process.ExitCode } else { $null }
    if (-not $closed -or $process.ExitCode -ne 1) { throw "UI startup failure did not exit with code 1. closed=$closed exit=$($proof.startup_error_exit_code)" }
    $proof['startup_error_visible'] = $true
    $proof.result = 'PASS'
    Write-Host 'PASS. Direct Windows GUI launch owns a separate visible WTS window and closes cleanly.'
    Write-Host 'PASS. Fatal startup failure shows a native error dialog and exits with code 1.'
} catch {
    $proof['failure'] = $_.Exception.Message
    throw
} finally {
    if ($null -ne $process -and -not $process.HasExited) {
        if ($handle -ne [IntPtr]::Zero) {
            [void][WtsUiProbe]::PostMessage($handle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
            [void]$process.WaitForExit(10000)
        }
        if (-not $process.HasExited) { $process.Kill($true); $process.WaitForExit() }
    }
    $proof | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $Output 'proof.json') -Encoding utf8NoBOM
    Write-Host "Evidence at $Output"
    if ($null -ne $process) { $process.Dispose() }
}
