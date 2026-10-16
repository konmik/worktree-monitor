# Registration and snapshots

Users register repositories, print changed-line counts, and remove registrations without modifying the monitored repository.

## Sub-features

- `registration-add` registers from a repository subdirectory and rejects duplicates.
- `registration-remove` removes the repository and handles repeated removal.
- `snapshot-counts` reports net changes against HEAD, untracked lines, ignored files, branch, and binary count.
- `cli-errors` exposes invalid usage, non-repository paths, and malformed configuration without overwriting it.

## How to get to it (user POV)

- Run `wtm add` with a repository, linked worktree, or subdirectory path.
- Run `wtm` without arguments for a snapshot.
- Run `wtm remove` with a path inside the registered repository.
- Run `wtm help`, `wtm --help`, or `wtm --version` for CLI information.

## Driving it with PowerShell

Preconditions:

- Complete Launch and Doctor from the skill. No desktop interaction is needed for this check.

- **Drive registration and counts.** Run `pwsh -NoProfile -File scripts/smoke-cli.ps1 -Binary target/debug/wtm.exe`. Require exit zero and `ok`.
- **Read proof.** Open `transcript.txt` in the printed `artifact:` directory. Require the real add, duplicate add, snapshot, remove, duplicate remove, invalid path, and malformed-config results. The changed fixture must report `+4 -1` and zero binaries. The script checks the saved registration set after each mutation and confirms the user's registry is unchanged.
- **Cover alternate paths.** Run `cargo test -p wtm --test cli --locked` for `--help`, `--version`, invalid syntax, and CLI isolation. Run `cargo test -p wtm-core --test behavior --locked` for linked-worktree registration, binary semantics, and additional Git states. Label these as integration tests, not window proof.

## Gotchas

- The final fixture registry is deliberately malformed. Preserve it as failure-path evidence before removing the scratch home.
- Do not add staged and unstaged diff totals. The fixture distinguishes those totals from net changes against HEAD.
- This harness drives `help`. The integration tests cover `--help` and `--version`; neither invocation proves the other was run.
