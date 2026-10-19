# Visual layout and styles

Coordinates below are terminal rows starting at 1 and columns starting at 0.
Integer divisions truncate. `W/H` are terminal width/height, `B` is the number of
fitted banner rows, `V` visible options, `N` all options and `K` wrapped hint rows.
This convention follows the reference's cell index `(row-1)*W+column`.

Sources: [render.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/render.bash),
[select.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/select.bash),
[theme.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/theme.bash).
Function names below are searchable source anchors, not inferred design intent.

## Bars and reference title

`ez_menu_status_text` fills row 1 to width W. Format is a leading space, local
`%a %b %d  %H:%M:%S`, ` | `, hostname before its first dot truncated to 12 characters,
` | `, working directory. Home becomes `~`, its descendants `~/...`. Available path
width is W minus that prefix minus one. When greater than 3, a long path becomes
`...` plus its rightmost characters; otherwise show only the leading space and
clock. Finally clip/pad to W. Clock updates once per second. Status is white,
not bold; background color follows the animation's soft accent.

`ez_menu_screen_header` puts a submenu/form label on row 2: one leading space,
title clipped to W-1, padded to W, white and bold, secondary animated background.
Optional descriptions start at row 4 with two-space indentation, gray, clipped to
W-4. Their complete rows are excluded from stars. Document pages wrap separately.

The reference home uses `ez_menu_banner` and
[font.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/font.bash):
five rows of `#` glyphs for uppercase A–Z, digits, space and hyphen, with one-column
inter-letter gaps. Unknown characters fall back to the whole plain title. Control
characters become spaces. Default `MAIN MENU` (local overrides excluded). Two blank
star-margin rows above/below, plus leading/trailing blank rows, surround centered
lettering. Animated banner generation creates placeholders; final cells paint it
without an original pink underlay. Width below title width+4 or insufficient
height switches to one-line clipped title; margins then shrink from 2 toward 0.
**None of this decorative title or its reserved height belongs in the rebuild.**

## Option geometry

`ez_menu_option_layout` centers one fixed-width block and left-aligns all labels:

1. Marker width is always 1, even with 100 entries. Available label width is W-3.
2. Measure the longest base label. A disabled note contributes only when the full
   label, one space and full note fit available width; notes are indivisible.
3. Clamp label width to available width, then at least 1. Block width is label+2.
4. Left indent is max(0, floor((W-block width)/2)); right padding fills the rest.

Home `Codex: Resume (...)` / `Start (...)` excludes the preview suffix from the
centering measurement. Selected-only durations and gray status suffixes also do
not recenter a row; an always-visible timer reserves `9999:23:59:59`. Accent/gray
suffix rows can extend rightward to W-indent-3, leaving one right-edge cell.

In the reference, options begin at row B+3, one row per entry. Available rows are
max(1, H-B-K-4). V=min(N, available rows). Scroll just enough to keep the selection
visible; wrapping to last/first adjusts that window. If V<N, the row immediately
below options shows centered `first-last of N` using one-based indices. Hints
start at B+V+4. Their group is centered by its longest line, each line left-aligned.

## Foreground styles

| Element | Glyph/style | Color behavior |
| --- | --- | --- |
| Enabled, unselected | `○`, regular label | Accent marker, gray label |
| Selected option | `●`, bold marker and label | Accent marker, white label |
| Disabled | `○`, struck marker and base label | Dim accent marker, disabled gray label; note unstruck |
| Separator | `•`, not selectable, regular | 60% accent at all times |
| Account/preview suffix | Unbolded colored text | 100% selected, 60% unselected or Back-focused; sweep may bold at peak |
| Gray suffix/timer | Regular | Gray, never promoted to selected white |
| Back unfocused | `◀—` | Gray |
| Back focused | `◀— Back` / `◀— Exit`, bold if room | White; options lose selection highlight |
| Hints | Regular | Soft accent, 55% brightness; shared sweep, no bold flash |

Back is originally beside the **first visible option**, not at screen bottom.
Unfocused column is max(0, indent-3). Focus expands left to indent-8 if indent≥8;
otherwise it stays short. The draw call requires indent>0. On Back focus, enabled
markers dim to 60%, labels become gray, selected-only durations disappear; activity
spinners dim too. Returning Right restores the previous option.

