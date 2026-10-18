# cli-main-menu reference specification

This is a source audit for rebuilding sessionsation-ui faithfully. It describes
the reference, not an implemented replacement. The installed UI is the earlier
numbered launcher; the rejected animated redesign remains recoverable in Git.

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
- [Architecture discussion](architecture.md): proposed implementation boundary and language choices for dot.

## Required adaptations

These override reference behavior. Other differences need an explicit explanation
and review; a similar-looking mockup is insufficient.

- Creation-only UI: New, Resume and Plain Shell. Account/session management stays
  in the separate CLI. No manager, jobs, stop/kill, or nested HERDR viewer action.
- No decorative title, ASCII lettering, replacement brand heading or blank title
  reservation. Functional page labels and the reference status bar are distinct.
- Star field, density and fade extend to the **terminal bottom**, independent of
  option count. Controls are bottom-anchored. Reference list-relative coordinates
  are documented, then adapted explicitly in [visuals](visuals.md#target-geometry).
- Existing public backend API v 1, opaque launch plans, stable identities,
  generation checks and retained HERDR shell remain authoritative. No copied
  Switcher/tmux lifecycle implementation, extra supervisor or credential writer.
- Responsiveness and exact terminal restoration remain requirements. Tests use
  inert fixtures; Agent DM's pinned acceptance, original receipts and live agents
  are outside this rebuild.

The architecture proposal is awaiting discussion with dot before implementation.
Tasks and acceptance gates belong to the linked GitHub issue, not a growing log here.
