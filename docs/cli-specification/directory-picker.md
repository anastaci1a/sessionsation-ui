# Directory picker and editable forms

Source: [sessions.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/sessions.bash),
especially `ez_codex_expand_dir`, `ez_codex_resolve_expanded_dir`,
`ez_codex_new_dir_target`, `ez_codex_new_dir_name_target`, `ez_codex_input_line`,
`ez_codex_field`; name uniqueness comes from
[session-menu.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/session-menu.bash#L4).

## Screen and initial state

This is an **inline path textbox with suggestions**, not a separate Type Path
menu. Row 1 is status, row 2 bold functional heading `Start directory`, row 4 the
input. The source banner includes two blank rows; options begin at row 6. It starts
with the calling menu's captured working directory, cursor at the end, selected
row 0 and empty per-picker history. The two fixed action rows are exactly:

1. `Use this directory`
2. `Add new directory`

Child directory suggestions follow, displayed as basenames plus `/`. Include
hidden directories and directory symlinks; require directory and search (`-x`)
access. Glob order follows Bash/current locale, not a separate natural-sort rule.
There is no parent `../` row, breadcrumb bar, fuzzy search, files list or mouse UI.
Names with control characters are displayed using `?` in suggestions/text input.

On each input edit, validate the whole path. If valid, suggestions are children of
its canonical physical directory. Otherwise split last slash into base/prefix;
resolve base, then suggest directories starting with that literal prefix. Without
a slash, base is the menu's initial directory; an empty base from `/prefix` becomes
root. Empty input produces no suggestions. Shell wildcards in the typed prefix
are quoted: only the appended `*` is glob syntax.

Use is enabled only for an existing resolvable directory. Add is enabled for a
valid directory or a creatable single leaf below an existing writable/searchable
parent. A missing parent chain is invalid. Disabled selection moves to the first
enabled row if one exists; with all rows disabled, Enter remains a no-op. The
textbox is white when existing **or creatable**, red otherwise. It does not create
anything while validating or browsing.

## Selection and browsing are distinct

| Key | Start-directory behavior |
| --- | --- |
| Up/Down | Wrap enabled action/child rows; clear Back focus; leave typed path unchanged |
| Enter on Use | Return canonical existing directory and close picker |
| Enter on child | **Accept that child's canonical directory and close picker** |
| Right/Tab on child | Browse into child, update textbox, retain/restore row as below |
| Right/Tab on action row | If any children exist, browse the first child; not action activation |
| Right while Back-focused | Only return focus to options; Tab may proceed into a child |
| Left on Use (row 0) | Focus Back |
| Left anywhere at resolved `/` | Focus Back |
| Left on row 1 or later away from root | Open parent, highlight directory just left |
| Left while already Back-focused | Keep Back focus |
| Enter on Back or standalone Esc | Cancel picker (return 130); no directory write |
| Space | Insert a literal space into the path; **not** selection |
| Backspace/DEL | Delete before cursor; revalidate/rebuild suggestions |
| Ctrl-U | Clear path and Back focus; show placeholder |
| Ctrl-D | Cancel |
| Ctrl-L | Ignore |
| Other printable characters | Insert at cursor and revalidate |

CSI and SS 3 arrows work. Form parser consumes a single arrow final after prefix;
it does not share chooser's full 16-character escape parser. Unsupported key
sequences, Home/End/Delete, bracketed paste and mouse input have no defined
reference convenience behavior. In the directory form Left/Right navigate folders,
not the text cursor; ordinary typing/backspace edits at its current cursor, usually
the end. Text-only name forms use those arrows for cursor movement instead.

## Per-picker history

History is keyed by canonical directory path. Its value is the selected child
path, or tagged action/row index `@N`, not a globally persisted directory list.

- Before descending, remember the child selected in the parent. On entering a
  previously visited child restore its stored identity; otherwise retain the row
  number where possible, clamped to available choices.
- Before going up, store the current selection (child identity or action tag).
  In the parent, select the directory just left by path, not its old sort offset.
- Returning Down/Right to a visited child restores its remembered child/action.
  If a remembered child vanished, reset to first available choice. Sorting or
  inserted siblings must not redirect remembered selection to another folder.
- History lasts only for the current picker; scrolling is local, reset/recomputed
  on path edits. Up/Down changes alone preserve field geometry when possible.

Example: in `branch`, select `leaf`, press Left: parent highlights `branch`.
Press Right: `branch` opens with `leaf` selected. Enter now returns `branch/leaf`.
The rejected redesign opened `leaf` on Enter instead; that is not reference parity.

## Path expansion and validation

`~` and `~/...` expand to current HOME; no `~otheruser`. `$NAME` and `${NAME}`
expand only existing environment variables with ASCII shell identifier names.
Undefined variables stay literal. `\$` becomes literal `$`. Expansion is one pass:
values are not recursively expanded. There is **no eval, command substitution,
quote interpretation, shell execution or wildcard expansion**.

Relative paths resolve against `ez_codex_start_dir` captured on menu entry,
falling back to PWD; they do not silently become relative to a browsed child.
Physical resolution uses `cd -P` and `pwd -P`, then normalizes multiple leading
slashes to one. Symlinks thus resolve to their physical destination.

For a missing leaf: strip trailing slashes, require a nonempty final name, resolve
parent physically, require writable/searchable parent, and reject existing paths
including dangling symlinks. Do not recursively create missing parents.
The name-only helper requires nonempty name, no slash and existing writable/
searchable parent; `.`/`..` fail because they already exist. `$`, spaces and other
printable punctuation in the **name-only** form are literal, not variable expansion.

## Add new directory

On Add from an existing directory, change the same form to `New directory name`
with empty input and that directory as parent. From a creatable typed path, use
its existing parent and prefill its final component. Store the old path input.
This subform has one `Create directory` action and no Left-focus Back control.

Name validity controls white/red prompt and whether Enter is advertised. Enter
on valid name performs exactly `mkdir -- target`, then returns the canonical new
directory and exits the entire picker. A mkdir failure revalidates and keeps the
form open; there is no retry loop issuing repeated mkdir without new input.
Esc from this subform restores Start directory and its old path input; Esc again
cancels. Cancellation creates nothing. No automatic cleanup of a folder that the
user explicitly created is specified.

Non-TTY fallback prompts for a path. Existing paths return immediately; a creatable
path asks `Create directory ...? [y/N]`, accepts y/Y/yes case-insensitively, then
makes one directory. Invalid input prints an error and repeats; EOF cancels.

## Text-only forms and cursor

`Session name` has one `Continue` action. Allowed ASCII characters: letters,
digits, space, `_+-=~()[]`; nonempty, not exactly `switcher`, no existing exact
tmux session `codex-NAME` unless an exception was supplied. Uniqueness is checked
again before launching. Target sessionsation must use its backend catalog/name
rules instead; public launch naming support needs resolution with dot/backend.

`Move directory name` is a reusable form returning a validated name under an
explicit parent; there is no current main-menu move action. It must not introduce
project-move management into the target UI.

The text cursor is drawn as a white background/black foreground cell. Before and
after retain prompt color. At end it highlights a blank; empty input highlights
the first placeholder character, remainder disabled gray. Placeholders are
`Type a unique session name`, `Type or navigate to a directory`,
`Type a new directory name`, `Type the destination folder name`.
Input row begins `> `. Shown controls become `?`; minimum internal width 4.
Horizontal slice starts max(0, cursor-width+4), with width-3 characters displayed.
Left/Right move cursor within the name, Backspace removes previous character,
printables insert, Ctrl-U clears, Enter accepts valid input, Esc cancels.

Rebuild acceptance must exercise these interactions end to end, including spaces,
hidden/symlink paths, variable literals, permission failures, renamed/deleted
children, tiny/large resize and cancellation during async enumeration. Asynchronous
I/O and paste safety are proposed implementation improvements; the interaction
contract above must remain visible and consistent.
