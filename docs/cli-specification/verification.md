# Reference verification and comparison method

This evidence concerns the pinned reference and reversible rollback, not a rebuilt
UI or Agent DM's native acceptance. Scope/source revisions are in [README](README.md).

## Checks run for this audit

On Satellite, 2026-10-18, the following completed with exit 0 against the unchanged
reference checkout. Every test uses stubs or explicitly isolated sockets/processes;
no model, account enrollment, live agent attachment or credential operation occurred.

| Command (from cli-main-menu) | Result |
| --- | --- |
| `bash tests/smoke.bash` | Passed: 3 reported groups |
| `bash tests/unraid.bash` | Passed: 1 group, redirected temporary installation only |
| `bash tests/backend.bash` | Passed: 2 groups |
| `bash tests/watch.bash` | Passed: 1 group |
| `bash tests/switcher.bash` | Passed: 1 group |
| `bash tests/session_focus.bash` | Passed: 1 group |
| `bash tests/launch.bash` | Passed: 6 groups, fake provider/isolated tmux socket |
| `bash tests/stars.bash` | Passed: 30 reported groups |
| `bash tests/selection.bash` | Passed: 7 identical animation/repair frames at each width 36, 80, 160 |
| `perl tests/terminal.pl` | Passed: 27 reported groups, many individual PTY scenarios |

