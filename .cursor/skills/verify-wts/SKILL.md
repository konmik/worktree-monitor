---
name: verify-wts
description: Verify the WTS Windows dashboard and CLI after behavior changes. Use for terminal release, repository counts, live updates, ordering, resizing, and native accessibility checks.
---

# Verify WTS

Read [the feature map](features/README.md), then the relevant feature file. Use the checked-in PowerShell harnesses against the built executable. The primary interface is the native Win32 dashboard. The CLI manages registrations and prints snapshots. There is no web server, port, account, or seed service.

## Launch

Run from the repository root on Windows with Rust, Git, PowerShell 7, and an interactive desktop.

```powershell
cargo build --locked
if ($LASTEXITCODE -ne 0) { throw 'WTS build failed' }
```

Use `target/debug/wts.exe`, not an installed copy from PATH. Each harness creates its own repositories and overrides HOME and USERPROFILE for its child processes. Each starts and closes its own app instances. A visible window with a title starting with `WTS`, owned by the launched host PID, is the GUI readiness signal. Harnesses also wait for their fixture counts before interacting.

Public `wts --live` exits before its detached window host. Its launcher PID is not the window PID. The resize and capture harnesses use the hidden `--live --live-child` host entry to retain direct ownership. Those checks do not prove public launcher behavior.

Run GUI checks serially. They share the Windows desktop, topmost stacking, focus, and input even with isolated profiles. Keep each target visible. Leave the user's windows alone. CLI fixtures can coexist with separate profiles.

## Doctor

Run this read-only check before driving and whenever the binary or environment looks wrong.

```powershell
& ./target/debug/wts.exe --version
if ($LASTEXITCODE -ne 0) { throw 'WTS version check failed' }
Get-FileHash -LiteralPath ./target/debug/wts.exe -Algorithm SHA256
git rev-parse HEAD
git status --short
```

Require the checkout's expected version from `Cargo.toml`, a readable executable hash, and a recorded source revision and working-tree state. Build first to establish which source produced the binary. Version and hash alone do not establish freshness. A missing binary or failed build blocks verification.

The fixture Git commands expect no inherited repository overrides such as GIT_DIR or GIT_WORK_TREE. If fixture initialization fails, inspect those variables in the verification shell. Preserve the user's global environment.

## Drive

Choose the exact command from the feature map. A script's nonzero exit is a failed check, even if earlier lines say PASS. Preserve the full output and the artifact directory it prints. Use a fresh PowerShell process for each harness because its native probe types have fixed names.

This complete recipe verifies public detached launch. Run it after Launch. It records the doctor, action, result, and cleanup. The launch harness closes its own child in `finally`, including failed checks.

```powershell
$binary = (Resolve-Path ./target/debug/wts.exe).Path
$run = Join-Path $env:LOCALAPPDATA "Temp/opencode/verify-wts-$([Guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $run | Out-Null
$fixture = $null
$code = $null
Start-Transcript -Path (Join-Path $run 'session.txt') | Out-Null
try {
    & $binary --version
    if ($LASTEXITCODE -ne 0) { throw 'WTS version check failed' }
    Get-FileHash -LiteralPath $binary -Algorithm SHA256
    git rev-parse HEAD
    git status --short
    Write-Output 'feature=detached-launch entry=wts --live'
    Write-Output "Action: pwsh -NoProfile -File scripts/smoke-launch.ps1 -Binary $binary"
    $output = @(& pwsh -NoProfile -File scripts/smoke-launch.ps1 -Binary $binary 2>&1)
    $code = $LASTEXITCODE
    $output | Tee-Object -FilePath (Join-Path $run 'drive.txt')
    $location = @($output | Where-Object { "$_" -like 'Evidence at *' }) | Select-Object -Last 1
    if ($location) {
        $fixture = "$location".Substring('Evidence at '.Length)
        $fixture | Set-Content -LiteralPath (Join-Path $run 'fixture-path.txt')
    }
    if ($code -ne 0) { throw "Launch verification failed with exit $code" }
    if (-not $fixture) { throw 'Launch check did not identify its evidence' }
    Get-Content -LiteralPath (Join-Path $fixture 'proof.json')
} finally {
    try {
        if ($fixture) {
            $fixtureHome = Join-Path $fixture 'home'
            if (Test-Path -LiteralPath $fixtureHome) { Remove-Item -LiteralPath $fixtureHome -Recurse -Force }
            if ($code -eq 0 -and -not (Test-Path -LiteralPath (Join-Path $fixture 'proof.json'))) { throw 'Cleanup lost launch evidence' }
        }
    } finally {
        Stop-Transcript | Out-Null
        Write-Output "Verification transcript at $run"
    }
}
```

Require launcher success, released stdin, stdout and stderr EOF, a visible detached host with a different PID, and clean host exit after closing. Read the proof rather than trusting the script's final line alone.

## Evidence

The standard recipe stores its session and action output under `%LOCALAPPDATA%/Temp/opencode/`, in a directory named `verify-wts-` followed by a generated GUID. Its `fixture-path.txt` points to the harness evidence directory.

Harnesses leave transcripts, `proof.json`, visible text, saved registrations, order files, or PNGs under the same OpenCode temporary parent. Record the feature ID, entry point, binary path and hash, source revision, dirty state, command, exit code, and emitted artifact path.

Exercise real commands and native window messages. Pair each action with the resulting state. Registration proof includes the saved registry. Ordering proof includes saved preferences and a restart. Counts use real Git fixtures, not injected snapshots. Visual proof includes normal rendering after the action, not only a screenshot forced to repaint by the capture tool. Use no mocks for these local paths.

Cover every entry point listed for the requested feature. Label a skipped entry point as unverified. Unit tests and compilation supplement these checks; they do not replace driving the app. The app has no verification dry-run mode. Checks create real temporary repositories and configuration, and some invoke local Git commits or worktree commands.

## Cleanup

Let each harness finish its `finally` block. It restores its environment and closes or terminates only processes it launched. Never kill by executable name. If a check is interrupted outside its cleanup, establish ownership from that run's recorded PID and executable before closing anything. If ownership cannot be established, report cleanup incomplete and leave unrelated processes alone.

For the standard detached-launch recipe, cleanup removes only that run's isolated `home` directory. Keep `proof.json`, `session.txt`, `drive.txt`, and `fixture-path.txt`. Confirm they still exist after cleanup. On a failed iteration, run the same scoped cleanup. If failure occurs before an artifact path is emitted, recover the path from the recorded run before deleting anything.

For other mapped checks, first preserve registry and order files under the artifact root. Then remove only the scratch directories listed in the feature index for that specific run. Retain root-level transcripts, JSON proofs, text observations, and all images. A failed check's evidence survives cleanup too.

## Helpers

No additional runner is needed. The feature files show executable `pwsh -NoProfile -File` invocations for every reused script. Native probes use window ownership and control handles, with viewport control ID `1001`. Dashboard drags resolve cards by caption before computing coordinates. Read the relevant script when extending an interaction instead of inventing screen coordinates.
