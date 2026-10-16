param(
    [Parameter(Mandatory)][string]$Baseline,
    [string]$Treatment = 'target/release/examples/measure-monitor.exe',
    [ValidateRange(5, 100)][int]$Samples = 5,
    [ValidateRange(60, 3600)][int]$WarmupSeconds = 60
)

$ErrorActionPreference = 'Stop'
$Baseline = (Resolve-Path -LiteralPath $Baseline).Path
$Treatment = (Resolve-Path -LiteralPath $Treatment).Path
if ((Get-FileHash -LiteralPath $Baseline).Hash -eq (Get-FileHash -LiteralPath $Treatment).Hash) { throw 'Baseline and treatment executables are identical' }
$evidence = Join-Path $env:LOCALAPPDATA "Temp/opencode/wtm-benchmark-$([Guid]::NewGuid().ToString('N'))"
[void][IO.Directory]::CreateDirectory($evidence)
$processes = @()
try {
    $results = @()
    foreach ($entry in @(@('baseline', $Baseline), @('treatment', $Treatment))) {
        $name, $binary = $entry
        $start = [Diagnostics.ProcessStartInfo]::new($binary)
        $start.ArgumentList.Add($Samples.ToString())
        $start.ArgumentList.Add($WarmupSeconds.ToString())
        $start.WorkingDirectory = (Get-Location).Path
        $start.UseShellExecute = $false
        $start.RedirectStandardOutput = $true
        $start.RedirectStandardError = $true
        $start.StandardOutputEncoding = [Text.Encoding]::UTF8
        $start.StandardErrorEncoding = [Text.Encoding]::UTF8
        $process = [Diagnostics.Process]::Start($start)
        $entry = [pscustomobject]@{ Name = $name; Binary = $binary; Process = $process; Output = $null; Errors = $null }
        $processes += $entry
        $entry.Output = $process.StandardOutput.ReadToEndAsync()
        $entry.Errors = $process.StandardError.ReadToEndAsync()
        if (-not $entry.Process.WaitForExit(($WarmupSeconds + 60 * $Samples + 120) * 1000)) { throw "$($entry.Name) timed out" }
        $output = $entry.Output.GetAwaiter().GetResult()
        $errors = $entry.Errors.GetAwaiter().GetResult()
        [IO.File]::WriteAllText((Join-Path $evidence "$($entry.Name).jsonl"), $output)
        [IO.File]::WriteAllText((Join-Path $evidence "$($entry.Name).stderr.txt"), $errors)
        if ($entry.Process.ExitCode -ne 0) { throw "$($entry.Name) failed with exit code $($entry.Process.ExitCode). $errors" }
        if ($errors.Trim()) { throw "$($entry.Name) wrote unexpected stderr. $errors" }
        $measurements = @($output -split '\r?\n' | Where-Object { $_ } | ForEach-Object { $_ | ConvertFrom-Json })
        if ($measurements.Count -ne $Samples) { throw "$($entry.Name) returned $($measurements.Count) samples instead of $Samples" }
        for ($index = 0; $index -lt $Samples; $index++) {
            $sample = $measurements[$index]
            foreach ($property in @('sample', 'elapsed_seconds', 'polling_seconds_last_minute', 'repositories', 'worktrees', 'snapshot')) {
                if ($null -eq $sample.PSObject.Properties[$property]) { throw "$($entry.Name) sample is missing $property" }
            }
            if ($sample.sample -ne ($index + 1)) { throw "$($entry.Name) returned an unexpected sample sequence" }
            $seconds = [double]$sample.polling_seconds_last_minute
            if (-not [double]::IsFinite($seconds) -or $seconds -lt 0) { throw "$($entry.Name) returned an invalid duration" }
            if ($sample.elapsed_seconds -lt ($WarmupSeconds + 60 * ($index + 1))) { throw "$($entry.Name) sampled before its minute elapsed" }
            if ($sample.repositories -lt 1 -or $sample.worktrees -lt 1 -or -not $sample.snapshot) { throw "$($entry.Name) did not monitor any worktrees" }
        }
        $values = @($measurements.polling_seconds_last_minute | Sort-Object)
        $middle = [int][Math]::Floor($values.Count / 2)
        $median = if ($values.Count % 2) { $values[$middle] } else { ($values[$middle - 1] + $values[$middle]) / 2 }
        $results += [pscustomobject]@{
            Name = $entry.Name
            Sha256 = (Get-FileHash -LiteralPath $entry.Binary).Hash
            Samples = $Samples
            MeanSecondsPerMinute = ($values | Measure-Object -Average).Average
            MedianSecondsPerMinute = $median
            MinSecondsPerMinute = $values[0]
            MaxSecondsPerMinute = $values[-1]
            Measurements = $measurements
        }
    }
    $snapshots = @($results | ForEach-Object { $_.Measurements } | ForEach-Object { $_.snapshot } | Select-Object -Unique)
    if ($snapshots.Count -ne 1) { throw "Repository snapshots changed during the benchmark. Evidence at $evidence" }
    $report = [ordered]@{ Method = 'Sequential release monitors, latest published rolling-minute metric sampled every 60 seconds after warmup'; WarmupSeconds = $WarmupSeconds; Results = $results }
    $report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $evidence 'results.json') -Encoding utf8NoBOM
    $results | Select-Object Name, Samples, MeanSecondsPerMinute, MedianSecondsPerMinute, MinSecondsPerMinute, MaxSecondsPerMinute | Format-Table -AutoSize
    Write-Host "Evidence at $evidence"
} finally {
    foreach ($entry in $processes) {
        try {
            if (-not $entry.Process.HasExited) { $entry.Process.Kill($true); $entry.Process.WaitForExit() }
        } catch {
            Write-Warning "Could not stop $($entry.Name). $_"
        } finally {
            $entry.Process.Dispose()
        }
    }
}