Indexed static palette: white 255 `(238,238,238)`, gray 250 `(188,188,188)`,
disabled 242 `(108,108,108)`, pink 177 `(215,135,255)`, red 160 `(215,0,0)`, orange 214
`(255,175,0)`, green 40 `(0,215,0)`, blue 69 `(95,135,255)`. Disabled accent is truecolor
`(129,81,153)`, exactly 60% pink. Star colors 103 `(135,135,175)`, 146 `(175,175,215)`,
139 `(175,135,175)`; static status background 60 `(95,95,135)`. Some of these action
colors are helpers rather than visible home options. Animated accents derive from
the star palette, not a continuous rainbow of every option label; see [motion](motion.md).

## Stars and text masks

`ez_stars_layout` begins at row 3 and originally ends at
min(last hint row+2, H-1), leaving the final terminal row unused to avoid scrolling.
This explains why the existing reference image has unused space below its field.
Titles reserve their width plus two columns each side; page indicator reserves
its text span. Do not confuse that geometry with the target below.

Foreground non-space glyphs win over stars. Ordinary label spaces are transparent
to transient twinkles but **baseline stars are excluded** from those spaces and
the marker-to-label gap. Baseline stars newly blocked by scrolling are relocated
through the replacement queue. A baseline hidden beneath a foreground glyph can
survive and reappear. Disabled-label spaces are foreground too, preserving a
continuous strike. Gray suffix spans, descriptions and the Back region are
occluded. Hint spaces remain transparent. Accent glyphs are separate animated
text cells and never twinkle. There is no outline/border around the star field.

Static mode differs: banner and option-side stars use random `*.+` drawn from
`*.+` plus 21 spaces; three muted palette colors. Three-cell empty gutters flank
options. Side brightness is `65-60*visibleRow/V`. It is cached until geometry changes.
The source's form widget always enables animation even when chooser animation is
disabled; this is an observed inconsistency, not a new opt-out promise.

## Hints

`ez_menu_hint_lines` packs tokens with three spaces between them into W-4 columns.
Exactly four tokens first try one line, then a two-column/two-row arrangement
(column width=max(length(token 1), length(token 3))). Otherwise wrap greedily;
an individual too-long token is clipped. No Ctrl-L hint is shown.

- Home with Exit control: `▲/▼/◀/▶: Move`, `Enter/Space: Select`, `Esc: Exit`.
  Without Exit control, show Up/Down only when more than one enabled option exists.
- Submenu: four arrows if more than one enabled option, otherwise Left/Right;
  followed by Select and Esc: Back.
- Name: `Enter: Accept` only while valid, then `Esc: Back`, `Ctrl-U: Clear`.
- Directory: `▲/▼: Choose` if children exist or path valid/creatable;
  `▶/Tab: Open`, `◀: Parent` if children exist; `Enter: Select` if useful;
  always `Esc: Back`, `Ctrl-U: Clear`.

## Target geometry

Required: omit home title and its reserved rows, retain functional bars/form
fields, draw stars down to row H, and anchor wrapped key hints to the
terminal bottom. Back sits beside the first visible option and expands on Left;
on the main page its label is Close. Start follows the summary fields and separator.
List length must affect only its viewport, never field bottom,
density depth or fade endpoint. Shrink/grow must reflow controls without losing
selection, picker path, cursor or history. Long lists scroll above reserved controls.

Proposed coordinate contract for dot's review: row 1 status; row 2 functional label
only on subpages; form input row 4; options row 4 on home/submenu and row 6 on forms.
Reserve K bottom hint rows and rows for status/pagination when needed. Compute capacity from this reservation,
not from option count. Use `stars_top=3`, `stars_bottom=H`; exclude occupied control
glyphs through masks, allowing the field through their spaces. The ordinary fade
starts at options but reaches its endpoint relative to H; density and shimmer
peak depth use the entire field. Empty-title coordinates and too-small-terminal
behavior need reviewed golden examples before coding.

Painting row H must not scroll. The future renderer must address row boundaries
explicitly, handle right-edge wrapping deliberately and restore any mode it changes.
The old H-1 workaround does not satisfy this requirement. A synthetic preview's
background is not terminal styling: the reference PNG uses `(9,11,17)` as its canvas
background, while the live renderer normally uses the terminal's default background.
