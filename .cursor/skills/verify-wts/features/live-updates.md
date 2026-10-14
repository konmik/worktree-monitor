# Live updates

The status window shows changing worktree counts, adapts its polling interval, and recovers from configuration errors without losing the last good rows.

## Sub-features

- `live-counts` refreshes real worktree changes without reopening the window.
- `live-polling` backs off from 5 to 10, 20, and 30 seconds, then resets after a change.
- `live-errors` retains rows during malformed configuration and recovers when repaired.
- `live-registry` reloads registration removal while running.
- `live-window` stays topmost without taking focus and preserves scroll across refreshes.

## How to get to it (user POV)

- Run `wts --live` after registering a repository with `wts add`.
- Edit monitored files, change registrations through the CLI, resize or scroll the window, and close it through the normal window close action.

## Driving it with PowerShell

Preconditions:

- Complete Launch and Doctor from the skill. Run no other GUI check concurrently.

- **Drive the lifecycle.** Run `pwsh -NoProfile -File scripts/smoke-window.ps1 -Binary target/debug/wts.exe`. Allow the check to complete its real polling intervals.
- **Inspect results.** Require exit zero and the final PASS covering live counts, polling, error recovery, registry reload, scroll, and close. Read `initial.txt`, `updated.txt`, `invalid-config.txt`, `empty.txt`, `scroll.txt`, and `proof.json` in the printed evidence directory.
- **Prove changes.** Initial counts are `+1 -0`. Editing the fixture produces `+2 -0` and resets polling to 5 seconds. Malformed configuration shows an error alongside retained rows. Removing the registration produces the empty state because this harness launches outside Git.
- **Prove window behavior.** Check the proof's topmost, focus, scroll, launcher, and destruction results. A text snapshot alone does not prove focus or process exit.

## Gotchas

- This check intentionally waits through the polling backoff. A short command timeout invalidates it. Allow at least 180 seconds.
- Scroll stability here covers refresh behavior. Use the resize feature for pixel corruption after geometry changes.
- Preserve configuration failure evidence before deleting the scratch home.
