# Directory watchers filter Git-ignored updates

Contract: Monitoring uses directory watchers that filter Git-ignored updates.

Acceptance criteria:

- Git-ignored updates do not trigger monitoring scans.
- Directory monitoring does not retain an inventory of every individual file path.
