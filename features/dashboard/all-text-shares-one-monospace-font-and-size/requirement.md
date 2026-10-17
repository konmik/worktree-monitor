# All text shares one monospace font and size

Contract: All dashboard text uses the same monospace font and size.

Acceptance criteria:

- Berkeley Mono is the default font, with fallbacks when it is unavailable.
- JetBrains Mono precedes Consolas in the fallback order.
- The default system monospace font is the final fallback when named fonts are unavailable.
- The font size is 12 and the shared line-height factor is 1.2.
