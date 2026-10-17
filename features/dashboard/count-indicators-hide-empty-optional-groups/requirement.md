# Count indicators hide empty optional groups

Contract: Change-category indicators are visible only when at least one counter is nonzero, except for S.

Acceptance criteria:

- S remains visible and grey when both source counters are zero.
- Nonzero S indicators are colored.
- Other change-category indicators are hidden when both counters are zero.
