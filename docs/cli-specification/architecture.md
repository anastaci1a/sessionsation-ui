# Architecture discussion for dot

Status: proposal for discussion before rebuilding. The reference audit is
complete; no new renderer or frontend framework is selected by this document.
The [current architecture](../architecture.md) remains the installed contract.

## Language decision

Ana prioritizes speed, consistency and fewer bugs. Dot leans toward a separate
Rust frontend; the language is **not finalized**. Keep Bash limited to necessary,
tested host bootstrap. Whichever
runtime is selected must port the reference algorithms and form state machines
faithfully, using cell traces and interaction fixtures before product adaptation.

The previous attempt failed as a specification/parity exercise: it simplified
motion, changed picker semantics, omitted inline path completion/name flow and
introduced a different page layout. Changing language alone does not fix those
differences. The source rules and reviewed comparison evidence must lead the port.

| Option | Fit here | Cost/risk to resolve |
| --- | --- | --- |
| Rust terminal frontend | Existing backend language; explicit types and integer behavior; possible single UI executable | Separate frontend build/deployment; do not link private backend lifecycle/storage internals; current host lacks native Cargo and uses offline build container |
| Node ESM + JSDoc or TypeScript | Existing Node client, argv validation, config and tests; async subprocess/filesystem I/O; terminal state isolated from backend | Exact integer/PRNG behavior needs care; TypeScript adds compiler/build artifacts if chosen; must handle backpressure and grapheme widths explicitly |
| Bash runtime port | Closest initial source behavior | Dynamic scope, shell job coupling, subprocess-heavy layout and duplicate lifecycle glue; contrary to avoiding an all-Bash rebuild |

No new dependency is needed to finish the audit. Framework adoption is undecided:
generic list widgets can change spacing, focus, timing and rendering semantics.
A minimal terminal driver with pure core is preferable unless a framework proves
exact control without making every frame a full-screen redraw.

## Preferred host flow: popup to a new normal tab

Ana requests a HERDR plugin terminal popup opened by a configurable keybinding.
Each installer chooses its own binding. Her preference is prefix then uppercase L:
the explicit modifier spelling is `prefix+shift+l`. Preserve the user's prefix
and unrelated bindings; `prefix+l` is the default focus-right action. Installation
must preview conflicts and retain a reversible configuration change.

The proposed plugin declares a launcher action and a pane entrypoint with
`placement = "popup"`. A `[[keys.command]]` with `type = "plugin_action"` invokes
the qualified action, which opens that entrypoint. Binding an action alone does
not make it a popup. HERDR also supports direct `type = "popup"` shell commands,
but the plugin route keeps launcher identity and installation together. The
existing manifest currently declares only the management overlay: neither this
action nor a popup launcher is installed. [Verification](verification.md#herdr-popup-compatibility)
records installed-version checks and the CLI-help discrepancy.

Proposed configuration shape, **not an installation command**; the action ID
below must be implemented before use:

```toml
[[keys.command]]
key = "prefix+shift+l"
type = "plugin_action"
command = "sessionsation.launcher"
description = "create or resume a session"
```

The popup is a terminal UI, not an agent pane. Official HERDR semantics give it
no pane ID, pane lifecycle events, persistence or agent attachment; focus context
still identifies the underlying tiled pane. Never substitute that underlying ID
as the new agent target. Popup input, including Escape, belongs to its process.
Normal cancellation restores the TTY and exits that process. Do not blindly issue
global `popup.close`, which could close a replacement popup after a race.

Selection requests a **separate normal tab**, with the chosen directory, account
and retained host shell. Only after confirmed creation/handoff does the popup
exit; the model process is never launched inside it or promoted out of it. Resume
must preserve stable identity and fresh generation checks. Plain Shell also opens
a normal tab. Errors stay visible; uncertain admission must be reconciled without
automatic retry. Viewer detachment and popup cancellation never mean agent Stop.

This needs a backend-owned public launch/host contract before wiring the UI:
today `launcher new/resume/shell` returns an opaque argv plan for execution in the
calling terminal. HERDR `tab.create` exposes cwd/env/label/focus but no command argv.
Executing today's plan in the popup would put the agent in the wrong terminal.
Coordinate a host adapter that creates and verifies the exact new tab/pane,
launches the plan with the existing retained-shell/environment behavior, and
reports stable session/runtime identities and failure/admission state. Do not
inject reconstructed provider commands via terminal text. Preserve the existing
launcher bypass so a newly created tab cannot recursively reopen the menu. The
UI consumes this public operation; it owns neither a lifecycle engine nor a
second data-root supervisor. Session naming should join the same atomic operation.

Full-height means the popup's **entire PTY content viewport**: field, gradient and
bottom controls reach its terminal bottom independently of list length. The HERDR
border is outside that viewport. Resize follows PTY dimensions, not the outer
host's dimensions. Popup size remains a reviewed installer preference, not a new
fixed geometry assumption. This is still creation-only with no decorative title.

Official references: [configuration](https://herdr.dev/docs/configuration/),
[plugin panes](https://herdr.dev/docs/plugins/),
[socket API](https://herdr.dev/docs/socket-api/). Source/version evidence is in the
linked verification section; no runtime popup acceptance is claimed yet.

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
6. **Launcher coordinator:** creation/resume/plain-shell only, consumes the agreed
   public host-aware operation above and closes its popup after confirmed handoff.
   Retains account IDs/generations/cwd semantics. Existing management CLI remains
   outside these models and screens. Never executes an agent in the popup.

Use one renderer and one form/selection state machine shared across pages, with
small explicit differences for directory navigation versus text cursor editing.
Avoid per-page copies or an independent lifecycle implementation in the UI.

## Decisions requested from dot

- Finalize Rust versus the existing Node runtime and choose build/dependency
  policy. Rust is dot's leading candidate, not an approved implementation choice.
- Agree the plugin action → popup → backend-created normal tab contract, including
  admission reconciliation, target identity, recursive-launch bypass and retained
  shell. Resolve the installed CLI-help mismatch with an isolated host test before
  deployment; do not test by manipulating existing live panes.
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
sizes. Next port inline forms and picker keys, then connect the agreed host-aware
public client. Final gate is the authorized future-terminal deployment with
rollback, plus actual HERDR viewer review. No new model usage is part of these slices.

[Issue 5](https://github.com/anastaci1a/sessionsation-ui/issues/5) owns acceptance
status and dot's responses. Approval of
this audit is not Agent DM adapter acceptance or authorization to alter pins,
original receipts, root supervisor, accounts or live sessions.
