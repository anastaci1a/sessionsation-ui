# Animation: time, color and state

Authoritative reference:
[stars.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/stars.bash),
[defaults](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/config.example.bash),
[deterministic assertions](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/tests/stars.bash).
All formulas use integer arithmetic with truncation toward zero. RGB channels are
0–255; saturation, progress, intensity and white mix use 0–1000 unless noted.
Do not replace these rules with approximate sine waves or continuously changing hue.

## Configuration and clock

| Setting (`EZ_MENU_` prefix) | Default | Reference validation/effect |
| --- | --- | --- |
| `ANIMATE_STARS` | 1 | Chooser animation enabled only for value 1 and a main/subpage banner; forms initialize it unconditionally |
| `TWINKLE_ADVANCE_MS` | 500 | 1–5 decimal digits including zero; invalid→500; advances whole probability envelope |
| `TWINKLE_RATE_PERCENT` | 250 | Positive up to four digits; invalid→250; capped 1000 |
| `STAR_DENSITY_PERCENT` | 50 | 1–3 decimal digits; invalid→50; cap 100; zero removes initial baseline, not twinkles |
| `HORIZON_DENSITY_PERCENT` | 600 | Positive up to four digits; invalid→600; clamp 100–800 |
| `SWEEP_INTERVAL_MS` | 4000 | Positive up to six digits; invalid→4000; raised to at least duration+1800 |
| `SWEEP_DURATION_MS` | 1467 | Positive up to five digits; invalid→1467; minimum 300 |
| `SWEEP_HUE_STEP` | random | 1–3 decimal digits select fixed degrees (including 0); everything else selects random |
| `STAR_SATURATION_MAX` | 800 | 1–4 digits; invalid→800; cap 1000; per-star lower bound may override it |
| `STAR_HUE_SPREAD` | 60 | 1–3 digits; invalid→60; cap 360 |
| `SWEEP_ACCENT_OFFSET` | 180 | 1–3 digits; invalid→180; subsequent hue math wraps mod 360 |

`ez_stars_now` reads `/proc/uptime` as monotonic milliseconds, falling back to
`EPOCHREALTIME`, then `SECONDS`. Each chooser/form owns mutable animation state
and its own origin. Selection, focus and clock ticks preserve that state; a new
screen creates new state. The shared parent caches only immutable timing curves,
keyed by validated sweep duration. Geometry rebuilds may regenerate the field.

Rendering targets 20 FPS: next input wait=max(1 ms, 50 ms-frame computation time).
There is no animation subprocess or worker. Slow frames skip time; they do not
replay every missed tick or create a burst of overdue twinkles. The clock updates
once per second independently of animation. The source also requests a full
recovery repair every five seconds for terminals missing focus/continue events.

## Baseline distribution and brightness

Let top/bottom be the field bounds in [visuals](visuals.md), depth=
`(row-top)*1000/(bottom-top)` (zero if height is one), and Dmax=
`HORIZON_DENSITY_PERCENT*10`. Row density is:

```text
D(row) = 1000 + (Dmax-1000)*depth*depth/1000000
baseline present iff random30bit % 800000 < D(row)*STAR_DENSITY_PERCENT
```

At defaults this is 6.25% at top and 37.5% at bottom before masks. Random 30 bit is
`RANDOM*32768+RANDOM`; do not confuse 50% baseline multiplier with 50% occupied cells.
Glyph is uniformly one of `*`, `.`, `+`; palette index uniform 0–2. Baseline masks
exclude option spaces/gaps; foreground and page/title masks have separate roles.

Ordinary fade is 100 above options except the blank interval just before options
(80). From option-start S to bottom B, fade percent is `65-60*(row-S)/(B-S+1)`.
The exclusive endpoint lies one row beyond the drawn field, so the last row is
still greater than 5%. For the **sweep's white endpoint only**, peak is
`clamp(10000*(B-row+1)/(7*(B-top+1)),4,1000)`: upper 30% can reach white, lower 70%
approaches black without reaching it. Twinkle-only peaks still use ordinary fade.
Rebuild uses terminal-height bounds per Ana, preserving these distinctions.

## Palette and hue changes

`ez_stars_init` converts the three theme star RGB/indexed colors to HSV using
integer max/min/delta. It averages hues relative to the first hue across the
360° boundary, then gives all three star palettes that common center; original
saturation/value remain per palette. Default center is 260° from individual
240°, 240°, 300°; default accent center is 80°.
The exact computation, rather than a manually picked accent, is authoritative:
`h0 + sum((hi-h0+540)%360-180)/3`, modulo 360.

Each star draws saturation uniformly between its palette saturation and
max(that minimum, configured maximum). Hue offset uses the sum of **twelve**
`RANDOM%1000` draws minus 5994, rejects outside[-3000, 3000], then multiplies by
spread/6000. This makes a Gaussian-like distribution bounded to ±spread/2;
clamping a uniform distribution is not equivalent. Saturation and offset persist
for that star's lifetime.

Palette 3 (markers/title/account accents) uses center+offset, saturation 700,
value 255. Palette 4 (hints/bars) shares that hue at saturation 350, value 255. All
palettes receive the same cumulative rotation. Random mode draws one uniform
integer 60–120° increment per sweep using an independent seeded 32-bit LCG:
`state=(1664525*state+1013904223)&4294967295`; reject state≥4294967239, then
`step=60+state%61`. Retain only three rotations; replay seed for older seeks.
Fixed mode uses `cycle*step`. No per-star independent hue-cycle randomization.

