# Verification

## Rust popup

The expanded popup passes **132 Rust tests** (130 unit tests and two tests covering
21 complete cell goldens), Clippy with warnings denied, formatting, and **22 Node
regressions**. Syntax, manifest fields and documentation links pass. The goldens
exercise fixed time/seed/geometry, wide graphemes, text masks, disabled strike,
shared palette/shimmer, reference Back/Close placement and fixed field geometry.
Management goldens cover name/bar focus, inline/compact accounts, limit markers, action separators,
aligned mixed-window picker usage and themed confirmation dialogs. The rendered
[summary preview](assets/popup-summary.png) was inspected against the reference;
random star coordinates intentionally use a deterministic Rust generator.

`scripts/test-terminal.py --binary PATH` uses isolated fake APIs and real PTYs:
20 process startups, 238 key events, two-second backend delays, summary edits,
bracketed paste, Close/Right focus restoration, unset Space without focus, staged directory/request identity, resize/cancel and exact TTY
restoration. The shared-directory extension verifies warning, Cancel without
submission, and confirmed launch with a synthetic inactive session in the same
directory. ANSI updates are decoded into cells. Simultaneous resize/input exposed
an intermittent key-readiness loss in the default Crossterm Mio reader; the
level-triggered TTY reader passes this regression. The cell decoder also recovers
at the next ESC when HERDR interrupts an output sequence during popup teardown.

`scripts/test-herdr.py --binary PATH` runs against installed HERDR 0.9.5 inside a
network-disabled container, with separate config/socket/state and a synthetic public
backend. It verifies the requested percentage popup geometry, origin context without a pane ID,
selection of a different Space, normal-tab handoff, focus before exit, second-popup
cancellation, held-arrow clamping/release-and-press wrapping through the actual
HERDR pane encoder, Home/End list jumps, and unchanged origin shell. The isolated host fixture (`13c72d2`) moves on the first press alone in 7.53 ms, before any repeat or release.
A boundary-origin hold wraps once and then clamps; the core tests both axes and
all starting positions. Mixed ordinary/enhanced bursts clamp at both boundaries.
Native release/press rearms immediately; press-only streams use the documented
150 ms fallback and cannot guarantee physical hold boundaries. Twenty actual prefix+uppercase-L openings
passed. Separate backend fixtures cover exact account/UUID/socket handoff, normal exit,
preserved-history resume after local account deletion and the pinned-original
supervisor handoff. Their details and limits belong to the backend guidance.
UI acceptance uses synthetic APIs and starts no model. Backend native acceptance
is documented separately; Agent DM pins/data remain unchanged. Production
capability/status previews are read-only.

PTY/host measurements for source `13c72d2`, p95: first responsive PTY paint
**6.61 ms**, key feedback **0.542 ms**, keybind-to-popup paint **190.77 ms**.
Frame computation/diff measured **0.123 ms** at 80×24 and **0.384 ms** at
160×48 (600 frames each; renderer source `7f048ed`). These exclude provider
startup/model response. External CLI plugin invocation took 193.18 ms; the
configured key path met 250 ms. Frame timings remain specific to their stated renderer source.

`scripts/test-session-activity.py --binary PATH` verifies the cold loading skeleton,
persistent spinner, live busy/idle/exit/error changes, held-arrow boundaries and
release/press wrapping with synthetic APIs. Refresh visibility is approximately
one second. No model is started.

Run `scripts/check-rust.sh`, `npm test` and `npm run check`. The PTY/host scripts
require Python 3; use the existing offline build container, mount only this checkout
and the HERDR executable at `/usr/local/bin/herdr`, and pass the release executable.
Never mount production homes, sockets or account roots into these fixtures.
`UPDATE_GOLDENS=1 cargo test --test visual` deliberately regenerates cell fixtures;
ordinary checks compare them unchanged. Deployment/provenance is recorded separately
in [deployment.json](deployment.json). [Machine-readable results](popup-evidence.json) include exact test counts, timing summaries and preserved-process checks.

## Preserved legacy client and limits

The 22 Node tests include existing literal argv, stale generation/version errors,
account removal confirmation, selected-session switching, viewer exit, retained
shell, default/named HERDR environment, and rollback/concurrent-edit protections.
The [earlier launcher timing evidence](latency.json) remains specific to the preserved
numbered launcher; it does not measure this Rust popup. Provider/lifecycle evidence
belongs to the [backend operational guidance](https://github.com/anastaci1a/sessionsation/blob/main/docs/operations.md).

Creation and lifecycle are Codex-first; Claude remains preparation-only. No
cross-provider conversation conversion, upstream account deletion, arbitrary history
purging or live model-start acceptance is included. Local account deletion delegates
to the public CLI and preserves conversations; the production owner must advertise
its capability before it can run. New/Resume uncertainty stays
visible through the original request ID; no automatic retry or resource deletion
is implemented. Continue in background detaches, not stops. Existing Agent DM pins,
receipt semantics and sole-root ownership remain intact. Ana approved activation
of the selected backend owner on the existing root; the original executable and
startup configuration remain preserved. The latest owner update retained 26 of 27 captured native process identities;
one codex process exited during the interval, without an agent-stop command. Its
cause is not established. Separately, the explicitly confirmed unauthenticated
account deletion was recovered to completion after the FUSE lease fix; saved
conversation storage remained separate. After reboot, an old
owner from the preserved startup configuration gates deletion until activation.

`scripts/test-management.py --binary PATH` exercises compact bars/action groups,
aligned picker percentages/slashes, saved-session selection, Resume identity and
explicitly selected existing/staged directory, background completion returning to the selected session, reopened receipts without
resubmission, and TTY restoration using synthetic APIs. The real-backend installed
HERDR test also covers same-account and transferred-account saved Resume, original
stable/conversation IDs, saved and changed directories, stale generation and conflicting-request
rejection, with zero model turns. Removal/limits/transfer are covered by backend
fixtures and UI reducer/legacy-client regressions; no existing live agent is a target
of acceptance.

The backend’s [current verification manifest](https://github.com/anastaci1a/sessionsation/blob/main/docs/verification.json)
records 258 offline tests, two installed-HERDR fixtures, and a separate authorized
native disposable lifecycle check. Native creation/switch/resume used `b0cadc1`;
final startup waiting, foreign-environment stop, archive and deletion used `37d9393`.
The disposable conversation had no user messages or model turns. Its history, host
and temporary socket alias were cleaned up; existing agents and the owner stayed
unchanged. This empty native history is separate from the 70-turn/270-item synthetic
retention proof.

Agent DM’s older shared-root reader remains pinned and needs reviewed compatible
discovery before custom HERDR namespaces have complete mixed-version acceptance.
The disposable test exposed only its owned host through a temporary standard
namespace alias, then removed it. The new client’s independent foreign-environment
routing has isolated host coverage; no Agent DM pin/data/receipt cutover occurred.
