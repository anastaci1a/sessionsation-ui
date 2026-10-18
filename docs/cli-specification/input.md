# Input and terminal lifecycle

Reference: [select.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/select.bash)
(`ez_menu_choose`, `ez_menu_read_character`, `ez_menu_choose_cleanup`,
`ez_menu_refresh_view`) and [menu.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/menu.bash)
(`ez_select`). Form input deliberately differs; see [directory-picker](directory-picker.md).

## Chooser keys

| Input | Exact reference behavior |
| --- | --- |
| Up/Down | Wrap through enabled options, skip disabled and separators; leave Back focus and move from the remembered option |
| Enter or Space | Return selected stable menu key (index fallback); on Back return 130; on home Exit return literal `exit` |
| Left | Focus Back when the chooser has Back/Exit; otherwise no action |
| Right | Return from Back to remembered option; otherwise no action, never activate |
| Standalone Esc | Return 130 after waiting 80 ms for a continuation |
| Double Esc | Return 130 |
| CSI/SS 3 arrows | Both `ESC [` and `ESC O`, then sequence ending A/B/C/D; chooser accumulates until letter/tilde or 16 characters |
| Focus-in `ESC [ I` | Full geometry/foreground repair, preserve selection |
| Focus-out `ESC [ O` | Ignore |
| Ctrl-C | Signal exit 130 through cleanup |
| EOF/read failure | Return 130 unless a timeout (>128 status) merely triggers next frame |
| Ctrl-L and other ordinary characters | Ignored by chooser |

No explicit Home/End, Page Up/Down, Delete, mouse, click or bracketed-paste support
exists. Ctrl-D is explicitly checked while awaiting initial discovery and in
forms; normal chooser EOF follows Bash read/TTY behavior, not a separate key map.
The reference continuation parser waits 80 ms per character. A different escape
timeout or added convenience keys require a declared deviation, not an assertion
that they were copied. Bracketed paste protection is a proposed safety improvement
to discuss; reference pasted newlines can activate controls.

Choice initialization clamps an out-of-range selected index to 0 and scans forward
past disabled entries. Zero choices returns 1; zero enabled choices returns 130.
Selections returned to actions are strings/IDs, not terminal display text.

## Focus and refresh

Selected option persists when Back takes focus and returns. A changing backend
list preserves selection first by menu key, then by exact thread ID if the old key
disappeared after saved→live. If neither matches, select first enabled. Up/Down
clears a pending requested session selection. Labels, keys, thread IDs, accents,
durations, busy rows, spacers, descriptions and disabled indices all participate
in refresh change detection; unchanged snapshots do not reset geometry/animation.

Home and Sessions share the last complete validated snapshot in memory. Cold
discovery waits for a complete first watch snapshot without painting a partial
list. During that wait, Esc/Ctrl-D cancel and queued arrows/Enter are preserved in
a nominal 64-character buffer (an escape-prefix append can cross the bound by one).
The regular terminal input queue preserves subsequent typeahead. Watch readiness
or two-second deadline drives refresh. Do not add synchronous rescans on Back.

## Screen ownership

TTY mode requires stdin and **stderr** to be terminals and TERM not `dumb`.
UI frames go to stderr; stdout returns selected value/state. This is different
from the discarded redesign's stdout-only TTY assumption.

The top-level menu enters the alternate screen (`?1049h`) once and hides cursor
(`?25l`). Its chooser subprocesses reuse that shared screen. Chooser enables focus
reporting (`?1004h`), homes cursor, and on standalone entry clears screen. It saves
exact `stty -g` and disables echo/echonl **before** blocking discovery/rendering;
per-character `read -rsn1` supplies silent timed input. Form fields additionally
disable canonical input with `min 1 time 0`.

- INT exits 130; TERM/HUP exits 143. EXIT cleanup restores exact saved TTY state.
- WINCH marks geometry/full repair. CONT suppresses echo again and requests repair.
- Geometry also polls real `stty size` on clock ticks in chooser, covering missed
  resize signals/phone keyboard changes. Forms measure on their dirty-frame path.
- Shared chooser exit disables focus and resets styling; parent keeps alternate
  screen/cursor ownership between menu/form pages. Standalone exit also shows
  cursor and leaves alternate screen. A handoff temporarily exits that screen,
  then reenters on return; New Terminal finishes it entirely.
- Owned inventory watcher is closed and reaped on every chooser exit, including
  initial-discovery cancellation. This must never target backend durable workers.
- Terminal recovery repairs use final animated colors. Clock-only ticks and
  same-viewport navigation avoid whole-screen clears. No relative vertical cursor
  movement remains in the reference chooser.

The rebuild must retain exact cleanup before opaque process replacement and on
cancel, resize, EOF, signals and errors. It must also preserve HERDR's existing
retained shell and distinguish detaching a viewer from stopping an agent. Proposed
bottom-row drawing may change wrap modes; restoring them is an additional obligation.

## Non-TTY and dumb-terminal fallback

Print banner/page label, description, numbered options and `Choose [1-N] >`.
Home Exit control adds `0  Exit`; ordinary submenu fallback has no separate
numeric Back row. Invalid/unavailable choices print an error and keep reading.
Disabled rows retain strike/colors and indivisible notes; separators print a
bullet but cannot be chosen by number. EOF cancels. Live durations are computed
once for the printed menu. This fallback still emits configured ANSI styles; it
is not a plain/no-color protocol. Machine consumers must use backend JSON.

## Small terminals and character widths

Reference sizing uses Bash string lengths under the current locale; markers get
special one-cell handling. It does not implement general grapheme/wide-character
width. It clamps option capacity to at least 1 even when other rows cannot fit,
and clips strings rather than guaranteeing an all-size minimum-screen layout.
These are observed limitations. Target full-height controls need a deliberate
minimum-size/cancel behavior and cell-width handling; do not silently claim those
enhancements are reference behavior. Test real emitted ANSI, not only layout arrays.
