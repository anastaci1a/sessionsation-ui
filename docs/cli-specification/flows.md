# Flows, backend boundary and installation

This audits every reference feature, including features intentionally excluded
from the target launcher. It does not authorize importing its legacy backend API.
Sources: [menu.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/menu.bash),
[session-menu.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/session-menu.bash),
[switcher.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/switcher.bash),
[actions.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/actions.bash).

## Reference home and return behavior

`ez_menu_define_items` always begins `New Terminal`, `Herdr`. With discovered
sessions it adds a shortcut to the first ranked session (`Codex: Resume (name)`
if live, `Codex: Start (name)` if inactive), then `Codex: Sessions`. Without sessions
it offers `Codex: New Session`, plus Sessions when Switcher/jq are available.
Discovery failure adds `(disconnected)` and an explicit description; failure is
not interpreted as successful empty inventory. Labels/actions are regenerated
together so changing inventory cannot leave a label targeting the previous action.

New Terminal prints a green readiness line and returns to the calling shell.
Focused home Exit actually exits that shell. Esc returns to shell without exiting
it. Herdr runs the configured binary, leaves the alternate screen while attached,
then returns to the same menu selection on detach. These are separate actions;
the target must not accidentally treat Plain Shell, cancel, viewer detach and
agent Stop as synonyms.

