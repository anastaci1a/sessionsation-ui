# Architecture

The Rust popup and preserved Node.js clients invoke `sessionsation api --api-version 1` without a shell and
consumes structured JSON envelopes. The backend owns account state, credential
writers, catalogs, runtime generations, operation locks, message receipts and all
provider behavior. UI state contains no account credentials.

Authoritative backend references:

- [Public contract and schema](https://github.com/anastaci1a/sessionsation/blob/main/docs/contract.md)
- [Architecture](https://github.com/anastaci1a/sessionsation/blob/main/docs/architecture.md)
- [Setup and data roots](https://github.com/anastaci1a/sessionsation/blob/main/docs/cli.md)
- [Acceptance, ownership and adapter coordination](https://github.com/anastaci1a/sessionsation/blob/main/docs/operations.md)
- [Verification scope](https://github.com/anastaci1a/sessionsation/blob/main/docs/operations.md)

The UI checks API version and consumes capability hints. Backend mutation-time
validation remains authoritative. Selections use backend IDs and captured
generations; stale errors require refresh. No automatic mutation retry, legacy
receipt adapter or Submit/Queue semantic change belongs here.

HERDR Space movement uses the backend's generation-bound `spaces/move-space`
actions. The management reducer captures the selected session, cancels pending
choice viewers without mutation, and discards late lookup responses. It never
calls HERDR's pane-move command itself or rewrites ownership metadata.

The management plugin is a viewer. It never launches an agent in an overlay.
The separate terminal launcher executes backend-produced argv with process
replacement. A Bash parent stays alive to retain the host prompt when an agent
exits; an explicit environment bypass prevents recursive menus.

Operators keep deployment selection and private rollback receipts outside Git.
The numbered launcher remains a compatibility entrypoint. The [reference specification](cli-specification/README.md)
owns source behavior; the [approved Rust design](cli-specification/architecture.md)
owns intentional adaptations.

## Reusable Rust launcher

The Rust executable implements the New Session summary, Configuration, account and
Space choices, name forms, shared directory picker and durable Start/status flow.
It also has a management reducer for responsive Home/accounts, capability-driven
account and session actions, asynchronous requests and operation observation.
Its reusable components are:

- `render`: terminal-sized animated field, Unicode cell canvas and output deltas.
  Background geometry is independent of option scrolling.
- `widgets`: option focus/scrolling, enabled/disabled row styling and grapheme-aware
  input editing/painting. Start uses the same row control as the pickers.
  Pinned action headers share the normal selection/navigation model; independent
  preview groups retain a list-wide fit decision within each group.
- `usage`: one measurement/rendering system for remaining-allowance bars, limit
  markers, status-only accounts and shared columns across responsive layouts.
- `popup`: shared frame, measured size and choice-list template (optional boxed
  editor, one scrollable choice list, selected preview and status). A shared
  list finisher places Cancel after one bullet spacer. A footer renderer mounts
  that group for directory browsing while every action above it still scrolls. Callers supply
  content and handle actions; they do not duplicate layout. Pickers preserve the
  opening screen and cache geometry/detail budgets until terminal resize. Labels
  determine ordinary opening width. Account/Space pickers fit their description;
  persistent per-row statuses reserve their own trailing cells and take priority
  over optional usage. Path editors scroll with a leading ellipsis.
  A hovered detail may increase the cached budget, retaining that space until
  close. Directory browsing uses its explanatory second line for normal width and
  90% height, independently of path length. Popup rows reuse the frame’s outer padding; borderless previews
  own one edge cell. Space IDs and archived IDs retain the list-wide
  inline/detached fit rule; persistent account statuses always stay inline.
  All popup templates use HERDR semantic colors: `panel_bg` for the surface,
  `accent` for borders/actions, `text` for primary text and control keys,
  `subtext0` for secondary information/counters, `overlay0` for muted labels
  and control descriptions, and `red`
  for invalid input. These roles stay flat while the main menu animates.
  Shared centered hints preserve opaque panel backgrounds. Compositing translates the native cursor and keeps
  background star lifetimes independent.
- Live session mutations perform a cancellable public `observe` check before
  opening their ordinary form. Failed/interrupted/stopped states show Confirm/Cancel
  first (including before the account picker). Unobserved states explain uncertainty;
  confirmation does not bypass backend safety. Cancellation discards late responses,
  and changed generations require selecting the action again. Focus and Details
  remain read-only and do not require this warning.
- `dialog`: shared one/multiple-field forms, confirmations, information and
  progress. The same boxed editor paints session/account names and directory
  paths. One-field forms remain editable while buttons are focused; multiple-field
  forms expose only the focused native cursor. Information accepts Enter/Esc.
  Geometry is measured once per open dialog; live body/status updates and progress
  completion scroll inside it. Async loading reserves its result viewport before
  the request completes. Options are left aligned below descriptive text and a
  blank line. Fields follow the actual visible text, not unused viewport rows.
  Lone Close actions are presentation-free Enter/Esc dismissals; active progress
  retains Continue in background. Controllers own validation, cancellation and CLI effects.
- `host_theme`: reads HERDR colors without changing host configuration.
- `management`: stable-ID list focus, account action groups and lifecycle menus;
  navigation history restores opener identity and scroll position, and shared
  session-row mapping keeps group separators outside the session identity space.
  Backend effects stay asynchronous and mutations are never automatically replayed.
- `directory`: read-only completion, canonical child resolution and staged paths.
  One picker serves session directories and Global/Per-Space configuration roots.
- `draft` / `preferences`: creation intent and atomic UI-only preferences.
- `app`: page navigation, validation revisions and explicit effects. Pages supply
  choices and policy; they do not implement their own editor or list renderer.
- `client` / runtime: asynchronous public API calls, bounded input/output channels,
  serialized off-thread directory reads and rejection of obsolete results.

Configuration root plans remain intent until Start. Already materialized canonical
root ancestors can be reused; an existing session leaf still requires explicit
selection. Resize/input uses Crossterm's level-triggered TTY reader to avoid losing
input readiness when resize and a key arrive together. No automatic Start replay.
See the [approved design](cli-specification/architecture.md) for target behavior;
PTY, decoded-cell snapshots and installed-HERDR acceptance have distinct evidence
in [verification](verification.md). Baseline/twinkle placement uses a deterministic
seeded Rust PRNG with the audited distributions, rather than Bash's process-global
RANDOM stream; exact random coordinates are not a compatibility promise. Shared
hue rotation retains the reference's independent 32-bit LCG and integer timing math.
