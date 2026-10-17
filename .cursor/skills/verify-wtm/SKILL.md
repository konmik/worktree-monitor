---
name: verify-wtm
description: Verify the WTM Windows dashboard and CLI after behavior changes. Use for registrations, snapshots, launch, live updates, ordering, resizing, accessibility, and stressed text opacity.
---

# Verify WTM

Read [the feature map](features/README.md), then the requested feature file. The primary interface is the native Win32 dashboard. The CLI manages registrations and prints snapshots. There is no web server, port, account, or seed service.

## Launch

Run from the repository root on Windows with Rust, Git, PowerShell 7, and an interactive desktop.

```powershell
cargo build --locked
if ($LASTEXITCODE -ne 0) { throw 'WTM build failed' }
```

Use the checkout's `target/debug/wtm.exe` and `target/debug/wtm-ui.exe`. Build both before verification. `wtm-ui` directly owns its console-free window. The removed `wtm --live` argument is invalid usage and exits with code 2.

Only one UI can run in a Windows session. Isolated HOME and USERPROFILE values do not bypass that limit. A pre-existing dashboard blocks GUI checks. Close it only with existing user authorization, record its exact PID and executable, and restart it after the run. Leave unrelated windows alone.

Use one fresh PowerShell process per drive. The driver launches a fresh isolated fixture session and each harness owns its windows through the check, including any necessary restarts. Run GUI drives serially on an unlocked desktop reserved for verification. A visible WTM window owned by the launched UI PID, followed by expected fixture captions, is readiness.

## Doctor

Run the doctor for the selected drive before interacting, and again after any failed drive:

```powershell
pwsh -NoProfile -File .cursor/skills/verify-wtm/scripts/drive.ps1 -Check launch -DoctorOnly
```

Replace `launch` with the selected check from the feature map. The driver also runs this doctor before every live drive. It checks the expected version from Cargo metadata, both binary hashes, the source revision, dirty state, and the single-instance prerequisite. A missing binary or failed build blocks verification. Version and hashes alone do not establish freshness; build first.

`-ValidateOnly` checks metadata and parses the harness adapter without interacting with the app. Neither `-ValidateOnly` nor `-DoctorOnly` proves a feature live. Do not run `wtm-ui --version`; it is not the CLI.

If a healthy process leaves the UI in an unexpected state, close the owned fixture and start a fresh session. The driver does not stop a pre-existing dashboard. If fixture Git commands fail, inspect inherited overrides such as GIT_DIR and GIT_WORK_TREE while preserving the user's global environment.

## Drive

Choose the exact driver command from the feature file. Use a new output directory for each attempt:

```powershell
pwsh -NoProfile -File .cursor/skills/verify-wtm/scripts/drive.ps1 -Check launch -Output "$env:LOCALAPPDATA/Temp/opencode/verify-wtm-launch-$([Guid]::NewGuid().ToString('N'))"
```

Require exit zero, `result.json` with `result` equal to `PASS`, and the feature's recorded observations. Earlier PASS lines do not override a later failure. Preserve failed output and inspect native proof files rather than trusting the last line.

The driver reuses checked-in smoke harnesses. Guarded adapters replace obsolete compatibility-launcher calls with direct UI launches, update CLI help assertions, and add the second-instance check. A changed source pattern fails explicitly instead of silently skipping an adaptation. Product scripts are not edited.

## Evidence

Each live output directory contains `session.txt`, `drive.txt`, `adapted-harness.ps1`, and `result.json`. The result records the feature check, binary paths and hashes, source revision and dirty state, harness path and hash, outcome, and emitted fixture directories. Doctor-only attempts have no drive log and are marked as not driven.

Read the source harness's proof, text observations, and images at the recorded artifact locations. Pair real commands and native window messages with resulting state. Registration proof includes the saved registry. Ordering proof includes saved preferences and a restart. Counts use real Git fixtures. Normal rendering must be clean before a capture forces repaint.

The driver preserves configuration under each fixture's `preserved-configs/` directory before successful scratch cleanup. It checks that root-level evidence files survive. Failed fixture state remains available for diagnosis. Capture places `dashboard.png` in the driver output directory and text observations in a separately recorded fixture directory; retain both.

Cover every entry point listed for the requested feature. Label skipped entries as unverified. Unit and integration tests supplement live checks. Bold and underline currently have renderer APIs but no dashboard control; their pixel tests do not establish a user route that does not exist.

## Cleanup

Let each harness finish its `finally` block. The driver closes any remaining direct child processes whose executable matches the tested CLI or UI, records that as failed cleanup, and preserves evidence. Never kill processes by executable name.

Successful drives remove only the scratch children listed in the feature map, after preserving configuration. Failed attempts retain fixture files for diagnosis, but no owned process may remain running. If a drive is interrupted, establish ownership from its recorded run before closing anything. Report incomplete cleanup when ownership cannot be established.

Restore any user-authorized dashboard temporarily closed after the last drive, including harness re-proofs. Keep transcripts, JSON proofs, text observations, preserved configuration, and all images.

## Helpers

All supported drives use this skill's `scripts/drive.ps1`. Each feature file documents an executable invocation. The driver uses the repository's PowerShell harnesses and native probes; it adds no application test mode. UI fixtures share the actual desktop, focus, and input. Desktop interference remains a failed or blocked check; do not weaken an assertion to turn it into a pass.