Target scope is New/Resume/Plain Shell. No nested Herdr action, destructive shell
Exit action, jobs or management row is required. Preserve visual focus/navigation,
not irrelevant action wiring. The separate `manage` command remains outside this
creation UI; its existing implementation is not being redesigned in this audit.
The planned HERDR entry and exit behavior is specified in
[architecture](architecture.md#preferred-host-flow-popup-to-a-new-normal-tab):
the popup requests a separate normal tab, then exits after confirmed handoff.
Today's opaque exec plans alone do not implement that flow.

## Reference creation and accounts

New is a sequence: unique session name → Start directory form → Choose Account →
recheck name → create/attach. Cancelling a form returns to the originating menu,
not the preceding form (the directory's Add-name subform is the local exception).
Do not assume a wizard-style Back stack from a screenshot.

Accounts come from `status --json`, schema 1, and require daemon.running=true.
Enabled=false is disabled with `(unavailable)` note. Enabled but auth_required
adds `(login required)`; otherwise eligible!=true adds `(check capacity)`.
Capacity is advisory, so these enabled rows remain selectable. No rows/no enabled
rows produce explanatory attention pages directing users to Session Manager.
Actual account name is passed to backend, never its display nickname. Target
account selectability must follow public capabilities, not reimplement legacy rules.

The legacy launch builds `codex-switcher run --account ACCOUNT`, optionally
`--thread UUID`. New sessions add the existing bypass-permissions flag; saved
thread launches do not override recorded permissions. It creates a detached tmux
session named `codex-NAME`, exact cwd, then attaches. A tiny `/bin/sh` exec wrapper
retains failed startup diagnostics (`remain-on-exit failed`), corrects TERM only
when current terminfo is unavailable and xterm-256 color exists, passes PATH and
explicit home variables, and clears absent homes inherited from stale tmux state.
Successful exit closes normally. These details explain the reference; **target
launch plans come unchanged from sessionsation and the HERDR retained-shell repair
stays intact**. No reconstructed permissions/account command or default fallback.

## Sessions presentation and open

Inventory uses one schema 1 sessions array, sorted by group: live busy/unknown,
live idle, inactive; then descending `last_accessed` milliseconds, missing=0.
tmux pane/session-created timestamps supplement uptime only, never inventory or
activity. Strip exactly one leading `codex-` for display; backend name is untouched.
Account label uses `display_account` with `account` fallback, whereas `account`
alone remains home provenance. Labels sanitize control characters.

Rows: `[new session]`, sessions, optional Retry Discovery, nonselectable bullet
separator, `Session Manager`. Session names and bracketed nicknames are padded so
brackets align using maximum name/account widths. Busy rows stay selectable.
Inactive suffix is always gray. Unknown/disconnected rows explicitly say so.
Selected live rows show elapsed uptime (clamped at 0): m: ss, h: mm: ss, d: hh: mm: ss.
It disappears on deselection/Back focus without recentering the option block.

Busy indicator is a bold braille spinner every 100 ms:
`⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏`. It occupies a reserved gap after account label before
timer, stays full accent brightness even off-selection, and dims only for Back
focus. It shares account hue/sweep timing while timers remain gray. Clock/duration/
spinner changes repair their own cells, not the entire row each frame.

One Enter delegates `open --session ID` directly, without another inventory
preflight or action submenu. Backend resolves saved/live state and original home;
returned pane/session target is validated and attached exactly. No display-name
targeting, implicit account move or `resume --last`. Focus is restored by returned
ID/thread after detach. tmux has-session check precedes recency/pane selection;
set-option uses exact session target plus colon, while attach/switch uses exact
target. One ordered tmux command queue prepares recency/window/pane, and failure
prevents attachment. Existing tmux clients switch; otherwise attach.

Target may use this presentation for Resume where public capabilities permit.
Whether to include live attachment alongside saved Resume needs explicit public
plan support; never copy the legacy key-changing/thread-fallback identity model
over sessionsation's stable IDs and generation checks.

## Discovery and errors

`ez_switcher_request` captures stderr in a temporary file, invokes argv literally,
requires exactly one JSON result and applies schema validation. Nonzero exit and
invalid envelopes surface distinct errors. Sessions require observed_at number,
unique nonempty IDs, string name/cwd/account, supported lifecycle, optional typed
nickname/last_accessed; live pane must match `%digits`, inactive thread UUID.
Missing/unrecognized live activity becomes unknown. Open validates pane, nonempty
tmux session and optional ID/thread types before using targets.

The read-only watch is `sessions --all --watch --interval 2`. Chooser owns the
coprocess and FD. Timed reads retain partial JSON until newline, drain at most
eight snapshots per call, preserve last complete snapshot even when EOF accompanies
it, and report malformed/ended stream as disconnected. Retained live rows become
unknown, busy indicators clear, and explicit Retry is offered. UI cancellation
closes/reaps only its own watcher. Complete snapshots pass between Home and
Sessions through an opt-in state return; ordinary chooser stdout remains just key.

`ez_codex_document` sanitizes and wraps full explanations to max(1, W-6), with
max(1, H-13) lines/page. Next/Previous navigation covers all text; approve/decline
buttons appear only on the last page so narrow terminals cannot hide consent
information. This generic helper supports attention dialogs. Management
confirmations belong to the separate management flow, not the creation launcher.

## Other helpers: audited, excluded

- Session Manager reuses exact tmux `codex-switcher` running `codex-switcher ui`;
  detach returns to its row. No in-menu account switching/rename/project move UI.
- Jobs helpers inspect the **calling Bash shell's** running/stopped jobs. Per-job
  action foregrounds with `fg` or sends TERM then CONT to a stopped job; bulk
  terminate iterates. They are defined but absent from current home items. Do not
  transplant job control into the creation launcher.
- Availability helpers for monitor/switching exist; automatic account switching
  stays hidden. `AGENTS.md` mentions preserving `cxr`, but no tracked definition
  exists in this checkout. It is an external integration reference, not a feature
  to invent or a reason to inspect/change private shell configuration.
- Opening timings are opt-in TSV stage/microseconds/status only, no IDs/JSON:
  watcher_shutdown, switcher_open, response_validation, response_extract,
  tmux_preparation. Write failure cannot break opening. Final interactive attach
  and Codex first screen are outside these timings.

## Loading, configuration and Unraid

Sources: [init.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/init.bash),
[installer](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/install-unraid.bash),
[unraid directory](https://github.com/anastaci1a/cli-main-menu/tree/ae786af695791fdf858eacf5780e04b5138be49d/unraid).

`.bashrc` is the reference entry; `bashrc.example` resolves explicit
CLI_MAIN_MENU_DIR, colocated init, or scripts/cli relative to its own source file.
init loads theme → tracked defaults → ignored personal config → font/render/stars
→ actions/switcher/forms/session-menu/select/menu. It defines
`startup='clear && ez_select'`, removes the loader helper and returns failure if a
module fails. Sourcing/repeated sourcing must not launch UI/tmux/Codex, change cwd
or caller shell options. Runtime prerequisites are Bash 4+, date fallback, stty,
clear; provider integration adds jq/tmux/Switcher. Preview dependencies are optional.

Installer requires root/Unraid and persistent absolute `/mnt` paths, backs up boot
go and root bashrc/profile once, writes mode 700 config directory/mode 600 settings
and hooks, appends a guarded boot restore/start block and runs restore. It does not
launch UI. Restore creates only the root config link before array mount, preserves
an existing HERDR link, installs profile hook, and appends idempotent login/bashrc
blocks. Profile restores PATH without duplicates, exports menu/backend/HERDR
settings and default CODEX_HOME, and sources init for Bash. Direct interactive
login (TTY, outside tmux, not already started, direct profile source depth 1)
auto-opens; nested/noninteractive/re-sourced shells do not.

The separate boot starter waits up to 300×2 seconds for array/backend/Codex binaries,
then starts the existing Switcher daemon and logs status; failure logs guidance.
**Do not run/import this installer or boot daemon starter for the rebuild.**
sessionsation-ui already has its own verified source-backed HERDR launcher,
configuration receipts and single backend-root owner. Those authoritative setup
details stay in [usage](../usage.md) and [deployment manifest](../deployment.json).
