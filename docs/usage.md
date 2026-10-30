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

## Editing fields

Name, account, Space and directory choices open flat themed popups over the current
configuration screen. Cancel/Esc restores the opening option and picker position;
only the current editor owns the terminal cursor. Pressing any configured HERDR
prefix closes the popup directly from any screen or dialog, without stopping an
agent or cancelling a backend operation. Prefixes are read from the same HERDR
configuration as the theme when opening the popup; absent/invalid bindings use
HERDR’s default Ctrl-B. Nested directory/Space creation
remains staged until Start. Choice popups measure their labels and currently known
details on opening. Account and Space pickers use the width of their opening
sentence, with exactly one space after its period before the border. Every account
keeps its status beside its name/usage, regardless of selection; constrained rows
give status priority over optional usage. Space IDs use the shared below-list
preview if they cannot fit beside the name. Other dialogs retain their cached
opening dimensions; a newly hovered longer detail may grow them within the viewport.
Long directory paths scroll inside the editor with a leading `…` for hidden text.
The directory description has two lines: “Choose a directory.” and the explanation
of deferred creation. That second line sets its normal maximum width; narrow
terminals shrink it. Height remains 90% of the main popup, rounded for equal
top/bottom margins and bounded by the terminal.
Popup frames supply exactly one outer padding cell; previews do not add another.
Fixed-choice heights fit their description, choices and Cancel button exactly.
Forms share the boxed account-name editor; all popup choices are left aligned,
with descriptive text followed by one blank line before fields or choices.
Information-only dialogs, including a dialog whose only action is Close, hide
that action and all bottom controls; Enter/Esc dismisses them. A dialog that shows a
URL (for example the Codex verification URL) adds "View link fullscreen" before its
closing option, and keeps it visible even when that option is a lone Close. It shows
only the link on the whole terminal in the popup colours, with no frame or side
padding: the link wraps hard from the first to the last column so terminals without
wrapped-link detection can select it as one block, and rewraps on resize. A link
the dialog replaces is followed; one it withdraws stays, marked out of date. Back,
Esc or Left returns to the same dialog with its cursor on that option; that Esc
never counts toward Esc-again-to-close. A popup opened over
another fades every popup beneath it 30% toward the terminal background; only the
top popup keeps its colours. Progress and loading dialogs
reserve a fixed body viewport and scroll updates within it. Long lists scroll
and selected previews can move below them. Action groups use sentence case and a
blank bullet spacer. Choice pickers end with Cancel after exactly one bullet
spacer; it follows the last choice directly rather than the bottom of the frame.
Choice lists scroll together. The directory picker alone mounts Cancel and its
bullet spacer at the list’s bottom; all actions above it scroll away. Directory actions
use the mounted `Use this folder` row and typed-name entries; Space creation uses
`[create new space]`. Space creation is the first row,
with Space choices immediately after it. Account selection opens on
the current selection (including a prefilled default), or the first account when
there is no matching selection; refreshed lists retain the hovered account by ID.

## Capabilities and limits

- The popup uses public backend API v1 only: accounts/usage/limits, new/resume,
  session details/rename, supported switch/restart/stop, HERDR Space moves, archive/restore/delete,
  and operation progress. Final checks and destructive confirmations belong to CLI.
- Claude Code, Gemini CLI and Antigravity CLI sessions offer what each session's backend capabilities
  allow: focus, rename, start, and (contract revision 17) archive, restore and delete of
  stopped conversations. Rows they cannot use stay greyed with the session's backend
  reason (`exit the agent first`, `hosted worker running`) or `not supported for
  Claude Code`/`Gemini CLI`/`Antigravity CLI` when the provider has no such control
  (Antigravity has no archive, restore, delete, stop, restart, account switch, Space move or
  Add to HERDR; new, resume and rename are supported).
  Cross-provider conversion is outside this version.
- Active Codex sessions support Change working directory and Move project folder
  when the backend advertises those capabilities. Enter a destination, review the
  backend plan, then confirm. Directory changes leave files in place; folder moves
  never merge or overwrite destinations. Both wait for safe idle and restart the
  selected session with its existing identity/history. Background-work approval
  and recovery use the normal operation dialog. Saved-session Resume can choose
  another directory. Experimental automation remains unavailable.
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

### Providers (Codex and Claude Code)

