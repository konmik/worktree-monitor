# wtm

Track changed lines across registered Git repositories and their worktrees. The Windows hacker dashboard uses dark repository cards, colored change counters, branch labels, and draggable groups. It stays above other windows without taking focus from your editor.

## Run

Rust and Git must be installed. Build and run from this checkout.

```powershell
cargo run -- add C:\src\project
cargo run
cargo run --bin wtm-ui
cargo run -- remove C:\src\project
```

Plain `wtm` prints one current snapshot. `wtm-ui` opens the Windows status window as a GUI application without creating a console. `wtm --live` remains a shortcut that launches the sibling `wtm-ui.exe` and returns without holding the terminal or its standard streams. Build both executables with `cargo build` before using that shortcut from the checkout. `add` accepts a repository or a directory inside any of its worktrees. Registering another worktree of the same repository does not create a duplicate. `remove` removes the whole repository from monitoring. These commands do not change the repository or its worktrees.

`cargo install --path crates/wtm --locked` installs both `wtm` and `wtm-ui`.

The live window includes the repository you launch it from, even when it is not registered. This does not register it. Drag a repository header to move its whole group. Drag a worktree card to reorder it within its repository. The window remembers these positions across refreshes and restarts. Change counts appear on individual worktrees, without summary totals.

The window has no title bar or visible frame. Drag the top line or empty space to move it, or drag an edge or corner to resize it. The app uses a notification-area icon instead of a taskbar button. Click the icon to show the window, or right-click for Opacity, Show/Hide, Reset, and Exit. Opacity sets transparency outside hover in 5% steps from 5% to 100%, with the selected value marked. The default is 20%, and the choice is saved in `config.opacity.json`. Hover shows the window fully opaque. Reset restores the default size and position on the current screen. The scrollbar uses the right gutter without changing card widths. It hides after three seconds without scrolling or resizing. Repository headers show names without branch totals.

Use **Hide** on a worktree to remove it from the list. **Show all** reveals hidden worktrees without clearing their saved settings. Use **Unhide** to keep a revealed worktree visible when you turn **Show all** off. Hiding a worktree does not unregister it or stop monitoring it.

## Counts

Each worktree shows its branch and `S +n/-n · T +n/-n · X +n/-n`. `S` counts source changes, `T` counts test changes, and `X` counts scripts and tooling changes. The categories match the OpenCode line-changes plugin. Only source additions and deletions use green and red. Counts include branch commits, staged and unstaged changes, and untracked files. Working files are compared with the common ancestor of `HEAD` and the local `main` branch, or `master` if `main` is absent. Changes made only on the base branch are excluded. If neither branch exists or their histories are unrelated, counts compare with `HEAD`. Changes reverted in the working files contribute no lines. Repositories without a first commit are supported.

Git's ignore rules and binary attributes apply. Binary files have a separate count, not an invented line count. Ignored untracked files and submodule contents are not counted. Missing or unreadable worktrees show errors rather than zero changes.

WTM installs filesystem monitors before scanning all worktrees once at startup. Later scan passes include only changed or newly discovered worktrees. All discovery and scanning runs serially, with at least ten seconds between the end of one pass and the start of the next. Changes during a pass remain queued for the next pass. Unchanged worktrees are not scanned again. There is no periodic rescan, backoff, or debounce. The window shows `[read: N%]`, the percentage of the last minute spent scanning and discovering repositories, rounded to the nearest integer. This measures elapsed scan time, not CPU use or time waiting between passes.

The monitor ignores changes inside Git-ignored folders, except tracked files and Git rule files that affect counts. On Windows, it filters native notifications before queuing monitoring work so parent folders can still be renamed. Changes to ignore rules update the filter.

Repository headers and worktree cards show a small refresh icon during scans. Each indicator stays visible for at least one second. Existing counts remain visible while a worktree refreshes.

## Configuration

Registrations live in `~/.config/wtm/config.json`. On Windows, this is `%USERPROFILE%\.config\wtm\config.json`. Configuration writes are locked and atomic. A malformed file is reported and is not overwritten by `add` or `remove`.

The live window saves display order in `config.order.json` and hidden worktrees in `config.visibility.json` beside the registration file. Neither changes which repositories are registered. A failed save shows a warning and leaves the previous settings in place.

The window saves its size and position separately in `config.placement.json`. Startup restores the saved window unless its screen changed or its bounds no longer overlap the screen's work area. Closing a minimized window preserves its normal bounds and does not reopen it minimized.

## Verify

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build
pwsh -NoProfile -File scripts/smoke-cli.ps1
pwsh -NoProfile -File scripts/smoke-launch.ps1
pwsh -NoProfile -File scripts/smoke-ui.ps1
pwsh -NoProfile -File scripts/smoke-window.ps1
pwsh -NoProfile -File scripts/smoke-frame.ps1
pwsh -NoProfile -File scripts/capture-window.ps1 -Height 330 -Client -VerifyScrollbar
pwsh -NoProfile -File scripts/smoke-dashboard.ps1
pwsh -NoProfile -File scripts/smoke-resize.ps1
pwsh -NoProfile -File scripts/smoke-features.ps1
pwsh -NoProfile -File scripts/smoke-visibility.ps1
pwsh -NoProfile -File scripts/smoke-refreshing.ps1
pwsh -NoProfile -File scripts/smoke-refreshing.ps1 -SavedOrder
```

The smoke checks use isolated temporary repositories and configuration. The window checks need an interactive Windows desktop. They check the real window, live counts, read percentage, registry reload, saved visibility, drag redraws, and closing.

`pwsh -NoProfile -File scripts/capture-window.ps1 -Client` creates three real Git fixtures, verifies native accessibility, and captures the actual dashboard without surrounding desktop content. It leaves the image and fixture evidence under the OpenCode temporary directory and closes its own window.

To check discovery against an existing repository with multiple worktrees, run `pwsh -NoProfile -File scripts/smoke-worktrees.ps1 -Repository C:\src\project`. This check uses an isolated registry and does not create or modify worktrees.

## Measure monitoring work

Build the release measurement example with `cargo build --release -p wtm-core --example measure-monitor --locked`. Run `target/release/examples/measure-monitor.exe 5 60` from the launch directory. The first argument selects five one-minute samples. The second argument selects a 60-second warmup. Each JSON record includes the rolling polling duration and the observed repository snapshot. Set `WTM_BENCH_CONFIG` to use an isolated registration file.

Run `pwsh -NoProfile -File scripts/check-monitor-passes.ps1` to verify serial Git execution, ten-second pass delays, unchanged worktree exclusion, and a newly added worktree's initial scan under continuous file changes.

To compare revisions, preserve the example executable from the older revision. Build the new revision, then run `pwsh -NoProfile -File scripts/benchmark-monitor.ps1 -Baseline C:\path\to\baseline-monitor.exe`. The script runs the two release monitors one at a time against the same registrations. It samples each monitor's published rolling-minute metric every 60 seconds after warmup. The comparison rejects errors or changed snapshots. Saved JSON includes executable hashes and every sample. Polling time counts serial discovery and scanning work and excludes filesystem-watcher overhead.
