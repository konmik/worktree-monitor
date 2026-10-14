# wts

Track changed lines across registered Git repositories and their worktrees. The Windows live view stays above other windows without taking focus from your editor.

## Run

Rust and Git must be installed. Build and run from this checkout.

```powershell
cargo run -- add C:\src\project
cargo run
cargo run -- --live
cargo run -- remove C:\src\project
```

Plain `wts` prints one current snapshot. `wts --live` opens the Windows status window. `add` accepts a repository or a directory inside any of its worktrees. Registering another worktree of the same repository does not create a duplicate. `remove` removes the whole repository from monitoring. These commands do not change the repository or its worktrees.

## Counts

Each worktree shows its branch and added and deleted lines. Counts compare the working files with `HEAD`, including staged changes once and untracked files. A staged edit reverted in the working file contributes no lines. Repositories without a first commit are supported.

Git's ignore rules and binary attributes apply. Binary files have a separate count, not an invented line count. Ignored untracked files and submodule contents are not counted. Missing or unreadable worktrees show errors rather than zero changes.

The live view scans immediately, then waits 5 seconds. Unchanged results increase the wait to 10, 20, then 30 seconds. A changed result resets it to 5 seconds. The window shows the next polling interval and reloads registrations while running.

## Configuration

Registrations live in `~/.config/wts/config.json`. On Windows, this is `%USERPROFILE%\.config\wts\config.json`. Configuration writes are locked and atomic. A malformed file is reported and is not overwritten by `add` or `remove`.

## Verify

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build
pwsh -NoProfile -File scripts/smoke-cli.ps1
pwsh -NoProfile -File scripts/smoke-window.ps1
```

The smoke checks use isolated temporary repositories and configuration. The window check needs an interactive Windows desktop. It checks the real window, live counts, polling backoff, registry reload, and closing.

To check discovery against an existing repository with multiple worktrees, run `pwsh -NoProfile -File scripts/smoke-worktrees.ps1 -Repository C:\src\project`. This check uses an isolated registry and does not create or modify worktrees.
