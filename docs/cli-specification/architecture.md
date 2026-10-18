# Rust popup design

Ana approved Rust and this interaction model. Implemented components are described
in [current architecture](../architecture.md); [deployment](../deployment.json)
identifies the selected artifacts and preserved rollback entrypoints.
[Issue 7](https://github.com/anastaci1a/sessionsation-ui/issues/7) owns the expanded
account/session management acceptance. The deployment record is authoritative for
the installed version; source capabilities and verification are tracked separately.

## User flow

The HERDR plugin opens a popup using the manifest's percentage dimensions. Ana uses `prefix+shift+l`; each
installer chooses a binding without replacing unrelated keys or the user's prefix.
Main menu: New Session, Manage Sessions, Add Account/Manage Accounts, a dot
separator, Settings, then the complete inline accounts module when it fits.
No decorative title or reserved title
area. Stars/gradient cover the entire popup PTY; only options scroll. Hints stay
at the bottom. The compact Back arrow sits immediately left of the first visible
option. Left focuses it and expands its label where space permits; Right returns
to the remembered option. The main page says Close and dismisses the popup;
other pages say Back. Expansion requires one left-padding cell after measuring
the complete label. Standalone shell remains excluded.

New Session is an editable summary: Account, Space, Name, Directory. Pickers open
as separate pages. Start Session uses the reference Sessions page's separator
before Session Manager: the fields, separator and Start form one contiguous group.
It is gray/struck through
and disabled until all fields are valid. Back preserves draft choices.

- Space defaults to HERDR's currently focused Space, using its explicit focused
  flag and stable ID. No focused Space means unset, even when other Spaces exist.
  The picker initially highlights the chosen Space and preserves highlighted IDs
  through reorder. Explicit choices survive focus changes; proposed new names
  reject duplicates. Creation waits until Start.
- Name is the unique backend name and normal tab label, also a valid single path
  component. No silent slugification.
- Directory is `<unset>` until a valid name/root yields `root/name`. Automatically
  derived paths follow Name/Space changes; manually chosen paths remain unchanged.
  An existing automatic path requires explicit selection. Every Create Directory
  action stages intent, including configuration pickers; no directory is created
  before Start. Enter accepts a child; Right browses only a child row, doing
  nothing on `<use this dir>` and `<create dir>`. Tab and Ctrl-U have no picker action.
  The path stays typable.
- Account remembers the last successful launch. Backend-computed fresh usage and
  existing limits govern initial prefill. Retain a selected account through refresh
  and usage expiry; clear it only on authoritative unavailability. Manual selection may
  override this preference restriction when the backend permits launch. Never
  silently substitute an account or infer permission to spend purchased credits.

Configuration has Global/Per-Space directory mode. Inactive controls use reference
gray/strike styling; all values survive mode switches. Back saves atomically without
Save/Cancel buttons. Missing per-Space roots require manual directory selection,
not global fallback. Preferences own no credentials or lifecycle state.
Labels are short: Defaults, Global root and Space roots. Root previews appear only
when the complete value fits beside the label without recentering the option block.

## Destination and ownership

| Component | Authority |
| --- | --- |
| Agent DM | Hierarchy and coordination |
| sessionsation | Account/authentication, catalog, lifecycle and launch admission |
| HERDR | Spaces/workspaces and terminal presentation |
| UI | Draft choices, preferences and public-contract results |

No Projects entity is introduced. Workspace selections are scoped to the HERDR
instance and stable workspace ID, never label/order. Focus supplies only the default;
it cannot retarget an explicit choice. Launch revalidates the
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

## Account and session management

Account circles align with the main option circles. The inline module displays
all accounts or none, measuring the complete menu, one blank line, per-account
height and bottom hints. Bars share a start derived from the longest displayed
name; preferred width is 24 cells, minimum 12. Existing long names are ellipsized
to 12 display cells, never renamed; details show the full ID. New names are limited
to 12 ASCII characters excluding the provider prefix. Disabled/removed accounts
remain selectable for management.

Verified five-hour accounts show adjacent 5h/weekly remaining bars, then one blank
line before the next account. Other accounts show status only in this module.
Unknown values remain explicit; stale observations are marked. Labels sit to the
right, left-aligned by period and right-aligned by percent. Shared percentage width
uses three digits only when a displayed value is 100. Custom limits are vertical
markers inside the bars, converted from maximum used to minimum remaining.

Compact Manage Accounts has `<add account>` first. The highlighted account's same
bar component appears immediately above the bottom controls with exactly one blank
row between. Resize returns only this list to Home when the full module fits,
retaining account focus. Account actions and open forms stay in place; Back resolves
to Home or the compact list according to current fit.

The account **picker** used by New, Resume and Switch keeps normal option names
and a separate right-aligned usage column. It shows `x% 5h / x% wkly` remaining,
omitting the short period and slash unless a five-hour window is verified. Percent
signs and slashes share columns, using the minimum padding required by the largest
displayed number. Weekly-only rows align with the weekly column. Unknown values
use `?`; freshness and full account ID appear in the selected-account note.

Account action groups use dot/blank separators: Usage details and Limits; Log in
and Enable/Disable; Remove/Restore. Limits edit minimum remaining percentages with
explicit Save/Cancel; the backend converts/stores existing used thresholds through
the sole owner, preserving plan review and leaving automation unchanged. Impactful
Disable/Remove actions require confirmation. Removal first presents affected
sessions/data and requires the exact account ID against a fresh preview revision;
history and credentials remain recoverable, with no provider deletion or purge.

Manage Sessions lists running, saved and unavailable sessions. Selecting opens
actions, never attaches. Details, Rename, Switch, Restart, Stop and Resume are
capability-gated; Stop/Restart/Switch require confirmation. Switch uses the shared
account picker. Focus tab explicitly verifies the current generation, HERDR target
and foreground PID, then focuses the existing tab and exits. No messaging/steering
controls belong here.

Resume uses the shared summary with editable account, Space and name, read-only
saved directory. Prefer the original Space only when its host/socket and ID still
match; otherwise prefer current focus, else unset. Stable session/conversation IDs
and history remain backend-owned. Changing account uses its existing safe transfer
worker. Canceling Resume restores the independent New draft.

Installed HERDR 0.9.5 supports one popup at a time, so confirmations/forms/progress
use internal nested dialogs over the preserved page. Theme colors are read from
HERDR's fixed theme/custom RGB overrides (Ana uses Vesper); dynamic/terminal themes
have an explicit configured-base fallback. Long consent text, paths and IDs wrap
without truncation and paginate before confirmation. Continue in background returns
to the menu, never cancels or steals focus later. Operations can reopen original
New/Resume receipts and transfer jobs after closing the popup; uncertain outcomes
are inspected, never replayed. Cancellation/recovery are separate backend actions.
