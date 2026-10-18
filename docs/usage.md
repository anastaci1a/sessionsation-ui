# Usage

Set `SESSIONSATION_BIN` to the verified absolute backend executable and
`SESSIONSATION_DATA_DIR` to the operator-selected root. Existing Ana accounts use
`/mnt/user/appdata/codex-switcher` with its existing credential owner. The independent
backend default is not an automatic import. See the backend ownership guidance.

Run `./bin/sessionsation-ui launcher` in an ordinary terminal for new, resume or
plain shell. New reads the account list directly; it does not scan conversation
histories or terminal attachments just to show accounts. Resume still discovers
current sessions and the backend revalidates the selected generation at execution. Run `./bin/sessionsation-ui manage` for account and selected-session
management. Exiting management only closes the viewer. Stop is a separate explicit
backend operation. Capability hints determine available choices.

No existing shell startup files or restored agents are modified.

## Interactive launcher

In a TTY the launcher uses an alternate screen with colored circle markers,
cycling accents, twinkling stars and a diagonal shimmer. The fade and field reach
the terminal bottom regardless of list length; Back and key hints stay there too.
There is no ASCII title. Pipes and `TERM=dumb` retain the numbered text interface.
No management controls appear in the creation flow; `manage` remains separate.

- Up/Down wrap through enabled options; Enter or Space selects. Home/End and
  Page Up/Down navigate long lists. Left focuses Back/Exit; Right returns to options.
- Esc returns one page, or exits the home menu to the retained shell. Ctrl-C/Ctrl-D
  cancel the whole launcher. Exiting a viewer never sends a lifecycle Stop.
- New asks for a directory and an enrolled selectable account. It uses the account
  list directly. The chosen directory is inherited by the opaque backend plan when
  its cwd is unset. Account/credential decisions and generated session names stay
  with the backend. No provider command is reconstructed.
- In the picker, Enter opens a child or chooses an action. Right/Tab opens a child;
  Left opens its parent and highlights the directory just left. Left on the first
  action row, or anywhere at filesystem root, focuses Back. Returning to a child
  restores its selection; an unvisited directory keeps the row where possible.
- Type a path or explicitly create one folder within the current directory. Text
  fields support cursor arrows, Home/End, Backspace/Delete and Ctrl-U. Pasted text
  cannot select menu actions; newlines cannot submit text fields.
- Resume lists saved sessions and preserves backend IDs, generation and cwd.
  Plain Shell uses the backend shell plan. Backend errors require an explicit
  return; no automatic mutation retry is made.

Resize preserves selection and scrolls long lists. Very small terminals ask to
enlarge and keep Escape available. Cancel stays responsive during asynchronous
reads; raw mode, cursor and the original screen are restored before handoff or exit.

## Management plugin

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

## Future-terminal launcher

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

The source-backed launcher is configured on Satellite for future terminals using
`/mnt/user/live/projects/sessionsation-ui/bin/sessionsation-shell`. Its client settings
are `/mnt/user/appdata/sessionsation-ui/config.json`. The authoritative
[deployment manifest](deployment.json) records the selected immutable backend
path/hash, source, build checks, explicit data root and repair rollback receipt.
This launcher-only build starts from e4d7833 with bounded discovery and HERDR handle
repairs. It excludes the staged Agent DM observer candidate. The existing data-root
supervisor retains credential ownership; no adapter activation is implied.

To undo the performance repair, run the documented rollback command with the
repair receipt from the manifest; it restores the previous e4d7833 selection.
No HERDR reload is needed for that client setting. To undo the entire launcher
setup afterward, apply the original private receipt:
`/mnt/user/appdata/sessionsation-ui/config-backups/launcher-1e1b8d65-5fb5-4b63-8db5-ff0d60e44c9b.json`,
then reload configuration on the intended server. Rollback refuses conflicting
edits, so restore the repair first. The default server reloaded without diagnostics
during original setup, and a new owned terminal displayed
the menu with its requested cwd. The smoke terminal was then closed. Existing
agents and the original pane retained their process/terminal identities. Other
already-running servers need their own explicit configuration reload for future
terminals; existing terminals are never retrofitted.
