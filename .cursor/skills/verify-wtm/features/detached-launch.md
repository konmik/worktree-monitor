# Detached launch

Users open the console-free `wtm-ui` application directly or launch it through the `wtm --live` CLI shortcut.

## Sub-features

- `ui-direct` opens the native window from a Windows GUI executable without console-suppression launch flags.
- `ui-startup-error` shows fatal startup failures in a native dialog and exits unsuccessfully after dismissal.
- `launch-return` exits the CLI launcher successfully while the window remains visible.
- `launch-streams` releases stdin, stdout, and stderr before the window closes.
- `launch-console` keeps the window host detached from the launching console.
- `launch-close` closes the owned window and exits its host cleanly.

## How to get to it (user POV)

- Run `wtm-ui` from a terminal or open `wtm-ui.exe` directly.
- Run `wtm --live` from a terminal, including callers that redirect standard streams.

## Driving it with PowerShell

Preconditions:

- Complete Launch and Doctor from the skill. Reserve the interactive desktop for this GUI check.

- **Open the UI directly.** Run `pwsh -NoProfile -File scripts/smoke-ui.ps1 -Binary target/debug/wtm-ui.exe`. Require PE subsystem 2, a visible WTM window owned by that process, no shared console, and clean exit after close. The harness must not mask a console executable with creation flags.
- **Read startup errors.** The same harness launches a second owned UI without USERPROFILE. Require the native error dialog to explain that missing variable and exit code 1 after dismissing it. The normal user's environment stays unchanged.
- **Launch through the CLI.** Run `pwsh -NoProfile -File scripts/smoke-launch.ps1 -Binary target/debug/wtm.exe`. The harness invokes public `--live` with redirected streams and finds the visible window by launcher or child ownership.
- **Prove independence.** Require `launcher_exited=True`, `stdin_released=True`, `stdout_eof=True`, `stderr_eof=True`, `different_window_pid=True`, and `console_detached=True` while the owned window exists.
- **Close and read proof.** Require `PASS. Window child closed cleanly.` and exit zero. Read `proof.json` in the printed evidence directory. Require `result` equal to `PASS`, a zero window exit code, and `window_destroyed` equal to true.
- **Preserve evidence.** Use the complete run recipe in the skill for the direct UI command transcript and scoped scratch cleanup. Retain both harnesses' proof directories.

## Gotchas

- `FreeConsole()` alone does not release a shell waiting on the launcher process.
- Null standard streams alone do not prove inherited pipe handles were released. The EOF and stdin checks are required.
- Direct UI and compatibility launch have different process-lifetime contracts. Verify both. The hidden `--live-child` entry has been removed.
