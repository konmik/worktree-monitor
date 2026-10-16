# WTM verification map

Read this index before choosing a check. Each file maps user entry points to executable proof.

## Baseline preconditions

- Build from the repository root with `cargo build --locked` and run the doctor in [the skill](../SKILL.md).
- Use checkout binaries. Pass `-Binary target/debug/wtm.exe` for CLI checks and `-Binary target/debug/wtm-ui.exe` for direct UI, resize, and capture checks.
- Use a fresh PowerShell 7 process for each script.
- GUI checks require an interactive Windows desktop and exclusive verification use of that desktop.
- Each harness creates an isolated profile and real Git fixtures. Use only instances it started.

## Driving conventions

- Require exit code zero and read the recorded result. Preserve failed output too.
- Capture the command and resulting state. Read the persisted file for mutations.
- Keep binary hash, source revision, dirty state, feature ID, and entry point with the proof.
- Cover all requested entry points. Report gaps instead of treating another path as equivalent.
- Keep the run's artifact directory. Remove only its listed scratch children after preserving configuration evidence.

## Features

| Feature | Harness | Scratch children inside that run's artifact directory |
| --- | --- | --- |
| [Registration and snapshots](registration-and-snapshots.md) | `scripts/smoke-cli.ps1` | `home`, `repo`, `plain` |
| [Detached launch](detached-launch.md) | `scripts/smoke-ui.ps1`, `scripts/smoke-launch.ps1` | `home`, `profile`, `work` for direct UI; `home` for CLI launch |
| [Live updates](live-updates.md) | `scripts/smoke-window.ps1` | `home`, `repo` |
| [Launch context and ordering](launch-context-and-ordering.md) | `scripts/smoke-dashboard.ps1` | `home`, `alpha`, `alpha-tree`, `beta`, `beta-tree`, `current`, `missing-git-home` |
| [Resize and accessibility](resize-and-accessibility.md) | `scripts/smoke-resize.ps1`, `scripts/capture-window.ps1` | `home`, `signal-core`, `edge-gateway`, `infra-tools` |

## Proof and cleanup

CLI artifacts are printed as `artifact: ...`. Other checks print `Evidence at ...` or `Real Git fixture and visible text at ...`. Preserve those paths in the session transcript. Capture may place its PNG outside the fixture directory; retain both paths.

Before removing a scratch home, copy any `.config/wtm/config.json` and `.config/wtm/config.order.json` into that run's artifact root. Dashboard checks already save `registrations.json` and `saved-order.json`. Keep fixture repositories when a failed count needs diagnosis. Record that retained scratch state explicitly rather than deleting useful evidence.

`scripts/smoke-worktrees.ps1 -Repository` is an additional read-only discovery check for an existing repository with multiple worktrees. It does not create worktrees. Run it only against a repository selected for that verification task. Its isolated fixture directory holds its snapshot evidence.
