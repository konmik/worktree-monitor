# All text shares one monospace font and size

Contract: Use the same monospace font and size for all dashboard text.

Acceptance criteria:

- Use Berkeley Mono by default, with fallbacks when it is unavailable.
- Place JetBrains Mono before Consolas in the fallback order.
- Use the default system monospace font when the named fonts are unavailable.
- Use font size 12 and a shared line-height factor of 1.2.
