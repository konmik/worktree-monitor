param(
    [Parameter(Position = 0)]
    [string]$Binary = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
$PSNativeCommandUseErrorActionPreference = $false

$artifact = $null
$code = 1
$utf8 = New-Object System.Text.UTF8Encoding $false

function Normalize {
    param([AllowNull()][string]$Text)
    if ($null -eq $Text) { return "" }
    return (($Text -replace "`r`n", "`n") -replace "`r", "`n")
}

function Add-Transcript {
    param([string]$Title, $Result)
    $block = "=== $Title`r`nexit: $($Result.ExitCode)`r`nstdout:`r`n$($Result.Stdout)`r`nstderr:`r`n$($Result.Stderr)`r`n"
    [System.IO.File]::AppendAllText((Join-Path $artifact "transcript.txt"), $block, $utf8)
}

function Write-Text {
    param([string]$Path, [string]$Text)
    [System.IO.File]::WriteAllText($Path, $Text, $utf8)
}

function Invoke-Git {
    param([int[]]$Ok = @(0), [string[]]$GitArgs)
    $errFile = Join-Path $artifact "git-stderr.txt"
    $output = & git -c core.fsmonitor=false -C $repoPath @GitArgs 2>$errFile
    $exit = $LASTEXITCODE
    if ($Ok -notcontains $exit) {
        $err = ""
        if (Test-Path -LiteralPath $errFile) {
            $err = [System.IO.File]::ReadAllText($errFile)
        }
        throw "git failed ($exit): git $($GitArgs -join ' ')`n$err"
    }
    if ($null -ne $output) {
        return $output
    }
}

function Get-Numstat {
    param([string[]]$GitArgs)
    $add = 0
    $del = 0
    foreach ($line in @(Invoke-Git -Ok @(0, 1) -GitArgs $GitArgs)) {
        if ($null -eq $line) { continue }
        $text = [string]$line
        if ([string]::IsNullOrWhiteSpace($text)) { continue }
        $parts = $text -split "`t"
        if ($parts.Count -lt 2) { throw "bad numstat: $text" }
        if ($parts[0] -eq "-" -or $parts[1] -eq "-") { throw "unexpected binary numstat: $text" }
        $add += [int]$parts[0]
        $del += [int]$parts[1]
    }
    return @{ Add = $add; Del = $del }
}

function Invoke-Wts {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$WtsArgs)
    if ($null -eq $WtsArgs) { $WtsArgs = @() }
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $resolvedBinary
    $psi.WorkingDirectory = $isolatedHome
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.RedirectStandardInput = $true
    $psi.CreateNoWindow = $true
    $psi.StandardOutputEncoding = $utf8
    $psi.StandardErrorEncoding = $utf8
    foreach ($arg in $WtsArgs) {
        [void]$psi.ArgumentList.Add($arg)
    }
    $psi.Environment["USERPROFILE"] = $isolatedHome
    $psi.Environment["HOME"] = Join-Path $artifact 'unix-home'
    $proc = New-Object System.Diagnostics.Process
    $proc.StartInfo = $psi
    try {
        [void]$proc.Start()
        $proc.StandardInput.Close()
        $outTask = $proc.StandardOutput.ReadToEndAsync()
        $errTask = $proc.StandardError.ReadToEndAsync()
        if (-not $proc.WaitForExit(60000)) {
            try { $proc.Kill($true) } catch { try { $proc.Kill() } catch {} }
            throw "FAIL timeout: wts $($WtsArgs -join ' ')"
        }
        [void]$proc.WaitForExit()
        $result = [pscustomobject]@{
            ExitCode = $proc.ExitCode
            Stdout = $outTask.GetAwaiter().GetResult()
            Stderr = $errTask.GetAwaiter().GetResult()
        }
        Add-Transcript ("wts " + ($WtsArgs -join " ")) $result
        return $result
    } finally {
        $proc.Dispose()
    }
}

function Assert-Result {
    param(
        [string]$Name,
        $Result,
        [int]$ExitCode,
        [string]$Stdout,
        [string]$Stderr
    )
    $out = Normalize $Result.Stdout
    $err = Normalize $Result.Stderr
    $expOut = Normalize $Stdout
    $expErr = Normalize $Stderr
    if ($Result.ExitCode -ne $ExitCode -or $out -ne $expOut -or $err -ne $expErr) {
        throw "FAIL $Name`nexit: $($Result.ExitCode) expected $ExitCode`nstdout: [$out]`nexpected stdout: [$expOut]`nstderr: [$err]`nexpected stderr: [$expErr]"
    }
}

