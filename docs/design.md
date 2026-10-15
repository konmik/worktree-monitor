# Design

## Usage

`wts add <path>` registers a repository from any of its worktrees. `wts` prints a snapshot. `wts --live` displays registered repositories and the repository it is launched from in an always-on-top Windows window. Launch context does not change registrations. `wts remove <path>` unregisters the repository.

## Shape

The workspace contains a reusable `wts-core` library, the `wts` command, and the `window-win` platform library. The core owns Git discovery, counting, configuration, and polling. Future window implementations can reuse it without changing the counter.

A snapshot contains repositories and their worktrees. Each worktree has a path and either successful branch and count data or an error. A failed scan cannot claim zero changed lines. The configuration representation stays private to the core.

One worker owns the previous snapshot and adaptive polling interval. The window reads immutable updates and never runs Git on its message thread. Configuration commands share one locked read-modify-replace operation. Monitoring does not write the registry.

## Synthesis

Two designs were compared. An independent reviewer agreed with the native Windows and Git subprocess design.

| Design | Choice |
| --- | --- |
| Native Win32 and installed Git | Selected. It follows Herald's existing Windows pattern and uses Git's own counting, ignore rules, attributes, and repository formats. |
| eframe and libgit2 | Rejected. It adds a renderer and C dependency, differs from installed Git, and cannot interrupt one in-process diff. |

The selected shape uses a normal captioned, resizable window. Herald's nonactivating show and topmost positioning prevent initial display and refresh from taking focus. Clicking the window may focus it for scrolling. Its animation, layered popup, and full-monitor canvas are not copied.

The competing design contributed the successful worktree object, immediate first publication, and last-good configuration behavior. A sortable table, custom process launcher, file watcher, and interchangeable Git engines were rejected as unnecessary scope.

Codex's worktree and Git utilities supplied a second local precedent for subprocesses, NUL-delimited worktree records, inherited-environment isolation, and disabling filesystem-monitor helpers. WTS keeps ordinary ignore rules and binary attributes while disabling fsmonitor during polling.

## Dashboard

The Windows dashboard projects structured snapshots into repository sections and worktree rows. It does not parse CLI output or show summary totals. Each worktree shows its own counts or scan error. Zero changed lines are not presented as proof of a clean Git status.

The visual design uses charcoal surfaces, monospace typography, acid-green additions, coral-red deletions and errors, and teal branch labels. Native owner-drawn controls retain real captions for accessibility. A clipped native viewport owns scrolling, while the header stays fixed. Drag handles move whole repository sections or worktrees within their own section. Saved display order does not change registrations or create worktrees. There are no invented history charts, activity scores, or decorative animations.

| Dashboard alternative | Choice |
| --- | --- |
| Native repository cards | Selected for clear hierarchy, colored counters, branch badges, and an actual visual redesign. |
| Dark terminal ledger | Rejected because a recolored text buffer still makes long paths and worktrees harder to scan. |

The terminal sketch contributed the vivid green and teal palette and WTS wordmark. Core counting, registration storage, CLI output, and monitor ownership remain unchanged. The viewport preserves scroll on unchanged and count-only updates.

Display order lives in a separate window-library model. Stable Git common-directory and worktree-root paths identify groups and rows. A completed drop updates `config.order.json` through its own nonblocking lock and atomic replacement. A failed save keeps the prior order and shows a warning. The core accepts explicit live launch context but does not register it. Saved ranks cannot discover or register repositories.

## Evidence and tradeoffs

Git probes confirmed unborn SHA-1 and SHA-256 counting, files without final newlines, and net counting when a staged edit is reverted in the working file. A binary-attribute probe found that no-index diffs must use relative untracked paths. Absolute paths lost those attributes.

A Rust probe confirmed replacement of an existing Windows file through `std::fs::rename`. The core can use portable temporary-file replacement rather than separate custom Win32 replacement code.

Git subprocesses cost more than an in-process scan, especially with many untracked files. They avoid maintaining a second interpretation of Git semantics. Scans are observations, not filesystem transactions. Files may change while a scan is running.

Timeout and cancellation manage the Git child process. Helper processes started by user-configured Git filters are not managed as a Windows process job.

The initial release provides a Windows live window. Other platforms can use the core and command-line view, but their live windows need separate implementations and verification.

## Verification

Core tests exercise real temporary Git repositories and literal expected counts. CLI smoke checks exercise the executable with an isolated registry. The Windows smoke check reads the actual visible controls, verifies topmost behavior, changes a file, checks polling backoff and reset, reloads the registry, and closes the real window. A separate read-only check compares discovered paths against existing Git worktrees.
