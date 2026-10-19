# Comparing the reference and implementation

The [source map](source-map.md) identifies the pinned reference files. The
[visual](visuals.md), [motion](motion.md), [input](input.md) and
[directory](directory-picker.md) documents describe their behavior. Current
implementation checks and the real-HERDR demo belong to [verification](../verification.md).

Compare the complete terminal viewport at fixed geometry and controlled time/seed,
including Unicode width, text masks, Back placement, scrolling, disabled labels,
colors and full-height stars. Compare animation over time, not just a still frame.
The Rust PRNG follows the audited distributions; exact Bash RANDOM coordinates
are not a compatibility promise. Reference glitches are documented, not reproduced
as requirements. Use synthetic paths/accounts and never capture an operator's host.

Cell goldens provide deterministic layout/color comparisons; PTY and isolated
HERDR tests separately verify input, restoration and host handoff. A generated
picture alone does not prove runtime safety or provider functionality.
