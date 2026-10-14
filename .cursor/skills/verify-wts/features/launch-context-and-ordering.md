# Launch context and ordering

The dashboard includes the launch repository without registering it. Users drag repository groups or individual worktrees and keep their order across refreshes and restarts.

## Sub-features

- `context-discovery` includes an unregistered launch repository from a nested directory.
- `context-deduplication` avoids duplicates after explicit registration and retains the launch repository after removal.
- `order-groups` moves whole repository groups up or down.
- `order-worktrees` reorders only siblings within a repository.
- `order-cancel` preserves order after Escape or capture cancellation.
- `order-persistence` preserves order across refresh and restart, and reports failed or malformed saves.

## How to get to it (user POV)

- Run `wts --live` inside a repository or its nested directory, or outside Git for registered repositories only.
- Drag the handle on a repository heading or a worktree card. Press Escape to cancel a drag.
- Register or remove the launch repository through `wts add` and `wts remove` while the window remains open.
- Close and reopen the dashboard to verify saved order.

## Driving it with PowerShell

Preconditions:

- Complete Launch and Doctor from the skill. Reserve the desktop and allow at least 180 seconds.

- **Drive all entries.** Run `pwsh -NoProfile -File scripts/smoke-dashboard.ps1 -Binary target/debug/wts.exe`. The harness resolves cards by their captions, sends native drag events, cancels drags, edits real files, and restarts owned windows.
- **Check visible order.** Require every PASS assertion and exit zero. Read `initial-order.txt`, `final-order.txt`, and `proof.json` in the printed evidence directory.
- **Check persistence and side effects.** Read `registrations.json` and `saved-order.json`. Reordering must leave registrations unchanged, and restarting must restore the selected group and worktree order.
- **Check failure states.** The harness makes preferences read-only or malformed and hides Git from its own child environment. Require visible warnings, unchanged saved order on failure, recovery after repair, and no registration write during failed discovery.

## Gotchas

- Repository drags move a whole group. A worktree dragged across repositories must leave both groups unchanged.
- Saved order is `config.order.json` beside `config.json`. It is not the registration list.
- This harness temporarily creates linked fixture worktrees and changes its own PATH. Its `finally` restores the environment. Remove only its own scratch worktrees during cleanup.
