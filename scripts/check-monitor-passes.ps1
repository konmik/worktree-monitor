param([string]$Binary = "$PSScriptRoot/../target/release/examples/measure-monitor.exe")
$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path $Binary).Path
$root = "$env:LOCALAPPDATA/Temp/opencode/wtm-serial-$([Guid]::NewGuid().ToString('N'))"
$repo = "$root/repo"
$linked = "$root/linked"
$added = "$root/added"
[void][IO.Directory]::CreateDirectory($repo)
function Invoke-FixtureGit([string[]]$Arguments) {
    & git.exe -C $repo -c user.name=Test -c user.email=test@example.invalid -c commit.gpgsign=false -c core.hooksPath=/dev/null @Arguments
    if ($LASTEXITCODE -ne 0) { throw 'Fixture Git failed' }
}
[IO.File]::WriteAllText("$repo/tracked.txt", "before`n")
Invoke-FixtureGit @('init', '-q', '-b', 'main')
Invoke-FixtureGit @('add', '.')
Invoke-FixtureGit @('commit', '-qm', 'Fixture')
Invoke-FixtureGit @('worktree', 'add', '--quiet', '--detach', $linked)
@{repos = @("$repo/.git")} | ConvertTo-Json | Set-Content "$root/config.json"
$start = [Diagnostics.ProcessStartInfo]::new($Binary)
$start.UseShellExecute = $false
$start.WorkingDirectory = $repo
$start.ArgumentList.Add('1')
$start.ArgumentList.Add('0')
$start.Environment['WTM_BENCH_CONFIG'] = "$root/config.json"
$start.Environment['GIT_TRACE2_PERF'] = "$root/git.trace"
$start.RedirectStandardOutput = $true
$start.RedirectStandardError = $true
$process = [Diagnostics.Process]::Start($start)
try {
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    Start-Sleep -Seconds 2
    foreach ($index in 1..200) {
        [IO.File]::WriteAllText("$repo/tracked.txt", "before`nafter`n")
        Start-Sleep -Milliseconds 100
    }
    Start-Sleep -Seconds 5
    Invoke-FixtureGit @('worktree', 'add', '--quiet', '--detach', $added)
    if (-not $process.WaitForExit(80000)) { throw 'Monitor timed out' }
    $text = $stdout.GetAwaiter().GetResult()
    $errors = $stderr.GetAwaiter().GetResult()
    $text | Set-Content "$root/result.json"
    Write-Host "Evidence at $root"
    if ($process.ExitCode -ne 0 -or $errors) { throw "Monitor failed: $errors" }
    $result = $text | ConvertFrom-Json
    if ($result.worktrees -ne 3 -or $text -notmatch 'S \+1/-0') { throw 'Final worktrees or counts are wrong' }
    $events = @(Get-Content "$root/git.trace" | Where-Object { $_ -match '\| (start|exit)\s+\|' })
    $active = 0
    $maximum = 0
    $lastExit = $null
    $passGaps = @()
    foreach ($event in $events) {
        $time = [TimeSpan]::Parse(($event -split ' ')[0])
        if ($event -match '\| start\s+\|') {
            if ($null -ne $lastExit -and ($time - $lastExit).TotalSeconds -ge 5) {
                $passGaps += ($time - $lastExit).TotalSeconds
            }
            $active++
            $maximum = [Math]::Max($maximum, $active)
        } else { $active--; $lastExit = $time }
    }
    if ($maximum -ne 1 -or $active -ne 0) { throw 'Git processes overlapped' }
    if ($passGaps.Count -lt 2 -or ($passGaps | Measure-Object -Minimum).Minimum -lt 9.9) { throw 'Passes did not wait ten seconds after completion' }
    $scans = @($events | Where-Object { $_ -match '\| start\s+\|' -and $_ -match ' diff --numstat ' })
    $counts = @{}
    foreach ($directory in @($repo, $linked, $added)) {
        $pattern = [regex]::Escape($directory.Replace('\', '/')) + ' diff --numstat '
        $counts[$directory] = @($scans | Where-Object { $_ -match $pattern }).Count
    }
    @{writes = 200; maximum_parallel_git = $maximum; pass_gaps_seconds = $passGaps; scans = $counts; polling_seconds = $result.polling_seconds_last_minute} | ConvertTo-Json -Depth 4 | Tee-Object -FilePath "$root/cadence.json"
    if ($counts[$linked] -ne 1) { throw "Unchanged worktree scanned $($counts[$linked]) times" }
    if ($counts[$added] -ne 1) { throw "New worktree scanned $($counts[$added]) times" }
    if ($counts[$repo] -lt 2 -or $counts[$repo] -gt 4) { throw 'Changed worktree scan cadence violated' }
} finally {
    if (-not $process.HasExited) { $process.Kill($true) }
    $process.Dispose()
}
