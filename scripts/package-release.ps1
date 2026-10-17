param(
    [string]$BinaryDirectory = "$PSScriptRoot/../target/release",
    [string]$Output = "$PSScriptRoot/../target/dist"
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$BinaryDirectory = (Resolve-Path -LiteralPath $BinaryDirectory).Path
$metadataText = & cargo metadata --manifest-path "$root/Cargo.toml" --no-deps --locked --format-version 1
if ($LASTEXITCODE -ne 0) { throw 'Could not read package version' }
$metadata = $metadataText | ConvertFrom-Json
$version = ($metadata.packages | Where-Object name -eq 'wtm').version
$Output = [IO.Path]::GetFullPath($Output)
[void][IO.Directory]::CreateDirectory($Output)
$staging = Join-Path $Output ".package-$([Guid]::NewGuid().ToString('N'))"
[void][IO.Directory]::CreateDirectory($staging)
try {
    foreach ($name in @('wtm.exe', 'wtm-ui.exe')) {
        Copy-Item -LiteralPath (Join-Path $BinaryDirectory $name) -Destination $staging
    }
    Copy-Item -LiteralPath (Join-Path $root 'LICENSE') -Destination $staging
    Copy-Item -LiteralPath (Join-Path $root 'target/THIRD-PARTY-NOTICES.txt') -Destination $staging
    @'
Worktree Monitor for Windows x64

Install Git for Windows and make sure git is on PATH:
https://git-scm.com/downloads

Run wtm-ui.exe to open the dashboard. Add this folder to PATH to use:

wtm add C:\src\project
wtm-ui
wtm

Source and documentation:
https://github.com/konmik/worktree-monitor
'@ | Set-Content -LiteralPath (Join-Path $staging 'README.txt') -Encoding utf8
    $archive = Join-Path $Output "worktree-monitor-$version-windows-x64.zip"
    Compress-Archive -Path "$staging/*" -DestinationPath $archive -Force
    $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $([IO.Path]::GetFileName($archive))" | Set-Content -LiteralPath "$archive.sha256" -Encoding ascii
    Write-Output $archive
    Write-Output "$archive.sha256"
} finally {
    $resolvedStaging = [IO.Path]::GetFullPath($staging)
    if (-not $resolvedStaging.StartsWith($Output + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Package staging directory is outside the output directory'
    }
    Remove-Item -LiteralPath $resolvedStaging -Recurse -Force
}
