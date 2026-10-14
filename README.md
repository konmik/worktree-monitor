# wts

Track changed lines across registered Git repositories and their worktrees. The Windows hacker dashboard uses dark repository cards, colored change counters, branch labels, and draggable groups. It stays above other windows without taking focus from your editor.

## Run

Rust and Git must be installed. Build and run from this checkout.

```powershell
cargo run -- add C:\src\project
cargo run
cargo run --bin wts-ui
cargo run -- remove C:\src\project
```

Plain `wts` prints one current snapshot. `wts-ui` opens the Windows status window as a GUI application without creating a console. `wts --live` remains a shortcut that launches the sibling `wts-ui.exe` and returns without holding the terminal or its standard streams. Build both executables with `cargo build` before using that shortcut from the checkout. `add` accepts a repository or a directory inside any of its worktrees. Registering another worktree of the same repository does not create a duplicate. `remove` removes the whole repository from monitoring. These commands do not change the repository or its worktrees.

`cargo install --path crates/wts --locked` installs both `wts` and `wts-ui`.

The live window includes the repository you launch it from, even when it is not registered. This does not register it. Drag a repository's handle to move its whole group. Drag a worktree's handle to reorder it within its repository. The window remembers these positions across refreshes and restarts. Change counts appear on individual worktrees, without summary totals.

## Counts

Each worktree shows its branch and `S +n/-n · T +n/-n · X +n/-n`. `S` counts source changes, `T` counts test changes, and `X` counts scripts and tooling changes. The categories match the OpenCode line-changes plugin. Only source additions and deletions use green and red. Counts compare the working files with `HEAD`, including staged changes once and untracked files. A staged edit reverted in the working file contributes no lines. Repositories without a first commit are supported.

Git's ignore rules and binary attributes apply. Binary files have a separate count, not an invented line count. Ignored untracked files and submodule contents are not counted. Missing or unreadable worktrees show errors rather than zero changes.

The live view scans immediately, then waits 5 seconds. Unchanged results increase the wait to 10, 20, then 30 seconds. A changed result resets it to 5 seconds. The window shows the next polling interval and reloads registrations while running.

## Configuration

Registrations live in `~/.config/wts/config.json`. On Windows, this is `%USERPROFILE%\.config\wts\config.json`. Configuration writes are locked and atomic. A malformed file is reported and is not overwritten by `add` or `remove`.

The live window saves display order in `config.order.json` beside the registration file. Reordering does not change which repositories are registered. A failed save shows a warning and leaves the previous order in place.

The window saves its size and position separately in `config.placement.json`. Startup restores the saved window unless its screen changed or its bounds no longer fit. Closing a minimized window preserves its normal bounds and does not reopen it minimized.

## Verify

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build
pwsh -NoProfile -File scripts/smoke-cli.ps1
pwsh -NoProfile -File scripts/smoke-launch.ps1
pwsh -NoProfile -File scripts/smoke-ui.ps1
pwsh -NoProfile -File scripts/smoke-window.ps1
pwsh -NoProfile -File scripts/smoke-dashboard.ps1
pwsh -NoProfile -File scripts/smoke-resize.ps1
pwsh -NoProfile -File scripts/smoke-features.ps1
```

The smoke checks use isolated temporary repositories and configuration. The window check needs an interactive Windows desktop. It checks the real window, live counts, polling backoff, registry reload, and closing.

`pwsh -NoProfile -File scripts/capture-window.ps1 -Client` creates three real Git fixtures, verifies native accessibility, and captures the actual dashboard without surrounding desktop content. It leaves the image and fixture evidence under the OpenCode temporary directory and closes its own window.

To check discovery against an existing repository with multiple worktrees, run `pwsh -NoProfile -File scripts/smoke-worktrees.ps1 -Repository C:\src\project`. This check uses an isolated registry and does not create or modify worktrees.
