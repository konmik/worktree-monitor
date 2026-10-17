param(
    [Parameter(Mandatory)][string]$Repository,
    [string]$Binary = "$PSScriptRoot/../target/debug/wtm.exe"
)

$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path $Binary).Path
$Repository = (Resolve-Path $Repository).Path

function Git-Text([string]$Directory, [string[]]$Arguments, [int[]]$AcceptCodes = @(0)) {
    $info = [Diagnostics.ProcessStartInfo]::new('git')
    foreach ($argument in @('-c', 'core.fsmonitor=false', '-c', 'diff.autoRefreshIndex=false', '-C', $Directory) + $Arguments) { $info.ArgumentList.Add($argument) }
    $info.UseShellExecute = $false
    $info.RedirectStandardInput = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.Environment['GIT_OPTIONAL_LOCKS'] = '0'
    $process = [Diagnostics.Process]::Start($info)
    try {
        $process.StandardInput.Close()
        $output = $process.StandardOutput.ReadToEndAsync()
        $errors = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(30000)) { throw 'Git baseline command timed out' }
        if ($process.ExitCode -notin $AcceptCodes) { throw "Git failed with $($process.ExitCode). $($errors.GetAwaiter().GetResult())" }
        return $output.GetAwaiter().GetResult()
    } finally {
        if (-not $process.HasExited) { $process.Kill($true); $process.WaitForExit() }
        $process.Dispose()
    }
}

function Get-ExpectedCounts([string]$Directory) {
    $head = (Git-Text $Directory @('rev-parse', '--verify', '--quiet', 'HEAD') @(0, 1)).Trim()
    $base = $head
    if ($head) {
        foreach ($reference in @('refs/heads/main', 'refs/heads/master')) {
            $commit = (Git-Text $Directory @('rev-parse', '--verify', '--quiet', $reference) @(0, 1)).Trim()
            if ($commit) {
                $ancestor = (Git-Text $Directory @('merge-base', $head, $commit) @(0, 1)).Trim()
                if ($ancestor) { $base = $ancestor }
                break
            }
        }
    } else {
        $base = (Git-Text $Directory @('hash-object', '-t', 'tree', '--stdin')).Trim()
    }
    $records = Git-Text $Directory @('diff', '--numstat', '-z', '--no-ext-diff', '--no-textconv', '--find-renames', '--ignore-submodules=all', $base, '--')
    $untracked = (Git-Text $Directory @('ls-files', '--others', '--exclude-standard', '-z')) -split "`0"
    foreach ($path in $untracked) {
        if ($path) { $records += Git-Text $Directory @('diff', '--no-index', '--numstat', '-z', '--no-ext-diff', '--no-textconv', '--', '/dev/null', $path) @(0, 1) }
    }
    $fields = $records -split "`0"
    $counts = @{added = 0L; deleted = 0L; binaries = 0L}
    for ($index = 0; $index -lt $fields.Count; $index++) {
        if (-not $fields[$index]) { continue }
        if ($fields[$index] -notmatch '^(\d+|-)\t(\d+|-)\t(.*)$') { throw 'Malformed reference numstat' }
        if ($Matches[1] -eq '-' -and $Matches[2] -eq '-') { $counts.binaries++ }
        else { $counts.added += [long]$Matches[1]; $counts.deleted += [long]$Matches[2] }
        if ($Matches[3] -eq '') { $index += 2 }
    }
    $counts
}

$records = (Git-Text $Repository @('worktree', 'list', '--porcelain', '-z')) -split "`0`0"
$paths = @()
$expected = @{}
$missing = 0
foreach ($record in $records) {
    if (-not $record -or ($record -split "`0") -contains 'bare') { continue }
    $pathField = ($record -split "`0" | Where-Object { $_.StartsWith('worktree ') })
    if (-not $pathField) { throw 'Worktree record lacks its path' }
    $path = $pathField.Substring(9)
    $paths += $path
    if (-not (Test-Path -LiteralPath $path)) { $missing++; continue }
    $expected[$path] = Get-ExpectedCounts $path
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
    foreach ($path in $expected.Keys) {
        $record = [regex]::Match($output, '(?m)^  ' + [regex]::Escape($path) + '\r?\n    ([^\r\n]+)')
        if (-not $record.Success) { throw "Missing counts for $path" }
        $added = 0L
        $deleted = 0L
        foreach ($group in [regex]::Matches($record.Groups[1].Value, '[STX] \+(\d+)/-(\d+)')) {
            $added += [long]$group.Groups[1].Value
            $deleted += [long]$group.Groups[2].Value
        }
        $binary = [regex]::Match($record.Groups[1].Value, 'binaries: (\d+)')
        $binaries = if ($binary.Success) { [long]$binary.Groups[1].Value } else { 0L }
        $wanted = $expected[$path]
        if ($added -ne $wanted.added -or $deleted -ne $wanted.deleted -or $binaries -ne $wanted.binaries) { throw "Counts disagree with Git for $path" }
    }
    if ($missing -gt 0 -and ($exit -eq 0 -or [regex]::Matches($output, '(?i)error').Count -lt $missing)) { throw 'Missing worktrees were not reported as errors' }
    if ($missing -eq 0 -and $exit -ne 0) { throw "Snapshot failed with $exit" }
    Write-Host "PASS. $($paths.Count) existing worktrees discovered, $($expected.Count) counts verified against Git, $missing missing. No worktrees created or changed. Evidence at $fixture"
} finally {
    $env:HOME = $oldHome
    $env:USERPROFILE = $oldProfile
}
