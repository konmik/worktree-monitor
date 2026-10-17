# Registration and snapshots

Users register repositories, print changed-line counts, and remove registrations without modifying the monitored repository.

## Sub-features

- `registration-add` registers from a repository subdirectory and rejects duplicates.
- `registration-remove` removes the repository and handles repeated removal.
- `snapshot-counts` reports branch and working-tree changes against the merge base with local main, then master, with HEAD as fallback. It includes non-ignored untracked files, S/T/X categories, and nonzero binary counts.
- `cli-errors` exposes invalid usage, non-repository paths, and malformed configuration without overwriting it.

## How to get to it (user POV)

- Run `wtm add` with a repository, linked worktree, or subdirectory path.
- Run `wtm` without arguments for a snapshot.
- Run `wtm remove` with a path inside the registered repository.
- Run `wtm help`, `wtm --help`, or `wtm --version` for CLI information.

## Driving it with PowerShell

Preconditions:

- Complete Launch and Doctor from the skill. No desktop interaction is needed for this check.

- **Drive registration and counts.** Run `pwsh -NoProfile -File .cursor/skills/verify-wtm/scripts/drive.ps1 -Check registration`. Require exit zero, driver PASS, and `ok`.
- **Read proof.** Open `transcript.txt` in the printed `artifact:` directory. Require the real add, duplicate add, snapshot, remove, duplicate remove, invalid path, and malformed-config results. The changed fixture must report `S +4/-1` without a binary indicator. The script checks the saved registration set after each mutation and confirms the user's registry is unchanged.
- **Cover alternate paths.** Run `cargo test -p wtm --test cli --locked` for `--help`, `--version`, invalid syntax, and CLI isolation. Run `cargo test -p wtm-core --test behavior --locked` for linked-worktree registration, binary semantics, and additional Git states. Label these as integration tests, not window proof.

## Gotchas

- The final fixture registry is deliberately malformed. Preserve it as failure-path evidence before removing the scratch home.
- Do not add staged and unstaged diff totals. The fixture distinguishes those totals from net changes against the comparison base.
- The driver also exercises `--help`, `--version`, and rejection of the removed `--live` flag with exit 2.
