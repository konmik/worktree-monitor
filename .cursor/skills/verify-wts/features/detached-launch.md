# Detached launch

Users run `wts --live`, regain the terminal and standard streams, and keep a separate status window open.

## Sub-features

- `launch-return` exits the CLI launcher successfully while the window remains visible.
- `launch-streams` releases stdin, stdout, and stderr before the window closes.
- `launch-console` keeps the window host detached from the launching console.
- `launch-close` closes the owned window and exits its host cleanly.

## How to get to it (user POV)

- Run `wts --live` from a terminal, including callers that redirect standard streams.

## Driving it with PowerShell

Preconditions:

- Complete Launch and Doctor from the skill. Reserve the interactive desktop for this GUI check.

- **Launch and observe.** Run `pwsh -NoProfile -File scripts/smoke-launch.ps1 -Binary target/debug/wts.exe`. The harness invokes public `--live` with redirected streams and finds the visible window by launcher or child ownership.
- **Prove independence.** Require `launcher_exited=True`, `stdin_released=True`, `stdout_eof=True`, `stderr_eof=True`, `different_window_pid=True`, and `console_detached=True` while the owned window exists.
- **Close and read proof.** Require `PASS. Window child closed cleanly.` and exit zero. Read `proof.json` in the printed evidence directory. Require `result` equal to `PASS`, a zero window exit code, and `window_destroyed` equal to true.
- **Preserve evidence.** Use the complete run recipe in the skill for a command transcript and scoped scratch cleanup.

## Gotchas

- `FreeConsole()` alone does not release a shell waiting on the launcher process.
- Null standard streams alone do not prove inherited pipe handles were released. The EOF and stdin checks are required.
- Direct `--live --live-child` invocation bypasses public launch and cannot satisfy this feature.