function Read-Config {
    if (-not (Test-Path -LiteralPath $configPath)) { return $null }
    $raw = [System.IO.File]::ReadAllText($configPath)
    try {
        $json = $raw | ConvertFrom-Json
    } catch {
        throw "FAIL config is not JSON: $raw"
    }
    return @{ Raw = $raw; Json = $json }
}

function Get-CommonDirs {
    param($Parsed)
    if ($null -eq $Parsed) { return @() }
    $json = $Parsed.Json
    if ($null -eq $json.PSObject.Properties["repos"]) {
        throw "FAIL config missing repos: $($Parsed.Raw)"
    }
    $dirs = @()
    foreach ($item in @($json.repos)) {
        if ($null -eq $item) { continue }
        if ($item -isnot [string]) {
            throw "FAIL config repo is not a string: $($Parsed.Raw)"
        }
        $dirs += [string]$item
    }
    if ($dirs.Count -eq 0) { return }
    return $dirs
}

function Assert-Registered {
    param([string]$Name, [string[]]$Expected)
    $parsed = Read-Config
    if ($null -eq $parsed) {
        throw "FAIL $Name config missing: $configPath"
    }
    $actual = @(Get-CommonDirs $parsed)
    $want = @($Expected)
    if ($actual.Count -ne $want.Count) {
        throw "FAIL $Name config count $($actual.Count) expected $($want.Count) raw: $($parsed.Raw)"
    }
    for ($i = 0; $i -lt $want.Count; $i++) {
        if ($actual[$i] -cne $want[$i]) {
            throw "FAIL $Name config path [$($actual[$i])] expected [$($want[$i])] raw: $($parsed.Raw)"
        }
    }
}

function Assert-Unregistered {
    param([string]$Name)
    $parsed = Read-Config
    if ($null -eq $parsed) { return }
    $actual = @(Get-CommonDirs $parsed)
    if ($actual.Count -ne 0) {
        throw "FAIL $Name config still has [$($actual -join ', ')] raw: $($parsed.Raw)"
    }
}

