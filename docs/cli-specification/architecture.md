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
other pages say Back. Odd centering rounds right; six free columns reserve four
on the left so the compact arrow retains an edge padding cell. Expansion requires
one left-padding cell after measuring
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
- Directory is `[unset]` until a valid name/root yields `root/name`. Automatically
  derived paths follow Name/Space changes; manually chosen paths remain unchanged.
  An existing automatic path requires explicit selection. Every Create Directory
  action stages intent, including configuration pickers; no directory is created
  before Start. Enter accepts a child; Right browses only a child row, doing
  nothing on `[use this dir]` and `[create dir]`. Tab and Ctrl-U have no picker action.
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
are intentional adaptations. Current text masks include internal spaces: baseline
stars and transient twinkles never appear inside labels, input text or controls.
The star gradient and twinkle/replacement fades blend from the terminal background
reported through a nonblocking OSC 11 query. The inherited background is never
repainted. Terminals without a reply use a dark fallback (16,16,16); host palette
limitations remain as documented. Color replies are filtered from keyboard input;
unmatched or incomplete input is replayed.
Tests separate reference geometry from target geometry.

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

Main options through Settings form their own centered block. The inline account/bar
module is centered independently, caps bar width at 24 cells and shrinks to fit
the popup. Names have no selector circles; selected names are bold with the normal
full animated accent; unselected names retain that accent without bold. Selection
adds a small triangle in a reserved lane. Names and bars have one space between. The inline module displays
all accounts or none, measuring the complete menu, one blank line, per-account
height and bottom hints. Bars share a start derived from the longest displayed
name; preferred width is 24 cells, minimum 12. Existing long names are ellipsized
to 12 display cells, never renamed; details show the full ID. New names are limited
to 24 ASCII characters excluding the provider prefix; session names allow 24 Unicode
scalar values, with session titles ellipsized on display cell 16. Disabled/removed accounts
remain selectable for management.

Verified five-hour accounts show adjacent 5h/weekly remaining bars, then one blank
line before the next account. Weekly-only accounts show their weekly bar. Missing observations show the applicable Disabled, Login required or Usage unavailable state; they do not imply an in-flight fetch.
Unknown values remain explicit; stale observations are marked. Bars use connected capped lines: heavy remaining segments and undepleted threshold
crossings share the animated accent. Thin depleted segments and mixed-weight
transitions are visible gray (RGB 144). The boundary uses `╾`; threshold crossings
use `┿`, `┽` or `┼` as allowance depletes. Caps switch between `╺`/`╶` and `╸`/`╴`
so adjacent stroke weights match even at 0%/100%. On the right, percentages come first in a
right-aligned column, then left-aligned period labels. Shared percentage width
uses three digits only when a displayed value is 100. Custom limits are vertical
markers inside the bars, converted from maximum used to minimum remaining.

Compact Manage Accounts has `[add account]` first. The highlighted account's same
bar component appears one blank row below the options, or below the visible-range
counter when scrolling. Enabled/Disabled status text shares a left-aligned column;
Enabled uses the selected/dimmed accent; Enabled* marks attention. Disabled is
muted gray and suppresses the hover panel. The hover panel uses selected Home
colors and weight, except Login required is never bold. Resize returns only this list to Home when the full module fits,
retaining account focus. Account actions and open forms stay in place; Back resolves
to Home or the compact list according to current fit.

The account **picker** used by New, Resume and Switch keeps normal option names
and a separate right-aligned usage column. It shows `x% 5h / x% wk` remaining,
omitting the short period and slash unless a five-hour window is verified. Percent
signs and slashes share columns, using the minimum padding required by the largest
displayed number. Weekly-only rows align with the weekly column. Unknown values
use `?`; freshness and the shortened account name appear in the selected-account note,
hidden while Back is focused.

Account action groups use dot/blank separators: Usage details and Limits; Log in
and Enable/Disable; Deregister/Restore/Delete. Limits edit minimum remaining percentages with
explicit Save/Cancel; the backend converts/stores existing used thresholds through
the sole owner, preserving plan review and leaving automation unchanged. Impactful
Disable/Remove actions require confirmation. Removal first presents affected
sessions/data and requires the exact account ID against a fresh preview revision;
history and credentials remain recoverable. Actual Delete is a separate public CLI
action: its confirmation opens without account-file scanning, accepts the displayed
friendly provider-name label and admits one durable request. Only after confirmation
does the CLI check activity, preserve saved/archived conversations and delete local
account files/credentials. Active accounts are blocked. The UI owns no deletion or
history logic; an older running account owner explicitly gates this capability.
Neither action deletes the upstream provider account.

