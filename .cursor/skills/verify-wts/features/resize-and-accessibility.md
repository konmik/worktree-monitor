# Resize and accessibility

The dashboard keeps cards readable after resizing and scrolling. Its real counts and captions remain available to native accessibility clients.

## Sub-features

- `resize-repaint` repaints descendants and frames without stale card fragments.
- `scroll-repaint` paints correctly after scrolling down and back to the top.
- `visual-counts` displays real per-worktree counts without summary totals.
- `native-accessibility` exposes fixture names and counts through Windows UI Automation.

## How to get to it (user POV)

- Open `wts-ui` directly or through `wts --live`, resize the window, and scroll the repository viewport.
- Read the dashboard visually or through native accessibility tools.

## Driving it with PowerShell

Preconditions:

- Complete Launch and Doctor from the skill. Keep the target visible on the interactive desktop. Run the following checks sequentially.

- **Resize and scroll.** Run `pwsh -NoProfile -File scripts/smoke-resize.ps1 -Binary target/debug/wts-ui.exe`. Require all 24 actions to report zero pixels changed by a full repaint, unchanged geometry during each comparison, and exit zero.
- **Inspect normal rendering.** Read the emitted `proof.json` and open a wide and a narrow `*-normal.png` from the evidence directory. Compare their corresponding `*-repaint.png` files. The normal screenshots must already show clean cards and intentional text ellipses.
- **Capture accessible counts.** Run `pwsh -NoProfile -File scripts/capture-window.ps1 -Binary target/debug/wts-ui.exe -Client`. Require exit zero. Open the printed PNG and the fixture's `visible-text.txt` and `accessibility.txt`. They must include signal-core with `+42 -8`, edge-gateway with `+18 -12`, and infra-tools with `+8 -0`.

## Gotchas

- Both harnesses directly launch `wts-ui.exe` and register their fixtures through its sibling `wts.exe`. Keep both built binaries together. Run the detached-launch feature separately to verify public terminal release.
- Capture forces repaint before taking its PNG. That image alone can hide stale pixels. The resize check captures normal paint before forcing the comparison repaint.
- Occlusion or unrelated always-on-top windows can invalidate screen pixels. Inspect the images before blaming layout.
- These scripts test discrete resize sizes and scroll actions. Continuous live dragging, another monitor's DPI, and other accessibility clients remain unverified unless separately driven.
