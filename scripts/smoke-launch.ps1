param([string]$Binary = "$PSScriptRoot/../target/debug/wtm-gui.exe")

. (Join-Path $PSScriptRoot 'verification-directories.ps1')
try {
$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path $Binary).Path
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class WtmLaunchProbe {
    public delegate bool EnumProc(IntPtr handle, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr handle, out uint processId);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr handle, StringBuilder text, int count);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr handle);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr handle);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr handle, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("kernel32.dll")] public static extern uint GetConsoleProcessList(uint[] processIds, uint count);
    public static bool SharesConsole(uint processId) {
        var ids = new uint[256];
        var count = GetConsoleProcessList(ids, (uint)ids.Length);
        if (count == 0 || count > ids.Length) { throw new InvalidOperationException("Console process list unavailable"); }
        return Array.IndexOf(ids, processId, 0, (int)count) >= 0;
    }
    public static IntPtr Find(uint pid) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((handle, data) => {
            uint owner;
            GetWindowThreadProcessId(handle, out owner);
            var text = new StringBuilder(256);
            GetWindowText(handle, text, text.Capacity);
            if (owner == pid && IsWindowVisible(handle) && text.ToString().StartsWith("WTM")) { found = handle; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }
}
'@

$verificationEvidence = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot "../target/verification/smoke-launch-evidence-$([Guid]::NewGuid().ToString('N'))"))

$fixture = New-VerificationDirectory 'smoke-launch' $verificationEvidence
$fixtureHome = Join-Path $fixture 'home'
New-Item -ItemType Directory -Path $fixtureHome | Out-Null
$launcher = $null
$windowProcess = $null
$handle = [IntPtr]::Zero
try {
    $start = [Diagnostics.ProcessStartInfo]::new($Binary)
    $start.WorkingDirectory = $fixtureHome
    $start.UseShellExecute = $false
    $start.RedirectStandardInput = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.Environment['HOME'] = Join-Path $fixture 'unix-home'
    $start.Environment['USERPROFILE'] = $fixtureHome
    $launcher = [Diagnostics.Process]::Start($start)
    $stdout = $launcher.StandardOutput.ReadToEndAsync()
    $stderr = $launcher.StandardError.ReadToEndAsync()
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        $candidates = @($launcher.Id) + @(Get-CimInstance Win32_Process -Filter "ParentProcessId = $($launcher.Id)" | Select-Object -ExpandProperty ProcessId)
        foreach ($candidate in $candidates) {
            $handle = [WtmLaunchProbe]::Find([uint32]$candidate)
            if ($handle -ne [IntPtr]::Zero) {
                $windowProcess = [Diagnostics.Process]::GetProcessById([int]$candidate)
                [void]$windowProcess.Handle
                break
            }
        }
        if ($handle -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($handle -eq [IntPtr]::Zero) { throw 'No visible WTM window belonging to this launch' }
    $exited = $launcher.WaitForExit(3000)
    $stdinReleased = $false
    try { $launcher.StandardInput.BaseStream.WriteByte(0); $launcher.StandardInput.BaseStream.Flush() }
    catch [IO.IOException] { $stdinReleased = $true }
    finally { try { $launcher.StandardInput.BaseStream.Close() } catch [IO.IOException] {} }
    $stdoutEof = $stdout.Wait(1000)
    $stderrEof = $stderr.Wait(1000)
    $differentPid = $windowProcess.Id -ne $launcher.Id
    $detached = -not [WtmLaunchProbe]::SharesConsole([uint32]$windowProcess.Id)
    $result = if ($exited -and $launcher.ExitCode -eq 0 -and $stdinReleased -and $stdoutEof -and $stderrEof -and $differentPid -and $detached) { 'PASS' } else { 'FAIL' }
    $proof = [ordered]@{
        result = $result
        launcher_exited = $exited
        launcher_exit_code = if ($exited) { $launcher.ExitCode } else { $null }
        stdin_released = $stdinReleased
        stdout_eof = $stdoutEof
        stderr_eof = $stderrEof
        different_window_pid = $differentPid
        window_visible = [WtmLaunchProbe]::IsWindowVisible($handle)
        console_detached = $detached
        launcher_pid = $launcher.Id
        window_pid = $windowProcess.Id
        binary = $Binary
        sha256 = (Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash
    }
    $proof | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $fixture 'proof.json') -Encoding utf8NoBOM
    Write-Host "$result. launcher_exited=$exited stdin_released=$stdinReleased stdout_eof=$stdoutEof stderr_eof=$stderrEof different_window_pid=$differentPid console_detached=$detached"
    Write-Host "Evidence at $verificationEvidence"
    if ($result -ne 'PASS') { throw 'Live launch did not release the terminal while keeping a separate visible window' }
    if (-not [WtmLaunchProbe]::PostMessage($handle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) { throw 'Window close failed' }
    if (-not $windowProcess.WaitForExit(10000)) { throw 'Window did not close within 10 seconds' }
    if ($windowProcess.ExitCode -ne 0) { throw "Window exited with $($windowProcess.ExitCode)" }
    if ([WtmLaunchProbe]::IsWindow($handle)) { throw 'Window survived close' }
    $proof['window_exit_code'] = $windowProcess.ExitCode
    $proof['window_destroyed'] = $true
    $proof | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $fixture 'proof.json') -Encoding utf8NoBOM
    Write-Host 'PASS. Window child closed cleanly.'
} finally {
    if ($handle -ne [IntPtr]::Zero -and [WtmLaunchProbe]::IsWindow($handle)) {
        [void][WtmLaunchProbe]::PostMessage($handle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
        if ($null -ne $windowProcess -and -not $windowProcess.WaitForExit(10000)) { $windowProcess.Kill($true); $windowProcess.WaitForExit() }
    }
    if ($null -ne $launcher) {
        foreach ($child in @(Get-CimInstance Win32_Process -Filter "ParentProcessId = $($launcher.Id)" -ErrorAction SilentlyContinue)) {
            try {
                $owned = [Diagnostics.Process]::GetProcessById([int]$child.ProcessId)
                if (-not $owned.HasExited) { $owned.Kill($true); $owned.WaitForExit() }
            } catch [ArgumentException] {} catch [InvalidOperationException] {}
        }
        if (-not $launcher.HasExited) { $launcher.Kill($true); $launcher.WaitForExit() }
    }
}

} finally { Remove-VerificationDirectories }
