# Worktree Monitor

An always-on-top Windows dashboard showing branches and changed lines across Git repositories and their worktrees. Updates automatically without taking focus from your editor.

## Install

Requires [Rust](https://www.rust-lang.org/tools/install) and [Git](https://git-scm.com/downloads). From the repository root:

```powershell
cargo install --path crates/wtm --locked --bins
```

## Use

Register a repository, then open the dashboard:

```powershell
wtm add C:\src\project
wtm-ui
```

All worktrees of a registered repository are included. The dashboard also includes the repository you launch it from.

Print a snapshot in the terminal:

```powershell
wtm
```

Stop monitoring a repository:

```powershell
wtm remove C:\src\project
```

## Controls

- Drag repository headers or worktree cards to reorder them.
- Use **Hide**, **Show all**, and **Unhide** to control which worktrees appear.
- Drag empty space to move the window; drag its edges to resize it.
- Right-click the window or notification-area icon for opacity, visibility, reset position, and exit controls.

Window settings are remembered. Hover makes the window fully opaque.

## Counts

`S` means source, `T` tests, and `X` scripts and tooling. Each shows added and deleted lines. Binary files are counted separately.

Counts include branch commits, staged and unstaged changes, and untracked files, excluding ignored files. The comparison starts at the common ancestor with the local `main` branch, or `master` if `main` is absent. If neither provides a common ancestor, it compares with `HEAD`.

## License

[MIT](LICENSE).
