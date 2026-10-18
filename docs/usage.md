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
appear inline, starting one cell from the left edge, without selector circles, and Add Account is available.
Otherwise Manage Accounts provides a scrolling list with `<add account>` first.
Its highlighted account's usage sits above the controls with a blank line between.
Growing the screen returns that list to Home; forms remain open and Back chooses
the appropriate current layout without losing their account selection.

Verified 5-hour accounts show paired 5h/weekly bars; weekly-only accounts show their weekly bar. Bars measure remaining
allowance, with custom limit markers inside them. Names share a bar start column;
percentages precede period labels, with shared right-/left-aligned columns. Missing observations show Loading… rather than Ready; stale observations are marked. In the selection picker,
usage is a separate right-aligned `x% 5h / x% wkly` column with aligned percent signs
and slashes. Weekly-only accounts omit the short period and slash; shared padding
uses the largest displayed number. New account names are
limited to 12 characters. Existing longer names are shortened only in list display.

Account actions group Usage details/Limits, Login/Enable/Disable, and Deregister/Restore
with dot separators. Limits edit minimum remaining percentages and use the owner's
existing threshold settings. Removal shows affected data and requires the exact
account ID; it preserves credentials/history and refuses active conflicts.

Manage Sessions selects an action menu, never implicitly attaches. Lifecycle
actions carry the captured backend ID/generation. Stop, Restart, Switch, Disable
and Remove require confirmation. Internal modal dialogs preserve their underlying
page; Continue in background dismisses progress without cancelling backend work.
Operations shows progress, fresh confirmation prompts and supported recovery.
It also lists original New/Resume receipts after reopening, without resending them.
Resume has editable account, Space and name, plus the saved read-only directory.
It prefers a verified original Space, then current focus, otherwise unset. Changing
account uses the backend safe transfer, preserving conversation history and ID.
Uncertain responses are not automatically retried. Focus tab selects an existing
HERDR tab in this instance after rechecking its catalog identity.

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
selects; Right browses child rows only, leaving `<use this dir>` and `<create dir>` unchanged; Left goes
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
Configuration uses short labels and shows a directory preview only when the full
path fits beside its label.

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
only backend-reported data. Account removal shows the backend preview and requires
typing the exact account ID; no credential/history purge is offered.

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
for recovery. The deployment check preserved all 31 captured agent/host PID/start
identities. Other running HERDR servers are not automatically reloaded.

Account deregistration previews open immediately while data loads. The confirmation
shows the friendly provider-name label (for example `codex-personal`), which the UI
maps to the backend ID only after an exact match. Signed-in accounts omit Login; it
appears only when authentication is required. Session action ordering/groups follow
Codex Switcher, with Focus Session, Session Details and Rename Session first.
Unsupported public capabilities remain disabled. Session account columns appear
for the complete list together; selected live sessions show the reference elapsed
time format when the backend reports verified `active_since`.
