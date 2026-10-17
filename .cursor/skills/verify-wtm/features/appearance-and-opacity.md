# Appearance and opacity

Hover makes the dashboard fully opaque. Outside hover, the window follows its saved opacity while stressed text remains fully opaque.

## Sub-features

- `hover-opacity` changes default window opacity from 20% outside hover to 100% during hover.
- `stressed-counts` keeps nonzero source, test, and tooling line-change groups fully opaque while other text follows window opacity.
- `opacity-position` keeps stressed text aligned after the window moves and removes its old pixels.
- `opacity-refresh` removes stressed source text when its count returns to zero.
- `opacity-close` removes the owned text overlay when the dashboard closes.

## How to get to it (user POV)

- Open the dashboard with repositories containing clean and changed worktrees.
- Move the mouse into and out of the window, then move the window.
- Restore source changes and wait for the displayed count to update.

## Driving it with PowerShell

- Complete Launch and Doctor from the skill. Reserve the interactive desktop.
- Run `pwsh -NoProfile -File .cursor/skills/verify-wtm/scripts/drive.ps1 -Check appearance`.
- Require driver PASS and every screen-pixel assertion across hover, leave, movement, source reset, and close. Inspect `regions.json` and the outside, hover, leave, moved, vacated, and source-reset PNGs in the recorded fixture directory.
- Require nonzero line groups to contain fully opaque foreground pixels outside hover. Idle text, zero source counts, binary text, and separators must follow window opacity.

## Gotchas

- Stressed is a cell flag, not a request to brighten its RGB color. Bold and underline are independent cell flags.
- The dashboard currently marks nonzero count groups stressed. It exposes no control applying bold or underline to arbitrary text. `cargo test -p window-win --locked` supplements this live check with styled-cell and native pixel tests.
- This drive checks default opacity and hover behavior, not every saved percentage or context-menu command.