Manage Sessions lists running, saved and unavailable sessions. Selecting opens
actions, never attaches. Details, Rename, Switch, Restart, Stop and Resume are
capability-gated; Stop/Restart/Switch require confirmation. Switch uses the shared
account picker. Focus tab explicitly verifies the current generation, HERDR target
and foreground PID, then focuses the existing tab and exits. No messaging/steering
controls belong here. Session `[account]` suffixes use the shared animated accent,
permanently dimmer and never bold, without a triangle selection marker or
asterisk; `(inactive)` is an unbolded muted-gray hover detail. Timers are unbolded
deselected gray. Busy sessions use the reference ten-frame spinner from
`cli-main-menu/select.bash` at 100 ms per frame, persistent regardless of selection,
one space after the account, followed by one space before an inline timer. Only
positively busy running snapshots animate. One-second single-flight read-only
refreshes remove it on idle/exit/error; no message or lifecycle action is sent.

Resume uses the shared summary with editable account, Space and name, read-only
saved directory. Prefer the original Space only when its host/socket and ID still
match; otherwise prefer current focus, else unset. Stable session/conversation IDs
and history remain backend-owned. Changing account uses its existing safe transfer
worker. Canceling Resume restores the independent New draft.

Installed HERDR 0.9.5 supports one popup at a time, so confirmations/forms/progress
use internal nested dialogs over the preserved page. Theme colors are read from
HERDR's fixed theme/custom RGB overrides (Ana uses Vesper); dynamic/terminal themes
have an explicit configured-base fallback. Text wraps at spaces/dashes, falling back to grapheme-safe splitting only for
overlong tokens. Scroll hints appear only for overflow. Cancel is always immediate,
including before the final page; late preview responses cannot reopen a dismissed
dialog. Single-field forms keep the input editable while only buttons take selection.
Validation colors come from actual validity, independently of focus. Continue in background returns
to the menu, never cancels or steals focus later. Operations can reopen original
New/Resume receipts, account deletions and transfer jobs after closing the popup.
Hover-only previews, including paths, session timers/markers and parenthesized
operation/request IDs, share one list-wide fit decision, including offscreen rows.
If any cannot fit inline, show only the selected detail centered below the list
or range counter, one blank line above and below. Back/Close focus hides it
without changing layout. Details stay gray and never bold; overly long detached
text is ellipsized to one line, and insufficient height hides it. Operation IDs
have exactly one space after the status when inline. Status columns align left. Range counters are centered and share the bottom controls’ animated soft accent
at brightness 550 without a bold flash. Dialog hints stay readable gray.
Operations combine New/Resume receipts and jobs by immutable creation date, newest
first. Unknown legacy creation dates come last with stable kind/ID ties; progress
updates never reorder them. The launch receipt phase `running` means successful
creation: display `complete`, independently of the agent lifetime. Complete items
show Details and dialogs show Close (without a period), updated by the one-second
status poll.

A cold Sessions menu contains only [new session], the dot separator and Operations;
Loading… is a centered non-option below it. Options consume available height down
to one blank line before controls; reserve counter/detail rows only when needed.
Held arrows stop at boundaries. Kitty repeat/release reporting rearms wrapping on
a fresh press. Legacy arrow bytes lack releases, so a 750 ms quiet gap separates
gestures. Only unavailable labels are struck; bullets stay intact and unavailable
labels never become bold.

Uncertain outcomes are inspected, never replayed. Cancellation/recovery are separate backend actions.

Account deregistration previews open immediately while data loads. The confirmation
shows the friendly provider-name label (for example `codex-personal`), which the UI
maps to the backend ID only after an exact match. Signed-in accounts omit Login; it
appears only when authentication is required. Session action ordering/groups follow
Codex Switcher, with Focus Session, Session Details and Rename Session first.
Unsupported public capabilities remain disabled. Session account columns appear
for the complete list together; selected live sessions show the reference elapsed
time format when the backend reports verified `active_since`.

Home usage focus moves Right from the selected account name only when public
account-limit support and a reported allowance are available. Enter/Space opens
the CLI-backed Limits form without changing the underlying page or account.
Cancel/save/close restore that same bar focus; Left returns to the name. Only bar focus selects percentage text. The focused bar retains normal color and
bolds its glyphs; other bars retain their saturation and scale brightness to 65%. It adds no arrow or layout shift.
