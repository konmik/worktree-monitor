# Design

## Usage

`wtm add <path>` registers a repository from any of its worktrees. `wtm` shows command help. `wtm-ui` displays registered repositories and the repository it is launched from in an always-on-top Windows window. Launch context does not change registrations. `wtm remove <path>` unregisters the repository.

## Shape

The workspace contains a reusable `wtm-core` library, the `wtm` command, and the `window-win` platform library. The core owns Git discovery, counting, configuration, and polling. Future window implementations can reuse it without changing the counter.

A snapshot contains repositories and their worktrees. Each worktree has a path and either successful branch and count data or an error. A failed scan cannot claim zero changed lines. The configuration representation stays private to the core.

One coordinator owns worktree scan schedules and pending filesystem changes. Native watchers report file changes through `notify`, which selects the platform backend. The coordinator combines bursts of changes and keeps changes that arrive during active jobs. Successful scans wait for another change or the periodic safety scan. Watcher failures and scan errors retain recovery polling. Scan jobs run outside the coordinator so a slow worktree does not block another worktree's update. The window reads immutable updates and never runs Git on its message thread. Configuration commands share one locked read-modify-replace operation. Monitoring does not write the registry.

At most four jobs run at once. Discovery and scans retain capacity for each other, and overdue jobs run in deadline order. A worktree never overlaps its own scan. Removed jobs are canceled and joined. Ready worktrees appear without waiting for slower initial scans.

The status shows idle percentage over the previous 60 seconds. It subtracts recorded polling work from the full minute and clamps the result to 0–100%. Concurrent scans contribute their durations separately. This measures monitor work, not system CPU usage.

## Synthesis

Two designs were compared. An independent reviewer agreed with the native Windows and Git subprocess design.

| Design | Choice |
| --- | --- |
| Native Win32 and installed Git | Selected. It follows Herald's existing Windows pattern and uses Git's own counting, ignore rules, attributes, and repository formats. |
| eframe and libgit2 | Rejected. It adds a renderer and C dependency, differs from installed Git, and cannot interrupt one in-process diff. |

The selected shape uses a normal captioned, resizable window. Herald's nonactivating show and topmost positioning prevent initial display and refresh from taking focus. Clicking the window may focus it for scrolling. Its animation, layered popup, and full-monitor canvas are not copied.

The competing design contributed the successful worktree object, immediate first publication, and last-good configuration behavior. A sortable table, custom process launcher, and interchangeable Git engines were rejected as unnecessary scope.

Codex's worktree and Git utilities supplied a second local precedent for subprocesses, NUL-delimited worktree records, inherited-environment isolation, and disabling filesystem-monitor helpers. WTM keeps ordinary ignore rules and binary attributes while disabling fsmonitor during polling.

The coordinator owns scan scheduling. A private watcher boundary owns native watch registration and reports invalidations. Configuration watches follow parent directories so atomic file replacement does not detach them. Shared Git metadata watches cover linked worktrees even when the main worktree's files do not change. A five-minute safety pass covers lost events. Short recovery polling remains available when native watches fail.

## Dashboard

The Windows dashboard projects structured snapshots into repository sections and worktree rows. It does not parse CLI output or show summary totals. Each worktree shows its own counts or scan error. Zero changed lines are not presented as proof of a clean Git status.

Line counts are grouped into source, tests, and scripts or tooling when Git's numstat records are parsed. Renames use the destination path. Test classification takes precedence over tooling classification, following the OpenCode line-changes plugin. The groups are the only stored line counts. Accessible window captions use `S +n/-n · T +n/-n · X +n/-n`. Only source numbers receive addition and deletion colors.

The visual design uses charcoal surfaces, monospace typography, acid-green additions, coral-red deletions and errors, and teal branch labels. Native owner-drawn controls retain real captions for accessibility. A clipped native viewport owns scrolling, while the header stays fixed. Dragging a repository header moves its whole section. Dragging a worktree card reorders it within its section. Saved display order does not change registrations or create worktrees. There are no invented history charts, activity scores, or decorative animations.

Drag movement redraws only the source card and changed drop-marker strips. Edge scrolling batches child-window moves and buffers each card before display. Idle-percentage updates redraw the fixed status row without rebuilding the list. Native Hide and Unhide buttons support keyboard activation, and Show all temporarily overrides the visibility filter.

Monitor updates carry scan targets and retained scan activities separately from completed snapshots. Activity starts before discovery or worktree scanning and finishes with the result. Repository activity includes scans of its worktrees. The dashboard holds each refresh icon for at least one second without delaying the scanner or clearing prior counts.

| Dashboard alternative | Choice |
| --- | --- |
| Native repository cards | Selected for clear hierarchy, colored counters, branch badges, and an actual visual redesign. |
| Dark terminal ledger | Rejected because a recolored text buffer still makes long paths and worktrees harder to scan. |

The terminal sketch contributed the vivid green and teal palette and WTM wordmark. Core counting, registration storage, CLI output, and monitor ownership remain unchanged. The viewport preserves scroll on unchanged and count-only updates.

Display order lives in a separate window-library model. Stable Git common-directory and worktree-root paths identify groups and rows. A completed drop updates `config.order.json` through its own nonblocking lock and atomic replacement. A failed save keeps the prior order and shows a warning. The core accepts explicit live launch context but does not register it. Saved ranks cannot discover or register repositories.

Hidden worktrees use the same stable identity pairs in a separate `config.visibility.json` file. Filtering changes the displayed rows, not monitoring, repository totals, or error health. Show all is a temporary display override that leaves saved hidden settings intact. Unhide clears one worktree's hidden setting.

Window placement has its own model and `config.placement.json` file. It records normal window bounds and the associated monitor's geometry. Startup restores bounds with visible overlap when the monitor's identity and pixel dimensions are unchanged. A work-area change alone does not discard a placement that still overlaps the work area. Normal bounds survive maximization and minimization, and startup never restores a minimized window.

## Evidence and tradeoffs

Git probes confirmed unborn SHA-1 and SHA-256 counting, files without final newlines, and net counting when a staged edit is reverted in the working file. A binary-attribute probe found that no-index diffs must use relative untracked paths. Absolute paths lost those attributes.

A Rust probe confirmed replacement of an existing Windows file through `std::fs::rename`. The core can use portable temporary-file replacement rather than separate custom Win32 replacement code.

Git subprocesses cost more than an in-process scan, especially with many untracked files. They avoid maintaining a second interpretation of Git semantics. Scans are observations, not filesystem transactions. Files may change while a scan is running.

Timeout and cancellation manage the Git child process. Helper processes started by user-configured Git filters are not managed as a Windows process job.

The initial release provides a Windows live window. Other platforms can use the core and registration commands, but their live windows need separate implementations and verification.

## Verification

Core tests exercise real temporary Git repositories and literal expected counts. Scheduler tests check scan lifecycle, recovery backoff, pending changes, and the rolling polling duration. CLI smoke checks exercise the executable with an isolated registry. The Windows smoke check reads the actual visible controls, verifies topmost behavior, changes a file, checks the idle-percentage label, reloads the registry, and closes the real window. The release measurement example and benchmark script compare published rolling-minute durations across unchanged repository snapshots.
