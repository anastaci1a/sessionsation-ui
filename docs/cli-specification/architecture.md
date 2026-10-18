# Rust launcher design

Ana approved Rust and this interaction model. Implementation is incremental; the
installed numbered launcher remains described in [current architecture](../architecture.md).
[Issue 5](https://github.com/anastaci1a/sessionsation-ui/issues/5) owns acceptance.

## User flow

The HERDR plugin opens a half-terminal popup. Ana uses `prefix+shift+l`; each
installer chooses a binding without replacing unrelated keys or the user's prefix.
Main menu: New Session and Configuration. No decorative title or reserved title
area. Stars/gradient cover the entire popup PTY; only options scroll. Back and hints
stay at the bottom. No existing-session operations or standalone shell action.

New Session is an editable summary: Account, Space, Name, Directory. Pickers open
as separate pages. Start Session uses the reference Sessions page's separator
before Session Manager, anchored above bottom navigation. It is gray/struck through
and disabled until all fields are valid. Back preserves draft choices.

- Space starts with the originating Space when known. Existing choices use IDs;
  proposed new names reject duplicates. Creation waits until Start.
- Name is the unique backend name and normal tab label, also a valid single path
  component. No silent slugification.
- Directory is `<unset>` until a valid name/root yields `root/name`. Automatically
  derived paths follow Name/Space changes; manually chosen paths remain unchanged.
  An existing automatic path requires explicit selection. Every Create Directory
  action stages intent, including configuration pickers; no directory is created
  before Start. Enter accepts a child, Right/Tab browses, and the path stays typable.
- Account remembers the last successful launch. Backend-computed fresh usage and
  existing limits govern prefill; otherwise show `<unset>`. Manual selection may
  override this preference restriction when the backend permits launch. Never
  silently substitute an account or infer permission to spend purchased credits.

Configuration has Global/Per-Space directory mode. Inactive controls use reference
gray/strike styling; all values survive mode switches. Back saves atomically without
Save/Cancel buttons. Missing per-Space roots require manual directory selection,
not global fallback. Preferences own no credentials or lifecycle state.

## Destination and ownership

| Component | Authority |
| --- | --- |
| Agent DM | Hierarchy and coordination |
| sessionsation | Account/authentication, catalog, lifecycle and launch admission |
| HERDR | Spaces/workspaces and terminal presentation |
| UI | Draft choices, preferences and public-contract results |

No Projects entity is introduced. Workspace selections are scoped to the HERDR
instance and stable workspace ID, never label/order/focus. Launch revalidates the
chosen target; disappearing targets fail instead of falling back to current focus.
The popup has no agent pane identity. Start creates a normal tab in the destination
(or uses the initial normal tab of a newly created Space), with retained shell,
chosen account environment and exact name/directory. The popup exits after confirmed
handoff; cancellation/detachment never means Stop. No process promotion.

## Implementation boundaries

Use Rust 2024, Crossterm, Tokio, Serde and Unicode segmentation/width with locked
dependencies. A pure reducer, cell renderer and shared picker/forms drive every
screen. Bounded async effects perform filesystem/public API work; revision tokens
reject stale completions. No backend calls on arrows or animation ticks, no history
scan to list accounts, no synchronous I/O in the render/input loop.

Backend public API additions own read-only launch validation, request-ID admission,
status reconciliation and account prefill eligibility. Start alone creates planned
resources. Serialize admissions, reserve the name, revalidate and retain durable
outcomes. No automatic replay after uncertain delivery. Separate admission from
running, and retain recoverable resources when ownership/execution is uncertain.
Existing exec plans, legacy message receipts and sole root supervision remain intact.

## Visual fidelity and deliberate repairs

Preserve audited colors, shared hue cycles, shimmer timing/shape, twinkle density,
replacement fades, masks and chooser/form keys. Reference source is attribution and
behavior evidence; Rust implementation does not import its unlicensed source files.
Fix pasted-newline activation, wide-text misalignment, tiny-window overlap, blocking
reads and lost draft state. Staged mkdir, summary flow and full-height fixed stars
are intentional adaptations. Tests separate reference geometry from target geometry.

## Performance and verification

At 80×24 and 160×48, release-build targets are p95: first input-responsive paint
100 ms; host plugin-to-popup 250 ms; ordinary key feedback 25 ms; page first paint
50 ms; frame computation/diff 8 ms. Preserve 20 FPS animation; input paints between
ticks. Esc allows 110 ms including sequence decoding. Measure data-ready time
separately, report counts/p50/p95/max and bytes, and inject two-second I/O delays.

Injected clock/random draws and decoded ANSI cells verify motion, masks and partial
repair; resize/scroll must not move stars. PTYs cover typing/paste, paths, disabled
controls, Back, cleanup and cancellation. Fixtures prove zero resource creation
before Start, explicit existing-directory consent, stale targets and no duplicate
Start. Isolated installed-HERDR tests use inert agents, separate config/socket/root,
and verify handoff/environment/retained shell without models or live panes.

Publish tests and artifact provenance before a reversible deployment. Preserve
original installs, credentials, Agent DM pins/data, receipts and live sessions.
