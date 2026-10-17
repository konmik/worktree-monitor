# Directory watchers filter Git-ignored updates

Contract: Watch directories and filter Git-ignored updates rather than keeping every individual file path in memory.

Acceptance criteria:

- Git-ignored updates do not trigger monitoring scans.
- Directory monitoring does not retain an inventory of every individual file path.
