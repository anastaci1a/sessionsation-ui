# Usage

Set `SESSIONSATION_BIN` to the verified absolute backend executable and
`SESSIONSATION_DATA_DIR` to the operator-selected root. Existing Ana accounts use
`/mnt/user/appdata/codex-switcher` with its existing credential owner. The independent
backend default is not an automatic import. See the backend ownership guidance.

The Rust popup creates sessions and presents account/session management through
public backend commands. Its [approved design](cli-specification/architecture.md)
describes the interaction and ownership boundaries. See the deployment record for
what is installed.

## Account and session menus

Home orders New Session, Manage Sessions, Add Account/Manage Accounts, a dot
separator and Settings. If every account fits above the bottom controls, accounts
appear inline without selector circles and Add Account is available. The account/bar
block is centered, caps bar width at 24 cells and shrinks to fit narrower popups.
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
Status-only accounts also accept Right. Their status and focused `◀` use the normal
bullet hue, without bold. While a usage/status item is focused on the right, other
text statuses use the darker bullet hue, like the other dimmed usage bars. Enter/Space opens Enable for Disabled, Login for Login
required, Restore for Deregistered, or usage details otherwise. These dialogs retain
the account and right-side focus when closed; mutations require confirmation.

Bars show remaining allowance for the periods the backend reports: 5h/weekly or
weekly only. Capped accent lines deplete into visible gray; custom limits appear
inside the bar. Percentages precede period labels in aligned columns. Missing
observations show Disabled, Login required or Usage unavailable as appropriate;
stale observations are marked. The account picker uses
a separate right-aligned `x% 5h / x% wk` column, omitting the short period and
slash for weekly-only accounts. Account names accept at most 24 ASCII characters;
typing and paste stop at the limit.
Duplicate names immediately invalidate the editor and disable confirmation.
Session creation/resume/rename accepts at most 24 Unicode scalar values, also
enforced by the CLI.
Only the focused input shows a cursor in multi-input dialogs; single-input dialogs
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

Session Details opens from cached state immediately and refreshes on the one-second
read-only state poll, retaining the last details during discovery failures. It shows
the native creation timestamp and saved rollout size when reported; older or
missing data shows Unknown. Shared account databases are excluded from that size. It does
not use a stale generation for reads or weaken generation checks on mutations.
Renaming to the existing name remains valid; other sessions’ names remain excluded.

Manage Sessions selects an action menu, never implicitly attaches. Lifecycle
actions carry the captured backend ID/generation. Stop, Restart, Switch, Disable
and account deregistration/deletion require confirmation. New Session labels and values use aligned columns with space padding and the
short label Dir. Internal modal dialogs preserve their underlying
page; Continue in background dismisses progress without cancelling backend work.
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
Resume has editable account, Space and name, plus the saved read-only directory.
It prefers a verified original Space, then current focus, otherwise unset. Changing
account uses the backend safe transfer, preserving conversation history and ID.
Uncertain responses are not automatically retried. Focus tab selects an existing
HERDR tab in this instance after rechecking its catalog identity.

Manage Sessions sorts busy, idle, then inactive sessions, newest access first
within each group. Refreshes retain selection by stable ID. It keeps busy activity visible with the reference ten-frame Braille
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
(55% brightness, regular weight). Dialog hints stay readable gray. Held arrows wrap only if that hold began at the corresponding edge (Up at the
top, Down at the bottom). Holds begun elsewhere stop at the edge; release and
press again there to wrap. Home, management menus, directory pickers and dialog
buttons share this selection-navigation implementation. Terminals
without repeat/release events use a 750 ms quiet gap to distinguish gestures for
wrapping. Mixed ordinary and enhanced key events never turn repeated presses
into new holds; only the held arrow's release clears it. The first press always moves immediately; repeat timing comes from the
terminal, with no UI hold-delay timer. Home/End jump to the first/last list option
(including directory lists); text editors retain cursor Home/End. These shortcuts
do not add footer hints.
Centering with Back/Close rounds
right; six spare columns use four on the left to keep the arrow one cell from the
edge. Options scroll only when they cannot fit with one blank line before controls;
range/detail rows reserve space only when required. Unavailable options accept
hover/keyboard focus but cannot be activated; separators are skipped. They strike
only their text, never the bullet, and their text never becomes bold.

Dialogs hide their sole Esc/Close hint and shrink to their content; action buttons remain.
scrolling dialogs retain scroll hints. Dialog items have at most one blank line
between them. Stars never appear within text or its spaces. Fades and the vertical
gradient blend from the terminal’s OSC 11 background reply without changing that
background; unanswered queries use a dark fallback.

Dialogs use read-only HERDR theme configuration and bundled attributed palette
tokens. Configured built-ins and RGB overrides are supported; dynamic/terminal
themes report an approximation because HERDR exposes no resolved-palette API.

## Popup setup

Build/copy the Rust executable into `.runtime/sessionsation-ui` as shown in the
[README](../README.md), or point that path to a verified immutable artifact.
Register the repository with `herdr plugin link /absolute/path/to/sessionsation-ui --enabled`.
Then configure an explicit, separate popup client file and your chosen binding:

```sh
node scripts/configure.mjs popup --config /absolute/herdr/config.toml --client-config /absolute/path/to/sessionsation-ui/.runtime/config.json --binary /absolute/pinned/sessionsation --data-dir /explicit/backend/root --backup-dir /private/backups --key prefix+shift+l
herdr --session YOUR-SERVER server reload-config
```

