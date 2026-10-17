# Layout stays compact when the window narrows

Contract: Use Yoga to lay out the dashboard without unwanted gaps.

Acceptance criteria:

- Leave no blank line between a repository name and its branches or between adjacent worktrees.
- Narrowing the window does not add extra blank lines between repositories or worktrees.
- Hidden buttons release the space needed to display worktree names.
- Reduce top and side padding in font-character units.
