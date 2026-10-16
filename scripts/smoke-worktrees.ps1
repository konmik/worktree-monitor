param(
    [Parameter(Mandatory)][string]$Repository,
    [string]$Binary = "$PSScriptRoot/../target/debug/wtm.exe"
)

$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path $Binary).Path
$Repository = (Resolve-Path $Repository).Path

function Git-Text([string]$Directory, [string[]]$Arguments) {
    $info = [Diagnostics.ProcessStartInfo]::new('git')
    foreach ($argument in @('-c', 'core.fsmonitor=false', '-C', $Directory) + $Arguments) { $info.ArgumentList.Add($argument) }
    $info.UseShellExecute = $false
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.Environment['GIT_OPTIONAL_LOCKS'] = '0'
    $process = [Diagnostics.Process]::Start($info)
    try {
        $output = $process.StandardOutput.ReadToEndAsync()
        $errors = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(30000)) { throw 'Git baseline command timed out' }
        if ($process.ExitCode -ne 0) { throw "Git failed with $($process.ExitCode). $($errors.GetAwaiter().GetResult())" }
        return $output.GetAwaiter().GetResult()
    } finally {
        if (-not $process.HasExited) { $process.Kill($true); $process.WaitForExit() }
        $process.Dispose()
    }
}

$records = (Git-Text $Repository @('worktree', 'list', '--porcelain', '-z')) -split "`0`0"
$paths = @()
$clean = 0
$missing = 0
foreach ($record in $records) {
    if (-not $record -or ($record -split "`0") -contains 'bare') { continue }
    $pathField = ($record -split "`0" | Where-Object { $_.StartsWith('worktree ') })
    if (-not $pathField) { throw 'Worktree record lacks its path' }
    $path = $pathField.Substring(9)
    $paths += $path
    if (-not (Test-Path -LiteralPath $path)) { $missing++; continue }
    $status = Git-Text $path @('status', '--porcelain', '-z', '--untracked-files=all')
    if (-not $status) { $clean++ }
}
if ($paths.Count -lt 2) { throw 'Choose an existing repository with at least two worktrees' }

$fixture = Join-Path $env:LOCALAPPDATA "Temp/opencode/wtm-worktrees-$([Guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $fixture | Out-Null
$oldHome = $env:HOME
$oldProfile = $env:USERPROFILE
try {
    $env:HOME = Join-Path $fixture 'unix-home'
    $env:USERPROFILE = $fixture
    & $Binary add $Repository
    if ($LASTEXITCODE -ne 0) { throw 'Registration failed' }
    if (-not (Test-Path -LiteralPath (Join-Path $fixture '.config/wtm/config.json'))) { throw 'Registry was not written at the required home path' }
    $output = (& $Binary 2>&1 | Out-String)
    $exit = $LASTEXITCODE
    [IO.File]::WriteAllText((Join-Path $fixture 'snapshot.txt'), $output)
    $normalized = $output.Replace('\', '/').ToLowerInvariant()
    foreach ($path in $paths) {
        if (-not $normalized.Contains($path.Replace('\', '/').ToLowerInvariant())) { throw "Worktree missing from snapshot. $path" }
    }
    if ([regex]::Matches($output, '\+0\s+-0').Count -lt $clean) { throw "Expected at least $clean clean worktrees" }
    if ($missing -gt 0 -and ($exit -eq 0 -or [regex]::Matches($output, '(?i)error').Count -lt $missing)) { throw 'Missing worktrees were not reported as errors' }
    if ($missing -eq 0 -and $exit -ne 0) { throw "Snapshot failed with $exit" }
    Write-Host "PASS. $($paths.Count) existing worktrees discovered, $clean clean, $missing missing. No worktrees created or changed. Evidence at $fixture"
} finally {
    $env:HOME = $oldHome
    $env:USERPROFILE = $oldProfile
}
