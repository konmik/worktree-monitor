$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'verification-directories.ps1')
$testDirectory = New-VerificationDirectory 'cleanup-tests'
try {
    & {
        . (Join-Path $PSScriptRoot 'verification-directories.ps1')
        $path = New-VerificationDirectory 'cleanup-success'
        try {
            $repo = Join-Path $path 'repo'
            $plain = Join-Path $path 'plain'
            New-Item -ItemType Directory -Path $repo, $plain | Out-Null
            & git -C $repo init --quiet
            if ($LASTEXITCODE -ne 0) { throw 'Cannot initialize test repository' }
            $repositoryRoot = (& git -C $repo rev-parse --show-toplevel | Out-String).Trim()
            if ($LASTEXITCODE -ne 0 -or $repositoryRoot -ne $repo.Replace('\', '/')) { throw 'Child Git repository was not discovered correctly' }
            $PSNativeCommandUseErrorActionPreference = $false
            $discovery = (& git -C $plain rev-parse --show-toplevel 2>&1 | Out-String).Trim()
            if ($LASTEXITCODE -eq 0 -or $discovery -notmatch 'not a git repository') { throw "Plain fixture escaped its Git boundary. $discovery" }
            & git -C (Split-Path $PSScriptRoot -Parent) check-ignore --quiet $path
            if ($LASTEXITCODE -ne 0) { throw 'Verification repository is not Git ignored' }
        } finally { Remove-VerificationDirectories }
        if (Test-Path -LiteralPath $path) { throw 'Successful verification left its repository behind' }
    }
    & {
        . (Join-Path $PSScriptRoot 'verification-directories.ps1')
        $evidence = Join-Path $testDirectory 'evidence'
        $path = New-VerificationDirectory 'cleanup-failure' $evidence
        try {
            try {
                New-Item -ItemType Directory -Path (Join-Path $path 'repo/.git'), (Join-Path $path 'linked') -Force | Out-Null
                Set-Content -LiteralPath (Join-Path $path 'linked/.git') -Value 'gitdir: ../repo/.git/worktrees/linked'
                Set-Content -LiteralPath (Join-Path $path '.git') -Value 'gitdir: repo/.git'
                Set-Content -LiteralPath (Join-Path $path 'proof.json') -Value '{"result":"FAIL"}'
                throw 'Expected verification failure'
            } finally { Remove-VerificationDirectories }
        } catch { if ($_.Exception.Message -ne 'Expected verification failure') { throw } }
        if (Test-Path -LiteralPath $path) { throw 'Failed verification left its repositories behind' }
        if (-not (Test-Path -LiteralPath (Join-Path $evidence 'proof.json'))) { throw 'Verification proof was not retained' }
        if ((Get-ChildItem -LiteralPath $evidence -Force).Count -ne 1) { throw 'Evidence contains fixture metadata' }
    }
    & {
        . (Join-Path $PSScriptRoot 'verification-directories.ps1')
        $blockedEvidence = Join-Path $testDirectory 'blocked-evidence'
        Set-Content -LiteralPath $blockedEvidence -Value 'This is a file'
        $path = New-VerificationDirectory 'cleanup-copy-failure' $blockedEvidence
        New-Item -ItemType Directory -Path (Join-Path $path 'repo/.git') -Force | Out-Null
        $copyFailed = $false
        try { Remove-VerificationDirectories } catch { $copyFailed = $true }
        if (-not $copyFailed) { throw 'Evidence copying unexpectedly succeeded' }
        if (Test-Path -LiteralPath $path) { throw 'Evidence copying failure left its repository behind' }
    }
    & {
        . (Join-Path $PSScriptRoot 'verification-directories.ps1')
        $verificationDirectories.Add(@{ Path = (Split-Path $PSScriptRoot -Parent); EvidenceDirectory = '' })
        try {
            Remove-VerificationDirectories
            throw 'An unsafe deletion path was accepted'
        } catch { if ($_.Exception.Message -notlike 'Refusing to remove verification directory outside*') { throw } }
    }
    Write-Host 'PASS. Verification cleanup handles success, failure, evidence copying, and unsafe paths.'
} finally { Remove-VerificationDirectories }
