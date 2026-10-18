# Architecture discussion for dot

Status: proposal for discussion before rebuilding. The reference audit is
complete; no new renderer or frontend framework is selected by this document.
The [current architecture](../architecture.md) remains the installed contract.

## Recommendation

Keep Node 22 ESM for the UI runtime, with explicit typed interfaces (JSDoc initially,
or TypeScript with a build step if dot prefers). Keep Bash only for the already
tested HERDR environment/retained-shell bootstrap. Port the reference renderer's
algorithms and form state machines faithfully into that runtime. Use reference
cell traces and interaction fixtures to prove parity before product adaptation.

The previous attempt failed as a specification/parity exercise: it simplified
motion, changed picker semantics, omitted inline path completion/name flow and
introduced a different page layout. Changing language alone does not fix those
differences. The source rules and reviewed comparison evidence must lead the port.

| Option | Fit here | Cost/risk to resolve |
| --- | --- | --- |
| Node ESM + JSDoc or TypeScript | Existing Node client, argv validation, config and tests; async subprocess/filesystem I/O; terminal state isolated from backend | Exact integer/PRNG behavior needs care; TypeScript adds compiler/build artifacts if chosen; must handle backpressure and grapheme widths explicitly |
| Rust terminal frontend | Existing backend language; explicit types and integer behavior; possible single UI executable | Separate frontend build/deployment; do not link private backend lifecycle/storage internals; current host lacks native Cargo and uses offline build container |
| Bash runtime port | Closest initial source behavior | Dynamic scope, shell job coupling, subprocess-heavy layout and duplicate lifecycle glue; contrary to avoiding an all-Bash rebuild |

No new dependency is needed to finish the audit. Framework adoption is undecided:
generic list widgets can change spacing, focus, timing and rendering semantics.
A minimal terminal driver with pure core is preferable unless a framework proves
exact control without making every frame a full-screen redraw.

## Proposed module boundary

1. **Model/reducer:** immutable page, option identity, focused control, text cursor,
   picker path/history and pending request token. `input → state + effects`;
   context-specific keys, not one universal Enter/Space handler.
2. **Geometry/style:** pure terminal dimensions + model → reserved bars/fields,
   option viewport, bottom controls, masks. Test reference geometry separately
   from Ana's no-title/full-height adaptation. Text widths are terminal cells.
3. **Animation:** seeded random source + monotonic time + geometry → baseline,
   births, replacement debts, palette/sweep and final styled cells. Preserve
   distributions and integer rounding. JS bitwise operators are signed 32-bit;
   do not copy Bash 63-bit packed RGB/tag arithmetic blindly. Use explicit tuples
   or BigInt and supplied random-draw fixtures.
4. **Terminal driver:** only owner of raw input, focus/paste/wrap modes, resize,
   diff output and cleanup. One bounded event queue; cancellation remains
   responsive during asynchronous reads and output backpressure. Restores state
   before any opaque plan handoff; never a provider supervisor.
5. **Effects/adapters:** async directory enumeration/validation/explicit mkdir and
   existing public backend client. No backend calls in animation ticks or arrows.
   Request tokens reject stale read completions; mutation cancellation is not a
   claim of rollback or non-admission. No automatic retry or duplicate sends.
6. **Launcher coordinator:** creation/resume/plain-shell only, executes validated
   backend plan unchanged. Retains account IDs/generations/cwd semantics. Existing
   management CLI remains outside these models and screens.

Use one renderer and one form/selection state machine shared across pages, with
small explicit differences for directory navigation versus text cursor editing.
Avoid per-page copies or an independent lifecycle implementation in the UI.

## Decisions requested from dot

- Prefer Node ESM/JSDoc for the next slice, TypeScript compiled to the same runtime,
  or a separate Rust frontend? Recommendation: Node with typed module boundaries;
  choose compiler policy before introducing dependencies.
- Confirm target coordinate examples from [visuals](visuals.md#target-geometry):
  no decorative title, functional status/page bars, full-height field and bottom
  controls. The source's Back-at-first-option placement is explicitly overridden.
- Reference asks for a session name before directory/account. Current public
  `launcher new` plan uses backend-generated naming. Should backend add an atomic
  public name parameter before this flow is ported? Do not fake it with a second
  rename mutation, tmux name or UI catalog. Resume/live-attach availability likewise
  stays capability-driven and must not be inferred from legacy code.
- Confirm deliberate enhancements: safe bracketed paste, full grapheme width,
  cancellation during async directory reads, minimum-size view. Retain reference
  key semantics; no extra Home/End/Page/Delete behavior by accident.
- Resolve attribution/license for literal source reuse before copying files or
  tests: the inspected reference tree has **no LICENSE file**. Ana has requested
  closer copying, but that does not identify the intended redistribution license.
  This audit links source and summarizes behavior; it imports no source/assets.
  Original Switcher/backend license obligations remain in their owning repository.

## Suggested implementation order after discussion

First build an inert reference-geometry renderer with injected clock/randomness
and trace equality for motion, colors and cell masks. Then apply only the explicit
no-title/full-height geometry changes and review real ANSI captures at multiple
sizes. Next port inline forms and picker keys, then connect the existing public
plan client. Final gate is the authorized future-terminal deployment with rollback,
plus actual HERDR viewer review. No new model usage is part of these slices.

[Issue 5](https://github.com/anastaci1a/sessionsation-ui/issues/5) owns acceptance
status and dot's responses. Approval of
this audit is not Agent DM adapter acceptance or authorization to alter pins,
original receipts, root supervisor, accounts or live sessions.
