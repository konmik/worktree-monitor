$verificationDirectories = [Collections.Generic.List[object]]::new()
$verificationRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../target/verification'))

function New-VerificationDirectory([string]$Name, [string]$EvidenceDirectory = '') {
    $path = Join-Path $verificationRoot "$Name-$([Guid]::NewGuid().ToString('N'))"
    [void][IO.Directory]::CreateDirectory($path)
    $verificationDirectories.Add(@{ Path = $path; EvidenceDirectory = $EvidenceDirectory })
    [IO.File]::WriteAllText((Join-Path $path '.git'), "gitdir: $($path.Replace('\', '/'))/not-a-repository`n")
    return $path
}

function Remove-VerificationDirectories {
    foreach ($entry in $verificationDirectories) {
        $path = [IO.Path]::GetFullPath($entry.Path)
        if (-not $path.StartsWith($verificationRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw "Refusing to remove verification directory outside $verificationRoot" }
        if (-not (Test-Path -LiteralPath $path)) { continue }
        try {
            if ($entry.EvidenceDirectory) {
                [void][IO.Directory]::CreateDirectory($entry.EvidenceDirectory)
                Get-ChildItem -LiteralPath $path -File | Where-Object Name -ne '.git' | Copy-Item -Destination $entry.EvidenceDirectory -Force -ErrorAction Stop
            }
        } finally {
            for ($attempt = 0; ; $attempt++) {
                try { Remove-Item -LiteralPath $path -Recurse -Force -ErrorAction Stop; break }
                catch { if ($attempt -ge 4) { throw }; Start-Sleep -Milliseconds 200 }
            }
        }
    }
}
