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
- [Exact CLI artifact provenance](https://github.com/anastaci1a/sessionsation/blob/main/docs/verification.json)

The UI checks API version and consumes capability hints. Backend mutation-time
validation remains authoritative. Selections use backend IDs and captured
generations; stale errors require refresh. No automatic mutation retry, legacy
receipt adapter or Submit/Queue semantic change belongs here.

The management plugin is a viewer. It never launches an agent in an overlay.
The separate terminal launcher executes backend-produced argv with process
replacement. A Bash parent stays alive to retain the host prompt when an agent
exits; an explicit environment bypass prevents recursive menus.

The [deployment record](deployment.json) identifies the selected artifacts. The
numbered launcher remains available for rollback. The [reference specification](cli-specification/README.md)
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
- `usage`: one measurement/rendering system for remaining-allowance bars, limit
  markers, status-only accounts and shared columns across responsive layouts.
- `dialog` / `host_theme`: shared paginated confirmations and forms, preserving
  input across resize and reading HERDR theme colors without changing host config.
- `management`: stable-ID list focus, account action groups and lifecycle menus;
  backend effects stay asynchronous and mutations are never automatically replayed.
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
