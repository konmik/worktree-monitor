# Live updates

The status window shows changing worktree counts and idle percentage, and recovers from configuration errors without losing the last good rows.

## Sub-features

- `live-counts` refreshes real worktree changes without reopening the window.
- `live-scheduling` queues filesystem changes for sequential scans, with scan passes at least ten seconds apart.
- `live-idle` shows unused monitoring time over the last minute as an idle percentage.
- `live-errors` retains rows during malformed configuration and recovers when repaired.
- `live-registry` reloads registration removal while running.
- `live-window` stays topmost without taking focus and preserves scroll across refreshes.

## How to get to it (user POV)

- Run `wtm-ui` after registering a repository with `wtm add`.
- Edit monitored files, change registrations through the CLI, resize or scroll the window, and close it through the normal window close action.

## Driving it with PowerShell

Preconditions:

- Complete Launch and Doctor from the skill. Run no other GUI check concurrently.

- **Drive the lifecycle.** Run `pwsh -NoProfile -File .cursor/skills/verify-wtm/scripts/drive.ps1 -Check live`. Allow real scan intervals to complete.
- **Inspect results.** Require exit zero and driver PASS. Read `initial.txt`, `polling-time.txt`, `updated.txt`, `invalid-config.txt`, `empty.txt`, `scroll.txt`, and `proof.json` in the recorded fixture directory.
- **Prove changes.** Initial source counts are `S +1/-0`. Editing the fixture produces `S +2/-0`. The idle percentage changes while work is performed. Malformed configuration shows an error alongside retained rows. Removing registration produces the empty state because the fixture launches outside Git.
- **Prove window behavior.** Check topmost, focus, scroll, direct UI ownership, and destruction results. A text snapshot alone does not prove focus or process exit.

## Gotchas

- This check waits for real monitor work and idle-percentage changes. A short timeout invalidates it. Allow at least 180 seconds.
- Scroll stability here covers refresh behavior. Use the resize feature for pixel corruption after geometry changes.
- Preserve configuration failure evidence before deleting the scratch home.
- The driver launches the UI directly while keeping the CLI for fixture registrations. The launch feature separately checks startup and duplicate-instance behavior.