Add Account first asks for a provider when the backend reports more than one
implemented provider (`capabilities.providers`). Codex shows the verification
URL and code. Claude Code returns an interactive login plan; the UI shows the
exact command to run in a terminal and never runs it or handles tokens. Account
names are unique per provider (the same name may exist under several providers;
UI state is keyed by the provider-qualified ID). Every account is preceded by a
provider symbol: Codex ⏣, Claude Code ✻, Gemini CLI ✦, Antigravity CLI ◬ (U+25EC, one cell wide; mapping in
`usage::provider_symbol`). Resume and Switch account lock the
account step to the session's provider. Usage details name the source, semantics,
age and reset time; a missing or unusable window is shown as unavailable or unknown,
never as 0%.

A Claude Code session offers only Focus Session (verified HERDR attachment),
Session Details, Rename Session (Sessionsation name; the native title follows at
the next launch or resume) and, when stopped, Start Session. Everything else is
omitted. Stale or unknown sessions explain why they cannot be started.

Home orders New session, Manage sessions, Add account, a dot separator and
Settings, followed by every account in backend order. There is no separate accounts
page: the list scrolls when it does not fit and bars shrink (down to 6 cells) on
narrow screens. A `…` row at the name column, with one blank row above and below
(at least one below at the bottom), marks two or more accounts hidden above or below;
a single hidden account is shown instead. It takes the names’ hue at its position at
the level of a name without the cursor, but stays at the bright level during usage
focus, and is never selectable. From five visible rows (`…` rows included), the list
scrolls one account early so an account always separates the cursor from a `…` row
ahead of it; shorter lists may put the cursor next to one. Lists too short for a `…`
row on each side of an account (such as one row) scroll plainly without them.
Wrapping past either end jumps the list to that end at once. Columns are fixed per list: selection lane, provider symbol, names
padded to one width, then identical-length bars and a shared percentage column.
Only action-required account states (Login required, Disabled, Deregistered,
Deletion failed) replace the bars with a status; it is lowercase, in parentheses and starts one column right of the bar start (never centred;
longer text grows right). Enter opens the account's own action page. With the backend capability
`account_reorder`, `m` picks up the focused account (marked `⇅`, `(moving)`),
Up/Down move it, Enter saves with `account reorder <provider:name> --position <i>`
(zero-based) and Esc restores the previous order; the footer shows `m: Move` only
while an account is focused. While moving, the moving row's accent elements (cursor,
icon, name, bars, statuses) pulse between the selected and dim accent levels with the
grabbed picker arrow's period; every other account looks as it does with the cursor
on the menu options above. Saving is silent; a failed save restores the backend's
order and shows the error.
Entering a page from its parent always starts at the first selectable row; Back/Esc
restores the parent's previous selection and scroll. Refreshed lists retain focus by
stable identity. Close on Home detaches the viewer without stopping agents; Esc
on Home needs a second press: the first fades in a band over the controls ("Press
Esc again to close", 112 ms fade-in, held 1 s, 1 s fade-out), and a second Esc closes
until three quarters of the fade-out have passed (about 1.9 s after the first);
later it counts as a first press again. The timings are the `close_band` constants.
Footers follow the top layer: "Esc: Back" in menus, "Esc: Cancel" in
dialogs and pickers, "Enter/Esc: Close" in information dialogs.

On Home, the name selector uses the outline `▷` (same width as `▶`; the right-side `◀` stays filled). While a name is selected, other account names
use the darker bullet hue. Focus on any account name or usage/status also dims
the top-option bullets, just like Close focus. Selecting a top option or Close dims all account names,
bars and text statuses; normal deselected gray-white labels/percentages retain
their brightness and are unbolded. Moving focus away restores the appropriate hue. Right from an account with available usage selects its bars; Enter/Space
opens Limits directly. Left returns to the name. Up/Down remember the column: moving onto
single-column rows keeps the cursor there, and the next account row puts it back on
the usage; Left onto Close resets it to the names. Cancel, close and save retain the
same account and bar focus. Only usage focus selects its percentages. The focused bar keeps its normal colors
and bolds its line characters; other bars darken without adding
another selector. A white `◀`, separated by one space, follows the first usage
value when it has focus.
Bar glyphs are always bold, including their gray remainder and threshold marker;
focus still changes their colors without changing label or percentage styling.
Unavailable usage (unsupported, unknown or not yet reported) draws a stub bar
at the bar's position and width instead: a symmetric heavy core with light dashes
thinning to half-cell ends (`stub_bar`, approved shapes for odd widths 7–33, even
and wider widths derived by the same rules), in the bars' unfilled gray, dimming
whenever the bars dim, never hue-shifted or bold. With the cursor on that account's usage, each stub is
hidden and `(usage unavailable)` appears from one column right of the bar start;
moving away restores the stub.
Status-only accounts also accept Right. Parenthetical statuses (`(disabled)`,
`(usage unavailable)`, `(moving)`) use the usage bars' filled hue at every position, dim whenever the bars dim, and are
bold only while the cursor is on them; the focused `◀` stays regular weight. Enter/Space opens Enable for Disabled, Login for Login
required, Restore for Deregistered, or usage details otherwise. These dialogs retain
the account and right-side focus when closed; mutations require confirmation.

Bars show remaining allowance (`61% 5h`) for the periods the backend reports: 5h/weekly or
weekly only. Bold lines with half-cell endcaps deplete into visible gray; custom limits appear
inside the bar as `┿`, explained by a dim legend in account Details. A missing
window keeps its row as a stub bar with `?% 5h`. The observation age
(`(42m ago)`) hangs right of the centred block, once per account; if any cannot
fit inline, every account shows it under its name. It stays dark grey unless the
usage itself is focused. Account Details show both meanings (`61% left · 39% used`)
and the backend-published account home as Directory when present. Session details
share one layout: Account, Status, Directory, Last active and Created, then a dim
Identifiers group. In account, session and operation details a long value wraps with
a hanging indent: continuation lines start under the value, not at the left edge. Thresholds at the extreme ends use `├`/`┝` or `┤`/`┥`,
matching the adjacent stroke weight. Percentages span the actual center-to-center
line length (one cell less than the rendered width). Percentages precede period labels in aligned columns. Missing
observations show Disabled, Login required or Usage unavailable as appropriate;
stale observations are marked. New/resume account pickers use standard bullets
with white/gray names and an aligned `x% 5h · x% wk` block one space after the longest name, omitting the short period and
slash for weekly-only accounts. Every account’s status stays visible on the right of its row, independently of selection. Optional usage gives way when width is constrained. Ready uses the popup accent, unbolded; other statuses remain unbolded gray. Account names accept at most 30 ASCII characters;
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
Unavailable field segments are struck
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
stay editable while navigating buttons. Menu account names are shortened to cell 12, session titles on cell 16; full values remain
editable and exact confirmation IDs remain intact. See the
[rendering specification](cli-specification/architecture.md#account-and-session-management)
for the precise glyph, alignment and motion rules.

Account actions begin with Details/Limits/Rename for every account, then group
Login/Enable/Disable and Deregister/Restore/Delete with dot separators. Rename sets a
backend display name (empty clears it) and is enabled only when the provider reports
`account_rename`; display names replace short names in lists and session tags.
Claude accounts on a revision-16 backend add a `Live steering` selector (On/Off)
that reads `api account live-steering` and changes it with `--enable`/`--disable`
through the usual operation dialog; the value is always the backend's reply. An
operator `hold`/`refuse`, a symlinked `settings.json` or a failed read shows the row
unavailable with its reason.
On a revision-22 backend they also get a `Bypass warning` selector (Skip on top, Show below)
that reads `api account bypass-warning` and changes it with `--skip`/`--show`; it follows the
same picker rules, shows the backend's reply afterwards and explains itself with the grey
text "(skip Claude's YOLO-mode startup warning)".
On the account page, picker info and the reasons of unavailable rows are hover notes:
dark grey, in parentheses without a bullet, shown only for the hovered row. They share
one decision: beside their rows when every note fits there whole, otherwise all of them
appear below the list while their row is hovered; they are never shortened.
Deregister/Restore and Delete are enabled only when the provider reports
`account_removal`/`account_deletion`; a disabled row shows the backend reason when
published. Delete confirmations show backend semantics text when present (for Claude
without it: a managed home is removed with history preserved, an adopted external
directory is left in place). Add and Login for Claude run the backend login plan
(`claude auth login --claudeai`) automatically in a HERDR overlay pane
(`login` plugin pane), poll the account every 2 s for up to 10 minutes, and return to
the account page on success; the command is shown only if the pane cannot open. Claude
sessions derive busy/idle from `native_status` and sort/render like Codex sessions. Limits edit minimum remaining
percentages through the existing owner. Deregister preserves credentials/history;
Restore reverses deregistration. Delete removes local account files and credentials
while preserving saved/archived conversations. The upstream provider account is
unchanged. Both confirmations require the displayed friendly provider-name label.
While a Claude, Gemini or Antigravity login runs, choosing Log in (or the account's
"Login required" status) again finds the open `Provider login` pane through HERDR,
reads only its visible text (`herdr pane read --source visible`; never credential
files) and says in its login popup that a login is already in progress. The provider CLI prints its OAuth link in that
pane; the newest `http(s)` link is rebuilt across terminal line wraps (rows as wide
as the screen's widest row continue on the next row, joined exactly) and offered as
"View link fullscreen". Until a link is printed the dialog says it is waiting and
offers no option. The pane is re-read every 2 s and the dialog follows a changed
screen; with no login pane the usual Login confirmation opens.
Every popup of a login is titled `<Provider> login` for all four providers alike
(`Codex login`, `Claude login`, `Gemini login`, `Antigravity login`): the check for a
running login, the confirmation or Gemini sign-in choice, the link, the wait, and the
result or failure, whose outcome is the first line of the body. The Gemini API-key
entry and its validation result are titled `Gemini API key`.

Gemini accounts choose a sign-in mode from the backend's ordered `auth_modes`
(`api capabilities`, contract revision 21). Add (after `account add`) and Log in open
a choice dialog listing them in backend order (Google login, Google Workspace login,
API key). An option the backend marks `available: false` is drawn struck through,
cannot be chosen, and shows the backend `reason` as a dim parenthetical only while the
cursor is on it; availability and reason are never hard-coded. A Google option runs
`account auth gemini:NAME --mode google` and then the returned login plan like any
login. API key opens a masked field (each character drawn as `•`, never in clear) and
runs `account auth gemini:NAME --mode api-key --key-stdin`; the key travels only as
stdin JSON `{"api_key": …}`, is cleared from the dialog when sent, and is excluded from
arguments, `Debug` output and traces. The result reports valid, rejected (backend
reason) or saved-but-unverified (with its code). Account Details show `Auth` (Google or
API key) and, for API key, whether a key is saved. A backend without `auth_modes`
keeps the plain Login confirmation.

Antigravity CLI (`agy`, provider `antigravity`, contract revision 24) is added like Gemini
but has no sign-in modes: its `capabilities.providers` entry has no `auth_modes` or
`auth_command`, so there is no API-key, Account auth or live-steering item, and
`account_logout` is false (log out with `/logout` inside agy). Add and Log in run the
backend's interactive login plan in a visible pane exactly as for Gemini; agy then shows
its Google OAuth URL and waits for the pasted code. Usage is unsupported and shown as a
stub bar, never as 0%, and no Limits row or usage windows are listed.

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
Archived sessions show only their names; two archives with the same name gain
a short ID suffix (`codex-root · 7792`). Full IDs live in Session details.

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
Long detached text stays on one line and scrolls. Space roots use this same rule.

Text longer than its space scrolls as a marquee: it shows its start for about a
second, then moves left one character every 160 ms around a loop of the text,
two spaces and the text again, until the start is back in place, and repeats.
Selectable items (menu rows, list entries, badges, previews) scroll only while
selected and otherwise end in `…`; titles, notes, hints and info lines scroll on
their own. Colours and weights stay those of each cell.
Manage Sessions → Operations (or `o` while the status lane shows operations) opens
the Operations popup over the current page: Running (operation spinner, kind,
subject, live elapsed counter), Queued (in order, `queued #n`) and Recent (the last
five finished or failed, newest first; a failure shows its reason, shortened to
the popup), then All operations (the full list below)
and Close. It updates live and keeps the selected item. A queued item offers Cancel
and Close; a recent one Dismiss and Close; a running one its progress dialog.
Esc/Left return to the opener at its remembered row.
Operations has a filter row (`Show › All`, an Up/Down picker like Settings: All, In
progress, Failed, Completed) above day groups (`• Today`, `• Yesterday`, `• Mon Oct 26`;
undated legacy records last). Each row shows relative creation time, the subject
(provider symbol and session or account), a plain kind (Move to Space, Delete
account, Resume…) and a dim status; failures carry their reason inline. IDs are
shown only in the operation's details, in a dim Identifiers group; details are
titled by kind.
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
Focus Session uses the backend's `attachment_lookup.version:1` capability to
check the selected catalog generation and process epoch without full discovery.
It still checks membership in the intended HERDR pane/socket and rechecks the
generation before focusing. Older backends retain the discovery-based check;
neither path launches or submits input to an agent.

Settings > Spinners has one row per provider (Claude, Codex, Antigravity, Gemini; wherever
the UI lists providers, Antigravity comes before Gemini). Every row lists, top to bottom:
**Native** (the provider's own spinner; Codex has none, so its first option, **Native/Classic**, is the
Braille cycle and it has no separate Classic), the themed options, **Classic** (the plain Braille
spinner, 100 ms), **Glow** and **Static**. Glow is a static glyph whose hueshift oscillates, once per
second like the active picker arrow and sampled at the glyph's cell: between the normal and the
darker hueshift on a selected or hovered row, and between the darker and an even darker one
otherwise (the Settings preview animates the same way). Static is the plain provider icon, with the
same hueshift as animated spinners (normal when selected, darker otherwise) and no animation.
Older stored values migrate: `claude` becomes Native and Codex `classic` becomes Native/Classic;
Gemini `classic` stays Classic; unknown values read as the default. The **Antigravity spinner** row
offers Native (default: agy's own 8-frame Braille cycle
`⣾ ⣽ ⣻ ⢿ ⡿ ⣟ ⣯ ⣷`, 80 ms), Pulse (`· ▵ ▴ ◬ ▴ ▵`), Tilt (`◬ ◭ ◬ ◮`), Classic, Glow (the static
`◬`) and Static; every glyph is one cell wide.

Manage sessions sorts busy, idle, then inactive sessions, newest access first
within each group, with a bullet separator between active and inactive groups.
Every row has the name, then a state marker in place of the provider symbol and the
account (`liq-glass-lead  ⏣ gate1`): the provider's chosen spinner while busy (or a
pulsing symbol for Glow), the provider symbol and account name in the position-based
accent while running idle, and both dim for saved history. Name width scales with the popup (about
30% of its width, 12–40 cells); unselected long names end in `…`, and the selected
one scrolls left (pause, scroll, pause, loop) to reveal the rest. A session with a
pending or recently failed operation shows a badge after the account (`◜ moving to
work` animates while active, `✗ move failed`), and a queued one `◌ restart queued`.
A failure badge shows only the session's most recent outcome: a later success hides
it, and failures older than a day are not shown; Right moves focus onto the badge,
Enter opens that operation's progress or details over the list, and Left returns.
A finished operation's dialog offers `Dismiss` before `Close`: it hides that badge
for good (kept in `dismissed-operations.json` beside the preferences, pruned when
the backend no longer lists the operation). Running operations cannot be
dismissed, and a newer operation on the same session shows its badge again. A
dismissed operation's dialog offers `Undismiss` instead, which removes it from
`dismissed-operations.json` so its badge shows again under the same rules.
The selected row's account name and static provider icon are bold at the selected
accent level (other rows use the dim level; an animated icon keeps its own styling);
with focus on the badge they return to the dim level, unbolded. Without a badge, the
selected row's live counter (`(1:01)`, the shared counter format) or state (`stale`)
follows one space after that row's own content and is shown only when it fits whole.
The counter uses `active_since`; when that is null it uses the attachment process
start (`attachment.process_start`) only after checking it against this host's
`/proc`, otherwise no counter is shown.
"Showing saved sessions; refreshing…" appears only on this list and only until
fresh state arrives. Unavailable actions stay visible, greyed, with a short dim
reason aligned in one column (`End session  · not supported for Claude Code`).

Errors and status notices use a centered bottom line, separated from controls by
one blank line. Without a notice, management pages use that lane for live operation
counts (`◜ 2 running · 1 queued`, zero parts omitted, hidden when idle and when it
does not fit); names stay in the Operations popup and badges, and text a dialog
already shows is not repeated there.

Operations are queued in the UI because the backend refuses a second operation on a
reserved conversation and has no queue. Requests on different sessions or accounts
run concurrently, up to three at once; requests on the same session or account run
one at a time in order (a New session queues on its account, Resume on its session).
A lifecycle action on a session with a running operation is offered as `queues after
current`. The queue is kept in `operation-queue.json` beside the preferences, scoped
to the backend root, and a detached `queue-worker` started by the popup dispatches
it, so closing the popup neither drops nor repeats work. Each entry is marked
running before its single attempt; an unconfirmed outcome becomes a failed entry and
is never retried. An uncertain or failed Start settles as a failed entry ("Outcome
uncertain — not retried" or the receipt's reason), so work queued behind it proceeds.
Restart, move and switch change a session's generation, so when a queued session
action starts it re-reads the session and uses its current generation, but only while
it is the same session ID (or the entry that now carries its conversation lineage
after a merge) holding the same conversation lineage; otherwise it fails with "choose
the action again". Menus reserve the status lane before calculating scroll capacity.
A failed preview/mutation closes its waiting dialog and shows the error there,
without an automatic retry.

Menu bottom controls and pagination share the reference soft accent and sweep
(55% brightness, regular weight). Popup controls use that same light colored
accent, including directory navigation, pagination and dialog scroll hints. Each
control line is centered independently inside its surface. Every key press is delivered at once; nothing is deferred. A press at an edge wraps.
A hold crosses an edge at most once and never spins around; native repeat events
never wrap. Without release or repeat events (Termux, SSH, HERDR) a press continues a
hold only on the auto-repeat timing signature: the first repeat 250 ms to 1.2 s after
the press, later repeats at most 110 ms apart. Taps in between are fresh presses, so
two quick taps from next to an edge reach it and then wrap. Home, management menus, directory pickers and dialog buttons share this
selection-navigation implementation. The first press always moves immediately; repeat timing comes from the
terminal; only ambiguous legacy follow-up events use the lookahead. Home/End jump to the first/last list option
(including directory lists); text editors retain cursor Home/End. These shortcuts
do not add footer hints.
Centering with Back/Close rounds
right; six spare columns use four on the left to keep the arrow one cell from the
edge. Options scroll only when they cannot fit with one blank line before controls;
range/detail rows reserve space only when required. Unavailable options accept
hover/keyboard focus but cannot be activated; separators are skipped. They strike
only their text, never the bullet, and their text never becomes bold.

Dialogs use HERDR's `┌Title──…┐` border style without title padding. Info dialogs
hide their sole Close option and shrink to their content; action buttons remain.
Every sub-popup (dialogs and choice popups such as Choose account, Choose Space,
Start directory and Operations) draws its own controls through one shared path:
centered on the bottom row(s) inside its frame after one blank row, keys in the
panel's text colour and actions muted, with `PgUp/PgDn: Scroll` leading them while
the body scrolls. Frames are wide enough for one hint row where the terminal allows.
While a sub-popup is open the main popup's footer keeps its rows, and the Esc close
band, but shows no controls. A dialog that replaces another keeps at least that
frame and grows when its own content needs more; options that appear later (such as
`View link fullscreen`) grow an open frame instead of being drawn past its border. Dialog items have at most one blank line
between them. Stars never appear within text or its spaces. Fades and the vertical
gradient blend from the terminal’s OSC 11 background reply without changing that
background. Star colors never fall below that background in any RGB channel;
unanswered queries use a dark fallback.

Dialogs use read-only HERDR theme configuration and bundled attributed palette
tokens. Configured built-ins and RGB overrides are supported; dynamic/terminal
themes report an approximation because HERDR exposes no resolved-palette API.

## New session and directory flow

The Space chooser starts with `[create new space]`. Starting a saved session, adding a running session to HERDR and moving it between Spaces open this same chooser (same title, description, inline `(id)` previews, Cancel placement and preselection). `[create new space]` asks for a Space name (existing labels refused), then confirms with `--new-space LABEL`. For those three actions it is enabled only when the backend advertises `capabilities.new_space.<resume|add_to_herdr|move_space>` (provisional path); otherwise it stays listed but unselectable with a "(not available for … yet)" reason. A session's current Space stays listed but unselectable, previewing `(current)`.
Only the highlighted Space's `(id)` appears, one space after its name, unbolded
gray; Back focus hides it. If any ID
cannot fit inline, the whole list uses the standard centered preview below the
options or range counter, separated by one blank line.
Global-root and Space-root paths follow the same preview rules, remain unbolded
gray, and appear as bare paths one space after the option text when inline.

Open **New Session**, edit Account, Space, Name and Directory in any order, then
select Start Session directly below the fields and separator. The top bar shows the
configured global or selected-Space project root, depending on the default-directory
mode. Unset values throughout the UI use a dim `not set` placeholder, including
the top bar, session fields and root previews. A staged new folder is marked
`(new)` beside Dir, and Start first confirms exactly which folders the launch
will create (Create/Cancel); Cancel creates and launches nothing.

The public CLI session catalog refreshes asynchronously while creating a session.
When another active or inactive session has the same directory, Start Session shows
`⚠` one space after its label. Selecting it reveals `(existing session in dir)`
after the warning. Activation asks for confirmation (Cancel selected initially);
Cancel preserves the form, and confirmation follows the normal CLI validation and
launch flow. The UI never creates directories or starts an agent before confirmation.

The default Space is
the one currently focused in HERDR; no focus leaves it unset. Pickers preserve the
draft on Cancel/Esc. Before a session has a name, the directory picker starts at
the configured projects root, using the global root for browsing if the selected
Space has none, rather than HERDR’s Space working directory. This does not select
a directory or stage a named child. Existing automatic paths require explicit
selection. Directory Enter
selects the current option. In an unfamiliar directory, focus starts on the first
subdirectory, or the non-activating `(none)` placeholder when empty; revisiting restores its selection.
`Use this folder` is mounted above the list and Cancel (with `Leave unset` for
defaults) below it; neither section moves the list, which keeps its own scroll
position. Up from the bottom section re-enters the list at its last item, Down
from the top section at its first. Home/End/PageUp/PageDown work in every
scrolling list. Typing filters the listed folders by the typed name: matched
characters are brighter and a faint ghost completes the first match (Tab accepts).
A typed name that matches nothing becomes one `name/  (new)` entry; nested missing
folders are listed by the confirmation. Left opens the parent from every row;
Right enters the selected subfolder. A new-session choice is only staged; the
launch creates it. In Settings, choosing a folder that does not exist opens a
confirmation stacked over the picker listing every folder to create; Cancel
returns to the picker unchanged, Create makes them, saves and closes the picker.
Missing per-Space roots remain unset, without a global fallback. Last-account prefill
initially requires fresh usage within existing limits. Once selected, the account remains
visible during refreshes and stale usage; only confirmed unavailability clears it.
Launch validation remains authoritative. Manually selecting a backend-permitted
account does not rewrite its limits.

On full-screen menus, the Back arrow is beside the top visible option. Left selects and expands it only
when the expanded label retains a left padding cell;
Right returns to the option. On the main page its expanded label is **Close**.
Settings submenu rows show the submenu's selected option in the darker grey, in one
column three spaces after the longest name (all inline or, when one does not fit,
none, with the selected one below the list). Selectors (`Starfield  › Normal`) show
a `›` one space after the longest name, in the bullet hue (bold when hovered, darker
while Back or another selector is focused); Right or Enter enters one, which shows
the remaining directions (`↕ ↑ ↓`) pulsing between the two hue levels; Up/Down
choose, Enter sets, Esc reverts. **Appearance** sets the star field
(Dense/Normal/Sparse/Off) and brightness (Bright/Normal/Dim), saved in
`preferences.json` as soon as Enter sets a value. Keys a newer build added to the
file are kept on save and unknown values read as the default, so an older build
still opens it. **Default directories** chooses global or per-Space defaults and
holds the roots; its values share one column. Leave
unset clears that default and its pending creation plan.
**Permissions** holds the **New session permissions** selector: **YOLO** (top, the
default) starts new
and resumed sessions without approval prompts (Codex approval `never` with full
access, Claude `--dangerously-skip-permissions`, Gemini `--approval-mode yolo`, Antigravity `--dangerously-skip-permissions`);
**Default** keeps each provider's own prompts. It is sent as the launch request's
`permissions` only to a backend that negotiates it (contract revision 19); an older
backend uses its own default.
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
