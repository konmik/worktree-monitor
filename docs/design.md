# Architecture

## Crates

The [workspace](../Cargo.toml) separates monitoring, shared presentation, and the two clients.

| Crate | Responsibility |
| --- | --- |
| [wtm-core](../crates/wtm-core/src/lib.rs) | Repository registration, Git discovery and counting, filesystem watches, and background monitoring. |
| [wtm-ui](../crates/wtm-ui/src/lib.rs) | Snapshot projection, render buffers, row formatting, Yoga layout, refresh tracking, saved order and visibility, and preference warning projection. |
| [wtm-tui](../crates/wtm-tui/src/lib.rs) | Terminal session, keyboard input, scrolling, resizing, and terminal output. |
| [wtm-gui-windows](../crates/wtm-gui-windows/src/lib.rs) | Win32 window, GDI presentation, fonts, input, notification icon, placement, and opacity. |
| [wtm](../crates/wtm/Cargo.toml) | The `wtm` registration CLI, `wtm-tui` terminal executable, and `wtm-gui` GUI executable, command parsing, startup, and distribution. |

Both clients depend on `wtm-ui` and `wtm-core`. Shared presentation has no dependency on either client. Its [Yoga dependency](../crates/wtm-ui/Cargo.toml) compiles C++ code.

Feature implementation lives in requirement directories under each crate's `src/features`. Each requirement directory's `requirement.md` lists its behavioral contracts. Tests live in separate files.

## Monitor data

A [snapshot](../crates/wtm-core/src/model.rs) contains repositories and their worktrees. Each worktree contains either branch and count data or a scan error. Failed scans do not produce zero counts.

The [monitor](../crates/wtm-core/src/features/live-monitoring/monitoring-runs-in-the-background/poll.rs) owns a worker thread and publishes immutable updates. Updates include snapshots, scan targets, scan activities, launch errors, and monitoring work time. Each client starts and stops its own monitor.

[Discovery and worktree scans run sequentially](../crates/wtm-core/src/features/live-monitoring/git-scans-run-sequentially/passes.rs) in repository order. Results appear as each scan finishes. Worktrees without results remain pending. A [completed scan pass waits ten seconds](../crates/wtm-core/src/features/live-monitoring/scan-passes-are-at-least-ten-seconds-apart/schedule.rs) before another pass. New events do not extend that deadline.

[Filesystem events](../crates/wtm-core/src/features/live-monitoring/filesystem-events-refresh-the-affected-targets/events.rs) queue affected repositories and worktrees. Events that arrive during a pass remain available for later scans. The [watcher boundary](../crates/wtm-core/src/features/live-monitoring/filesystem-events-refresh-the-affected-targets/watch.rs) watches configuration, Git metadata, worktrees, and ignore rules. Failed watches retry, and [watch recovery](../crates/wtm-core/src/features/live-monitoring/filesystem-watchers-recover-after-failures/reconcile.rs) queues a full refresh. Changes inside ignored directories do not trigger scans unless tracked files or ignore rules require them.

The [registration service](../crates/wtm-core/src/features/repository-registration/users-can-register-and-remove-repositories/service.rs) stores Git common-directory paths. Registration changes use a locked read-modify-replace operation. A [launch-directory repository](../crates/wtm-core/src/features/live-monitoring/launch-directory-repositories-appear-without-registration/launch.rs) joins monitoring without changing registrations.

## Shared presentation

[Dashboard projection](../crates/wtm-ui/src/features/dashboard/repo-list/users-can-view-repository-and-worktree-status/scan.rs) turns snapshots and scan targets into repository headings, worktree rows, pending rows, and notices. Health includes errors from hidden worktrees.

The [render buffer](../crates/wtm-ui/src/features/render-buffer/application-renders-into-a-render-buffer/buffer.rs) is a rectangular grid of Unicode grapheme cells. Each cell has a color and stressed, bold, and underline flags. Wide graphemes reserve continuation cells. [Row rendering and frame composition](../crates/wtm-ui/src/features/dashboard/repo-list/users-can-view-repository-and-worktree-status/rendering.rs) produce the buffer independently of either client.

Counts use source, test, and tooling groups, with binaries separate. Source counts stay visible. Empty optional groups disappear. Scan errors show unavailable counts and their cause. [Refresh tracking](../crates/wtm-ui/src/features/dashboard/repo-list/refresh-markers-identify-only-the-active-target/refresh.rs) identifies the active target and keeps its marker visible for at least one second.

The idle indicator derives its percentage from [monitoring work time within the previous minute](../crates/wtm-core/src/features/live-monitoring/monitoring-work-time-is-reported-for-the-last-minute/time.rs). It measures monitor work rather than system CPU usage.

## Clients

The [terminal client](../crates/wtm-tui/src/features/terminal/users-can-view-the-render-buffer-live-in-the-terminal/live.rs) uses an alternate screen and raw input. It redraws changed buffers, handles keyboard scrolling and resize events, and restores the terminal on exit. It uses the default repository order and displays all worktrees.

The [Windows client](../crates/wtm-gui-windows/src/window.rs) paints the shared buffer through GDI. It owns mouse and keyboard interaction, drag reordering, visibility controls, scrolling, window movement, and resizing. Window opacity and the stressed-cell overlay are native presentation concerns. The window opens above other windows without taking focus and has a notification-area icon.

## Saved preferences

The Windows client reads and writes preference files next to the registration file. Preference files are separate from repository registrations.

| File | Contents and owner |
| --- | --- |
| `config.order.json` | Repository and worktree display order, stored by the [shared order model](../crates/wtm-ui/src/features/dashboard/repo-list/row-order-survives-restart/order.rs). |
| `config.visibility.json` | Hidden repository and worktree identity pairs, stored by the [shared visibility model](../crates/wtm-ui/src/features/dashboard/repo-list/worktree/hidden-worktrees-stay-hidden-after-restart/visibility.rs). |
| `config.placement.json` | Normal window bounds, monitor geometry, and normal or maximized state, stored by the [Windows placement model](../crates/wtm-gui-windows/src/features/window/window-position-survives-restart/placement.rs). |
| `config.opacity.json` | Selected opacity, stored by the [Windows opacity model](../crates/wtm-gui-windows/src/features/context-menu/selected-opacity-survives-restart/storage.rs). |

Order and visibility identify repositories by Git common-directory paths and worktrees by root paths. Reordering changes presentation without changing registrations. Hiding changes displayed rows without stopping monitoring. Show all temporarily bypasses saved visibility settings.

The Windows client reloads saved order and visibility while open. Failed order and visibility reads and writes retain current settings and show warnings. Placement restoration requires the same monitor identity and pixel dimensions, with visible overlap in the current work area. Normal bounds survive maximization and minimization. Startup does not restore a minimized window.
