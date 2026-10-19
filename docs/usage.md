# Usage and configuration

## Install and configure

Build the binary and link the plugin as shown in the [README](../README.md).
For a provider-free preview, use the [isolated demo](verification.md#animated-demo).
Real account setup belongs to the [backend](https://github.com/anastaci1a/sessionsation/blob/main/docs/cli.md).
Obtain HERDR from its [upstream installation/release instructions](https://herdr.dev/docs/install/);
0.9.5/protocol 22 is the tested combination, not a guarantee for every newer binary.

The configuration installer needs Node.js 22.18+ and an existing HERDR config.
In the following example, replace every `/path/to/...` and `YOUR-SERVER` with
your own values. The backend binary and data-root paths must be absolute. Select
the same root as your existing sole supervisor; do not start another owner or copy
credentials. Use a private backup directory outside the checkout.

```sh
node scripts/configure.mjs popup \
  --config /path/to/herdr/config.toml \
  --client-config "$PWD/.runtime/config.json" \
  --binary /path/to/sessionsation \
  --data-dir /path/to/data-root \
  --backup-dir /path/to/private/backups \
  --key prefix+shift+l
herdr --session YOUR-SERVER server reload-config
./bin/sessionsation-popup open
```

The example binding is prefix + uppercase L; choose your own and retain your
HERDR prefix. The installer preserves unrelated bindings, rejects conflicts, and
prints a private rollback receipt. It changes only the selected files, not running
agents, shell startup files or other HERDR servers. Optional `--shell /bin/bash`
sets the shell of future tabs when replacing a previous menu wrapper.

The Rust wrapper uses `.runtime/sessionsation-ui` and `.runtime/config.json`;
override them with `SESSIONSATION_UI_BIN` and `SESSIONSATION_UI_CONFIG`.
Config contains `binary` and `dataDir`. `SESSIONSATION_BIN` and
`SESSIONSATION_DATA_DIR` override stored backend settings. Preferences are
`preferences.json` beside the resolved client-config target. If using a config
symlink, its writable sidecar stays beside the real target. No account credentials
are stored in UI preferences. Defaults used by direct invocation are
`$HOME/.config/sessionsation-ui/config.json`, or the product-specific
`/mnt/user/appdata/sessionsation-ui/config.json` when that Unraid directory exists;
the explicit popup config above avoids platform-dependent selection.

To roll back the installer change, use the receipt it printed, then reload only
the intended host:

```sh
node scripts/configure.mjs rollback /path/to/private/backups/launcher-RECEIPT.json
herdr --session YOUR-SERVER server reload-config
```

Rollback restores original config bytes/modes/ownership and refuses conflicting
later edits. Keep receipts and binaries outside public Git; a client update is
not a supervisor upgrade or data-root migration.

## Capabilities and limits

- The popup uses public backend API v1 only: accounts/usage/limits, new/resume,
  session details/rename, supported switch/restart/stop, HERDR Space moves, archive/restore/delete,
  and operation progress. Final checks and destructive confirmations belong to CLI.
- Codex is implemented; Claude enrollment, usage, messaging and lifecycle are not.
  Cross-provider conversion is outside this version.
- Active folder relocation, working-directory changes and experimental automation
  are inherited backend capabilities without this popup's public-API integration.
  Their unavailable menu entries do not imply working actions. Saved-session
  Resume can select another directory when the backend advertises that capability.
- Idle Pause is hidden; busy Pause remains unavailable pending public integration.
  Provider-login cancellation is also unfinished: closing the login viewer does
  not cancel a provider-owned login.
- HERDR's outer popup border cannot follow the animated status bar in 0.9.5.
  Built-in fixed themes/RGB overrides are supported; dynamic/terminal themes use
  an explicit approximation because no resolved runtime palette API is available.
- Clearing a viewer or moving focus never stops an agent. Closing a host server
  is different. Existing tmux sessions are retained; they are not automatically
  converted to HERDR tabs. Cross-instance discovery requires compatible backend
  readers; keep all consumers of one root on reviewed versions.

## Move a running HERDR session

In Manage Sessions, open the session, choose **Move to Space**, then an existing
Space on that session's host and confirm. The current Space stays hoverable but
cannot be submitted. Choices come from `api spaces` on the bound host, not the
popup's own Space list. Loading/cancel remain responsive; late cancelled responses
are discarded. The selected session's captured generation is checked by CLI at
submission, rather than silently refreshed underneath the confirmation.

The CLI moves only that pane into a new normal tab. Account, provider process,
conversation, directory and permissions stay intact, including during active work.
Split siblings remain; an empty source tab/Space can close. No move between HERDR
instances, no tmux conversion, and no inactive-session move is implied. Choose the
destination in New/Resume for an inactive conversation instead.

Progress closes back to Manage Sessions on the same stable session. An uncertain
move is never retried automatically: Operations → Recover asks CLI to reconcile
placement without another host move. Until verified, the CLI reserves that session
against competing lifecycle actions. The
[backend contract](https://github.com/anastaci1a/sessionsation/blob/main/docs/contract.md#herdr-space-placement)
owns recovery and safety semantics. Older backends without the per-session
`move_space` capability leave this action unavailable.

## Account and session menus

Home orders New Session, Manage Sessions, Add Account/Manage Accounts, a dot
separator and Settings. If every account fits above the bottom controls, accounts
appear inline without selector circles and Add Account is available. The account/bar
block is centered, caps bar width at 24 cells and shrinks to fit narrower popups.
While inventory loads, Home starts with Add Account; Manage Accounts appears only
when the received account rows exceed the available space.
Otherwise Manage Accounts provides a scrolling list with `[add account]` first,
normal circle bullets and white/gray account-name text.
Enabled/Disabled values share a left-aligned column. Enabled uses the selected or
dimmed accent; Enabled* needs attention. Disabled is muted gray and has no hover
usage panel.
Its highlighted enabled account's usage uses white, unbolded percentage/period text
and starts one blank line below the list, or below
the visible-range counter when scrolling.
Growing the screen returns that list to Home; forms remain open and Back chooses
the appropriate current layout without losing their account selection.
Back/Esc returns to the actual previous screen and restores the option that opened
the submenu, including its scroll position. Refreshed lists retain that focus by
stable identity. Close on Home detaches the viewer without stopping agents.

On Home, the name selector uses `▶`. While a name is selected, other account names
use the darker bullet hue. Focus on any account name or usage/status also dims
the top-option bullets, just like Close focus. Selecting a top option or Close dims all account names,
bars and text statuses; normal deselected gray-white labels/percentages retain
their brightness and are unbolded. Moving focus away restores the appropriate hue. Right from an account with available usage selects its bars; Enter/Space
opens Limits directly. Left returns to the name. Cancel, close and save retain the
same account and bar focus. Only usage focus selects its percentages. The focused bar keeps its normal colors
and bolds its line characters; other bars darken without adding
another selector. A white `◀`, separated by one space, follows the first usage
value when it has focus.
Bar glyphs are always bold, including their gray remainder and threshold marker;
focus still changes their colors without changing label or percentage styling.
Status-only accounts also accept Right. Their status and focused `◀` use the normal
bullet hue; the focused status text is bold, and the triangle stays regular weight. While a usage/status item is focused on the right, other
text statuses use the darker bullet hue, like the other dimmed usage bars. Enter/Space opens Enable for Disabled, Login for Login
required, Restore for Deregistered, or usage details otherwise. These dialogs retain
the account and right-side focus when closed; mutations require confirmation.

Bars show remaining allowance for the periods the backend reports: 5h/weekly or
weekly only. Bold lines with half-cell endcaps deplete into visible gray; custom limits appear
inside the bar. Thresholds at the extreme ends use `├`/`┝` or `┤`/`┥`,
matching the adjacent stroke weight. Percentages span the actual center-to-center
line length (one cell less than the rendered width). Percentages precede period labels in aligned columns. Missing
observations show Disabled, Login required or Usage unavailable as appropriate;
stale observations are marked. New/resume account pickers use standard bullets
with white/gray names and an aligned `x% 5h / x% wk` block one space after the longest name, omitting the short period and
slash for weekly-only accounts. The selected account’s status appears one blank line below the list or range counter, without repeating its name. Ready uses the animated selected-bullet hue, unbolded; other statuses remain unbolded gray. Account names accept at most 30 ASCII characters;
typing and paste stop at the limit.
Duplicate names immediately invalidate the editor and disable confirmation.
Session creation/resume/rename accepts at most 30 Unicode scalar values, also
enforced by the CLI.
Inputs use the terminal's native cursor without overriding its shape, color or
blink settings. Only the focused editor shows it; a single-field dialog keeps it
editable while its buttons are selected. Menus hide it, and closing the popup
restores normal terminal cursor visibility.
Dialogs cover the animated star field without changing its generation or lifetimes;
closing a dialog reveals the continuing animation. Valid confirmation does not
require scrolling explanatory text to the bottom.
Menus initially focus the first non-separator option, even if unavailable.
Host commands use the popup’s explicit socket rather than a CLI session selector that could resolve in another configuration directory.
An attachment whose host cannot be confirmed shows `(host unavailable)` rather than being labelled inactive; its lifecycle actions remain gated until positive observation returns.
Session action hints such as `(tmux)` or `(missing directory)` appear only on
hover and hide on Back; resume reasons come from the public CLI state.
The inactive asterisk remains on its session row when the parenthetical moves
to the centered preview below the list. Unavailable field segments are struck
through separately; alignment padding is never struck through.
Resume forms retain their chosen destination through stale or pending checks.
Only confirmed invalid choices disable Resume; selected-only parentheticals explain
validation state. Activation verifies again before admission. A generation refresh
must still identify the same inactive conversation, source account, name and directory.
Definitively failed launch receipts release the form; uncertain requests remain
pending for reconciliation and are never automatically replayed. Temporary receipt
freshness errors show “Checking session state…”; confirmed failures remain explicit.
An unfinished conversation offers Start Session with an explicit warning first.
Confirming acknowledges only unfinished history, not active writers or conflicting copies.
Input focus is shown by the cursor alone, with no triangle. Only the focused input
shows a cursor in multi-input dialogs; single-input dialogs
stay editable while navigating buttons. Menu account names are ellipsized on cell 12, session titles on cell 16; full values remain
editable and exact confirmation IDs remain intact. See the
[rendering specification](cli-specification/architecture.md#account-and-session-management)
for the precise glyph, alignment and motion rules.

Account actions group Usage details/Limits, Login/Enable/Disable, and
Deregister/Restore/Delete with dot separators. Limits edit minimum remaining
percentages through the existing owner. Deregister preserves credentials/history;
Restore reverses deregistration. Delete removes local account files and credentials
while preserving saved/archived conversations. The upstream provider account is
unchanged. Both confirmations require the displayed friendly provider-name label.

Delete opens an intent preview without scanning account files. The CLI checks
activity, preserves history and deletes data only after confirmation. Active
sessions block deletion. Progress offers Continue in background, which detaches
the dialog without cancelling or resending the operation. The CLI owns every
preservation/deletion step. An older account owner gates deletion until explicitly
updated; changing the popup's client binary alone does not replace that owner.

Deletion reservations replace ordinary account actions with the original operation;
failed cleanup offers Recover only when the backend reports it safe to request.
Operation menus respect explicit cancellation/recovery capabilities, rather than
assuming every failed operation supports the same recovery command.

Manage Sessions selects the first session when its initial inventory arrives; later
refreshes preserve selection. It lists sessions, then a dot separator and Operations. Archived
Sessions appears above Operations only when archives exist. With neither saved/live
sessions nor archives, the list contains only Operations, without a separator.
New Session remains on the home menu. Archive, Restore and permanent Delete call
the public CLI with stable identity, generation and request ID after confirmation.
Delete requires typing the displayed session name. Archive storage and restoration
belong to the backend; restoration does not start an agent or send a message.

Session Details opens from cached state immediately and refreshes on the one-second
read-only state poll, retaining the last details during discovery failures. It shows
the native creation timestamp and saved rollout size when reported; older or
missing data shows Unknown. Shared account databases are excluded from that size. It does
not use a stale generation for reads or weaken generation checks on mutations.
Renaming to the existing name remains valid; other sessions’ names remain excluded.

Individual account and session menus show `Account: <name>` and `Session: <name>`
in their title bar, retaining the full name up to the terminal width; list labels remain shortened.
Archive stays above Delete and explains its disabled state on hover. Pause work is hidden for idle sessions; its busy-session entry remains unavailable pending public CLI integration. Delete account uses a scan-free CLI preview to distinguish active sessions from an owner update requirement, with the reason shown only on hover.
Manage Sessions selects an action menu, never implicitly attaches. Lifecycle
actions carry the captured backend ID/generation. Stop, Restart, Switch, Disable
and account deregistration/deletion require confirmation. New Session labels and values use aligned columns with space padding and the
short label Dir. Internal modal dialogs preserve their underlying
page; Continue in background dismisses progress without cancelling backend work.
Session mutations return to Manage Sessions with the affected session selected; Archive/Delete return to the top. Views opened through Operations retain their submenu return. Switching confirms the latest polled session snapshot, then binds its generation through submission. Changed targets require a fresh choice, never an automatic retry. Operation lists show outcome/confirmation/running states; details show readable phase descriptions rather than wire-format identifiers.
Raw backend freshness/receipt errors are described in plain language in dialogs and bottom status lines; missing receipts remain unconfirmed.
Operations shows progress, fresh confirmation prompts and supported recovery.
Hover details are selected-only and disappear on Back/Close. Fit is decided for
the whole list: if any detail cannot fit inline, the selected detail moves below
the options or pagination, centered, unbolded, with one blank line above and below.
Long detached text is ellipsized to one line. Paths in Settings and Space roots use
this same rule. Only the selected operation shows its gray ID. If any does not fit beside the row,
it is centered below the list (below the visible-range counter while scrolling),
with one blank line above and at least one before the bottom controls. The list
and counter move upward to reserve that space; if width/height cannot fit it,
the ID stays hidden.
It also lists original New/Resume receipts after reopening, without resending them.
Operations are newest-first by creation time; unknown legacy creation dates sort
last. Status updates retain the selected ID. New/Resume launches display complete
once creation is verified, independently of the agent continuing to run. Completed
submenus offer Details, and completed dialogs offer Close (no period), updating on
the one-second status poll. Closing Progress/Details opened from a dedicated
operation submenu returns to that submenu and retains its selection; subsequent
status polls do not reopen the dialog.
Resume has editable account, Space and name. Its directory picker is enabled when the backend advertises `resume.directory_selection`; it initially preserves the saved directory and sends the displayed choice explicitly, preventing a later default-location fallback. Existing/new directory choices use the same staged picker as creation and create nothing until Start is confirmed. The CLI performs final path/history checks and records the selected location after verified handoff; project files are not moved. Older backends retain the read-only saved directory. If checking a replacement directory reveals unfinished history, Start asks for separate confirmation while preserving the chosen account, Space, name and directory. Cancelling that warning starts nothing.
It prefers a verified original Space, then current focus, otherwise unset. Changing
account uses the backend safe transfer, preserving conversation history and ID.
Uncertain responses are not automatically retried. Focus tab selects an existing
HERDR tab in this instance after rechecking its catalog identity.

Manage Sessions sorts busy, idle, then inactive sessions, newest access first
within each group. A blank-line bullet separator divides active and inactive
sessions only when both groups exist. Refreshes retain selection by stable ID. It keeps busy activity visible with the reference ten-frame Braille
spinner, advancing every 100 ms after one space following the account. The timer
follows after another space when it fits inline, otherwise uses the shared hover
panel. Read-only state refreshes once per second, one request at a time; idle, exit,
unknown activity or discovery failure removes the spinner. Active session account tags use the normal bullet hue on row hover and the dimmer
hue otherwise. Inactive tags stay dim and show `[account] *`, with an
asterisk separated by one space. The asterisk uses the same muted gray as
`(inactive)` and disappears while that parenthetical is visible inline or below the
list. Account tags are bold on row selection, including inactive sessions; they return
to normal weight on deselection or Back focus. Asterisks stay unbolded. Inactive hover markers stay muted and unbolded. Timers use normal deselected gray.

Errors and status notices use a centered bottom line, separated from controls by
one blank line. Menus reserve the status lane before calculating scroll capacity.
A failed preview/mutation closes its waiting dialog and shows the error there,
without an automatic retry.

Menu bottom controls and pagination share the reference soft accent and sweep
(55% brightness, regular weight). Dialog hints stay readable gray. Held arrows wrap once per hold, only if that hold began at the corresponding edge (Up at the
top, Down at the bottom). Holds begun elsewhere stop at the edge; release and
press again there to wrap. Home, management menus, directory pickers and dialog
buttons share this selection-navigation implementation. Terminals
without repeat/release events use a 150 ms quiet gap to estimate gesture boundaries.
This fallback cannot distinguish a physical release from an equally long pause
in repeated bytes (including an initial repeat delay or transport stall). Native
repeat/release events provide exact hold boundaries. Unrelated releases never
rearm a held arrow. The same navigation core handles either axis; existing
Left/Right menu actions are unchanged. The first press always moves immediately; repeat timing comes from the
terminal, with no UI hold-delay timer. Home/End jump to the first/last list option
(including directory lists); text editors retain cursor Home/End. These shortcuts
do not add footer hints.
Centering with Back/Close rounds
right; six spare columns use four on the left to keep the arrow one cell from the
edge. Options scroll only when they cannot fit with one blank line before controls;
range/detail rows reserve space only when required. Unavailable options accept
hover/keyboard focus but cannot be activated; separators are skipped. They strike
only their text, never the bullet, and their text never becomes bold.

Dialogs use HERDR's `┌Title──…┐` border style without title padding. They hide their
sole Esc/Close hint and shrink to their content; action buttons remain.
Scrolling dialogs retain scroll hints. Dialog items have at most one blank line
between them. Stars never appear within text or its spaces. Fades and the vertical
gradient blend from the terminal’s OSC 11 background reply without changing that
background. Star colors never fall below that background in any RGB channel;
unanswered queries use a dark fallback.

Dialogs use read-only HERDR theme configuration and bundled attributed palette
tokens. Configured built-ins and RGB overrides are supported; dynamic/terminal
themes report an approximation because HERDR exposes no resolved-palette API.

## New session and directory flow

The Space chooser starts with `[create new space]`.
Only the highlighted Space's `(id)` appears, one space after its name, unbolded
gray; Back focus hides it. If any ID
cannot fit inline, the whole list uses the standard centered preview below the
options or range counter, separated by one blank line.
Global-root and Space-root paths follow the same preview rules, remain unbolded
gray, and appear as bare paths one space after the option text when inline.

Open **New Session**, edit Account, Space, Name and Directory in any order, then
select Start Session directly below the fields and separator. The top bar shows the
configured global or selected-Space project root, depending on the default-directory
mode. Unset values throughout the UI use `<unset>`, including the top bar,
session fields and root previews.

The public CLI session catalog refreshes asynchronously while creating a session.
When another active or inactive session has the same directory, Start Session shows
`⚠` one space after its label. Selecting it reveals `(existing session in dir)`
after the warning. Activation asks for confirmation (Cancel selected initially);
Cancel preserves the form, and confirmation follows the normal CLI validation and
launch flow. The UI never creates directories or starts an agent before confirmation.

The default Space is
the one currently focused in HERDR; no focus leaves it unset. Pickers preserve the
draft on Back. Existing automatic paths require explicit selection. Directory Enter
selects; Right browses child rows only, leaving `[use this dir]` and `[create dir]` unchanged; Left goes
up or focuses Back. Tab and Ctrl-U have no directory-picker action. Typing and paste edit the path. Create stages
intent and never calls mkdir. Configuration uses the same picker and saves on Back.
Missing per-Space roots remain unset, without a global fallback. Last-account prefill
initially requires fresh usage within existing limits. Once selected, the account remains
visible during refreshes and stale usage; only confirmed unavailability clears it.
Launch validation remains authoritative. Manually selecting a backend-permitted
account does not rewrite its limits.

The Back arrow is beside the top visible option. Left selects and expands it only
when the expanded label retains a left padding cell;
Right returns to the option. On the main page its expanded label is **Close**.
Settings’ **Default Dir:** chooses global or per-Space defaults. In either default-directory
picker, `[leave unset]` clears that default and its pending creation plan. Left from
any of its top three actions focuses Back; Right does nothing on those actions.
The session-directory picker retains its two top actions. Configuration uses short
labels; path previews follow the shared hover-only, list-wide fit rule.

HERDR owns the outer popup border. Installed 0.9.5 draws it using the shared theme
accent and exposes no per-popup color override, so it cannot track the launcher's
animated status bar. This is verified in its
[popup renderer](https://github.com/herdrdev/herdr/blob/v0.9.5/src/client/shell/composition.rs#L571)
and [plugin-pane API](https://herdr.dev/docs/socket-api/). No host theme changes are required.

Start records one request UUID. A timeout is reconciled using that ID, never
replayed. The popup exits after the backend confirms the exact process and HERDR
focuses its new normal tab. Esc while starting detaches the viewer; inspect the
printed UUID with `sessionsation api launch status --request-id UUID`. It does not
stop or cancel an admitted launch. No credentials are stored in UI preferences.

## Compatibility entrypoints

The older Node viewer and numbered launcher remain available:

```sh
./bin/sessionsation-ui manage
./bin/sessionsation-ui launcher
```

They are separate from the Rust popup. The numbered launcher supports new/resume
or plain shell; the viewer delegates management to public CLI v1. Closing either
viewer is not Stop. New uses account discovery directly; Resume discovers saved
sessions and revalidates the selected generation at execution.

The optional legacy installer uses `node scripts/configure.mjs apply` with the
same absolute `--config`, `--binary`, `--data-dir`, `--client-config` and
`--backup-dir` arguments documented above. It changes the default shell of future
tabs to the retained-shell wrapper, not existing terminals. Use `popup` for the
recommended popup-only setup. Keep the checkout and binary paths available.

The Bash parent retains the host prompt after an agent exits. A recursion bypass
prevents menus in backend-created panes. Named/explicit HERDR context is retained;
missing or ambiguous context is rejected. Default-session inference is limited to
a socket positively matching that host's default path. The wrapper sources the
user's .bashrc while preserving the requested working directory. These details
support compatibility and do not authorize migration of live agents.
