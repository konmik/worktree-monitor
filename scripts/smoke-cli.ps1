param(
    [Parameter(Position = 0)]
    [string]$Binary = ""
)

. (Join-Path $PSScriptRoot 'verification-directories.ps1')
try {
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

function Invoke-Wtm {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$WtmArgs)
    if ($null -eq $WtmArgs) { $WtmArgs = @() }
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
    foreach ($arg in $WtmArgs) {
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
            throw "FAIL timeout: wtm $($WtmArgs -join ' ')"
        }
        [void]$proc.WaitForExit()
        $result = [pscustomobject]@{
            ExitCode = $proc.ExitCode
            Stdout = $outTask.GetAwaiter().GetResult()
            Stderr = $errTask.GetAwaiter().GetResult()
        }
        Add-Transcript ("wtm " + ($WtmArgs -join " ")) $result
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
        "--help"
    )) {
        if (-not $out.Contains($needle)) {
            throw "FAIL help missing [$needle]`nstdout: [$out]"
        }
    }
}

try {
    $repoRoot = Split-Path -Parent $PSScriptRoot
    if ([string]::IsNullOrWhiteSpace($Binary)) {
        $resolvedBinary = Join-Path $repoRoot "target\debug\wtm.exe"
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
    $parentConfig = Join-Path $parentProfile ".config\wtm\config.json"
    $parentExisted = Test-Path -LiteralPath $parentConfig
    $parentHash = $null
    if ($parentExisted) {
        $parentHash = (Get-FileHash -LiteralPath $parentConfig -Algorithm SHA256).Hash
    }

    $verificationEvidence = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot "../target/verification/smoke-cli-evidence-$([Guid]::NewGuid().ToString('N'))"))

    $artifact = New-VerificationDirectory 'smoke-cli' $verificationEvidence
    $isolatedHome = Join-Path $artifact "home"
    $repoPath = Join-Path $artifact "repo"
    New-Item -ItemType Directory -Path $isolatedHome | Out-Null
    New-Item -ItemType Directory -Path $repoPath | Out-Null
    $isolatedHome = [System.IO.Path]::GetFullPath($isolatedHome)
    $repoPath = [System.IO.Path]::GetFullPath($repoPath)
    if ($isolatedHome -eq $parentProfile) {
        throw "isolated home matches parent profile"
    }
    $configPath = Join-Path $isolatedHome ".config\wtm\config.json"
    [System.IO.File]::WriteAllText((Join-Path $artifact "transcript.txt"), "binary: $resolvedBinary`r`nhome: $isolatedHome`r`nconfig: $configPath`r`n", $utf8)

    Assert-Help (Invoke-Wtm help)
    $usage = Invoke-Wtm
    if ($usage.ExitCode -ne 2 -or $usage.Stdout -ne '' -or $usage.Stderr -notmatch 'Usage:') { throw 'Default command did not show usage' }
    Assert-Unregistered "empty"

    [void](Invoke-Git -GitArgs @("init", "-q"))
    [void](Invoke-Git -GitArgs @("config", "core.autocrlf", "false"))
    Write-Text (Join-Path $repoPath ".gitignore") "ignored.txt`n"
    Write-Text (Join-Path $repoPath "tracked.txt") "alpha`nbravo`ncharlie`ndelta`n"
    [void](Invoke-Git -GitArgs @("add", "--", ".gitignore", "tracked.txt"))
    [void](Invoke-Git -GitArgs @(
        "-c", "user.name=wtm-smoke",
        "-c", "user.email=wtm-smoke@example.com",
        "-c", "commit.gpgsign=false",
        "-c", "core.hooksPath=$artifact",
        "commit", "-q", "-m", "baseline"
    ))
    $fromGit = [string](@(Invoke-Git -GitArgs @("rev-parse", "--path-format=absolute", "--git-common-dir"))[0])
    $common = [System.IO.Path]::GetFullPath($fromGit.Trim())
    $expectedCommon = [System.IO.Path]::GetFullPath((Join-Path $repoPath ".git"))
    if ($common -cne $expectedCommon) {
        throw "fixture common dir [$common] expected [$expectedCommon]"
    }
    $sub = Join-Path $repoPath "sub"
    New-Item -ItemType Directory -Path $sub | Out-Null
    $sub = [System.IO.Path]::GetFullPath($sub)
    $plain = Join-Path $artifact "plain"
    New-Item -ItemType Directory -Path $plain | Out-Null
    $plain = [System.IO.Path]::GetFullPath($plain)
    Write-Text (Join-Path $plain '.git') 'invalid'
    $cleanListed = @(Invoke-Git -GitArgs @("status", "--porcelain") | Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) })
    if ($cleanListed.Count -ne 0) {
        throw "fixture not clean: $($cleanListed -join ', ')"
    }

    Assert-Result "add" (Invoke-Wtm add $sub) 0 "Repository registered.`n" ""
    Assert-Registered "add" @($common)
    Assert-Result "duplicate-add" (Invoke-Wtm add $sub) 0 "Repository already registered.`n" ""
    Assert-Registered "duplicate-add" @($common)
    Assert-Result "remove" (Invoke-Wtm remove $sub) 0 "Repository removed.`n" ""
    Assert-Unregistered "remove"
    Assert-Result "duplicate-remove" (Invoke-Wtm remove $sub) 0 "Repository is not registered.`n" ""
    Assert-Unregistered "duplicate-remove"
    $invalid = Invoke-Wtm add $plain
    if ($invalid.ExitCode -ne 1 -or $invalid.Stdout -ne '' -or $invalid.Stderr -notmatch 'invalid gitfile format') { throw 'Invalid repository was accepted' }
    Assert-Unregistered "add-invalid"

    Write-Text $configPath '{broken'
    $malformed = Invoke-Wtm add $sub
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
        Write-Output "artifact: $verificationEvidence"
    }
}
exit $code

} finally { Remove-VerificationDirectories }