function Assert-Snapshot {
    param($Result, [int]$Add, [int]$Del)
    if ($Result.ExitCode -ne 0 -or $Result.Stderr -ne '') { throw "snapshot failed: $($Result.Stderr)" }
    $out = Normalize $Result.Stdout
    if (-not $out.Replace('\', '/').Contains($worktreeDisplay.Replace('\', '/'))) { throw 'snapshot lacks the worktree path' }
    if (-not $out.Contains($branch)) { throw 'snapshot lacks the branch' }
    $counts = [regex]::Matches($out, '\bS\s+\+(\d+)/-(\d+)\b')
    if ($counts.Count -ne 1 -or [int]$counts[0].Groups[1].Value -ne $Add -or [int]$counts[0].Groups[2].Value -ne $Del) {
        throw "snapshot expected S +$Add/-$Del, got $out"
    }
}

function Assert-Help {
    param($Result)
    $out = Normalize $Result.Stdout
    $err = Normalize $Result.Stderr
    if ($Result.ExitCode -ne 0 -or $err -ne "") {
        throw "FAIL help exit $($Result.ExitCode)`nstdout: [$out]`nstderr: [$err]"
    }
    foreach ($needle in @(
        "add",
        "remove",
        "--live"
    )) {
        if (-not $out.Contains($needle)) {
            throw "FAIL help missing [$needle]`nstdout: [$out]"
        }
    }
}

try {
    $repoRoot = Split-Path -Parent $PSScriptRoot
    if ([string]::IsNullOrWhiteSpace($Binary)) {
        $resolvedBinary = Join-Path $repoRoot "target\debug\wts.exe"
    } elseif (-not [System.IO.Path]::IsPathRooted($Binary)) {
        $resolvedBinary = Join-Path $repoRoot $Binary
    } else {
        $resolvedBinary = $Binary
    }
    $resolvedBinary = [System.IO.Path]::GetFullPath($resolvedBinary)
    if (-not (Test-Path -LiteralPath $resolvedBinary)) {
        throw "binary not found: $resolvedBinary"
    }

    $parentProfile = [Environment]::GetEnvironmentVariable("USERPROFILE")
    $parentConfig = Join-Path $parentProfile ".config\wts\config.json"
    $parentExisted = Test-Path -LiteralPath $parentConfig
    $parentHash = $null
    if ($parentExisted) {
        $parentHash = (Get-FileHash -LiteralPath $parentConfig -Algorithm SHA256).Hash
    }

    $artifact = Join-Path (Join-Path $env:LOCALAPPDATA 'Temp/opencode') ("wts-smoke-" + [guid]::NewGuid().ToString("n"))
    $isolatedHome = Join-Path $artifact "home"
    $repoPath = Join-Path $artifact "repo"
    New-Item -ItemType Directory -Path $isolatedHome | Out-Null
    New-Item -ItemType Directory -Path $repoPath | Out-Null
    $isolatedHome = [System.IO.Path]::GetFullPath($isolatedHome)
    $repoPath = [System.IO.Path]::GetFullPath($repoPath)
    if ($isolatedHome -eq $parentProfile) {
        throw "isolated home matches parent profile"
    }
    $configPath = Join-Path $isolatedHome ".config\wts\config.json"
    [System.IO.File]::WriteAllText((Join-Path $artifact "transcript.txt"), "binary: $resolvedBinary`r`nhome: $isolatedHome`r`nconfig: $configPath`r`n", $utf8)

    $empty = "No registered repositories.`n"
    Assert-Help (Invoke-Wts help)
    Assert-Result "empty" (Invoke-Wts) 0 $empty ""
    Assert-Unregistered "empty"

    [void](Invoke-Git -GitArgs @("init", "-q"))
    [void](Invoke-Git -GitArgs @("config", "core.autocrlf", "false"))
    Write-Text (Join-Path $repoPath ".gitignore") "ignored.txt`n"
    Write-Text (Join-Path $repoPath "tracked.txt") "alpha`nbravo`ncharlie`ndelta`n"
    [void](Invoke-Git -GitArgs @("add", "--", ".gitignore", "tracked.txt"))
    [void](Invoke-Git -GitArgs @(
        "-c", "user.name=wts-smoke",
        "-c", "user.email=wts-smoke@example.com",
        "-c", "commit.gpgsign=false",
        "-c", "core.hooksPath=$artifact",
        "commit", "-q", "-m", "baseline"
    ))
    $branch = [string](@(Invoke-Git -GitArgs @("branch", "--show-current"))[0])
    $branch = $branch.Trim()
    if ([string]::IsNullOrWhiteSpace($branch)) { throw "fixture has no branch" }
    $fromGit = [string](@(Invoke-Git -GitArgs @("rev-parse", "--path-format=absolute", "--git-common-dir"))[0])
    $common = [System.IO.Path]::GetFullPath($fromGit.Trim())
    $worktree = $repoPath
    $expectedCommon = [System.IO.Path]::GetFullPath((Join-Path $repoPath ".git"))
    if ($common -cne $expectedCommon) {
        throw "fixture common dir [$common] expected [$expectedCommon]"
    }
    $porcelain = @(Invoke-Git -GitArgs @("worktree", "list", "--porcelain"))
    $worktreeDisplay = $null
    foreach ($line in $porcelain) {
        $text = [string]$line
        if ($text.StartsWith("worktree ")) {
            $worktreeDisplay = $text.Substring(9).Trim()
            break
        }
    }
    if ([string]::IsNullOrWhiteSpace($worktreeDisplay)) { throw "fixture missing worktree path" }
    $sub = Join-Path $repoPath "sub"
    New-Item -ItemType Directory -Path $sub | Out-Null
    $sub = [System.IO.Path]::GetFullPath($sub)
    $plain = Join-Path $artifact "plain"
    New-Item -ItemType Directory -Path $plain | Out-Null
    $plain = [System.IO.Path]::GetFullPath($plain)
    $cleanListed = @(Invoke-Git -GitArgs @("status", "--porcelain") | Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) })
    if ($cleanListed.Count -ne 0) {
        throw "fixture not clean: $($cleanListed -join ', ')"
    }

    Assert-Result "add" (Invoke-Wts add $sub) 0 "Repository registered.`n" ""
    Assert-Registered "add" @($common)
    Assert-Result "duplicate-add" (Invoke-Wts add $sub) 0 "Repository already registered.`n" ""
    Assert-Registered "duplicate-add" @($common)
    Assert-Snapshot (Invoke-Wts) 0 0

    Write-Text (Join-Path $repoPath "tracked.txt") "alpha`nBRAVO`ncharlie`ndelta`n"
    [void](Invoke-Git -GitArgs @("add", "--", "tracked.txt"))
    Write-Text (Join-Path $repoPath "tracked.txt") "alpha`nbravo`ncharlie`nECHO`n"
    Write-Text (Join-Path $repoPath "notes.txt") "one`ntwo`nthree`n"
    Write-Text (Join-Path $repoPath "ignored.txt") "secret1`nsecret2`nsecret3`nsecret4`n"
    $head = Get-Numstat -GitArgs @("diff", "--numstat", "--no-color", "--no-ext-diff", "--no-textconv", "--find-renames=50%", "-l0", "--diff-algorithm=myers", "--no-indent-heuristic", "--ignore-submodules=all", "HEAD", "--")
    $cached = Get-Numstat -GitArgs @("diff", "--cached", "--numstat", "HEAD", "--")
    $work = Get-Numstat -GitArgs @("diff", "--numstat", "--")
    $notes = Get-Numstat -GitArgs @("diff", "--no-index", "--numstat", "--no-color", "--no-ext-diff", "--no-textconv", "--", "/dev/null", "notes.txt")
    if ($head.Add -ne 1 -or $head.Del -ne 1) {
        throw "fixture net HEAD +$($head.Add) -$($head.Del) expected +1 -1"
    }
    if ($notes.Add -ne 3 -or $notes.Del -ne 0) {
        throw "fixture untracked +$($notes.Add) -$($notes.Del) expected +3 -0"
    }
    if (($cached.Add + $work.Add) -eq $head.Add -and ($cached.Del + $work.Del) -eq $head.Del) {
        throw "fixture does not distinguish net HEAD from summed diffs"
    }
    $listed = @(Invoke-Git -GitArgs @("ls-files", "--others", "--exclude-standard") | Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) })
    if ($listed.Count -ne 1 -or [string]$listed[0] -ne "notes.txt") {
        throw "fixture untracked [$($listed -join ', ')]"
    }
    Assert-Snapshot (Invoke-Wts) 4 1

    Assert-Result "remove" (Invoke-Wts remove $sub) 0 "Repository removed.`n" ""
    Assert-Unregistered "remove"
    Assert-Result "duplicate-remove" (Invoke-Wts remove $sub) 0 "Repository is not registered.`n" ""
    Assert-Unregistered "duplicate-remove"
    Assert-Result "add-plain" (Invoke-Wts add $plain) 1 "" "Error: fatal: not a git repository (or any of the parent directories): .git`n"
    Assert-Unregistered "add-plain"
    Assert-Result "empty-after-remove" (Invoke-Wts) 0 $empty ""

    Write-Text $configPath '{broken'
    $malformed = Invoke-Wts add $sub
    if ($malformed.ExitCode -ne 1 -or [string]::IsNullOrWhiteSpace($malformed.Stderr)) { throw 'malformed configuration was accepted' }
    if ([IO.File]::ReadAllText($configPath) -cne '{broken') { throw 'malformed configuration was overwritten' }

    $parentStill = Test-Path -LiteralPath $parentConfig
    if ($parentStill -ne $parentExisted) {
        throw "parent config presence changed: $parentConfig"
    }
    if ($parentExisted) {
        $nowHash = (Get-FileHash -LiteralPath $parentConfig -Algorithm SHA256).Hash
        if ($nowHash -ne $parentHash) {
            throw "parent config modified: $parentConfig"
        }
    }
    [IO.File]::AppendAllText((Join-Path $artifact 'transcript.txt'), "PASS`n", $utf8)
    $code = 0
} catch {
    $msg = $_.Exception.Message
    if ([string]::IsNullOrWhiteSpace($msg)) { $msg = "$_" }
    [Console]::Error.WriteLine($msg)
    $code = 1
} finally {
    if ($code -eq 0) {
        Write-Output "ok"
    }
    if ($null -ne $artifact) {
        Write-Output "artifact: $artifact"
    }
}
exit $code
