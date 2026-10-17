param(
    [Parameter(Mandatory)]
    [ValidateSet('registration', 'launch', 'live', 'ordering', 'resize', 'capture', 'appearance')]
    [string]$Check,
    [string]$Output = "$env:LOCALAPPDATA/Temp/opencode/verify-wtm-$([Guid]::NewGuid().ToString('N'))",
    [switch]$DoctorOnly,
    [switch]$ValidateOnly
)

$ErrorActionPreference = 'Stop'
$verificationRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../..'))
$VerificationOriginalScriptRoot = Join-Path $verificationRoot 'scripts'
$verificationCli = Join-Path $verificationRoot 'target/debug/wtm.exe'
$verificationUi = Join-Path $verificationRoot 'target/debug/wtm-ui.exe'
$Output = [IO.Path]::GetFullPath($Output)
[void][IO.Directory]::CreateDirectory($Output)
$verificationResult = [ordered]@{ check = $Check; result = 'FAIL'; artifacts = @(); failure = $null }

function Replace-HarnessText([string]$Text, [string]$Pattern, [string]$Replacement) {
    $expression = [regex]::new($Pattern, [Text.RegularExpressions.RegexOptions]::Singleline)
    if ($expression.Matches($Text).Count -ne 1) { throw "Harness source changed; expected one match for $Pattern" }
    $expression.Replace($Text, [Text.RegularExpressions.MatchEvaluator]{ param($match) $Replacement })
}

