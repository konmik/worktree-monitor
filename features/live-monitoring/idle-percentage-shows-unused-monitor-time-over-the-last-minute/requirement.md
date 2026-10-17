# Idle percentage is the inverse of scan time over the last minute

Contract: The status line displays the inverse of the percentage of the last minute spent scanning.

Acceptance criteria:

- The status indicator has the form [idle: N%].
- The displayed percentage equals 100% minus the scan-time percentage over the last minute.
