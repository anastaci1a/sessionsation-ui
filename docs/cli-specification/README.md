# cli-main-menu reference specification

This is a source audit for rebuilding sessionsation-ui faithfully. It describes
the reference. The [deployment record](../deployment.json) identifies the installed
Rust replacement; the earlier numbered launcher remains available for rollback.

Reference: [cli-main-menu ae 786 af](https://github.com/anastaci1a/cli-main-menu/tree/ae786af695791fdf858eacf5780e04b5138be49d),
the clean Satellite checkout inspected on 2026-10-18. Fetched remote main
`8dad98276dd8b6e5c55c360a6802813bb99f4a9c` changes only the generated PNG/GIF;
all audited code, tests and tooling are identical. The local ignored configuration
changes only the title to `SATELLITE`; it is not copied into this repository.

Read by purpose:

- [Visual layout and styles](visuals.md): coordinates, text, colors, masks and sizing.
- [Animation](motion.md): exact timing, distributions, hue, fades and repair rules.
- [Input and terminal lifecycle](input.md): selection, focus, fallback and cleanup.
- [Directory and name forms](directory-picker.md): inline editing, browsing, validation and creation.
- [Flows and integration boundaries](flows.md): every reference action, discovery and installation.
- [Complete source map](source-map.md): all tracked files and their audit destinations.
- [Verification](verification.md): reference evidence, limitations and comparison method.
- [Approved Rust design](architecture.md): implementation boundary and reviewed interactions.

## Required adaptations

These override reference behavior. Other differences need an explicit explanation
and review; a similar-looking mockup is insufficient.

- New-session-only UI: choose account, directory and destination HERDR
  Space/workspace. Existing-session resume, switching, attachment, standalone
  Plain Shell and management are outside this replacement's scope.
- Preferred entry is a HERDR plugin popup with an installer-chosen keybinding;
  selection creates a normal tab, then the popup exits. It never hosts/promotes an
  agent. The proposed contract and Ana's binding are in [architecture](architecture.md#user-flow).
- No decorative title, ASCII lettering, replacement brand heading or blank title
  reservation. Functional page labels and the reference status bar are distinct.
- Star field, density and fade extend to the **terminal bottom**, independent of
  option count. In a popup, terminal means its full PTY content viewport, excluding
  the HERDR border. Hints are bottom-anchored; Back sits beside the top option and
  Start follows the summary fields and separator. Reference list-relative coordinates
  are documented, then adapted explicitly in [visuals](visuals.md#target-geometry).
- Existing public backend API v 1, opaque launch plans, stable identities,
  generation checks and retained HERDR shell remain authoritative. No copied
  Switcher/tmux lifecycle implementation, extra supervisor or credential writer.
- Responsiveness and exact terminal restoration remain requirements. Tests use
  inert fixtures; Agent DM's pinned acceptance, original receipts and live agents
  are outside this rebuild.

Ana approved the Rust design and editable-summary flow. Implementation and
verification follow the architecture document; current evidence lives in
[verification](../verification.md).
[Issue 5](https://github.com/anastaci1a/sessionsation-ui/issues/5) owns decisions,
tasks and acceptance gates; this directory owns the specification.