function Get-Harness {
    $name = switch ($Check) {
        registration { 'smoke-cli.ps1' }
        launch { 'smoke-ui.ps1' }
        live { 'smoke-window.ps1' }
        ordering { 'smoke-dashboard.ps1' }
        resize { 'smoke-resize.ps1' }
        capture { 'capture-window.ps1' }
        appearance { 'smoke-indicator-opacity.ps1' }
    }
    $harnessPath = Join-Path $VerificationOriginalScriptRoot $name
    $verificationResult['harness'] = $harnessPath
    $verificationResult['harness_sha256'] = (Get-FileHash -LiteralPath $harnessPath).Hash
    $text = [IO.File]::ReadAllText($harnessPath)
    $text = $text.Replace('$PSScriptRoot', '$VerificationOriginalScriptRoot')
    if ($Check -eq 'registration') {
        $text = Replace-HarnessText $text '(?m)^        "--live"\r?$' '        "--help", "--version"'
        $text = Replace-HarnessText $text 'Assert-Help \(Invoke-Wtm help\)' @'
Assert-Help (Invoke-Wtm help)
    Assert-Help (Invoke-Wtm --help)
    $version = Invoke-Wtm --version
    if ($version.ExitCode -ne 0 -or $version.Stdout.Trim() -ne "wtm $verificationExpectedVersion") { throw 'CLI version mismatch' }
    $removedLive = Invoke-Wtm --live
    if ($removedLive.ExitCode -ne 2 -or $removedLive.Stderr -notmatch '\-\-live') { throw 'Removed live flag was accepted' }
'@
        $text = Replace-HarnessText $text '    \$counts = \[regex\]::Matches\(\$out,' @'
    if ($out -match 'binaries:') { throw 'Source-only fixture unexpectedly reports binaries' }
    $counts = [regex]::Matches($out,
'@
        $text = Replace-HarnessText $text '(?m)^exit \$code\r?$' 'if ($code -ne 0) { throw "CLI verification failed with exit $code" }'
    }
    if ($Check -eq 'launch') {
        $text = Replace-HarnessText $text '    if \(\(Get-FileHash -LiteralPath \$Binary -Algorithm SHA256\)\.Hash -ne \$proof.instance.sha256\)' @'
    $duplicate = [Diagnostics.Process]::Start($start)
    try {
        if (-not $duplicate.WaitForExit(5000)) { throw 'Second UI instance did not exit' }
        if ($duplicate.ExitCode -ne 0 -or [WtmUiProbe]::Find([uint32]$duplicate.Id) -ne [IntPtr]::Zero) { throw 'Second UI instance opened a window or failed' }
        if ($process.HasExited -or -not [WtmUiProbe]::IsWindowVisible($handle)) { throw 'Second launch disturbed the first UI' }
        $proof['second_instance_rejected'] = $true
    } finally {
        if (-not $duplicate.HasExited) { $duplicate.Kill($true); $duplicate.WaitForExit() }
        $duplicate.Dispose()
    }
    if ((Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash -ne $proof.instance.sha256)
'@
    }
    if ($Check -eq 'live') {
        $text = Replace-HarnessText $text '    \$start = \[Diagnostics.ProcessStartInfo\]::new\(\$Binary\).*?(?=    \$initialStyle =)' @'
    $uiBinary = Join-Path (Split-Path -Parent $Binary) 'wtm-ui.exe'
    $start = [Diagnostics.ProcessStartInfo]::new($uiBinary)
    $start.WorkingDirectory = $fixtureHome
    $start.UseShellExecute = $false
    $script:process = [Diagnostics.Process]::Start($start)
    [void]$script:process.Handle
    Wait-For {
        if ($script:process.HasExited) { throw 'Direct UI exited before its window appeared' }
        $script:handle = [WtmWindowProbe]::Find([uint32]$script:process.Id)
        $script:handle -ne [IntPtr]::Zero
    } 'visible direct UI window'
    if ([WtmWindowProbe]::SharesConsole([uint32]$script:process.Id)) { throw 'UI shares the launching console' }
    $uiHash = (Get-FileHash -LiteralPath $uiBinary -Algorithm SHA256).Hash
    Write-Host 'PASS. Direct UI owns a window detached from the launching console.'

'@
        $text = Replace-HarnessText $text '        launcher_exited = \$launcher.HasExited.*?        launcher_pid = \$launcher.Id\r?\n' ''
        $text = $text.Replace('PASS. Terminal detached, always on top,', 'PASS. Direct UI detached, always on top,')
    }
    if ($Check -eq 'ordering') {
        $text = Replace-HarnessText $text '(function Start-Dashboard\(\[int\]\$ExpectedCards = 8\) \{\r?\n).*?(?=    \$dpi =)' @'
function Start-Dashboard([int]$ExpectedCards = 8) {
    $uiBinary = Join-Path (Split-Path -Parent $Binary) 'wtm-ui.exe'
    $start = [Diagnostics.ProcessStartInfo]::new($uiBinary)
    $start.WorkingDirectory = $launchDirectory
    $start.UseShellExecute = $false
    $script:process = [Diagnostics.Process]::Start($start)
    [void]$script:process.Handle
    $script:handle = [IntPtr]::Zero
    Wait-For {
        if ($script:process.HasExited) { throw 'Direct UI exited before its window appeared' }
        $script:handle = [WtmDragProbe]::Find([uint32]$script:process.Id)
        $script:handle -ne [IntPtr]::Zero
    } 'direct dashboard window'

'@
    }
    if ($Check -in @('live', 'ordering', 'capture', 'appearance')) {
        if ([regex]::Matches($text, '(?m)^\$fixture = [^\r\n]+\r?$').Count -ne 1) { throw 'Harness fixture declaration changed' }
        $text = [regex]::Replace($text, '(?m)^(\$fixture = [^\r\n]+)\r?$', '$1' + "`n" + 'Write-Output "Fixture path: $fixture"')
    }
    [ScriptBlock]::Create($text)
}

$verificationMutex = [Threading.Mutex]::new($false, 'Local\wtm-verification-drive')
$verificationLock = $false
Start-Transcript -Path (Join-Path $Output 'session.txt') | Out-Null
try {
    $verificationLock = $verificationMutex.WaitOne(0)
    if (-not $verificationLock) { throw 'Another verification drive is using the desktop' }
    $metadataText = & cargo metadata --manifest-path (Join-Path $verificationRoot 'Cargo.toml') --no-deps --locked --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Could not read expected version' }
    $metadata = $metadataText | ConvertFrom-Json
    $verificationExpectedVersion = ($metadata.packages | Where-Object name -eq 'wtm').version
    $version = & $verificationCli --version
    if ($LASTEXITCODE -ne 0 -or $version -ne "wtm $verificationExpectedVersion") { throw 'WTM doctor version mismatch' }
    $verificationResult['doctor'] = [ordered]@{
        version = $version
        cli = $verificationCli
        cli_sha256 = (Get-FileHash -LiteralPath $verificationCli).Hash
        ui = $verificationUi
        ui_sha256 = (Get-FileHash -LiteralPath $verificationUi).Hash
        revision = (& git -C $verificationRoot rev-parse HEAD | Out-String).Trim()
        dirty = @(& git -C $verificationRoot status --short)
    }
    $harness = Get-Harness
    if ($ValidateOnly) {
        $verificationResult.result = 'PASS'
        $verificationResult['drive_performed'] = $false
        return
    }
    if ($Check -ne 'registration') {
        $sessionId = (Get-Process -Id $PID).SessionId
        $existing = @(Get-Process wtm-ui -ErrorAction SilentlyContinue | Where-Object SessionId -eq $sessionId)
        if ($existing.Count) { throw "Existing UI blocks isolated verification: $($existing.Id -join ', ')" }
    }
    if ($DoctorOnly) {
        $verificationResult.result = 'PASS'
        $verificationResult['drive_performed'] = $false
        return
    }
    $arguments = @{ Binary = $verificationCli }
    if ($Check -in @('launch', 'resize', 'capture', 'appearance')) { $arguments.Binary = $verificationUi }
    if ($Check -in @('launch', 'resize')) { $arguments.Output = Join-Path $Output 'artifacts' }
    if ($Check -eq 'capture') { $arguments.Output = Join-Path $Output 'dashboard.png'; $arguments.Client = $true; $arguments.Opaque = $true }
    $adaptedHarness = Join-Path $Output 'adapted-harness.ps1'
    [IO.File]::WriteAllText($adaptedHarness, $harness.ToString())
    & $adaptedHarness @arguments *>&1 | Tee-Object -FilePath (Join-Path $Output 'drive.txt')
    $verificationResult.result = 'PASS'
} catch {
    $verificationResult.failure = $_.Exception.Message
    throw
} finally {
    try {
        $residue = @(Get-CimInstance Win32_Process -Filter "ParentProcessId = $PID" | Where-Object { $_.ExecutablePath -in @($verificationCli, $verificationUi) })
        foreach ($ownedProcess in $residue) {
            $owned = Get-Process -Id $ownedProcess.ProcessId -ErrorAction SilentlyContinue
            if ($owned) {
                [void]$owned.CloseMainWindow()
                if (-not $owned.WaitForExit(5000)) { $owned.Kill($true); $owned.WaitForExit() }
            }
        }
        if ($residue.Count) {
            $verificationResult.result = 'FAIL'
            $verificationResult.failure = "$($verificationResult.failure) Harness left owned processes; cleanup closed them".Trim()
        }
        $driveLog = Join-Path $Output 'drive.txt'
        if (Test-Path -LiteralPath $driveLog) {
            $verificationResult.artifacts = @([IO.File]::ReadAllLines($driveLog) | ForEach-Object {
                if ($_ -match '(?:artifact: |Evidence at |Fixture path: |Real Git fixture and visible text at |Captures at )(.+)$') { $Matches[1].Trim() }
            } | Select-Object -Unique)
        }
        foreach ($artifact in $verificationResult.artifacts) {
            if (-not (Test-Path -LiteralPath $artifact -PathType Container)) { throw "Evidence directory missing: $artifact" }
            foreach ($config in @(Get-ChildItem -LiteralPath $artifact -Filter 'config*.json' -Recurse -File)) {
                if ($config.DirectoryName -like '*\.config\wtm') {
                    $preserved = Join-Path (Join-Path $artifact 'preserved-configs') ([IO.Path]::GetRelativePath($artifact, $config.FullName))
                    [void][IO.Directory]::CreateDirectory((Split-Path -Parent $preserved))
                    Copy-Item -LiteralPath $config.FullName -Destination $preserved -Force
                }
            }
            if ($verificationResult.result -eq 'PASS') {
                $evidenceFiles = @(Get-ChildItem -LiteralPath $artifact -File | Select-Object -ExpandProperty FullName)
                $scratchNames = switch ($Check) {
                    registration { 'home'; 'repo'; 'plain' }
                    launch { 'home'; 'profile'; 'work' }
                    live { 'home'; 'repo' }
                    ordering { 'home'; 'alpha'; 'alpha-tree'; 'beta'; 'beta-tree'; 'current'; 'missing-git-home' }
                    resize { 'home'; 'unix-home'; 'signal-core'; 'edge-gateway'; 'infra-tools' }
                    capture { 'home'; 'unix-home'; 'signal-core'; 'edge-gateway'; 'infra-tools' }
                    appearance { '.config'; 'xdg'; 'dirty'; 'clean' }
                }
                $artifactRoot = [IO.Path]::GetFullPath($artifact)
                foreach ($scratchName in $scratchNames) {
                    $scratchPath = [IO.Path]::GetFullPath((Join-Path $artifactRoot $scratchName))
                    if (-not $scratchPath.StartsWith($artifactRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Scratch path escaped its fixture directory' }
                    if (Test-Path -LiteralPath $scratchPath) { Remove-Item -LiteralPath $scratchPath -Recurse -Force }
                }
                foreach ($evidenceFile in $evidenceFiles) {
                    if (-not (Test-Path -LiteralPath $evidenceFile -PathType Leaf)) { throw "Cleanup lost evidence: $evidenceFile" }
                }
            }
        }
        $verificationResult | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $Output 'result.json') -Encoding utf8NoBOM
        if ($verificationLock) { $verificationMutex.ReleaseMutex() }
        if ($residue.Count) { throw $verificationResult.failure }
    } finally {
        $verificationMutex.Dispose()
        Stop-Transcript | Out-Null
        Write-Output "Verification evidence at $Output"
    }
}
