param(
    [switch]$Worker,
    [string]$ResultPath
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

if ($Worker) {
    try {
        Set-Location -LiteralPath $root
        $installation = Join-Path $env:LOCALAPPDATA 'Programs\WorktreeMonitor'
        $aliases = Join-Path $env:USERPROFILE 'bin'
        $log = [IO.Path]::ChangeExtension($ResultPath, '.log')
        $cargo = Get-Command cargo -ErrorAction Stop
        try {
            $ErrorActionPreference = 'Continue'
            & $cargo install --root $installation --path crates/wtm --locked --bins --force 2>&1 | Out-File -LiteralPath $log
            $cargoExit = $LASTEXITCODE
        } finally {
            $ErrorActionPreference = 'Stop'
        }
        if ($cargoExit -ne 0) { throw "cargo install failed with exit code $cargoExit. See $log" }
        [void][IO.Directory]::CreateDirectory($aliases)
        $installed = foreach ($name in @('wtm', 'wtm-tui', 'wtm-gui')) {
            $binary = Join-Path $installation "bin\$name.exe"
            $alias = Join-Path $aliases "$name.exe"
            $links = @(& fsutil hardlink list $binary)
            if ($LASTEXITCODE -ne 0 -or $links -match '\\Packages\\' -or
                $links -notcontains $binary.Substring([IO.Path]::GetPathRoot($binary).Length - 1)) {
                throw "Could not confirm an installation outside package storage for $name"
            }
            if (Test-Path -LiteralPath $alias) { Remove-Item -LiteralPath $alias -Force }
            New-Item -ItemType HardLink -Path $alias -Target $binary | Out-Null
            $links = @(& fsutil hardlink list $binary)
            if ($LASTEXITCODE -ne 0 -or $links -match '\\Packages\\' -or
                $links -notcontains $binary.Substring([IO.Path]::GetPathRoot($binary).Length - 1) -or
                $links -notcontains $alias.Substring([IO.Path]::GetPathRoot($alias).Length - 1)) {
                throw "Could not confirm executable links outside package storage for $name"
            }
            [ordered]@{ name = $name; binary = $binary; alias = $alias; sha256 = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash; links = $links }
        }
        $result = @{ success = $true; installed = @($installed) }
    } catch {
        $result = @{ success = $false; error = $_.Exception.Message }
    }
    $result | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath "$ResultPath.tmp" -Encoding UTF8
    Move-Item -LiteralPath "$ResultPath.tmp" -Destination $ResultPath
    if (-not $result.success) { exit 1 }
    exit 0
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
