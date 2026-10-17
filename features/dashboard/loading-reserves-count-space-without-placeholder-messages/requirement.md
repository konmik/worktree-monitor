# Loading reserves count space without placeholder messages

Contract: Reserve room for counts while the app scans without displaying confusing pending messages.

Acceptance criteria:

- Keep the count line reserved before lines have been counted.
- Do not show pending messages during startup.
- The window has its intended size from the start and does not resize after scans complete.
