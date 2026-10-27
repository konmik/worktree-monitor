param(
    [switch]$Worker,
    [string]$ResultPath
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$names = @('wtm', 'wtm-tui', 'wtm-gui')

if ($Worker) {
    try {
        Set-Location -LiteralPath $root
        $cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
        $log = [IO.Path]::ChangeExtension($ResultPath, '.log')
        $buildErrors = [IO.Path]::ChangeExtension($ResultPath, '.stderr.log')
        $build = Start-Process -FilePath $cargo -ArgumentList 'build', '--release', '--locked', '-p', 'wtm', '--bins' -WindowStyle Hidden -Wait -PassThru -RedirectStandardOutput $log -RedirectStandardError $buildErrors
        Get-Content -LiteralPath $buildErrors | Add-Content -LiteralPath $log
        if ($build.ExitCode -ne 0) { throw "Build failed. See $log" }

        $destination = Join-Path $env:USERPROFILE '.cargo\bin'
        $aliases = Join-Path $env:USERPROFILE 'bin'
        $running = @(Get-Process -Name $names -ErrorAction SilentlyContinue)
        $restartGui = @($running | Where-Object ProcessName -eq 'wtm-gui').Count -gt 0
        $running | Stop-Process -Force
        foreach ($process in $running) { $process.WaitForExit() }

        [void][IO.Directory]::CreateDirectory($destination)
        [void][IO.Directory]::CreateDirectory($aliases)
        $installed = foreach ($name in $names) {
            $source = Join-Path $root "target\release\$name.exe"
            $binary = Join-Path $destination "$name.exe"
            Copy-Item -LiteralPath $source -Destination $binary -Force
            $alias = Join-Path $aliases "$name.exe"
            foreach ($extension in @('exe', 'cmd')) {
                $previous = Join-Path $aliases "$name.$extension"
                if (Test-Path -LiteralPath $previous) { Remove-Item -LiteralPath $previous -Force }
            }
            New-Item -ItemType HardLink -Path $alias -Target $binary | Out-Null
            $hash = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash
            if ((Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash -ne $hash -or
                (Get-FileHash -LiteralPath $alias -Algorithm SHA256).Hash -ne $hash) {
                throw "Installed executable or alias differs from the build for $name"
            }
            $links = @(& fsutil hardlink list $binary)
            if ($LASTEXITCODE -ne 0 -or $links -match '\\Packages\\') {
                throw "Could not confirm an installation outside package storage for $name"
            }
            [ordered]@{ name = $name; binary = $binary; alias = $alias; sha256 = $hash; links = $links }
        }

        & (Join-Path $aliases 'wtm.exe') --version >> $log
        if ($LASTEXITCODE -ne 0) { throw 'Installed CLI failed to start' }
        if ($restartGui) {
            $gui = Start-Process -FilePath (Join-Path $aliases 'wtm-gui.exe') -WorkingDirectory $env:USERPROFILE -WindowStyle Hidden -PassThru
            $deadline = [DateTime]::UtcNow.AddSeconds(15)
            do {
                $gui.Refresh()
                if ($gui.HasExited) { throw "GUI exited with code $($gui.ExitCode)" }
                if ($gui.MainWindowHandle -ne 0) { break }
                Start-Sleep -Milliseconds 100
            } while ([DateTime]::UtcNow -lt $deadline)
            if ($gui.MainWindowHandle -eq 0) { throw 'GUI did not open a window' }
        }
        $result = @{ success = $true; installed = @($installed); gui_restarted = $restartGui }
    } catch {
        $result = @{ success = $false; error = $_.Exception.Message }
    }
    $result | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath "$ResultPath.tmp" -Encoding UTF8
    Move-Item -LiteralPath "$ResultPath.tmp" -Destination $ResultPath
    exit
}

$output = Join-Path $root "target\deploy\$([Guid]::NewGuid().ToString('N'))"
[void][IO.Directory]::CreateDirectory($output)
$ResultPath = Join-Path $output 'result.json'
$scriptPath = $PSCommandPath.Replace("'", "''")
$quotedResult = $ResultPath.Replace("'", "''")
$command = "& '$scriptPath' -Worker -ResultPath '$quotedResult'"
$encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($command))
$powershell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
$startup = New-CimInstance -ClassName Win32_ProcessStartup -ClientOnly -Property @{ ShowWindow = [uint16]0 }
$process = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{
    CommandLine = "`"$powershell`" -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand $encoded"
    ProcessStartupInformation = $startup
}
if ($process.ReturnValue -ne 0) { throw "Could not start deployment outside the app. WMI returned $($process.ReturnValue)" }
Write-Output "Deployment log: $([IO.Path]::ChangeExtension($ResultPath, '.log'))"
$deadline = [DateTime]::UtcNow.AddMinutes(30)
while (-not (Test-Path -LiteralPath $ResultPath)) {
    if ($null -eq (Get-Process -Id $process.ProcessId -ErrorAction SilentlyContinue) -and -not (Test-Path -LiteralPath $ResultPath)) { throw 'Deployment process exited without a result' }
    if ([DateTime]::UtcNow -ge $deadline) { throw 'Deployment timed out' }
    Start-Sleep -Milliseconds 200
}
$result = Get-Content -LiteralPath $ResultPath -Raw | ConvertFrom-Json
if (-not $result.success) { throw $result.error }
$result.installed | Select-Object name, binary, alias
