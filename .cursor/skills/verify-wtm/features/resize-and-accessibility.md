# Resize and accessibility

The dashboard keeps cards readable after resizing and scrolling. Its real counts and captions remain available to native accessibility clients.

## Sub-features

- `resize-repaint` repaints descendants and frames without stale card fragments.
- `grid-layout` preserves row spacing and wraps errors according to available character columns.
- `scroll-repaint` paints correctly after scrolling down and back to the top.
- `visual-counts` displays real per-worktree counts without summary totals.
- `native-accessibility` exposes fixture names and counts through Windows UI Automation.

## How to get to it (user POV)

- Open `wtm-ui`, resize the window, and scroll the repository viewport.
- Read the dashboard visually or through native accessibility tools.

## Driving it with PowerShell

Preconditions:

- Complete Launch and Doctor from the skill. Keep the target visible on the interactive desktop. Run the following checks sequentially.

- **Resize and scroll.** Run `pwsh -NoProfile -File .cursor/skills/verify-wtm/scripts/drive.ps1 -Check resize`. Require all 36 comparisons to report zero pixels changed by a full repaint, unchanged geometry during each comparison, and exit zero.
- **Inspect normal rendering.** Read the emitted `proof.json` and open a wide and a narrow `*-normal.png` from the evidence directory. Compare their corresponding `*-repaint.png` files. The normal screenshots must already show clean cards and intentional text ellipses.
- **Capture accessible counts.** Run `pwsh -NoProfile -File .cursor/skills/verify-wtm/scripts/drive.ps1 -Check capture`. Require exit zero. Open `dashboard.png` and the fixture's `visible-text.txt` and `accessibility.txt`. They must include signal-core with `S +42/-8` and one binary, edge-gateway with `S +18/-12`, and infra-tools with `S +8/-0`.

## Gotchas

- Both harnesses directly launch `wtm-ui.exe` and register fixtures through its sibling `wtm.exe`. Run the launch feature separately for console separation, single-instance behavior, and startup failures.
- Capture uses an opaque fixture and forces repaint before taking its PNG. That image alone can hide stale pixels. The resize check captures normal paint before forcing the comparison repaint. The appearance feature checks transparency separately.
- Occlusion or unrelated always-on-top windows can invalidate screen pixels. Inspect the images before blaming layout.
- These scripts test discrete resize sizes and scroll actions. Continuous live dragging, another monitor's DPI, and other accessibility clients remain unverified unless separately driven.