HSV conversion uses chroma=`value*saturation/1000`, secondary=
`chroma*(60-abs(hue%120-60))/60`, minimum=value-chroma, mapped to six sectors.
RGB is cached by palette/cycle/saturation/offset, with zero RGB a valid value.

## Diagonal shimmer

Cycle=floor(elapsed/period), phase=elapsed%period. Cycle 0 has **no sweep**;
first sweep begins at 4000 ms with defaults. Each later cycle sweeps while
phase<duration. Duration 1467 ms = travel 1167 ms + flash 300 ms. Flash rise 60 ms and
tail 240 ms stay fixed unless duration<600: then flash=duration/2, rise=flash/5,
tail=flash-rise. Ordinary easing is smoothstep:
`S(x)=x*x*(3000-2*x)/1000000`, x clamped 0–1000.

Wave travels top-left to bottom-right. A cell's normalized distance is
`(col*1000/max(W-1,1)+(row-top)*1000/max(sweepBottom-top,1))/2`.
Arrival uses an inverse table of this special broad-shoulder curve, not smoothstep:

```text
p = clamp(progress, 0, 1000)
reflect = p > 500; if reflect: p = 1000-p
if p <= 400: q = p*p*25
else: u = p-400; q = 4000000 + 2*(u*10000 + u*u*u)
if reflect: q = 16000000-q
distance = q/16000
```

Build arrivals by stepping progress 0..1000 and assigning each newly reached
distance `travel*progress/1000`. First and last quarter of distance each take 40%
of travel; center half takes 20%. At defaults quarter arrival≈466 ms, midpoint 583 ms,
three-quarter≈700 ms, endpoint 1167 ms. Peak speed is preserved from the older
1000 ms sweep; changing only total duration with a generic easing would differ.

Local phase=phase-arrival. Before arrival or during rise, use previous hue cycle;
at the white peak switch to new cycle. White mix rises through S(local/rise), then
falls through S(1-(local-rise)/tail). Star style is bold when sweep white≥900.
Accents marked flashable also become bold then; disabled markers retain strike,
hints retain normal weight. Glyphs do not travel; a light wave crosses fixed cells.
After passage, new hue stays until the next sweep.

Top status bar uses palette 4 at 55% brightness. Secondary page bar uses the same
palette with negative accent-offset (star-side hue). Both interpolate their full
background RGB from previous to next over the **whole duration** with the broad
curve. They never receive the stars' white flash.

## Twinkles and replacement stars

`ez_stars_twinkle_weight` gives a smooth 20–100% opportunity probability:

```text
phase = (elapsed-duration/2+advance+period)%period
x = phase*1000/period; product=x*(1000-x)
sine = 16000*product/(5000000-4*product)
weight = 200+800*sine*sine/1000000
```

Baseline deadline is random 1..`50000/rate`ms (default 1..200). At deadline,
`RANDOM%1000<weight` permits a birth. Uniform stream chooses a random starting
candidate then searches cyclically for a nonforeground, nonactive-twinkle cell;
it may consume a baseline star. A second stream adds horizon-weighted opportunities
from density above 1000. Its interval is
`max(1,50000000*eligibleCount/(rate*sum(D(row)-1000)))`; absent excess means disabled.
It rejection-samples by excess density and only runs if baseline stream did not
spawn. **At most one twinkle birth per frame**, baseline gets priority.

A twinkle lasts 900 ms: intensity smoothstep from 0 to 1000 over 120 ms, then down to 0
over 780 ms. Glyph is `.` below 250, `+` below 650, otherwise `*`. Twinkle white mix equals
intensity; it is not independently bold. Sweep white combines as
`1-(1-twinkleWhite)*(1-sweepWhite)` and can add bold. Its color is scaled by the
star's intensity and row fade; sweep white additionally uses the special peak
curve. Tests distinguish overlapping shimmer/twinkle from standalone twinkles.

At retirement the glyph is cleared permanently at that coordinate. Consuming a
baseline owes one replacement elsewhere; an empty-cell birth owes none. A bounded
queue retries replacement if no eligible cell exists. Up to 32 attempts per tick:
different from origin, no existing star/birth, not baseline-blocked, weighted by
row density/Dmax. Replacement chooses fresh glyph/palette/saturation/hue and
fades **color intensity only** through smoothstep over 500 ms, with no separate white
flash. Crossing shimmer still applies. Hidden fades retire; consuming a fading
replacement transfers its single debt. Population accounting includes present
baseline, consumed baseline debts and queued replacements.

## Repairs and performance invariants

- Selection modifies marker glyph/weight and accent fade only. No star respawn,
  random draws or full-screen repaint on ordinary arrows. Back can rebuild text
  masks while retaining animated stars; scroll relocates newly blocked baseline.
- Geometry/text-mask changes rebuild bounded 16 ms arrival buckets. Buckets choose
  work; exact per-cell millisecond arrivals determine color. Active births,
  replacements, dirty text and crossing-band cells update; settled cells emit nothing.
- Full and partial repairs use the same final `RGB:style:glyph` tokens as ticks.
  No blank field/original-pink underlay. Repairs suppress redundant deltas.
- Idle prefetch computes next colors in 32–128-cell slices, holding only current/
  next RGB per cell. Cycle tags modulo 16384 clear on epoch change. No unbounded
  history, stale sweep entries or per-frame external processes.
- Emit cells in coordinate order, explicit row positioning, batched style/color
  runs and relative horizontal movement. Reset strike before blank gaps; preserve
  complete UTF-8 markers. Sparse/dense repair equivalence is tested at narrow sizes.

The language port must preserve these state transitions and distributions. A
different PRNG must be an explicit decision; identical supplied random draws and
time inputs should yield identical semantic cells in the compatibility geometry.