Reported groups are the scripts' PASS lines, not equivalent numbers of individual
assertions. Logs remained in temporary files. Source and media were not modified.
The inherited C.UTF-8 locale is unavailable here; fixtures explicitly using C ran
that way and Perl reported fallback. This does not establish general wide-Unicode
behavior; source width limitations are in [input](input.md#small-terminals-and-character-widths).

The rolled-back UI passed **20 Node tests, 0 failures, 0 skips** and `npm run check`
(JS/Bash syntax, manifest and local documentation links). Installed runtime tree
was Git-equivalent to pre-redesign `f18979c` after revert commit `00af5a2`.
History was retained; reversing that revert is possible as a new reviewed change.
Existing environment/handle/latency fixes remain. HERDR configuration, UI settings,
selected launcher backend, pinned review backend and original Switcher SHA256
checks matched before/after source deployment. No HERDR reload, live process signal,
startup-file change or backend-data write was part of rollback. New terminals use
the old numbered menu; existing processes were not restarted. Historical redesign
evidence remains in Git/issue 4, not current docs.

## Reference responsiveness samples

One-run observations, not promises or a before/after improvement claim:
`perl tools/benchmark-selection.pl` at 80×24: 70 transitions, median 8.128 ms,
p95 18.189 ms, max 36.852 ms, mean 1457.24 output bytes/transition. It exercises arrows
through a sweep without a backend; timing ends at observed selected-label output.

`perl tools/benchmark-navigation.pl` at 80×24 uses a fake backend with 400 ms
discovery delay and inert actions. Measured milliseconds:

| Transition | ms |
| --- | ---: |
| Initial home | 641.8 |
| Home→Sessions | 117.4 |
| Sessions→Home | 253.8 |
| Reopen Sessions | 119.5 |
| New name field | 54.1 |
| Name→directory | 102.7 |
| Directory→account | 104.9 |
| Cancel accounts→Sessions | 164.2 |
| Return Home | 251.6 |
| Exit | 85.5 |

Six inventory commands occurred. Samples were collected with other isolated checks
active and exclude provider startup, HERDR transport and perceived paint completion.
They show why parity and responsiveness need separate tests. The deployed backend's
prior latency repair evidence remains in [latency.json](../latency.json).

## What the supplied tools prove

- `benchmark.bash`: 240 ticks at 50 ms over 12 seconds, seeded 1967. Optional
  BENCH_SCENARIO changes selection, scroll, width and late time jump; BENCH_JITTER
  adds irregular intervals; WARMUP/TIME_OFFSET/HUE_STEP exercise long runtimes.
  Optional reference stars module permits differential checks.
- `benchmark-foreground.bash`: 40 full-menu repairs at 250 ms over 10 seconds, including
  command substitution; optional tick cost/reference renderer. Neither invokes a
  backend. Captures record NUL-delimited time, delta, status-color, repair.
- `compare-frames.cjs`: compares glyph/RGB/bold/strike cells after accumulated
  deltas, exact status color and fresh full repair. Its small ANSI subset ignores
  ordinary blank color and has no full terminal autowrap/grapheme/mode model.
  Extend the comparator before claiming bottom-row parity.
- `benchmark-summary.cjs`: mean/p95 ms and mean bytes split into idle/sweep using
  fixed 4000/1467 defaults. Custom timing needs matching classification.
- `benchmark-sessions.pl`: fake read-only backend; entry, bytes, label paints and
  inventory count. Navigation benchmark covers page transitions; selection covers
  keys without backend.
- `benchmark-open.bash`: installed legacy Switcher, isolated home/socket and copied
  sleep named Codex; watcher shutdown/open/extract/tmux preparation timings. It was
  source-audited, not executed here. It is neither model nor sessionsation API
  acceptance and must never target Agent DM's root/pinned disposable.

## Media and preview pipeline

The reference PNG was visually inspected: full-width status bar, centered block
title, left-aligned centered option group, bright selected label/account accent,
Back beside first option, hints below list and unused lower terminal area.
This validates the source-layout finding; it is not Ana's target geometry.
Binary metadata inspection confirms the checked-in GIF has 201 frames at 50 ms
and both media assets are 920×520. No media was copied or regenerated in this audit.

`preview-frames.bash` selects an installed UTF-8 locale (fails if absent), sets
80×24, seed 1967, fixed date/host/path, fake availability, reads actual menu labels
and emits 201 NUL-delimited full ANSI frames at 0..10000 ms/50 ms. It never invokes user
backend/jobs. `render-preview.cjs` validates UTF-8, interprets its ANSI subset,
paints DejaVu Sans Mono regular/bold at 17 px in 11×20 cells with 20 px padding on
background(9, 11, 17), giving 920×520. PNG captures frame 48 (2400 ms); GIF uses 128-color
palettes, 50 msdelay and repeat 0. Its simplified code-unit decoder/fixed size are not
a substitute for real-terminal tests.

Optional tooling pins `@napi-rs/canvas`1.0.9 and `gifenc`1.0.3, with platform canvas
optional packages and MIT metadata in lockfile v3. No live runtime dependency is
implied. GitHub workflow installs with scripts/audit/fund disabled plus DejaVu fonts
on Ubuntu 24.04/Node 22, regenerates for relevant changes/dispatch, serializes runs,
checks main has not advanced, and commits only PNG/GIF as github-actions bot.
Remote's newer commit changes only those assets.

## Rebuild comparison method

Verification requirements below are for future work; the issue tracks completion.

1. Inject clock/random draws. Reproduce reference geometry first; compare cells at
   pre-sweep, arrivals, peak, fade endpoints, twinkle collision/retirement/replacement
   and idle. Include custom durations, skipped/backward times, long cycles and cache
   rollover. Selection/focus preserve random consumption; population/cache bounds
   are invariants.
2. Apply only explicit geometry changes separately. Captures must show row H stars/
   controls at multiple sizes/list lengths, including short lists in tall terminals.
   Decode actual emitted ANSI with wrap/scroll accounted for, including bottom-right
   cell. Selection repairs must equal full paint without resetting hue/population.
3. Real PTYs: CSI/SS 3 and fragmented/queued input, chooser versus form Space/Enter,
   Back, child accept versus browse, parent/history/root, inline prefix editing,
   hidden/symlink paths, mkdir failure/cancel and exact TTY restoration. Resize
   during scroll/edit/I/O/sweep; test EOF/INT/TERM/HUP, focus and output backpressure.
   Any added paste handling is an explicit deviation.
4. Inert backend fixtures assert account-list-only New, stable-ID/generation Resume,
   stale completion rejection, literal opaque plans and no automatic retries.
   Measure every transition and backend-call count, not spinner visibility.
5. Review actual HERDR viewer appearance and detach/cancel before claiming that
   host's visual acceptance. This audit does not open/control a live viewer. No
   models or repeated Queue submissions are needed for visual verification.

Documentation-only updates run `npm run check` plus source-map/link/anchor validation,
not the entire backend suite. Current tests do not prove an unbuilt target matches.