The installer preserves the prefix and unrelated bindings, refuses a conflicting
key, and prints a private rollback receipt. `--shell /bin/bash` optionally restores
plain future tabs when replacing the previous default-menu setup; it does not
restart terminals. The wrapper reads only its runtime artifact/client paths; all
rendering, interaction and backend communication is Rust.

Preferences live in `preferences.json` beside the resolved client configuration.
A symlink from `.runtime/config.json` therefore keeps writable settings beside the
real config, rather than attempting to replace a symlink or writing into the checkout.

Open **New Session**, edit Account, Space, Name and Directory in any order, then
select Start Session directly below the fields and separator. The default Space is
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

Rollback uses `node scripts/configure.mjs rollback /private/backups/launcher-RECEIPT.json`,
then reload the intended HERDR server. It restores both configuration files exactly
and refuses conflicting later edits. Versioned binaries can remain for recovery.

## Preserved legacy entrypoints

Run `./bin/sessionsation-ui launcher` in an ordinary terminal for new, resume or
plain shell. New reads the account list directly; it does not scan conversation
histories or terminal attachments just to show accounts. Resume still discovers
current sessions and the backend revalidates the selected generation at execution. Run `./bin/sessionsation-ui manage` for account and selected-session
management. Exiting management only closes the viewer. Stop is a separate explicit
backend operation. Capability hints determine available choices.

No existing shell startup files or restored agents are modified.

### Management plugin

```sh
herdr plugin link /absolute/path/to/sessionsation-ui --enabled
herdr --session YOUR-SERVER plugin pane open --plugin sessionsation --entrypoint manage --placement overlay
```

The overlay targets that server's active pane. Choose sessions by their unique
backend names; account switching uses the selected session's captured generation.
Returned operations are admissions, not completion. Inspect Operations for progress
or an explicit confirmation. Exiting the viewer never sends Stop. Usage displays
only backend-reported data. Account confirmations use the friendly label shown
above the always-editable input; only the buttons take navigation focus.

### Legacy future-terminal launcher

First verify the backend artifact and choose its root explicitly. Then:

```sh
node scripts/configure.mjs apply --config /absolute/herdr/config.toml --binary /absolute/sessionsation --data-dir /explicit/backend/root --backup-dir /private/config-backups
```

This prints the private rollback receipt path. It saves backend settings in
`/mnt/user/appdata/sessionsation-ui/config.json` on Unraid, otherwise
`~/.config/sessionsation-ui/config.json` (override with `--client-config`; clients
then need `SESSIONSATION_UI_CONFIG`). Environment variables override stored settings.
The configuration change sets HERDR's `terminal.default_shell` to this checkout's
retained-shell wrapper. Keep the checkout and verified backend executable available.
It changes neither user startup files nor running terminals. Reload configuration
only on the intended HERDR server:

```sh
herdr --session YOUR-SERVER server reload-config
node scripts/configure.mjs rollback /private/config-backups/launcher-RECEIPT.json
herdr --session YOUR-SERVER server reload-config
```

Rollback restores exact original bytes, modes and ownership, and refuses to
clobber edits made after apply. The receipt contains configuration backups and
must stay outside Git. A plain shell and backend-created panes set
`SESSIONSATION_LAUNCHER_BYPASS=1`; the wrapper also guards against recursive menus.
The retained Bash parent permits agent restart/resume after the foreground agent
exits. HERDR 0.9.5 omits `HERDR_SESSION` on its default server. The wrapper sets
`SESSIONSATION_HERDR_SESSION=default` only when the inherited Unix socket resolves
to the actual default socket under `${XDG_CONFIG_HOME:-$HOME/.config}/herdr`.
Named/explicit session context is preserved; missing or ambiguous context is
rejected before invoking the backend. The variable is exported before the retained
shell starts, so later relaunches inherit it as well. User `.bashrc` is sourced while retaining HERDR's requested working directory.

## Ana's configured host

Satellite's default HERDR server uses **prefix + uppercase L** to open the Rust
popup. The global projects root starts at `/mnt/user/live/projects`; changing it
in Configuration creates no directories. Choose an account on the first launch;
only a successful handoff records it for eligible future prefill. Future plain
HERDR tabs use Bash. Existing shells/agents are unchanged.

[deployment.json](deployment.json) is authoritative for both immutable artifact
paths/hashes, source commits, UI settings, the shared account-data root, binding
and private rollback receipt. The old numbered launcher/client settings, original
Switcher and Agent DM pin remain intact. The existing supervisor retains credential
ownership. This popup deployment is not an Agent DM adapter cutover.

To roll back, run the record's rollback command, then reload its intended HERDR
server. Exact original config/client bytes are restored; the numbered launcher
becomes the future-tab default again. Artifacts and UI preferences remain available
for recovery. Current artifact hashes and process-identity checks are recorded in the deployment
evidence. Other running HERDR servers are not automatically reloaded.

Account deregistration previews open immediately while data loads. The confirmation
shows the friendly provider-name label (for example `codex-personal`), which the UI
maps to the backend ID only after an exact match. Signed-in accounts omit Login; it
appears only when authentication is required. Session action ordering/groups follow
Codex Switcher, with Focus Session (Start Session for saved history), Session Details
and Rename Session first. Inactive sessions hide running-session/experimental
controls. Active tmux-only attachments show an unbolded gray `(tmux)` after Focus.
Unsupported public capabilities remain disabled. Session account columns appear
for the complete list together; selected live sessions show the reference elapsed
time format when the backend reports verified `active_since`.
