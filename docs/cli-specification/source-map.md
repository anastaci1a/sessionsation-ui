# Complete reference source map

All 47 tracked files at the pinned reference commit were inspected. This table is
the single coverage inventory; behavior lives in the linked purpose documents.
Every source link is immutable. Remote main differs only in the two generated
media files, as recorded in [README](README.md).

| Source | Audited purpose | Specification |
| --- | --- | --- |
| [switcher.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/switcher.bash) | Validated legacy JSON boundary, account status, partial watch snapshots, owned-stream cleanup and exact identity lookup. | [flows](flows.md) |
| [.github/workflows/menu-preview.yml](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/.github/workflows/menu-preview.yml) | Preview CI on main: path filters, serialized runs, Node 22, fonts/deps, regenerate, no stale-head push, bot-only asset commit. | [verification](verification.md) |
| [.gitignore](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/.gitignore) | Editor artifacts, private config.bash, optional tools/node_modules exclusions. | [flows](flows.md) |
| [AGENTS.md](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/AGENTS.md) | Loading, backend, renderer and safe-test invariants; local Git identity and preview workflow. | [verification](verification.md) |
| [README.md](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/README.md) | Public setup, interactions, customization, timing and legacy contract entry points. | [flows](flows.md) |
| [actions.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/actions.bash) | Shell return, Herdr handoff, dashboard reuse, exact environment/exec bootstrap, shell jobs. | [flows](flows.md) |
| [bashrc.example](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/bashrc.example) | Source-path resolution and alias loading without starting UI. | [flows](flows.md) |
| [config.example.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/config.example.bash) | All tracked title/animation defaults, private override guidance. | [motion](motion.md) |
| [font.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/font.bash) | Five-row A–Z/0–9/space/hyphen font, sanitization, unsupported-character fallback; excluded decorative title. | [visuals](visuals.md) |
| [init.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/init.bash) | Module order, private overrides, startup alias, loader cleanup. | [flows](flows.md) |
| [install-unraid.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/install-unraid.bash) | Root/persistent-path checks, backups, permissions, settings, guarded boot hooks. | [flows](flows.md) |
| [media/menu-demo.gif](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/media/menu-demo.gif) | Generated 80×24 ANSI-derived motion preview; illustrative, not live acceptance. | [verification](verification.md) |
| [media/menu-preview.png](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/media/menu-preview.png) | Generated still from frame 48; visually inspected; no asset imported. | [verification](verification.md) |
| [menu.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/menu.bash) | Dynamic home entries, shared screen, refresh/actions aligned, return/exit semantics. | [flows](flows.md) |
| [render.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/render.bash) | Bars, clipping, hint wrapping, static stars, centered labels, suffixes, Back, masks/repair layout. | [visuals](visuals.md) |
| [select.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/select.bash) | Chooser keys, disabled skip, scrolling, focus/resize/signals, timers/spinners and watch refresh. | [input](input.md) |
| [session-menu.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/session-menu.bash) | Names/accounts, documents, grouped inventory, state exchange, exact open/attach, new flow. | [flows](flows.md) |
| [sessions.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/sessions.bash) | Inline text/path forms, expansion, directory validation, creation, history and form keys. | [directory-picker](directory-picker.md) |
| [stars.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/stars.bash) | Complete animation engine: timing, probability, masks, palette, replacement, caches and emit/repair. | [motion](motion.md) |
| [tests/backend.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/tests/backend.bash) | Malformed schema/offline/unknown/provenance and safe open-target validation; timing privacy. | [verification](verification.md) |
| [tests/fixtures/terminal.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/tests/fixtures/terminal.bash) | PTY driver modes; fake backend/tmux/Herdr; owned disposable shell jobs; fixed visual settings. | [verification](verification.md) |
| [tests/launch.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/tests/launch.bash) | Real isolated tmux socket with fake provider; exact argv/env/cwd/PID, retained failure and attach ordering. | [verification](verification.md) |
| [tests/selection.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/tests/selection.bash) | Full versus partial selection ANSI cell comparison at widths 36/80/160, 7 frames each. | [verification](verification.md) |
| [tests/session_focus.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/tests/session_focus.bash) | Group/recency order, nickname alignment, prefix hiding, identity/thread focus, disconnected state. | [verification](verification.md) |
| [tests/smoke.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/tests/smoke.bash) | Syntax, relocation/spaces, repeated-source safety, defaults/override, paths, durations, responsive title/hints. | [verification](verification.md) |
| [tests/stars.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/tests/stars.bash) | Deterministic animation, distributions, geometry/masks, cache bounds, population and repair equivalence. | [verification](verification.md) |
| [tests/switcher.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/tests/switcher.bash) | Dashboard reuse, advisory capacity, original-account opening, exact ID and no extra preflight. | [verification](verification.md) |
| [tests/terminal.pl](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/tests/terminal.pl) | Real PTY keyboard/form/focus/resize/cleanup, live fixture watch, selection styling and handoffs. | [verification](verification.md) |
| [tests/unraid.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/tests/unraid.bash) | Redirected fixture boot restoration, idempotence, preserved shell behavior and path/link setup. | [verification](verification.md) |
| [tests/watch.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/tests/watch.bash) | Partial-line snapshots, EOF/error, last valid state and owned watcher cleanup. | [verification](verification.md) |
| [theme.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/theme.bash) | Indexed colors, truecolor disabled accent, strike/bold and star/status palette. | [visuals](visuals.md) |
| [tools/benchmark-foreground.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/tools/benchmark-foreground.bash) | Full repair cost with real command substitution; optional tick and reference renderer. | [verification](verification.md) |
| [tools/benchmark-navigation.pl](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/tools/benchmark-navigation.pl) | Whole-page PTY transitions, deliberately slow fake discovery, backend-call count. | [verification](verification.md) |
| [tools/benchmark-open.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/tools/benchmark-open.bash) | Installed Switcher on isolated fake-Codex/tmux fixture; stage timings, not native/model acceptance. | [verification](verification.md) |
| [tools/benchmark-selection.pl](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/tools/benchmark-selection.pl) | 70 arrow transitions across idle/sweep, elapsed latency and output bytes. | [verification](verification.md) |
| [tools/benchmark-sessions.pl](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/tools/benchmark-sessions.pl) | Fixture Sessions entry latency, output volume/option paints/discovery count. | [verification](verification.md) |
| [tools/benchmark-summary.cjs](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/tools/benchmark-summary.cjs) | CSV sweep/idle mean, p95 and bytes; assumes default 4000/1467 timing. | [verification](verification.md) |
| [tools/benchmark.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/tools/benchmark.bash) | Seeded 12 second/50 ms ticks, jitter/layout/scroll/resize/time-jump scenarios, capture stream. | [verification](verification.md) |
| [tools/compare-frames.cjs](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/tools/compare-frames.cjs) | ANSI semantic cell/color/bold/strike comparator and repair equality; not a complete terminal emulator. | [verification](verification.md) |
| [tools/package-lock.json](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/tools/package-lock.json) | Lockfilev 3, canvas 1.0.9 optional platform binaries and gifenc 1.0.3; MIT dependency metadata. | [verification](verification.md) |
| [tools/package.json](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/tools/package.json) | Optional preview-only canvas/gif encoder dependencies, no live animation dependency. | [verification](verification.md) |
| [tools/preview-frames.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/tools/preview-frames.bash) | UTF-8 locale check, real seeded renderer/menu labels, inert snapshot, fixed date, 201 frames. | [verification](verification.md) |
| [tools/render-preview.cjs](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/tools/render-preview.cjs) | ANSI-to-canvas decode, DejaVu fonts, 920×520 PNG/GIF, 50 ms frames, indexed-color handling. | [verification](verification.md) |
| [unraid/login.bash](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/unraid/login.bash) | Direct interactive-login TTY/outside-tmux/start-once guard. | [flows](flows.md) |
| [unraid/profile.sh](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/unraid/profile.sh) | Idempotent PATH, exported settings/homes, Bash-only source, never opens UI. | [flows](flows.md) |
| [unraid/restore.sh](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/unraid/restore.sh) | Restore config symlink/profile and guarded shell blocks without pre-mount appdata creation. | [flows](flows.md) |
| [unraid/start-switcher.sh](https://github.com/anastaci1a/cli-main-menu/blob/ae786af695791fdf858eacf5780e04b5138be49d/unraid/start-switcher.sh) | Array-ready bounded wait and original daemon startup; excluded from target. | [flows](flows.md) |

The ignored two-line `config.bash` was checked only for effective visual overrides:
local title `SATELLITE`. It is not redistributed. Dependency installations under
`tools/node_modules`, Git internals, user shell files, credentials and account
stores are not project source. Lockfile/package declarations were inspected;
no dependency installation or provider invocation was needed for this audit.

The reference tree has no LICENSE file. This specification records provenance
without importing source/assets; attribution and license for a literal port are
an explicit [architecture discussion](architecture.md) item.
