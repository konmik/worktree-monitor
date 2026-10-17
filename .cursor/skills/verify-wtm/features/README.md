# WTM verification map

Read this index before choosing a check. Each file maps user entry points to executable proof.

## Baseline preconditions

- Build from the repository root with `cargo build --locked` and run the doctor in [the skill](../SKILL.md).
- Use the skill-owned driver, which selects both checkout binaries explicitly.
- Use a fresh PowerShell 7 process for each script.
- GUI checks require an interactive Windows desktop and exclusive verification use of that desktop.
- No other WTM UI may be running in the Windows session. Profiles do not isolate the single-instance guard.
- Each harness creates an isolated profile and real Git fixtures. Use only instances it started.

## Driving conventions

- Require exit code zero and read the recorded result. Preserve failed output too.
- Capture the command and resulting state. Read the persisted file for mutations.
- Keep binary hash, source revision, dirty state, feature ID, and entry point with the proof.
- Cover all requested entry points. Report gaps instead of treating another path as equivalent.
- Keep the run's artifact directory. Remove only its listed scratch children after preserving configuration evidence.

## Features

| Feature | Driver check | Scratch children inside that run's artifact directory |
| --- | --- | --- |
| [Registration and snapshots](registration-and-snapshots.md) | `registration` | `home`, `repo`, `plain` |
| [Detached launch](detached-launch.md) | `launch` | `home`, `profile`, `work` |
| [Live updates](live-updates.md) | `live` | `home`, `repo` |
| [Launch context and ordering](launch-context-and-ordering.md) | `ordering` | `home`, `alpha`, `alpha-tree`, `beta`, `beta-tree`, `current`, `missing-git-home` |
| [Resize and accessibility](resize-and-accessibility.md) | `resize`, `capture` | `home`, `unix-home`, `signal-core`, `edge-gateway`, `infra-tools` |
| [Appearance and opacity](appearance-and-opacity.md) | `appearance` | `.config`, `xdg`, `dirty`, `clean` |

## Proof and cleanup

The driver records emitted fixture paths in `result.json` and prints its own output directory. Capture places its PNG outside the fixture directory; retain both paths.

The driver copies configuration into each fixture's `preserved-configs/` directory before removing successful scratch children. Dashboard checks also save `registrations.json` and `saved-order.json`. Failed fixture state is retained for diagnosis. Confirm that results, transcripts, proofs, and images remain readable after cleanup.

`scripts/smoke-worktrees.ps1 -Repository` is an additional read-only discovery check for an existing repository with multiple worktrees. It does not create worktrees. Run it only against a repository selected for that verification task. Its isolated fixture directory holds its snapshot evidence.
