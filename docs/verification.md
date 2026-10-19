# Verification

## Rust popup

The expanded popup passes **101 Rust tests** (99 unit tests and two tests covering
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
restoration. ANSI updates are decoded into cells. Simultaneous resize/input exposed
an intermittent key-readiness loss in the default Crossterm Mio reader; the
level-triggered TTY reader passes this regression. The cell decoder also recovers
at the next ESC when HERDR interrupts an output sequence during popup teardown.

`scripts/test-herdr.py --binary PATH` runs against installed HERDR 0.9.5 inside a
network-disabled container, with separate config/socket/state and a synthetic public
backend. It verifies the requested percentage popup geometry, origin context without a pane ID,
selection of a different Space, normal-tab handoff, focus before exit, second-popup
cancellation, held-arrow clamping/release-and-press wrapping through the actual
HERDR pane encoder, Home/End list jumps, and unchanged origin shell. The installed
release moves on the first press alone in 32.44 ms, before any repeat or release. Twenty actual prefix+uppercase-L openings
passed. Separate backend fixtures cover exact account/UUID/socket handoff, normal exit,
preserved-history resume after local account deletion and the pinned-original
supervisor handoff. Their details and limits belong to the backend guidance.
No acceptance test starts a model or changes production accounts/Agent DM state;
production capability/status previews are read-only.

PTY measurements for source `1f30e96` (before the Home/End-only follow-up), p95: first responsive PTY paint **6.31 ms**, key feedback **0.46 ms**.
The earlier popup-core host/frame measurement (source `8809571`, recorded in the
machine-readable results) measured keybind-to-popup paint **183.61 ms**,
frame computation/diff **0.105 ms** at 80×24 and
**0.412 ms** at 160×48 (600 frames each). These exclude provider startup/model response.
CLI-triggered plugin invocation additionally pays the external command round trip;
the measured invocation was 282.44 ms, whereas the configured key path met 250 ms.

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
cause is not established. No account was deleted. After reboot, an old
owner from the preserved startup configuration gates deletion until activation.

`scripts/test-management.py --binary PATH` exercises compact bars/action groups,
aligned picker percentages/slashes, saved-session selection, Resume identity and
fixed directory, background completion without focus, reopened receipts without
resubmission, and TTY restoration using synthetic APIs. The real-backend installed
HERDR test also covers same-account and transferred-account saved Resume, original
stable/conversation IDs, saved directory, stale generation and conflicting-request
rejection, with zero model turns. Removal/limits/transfer are covered by backend
fixtures and UI reducer/legacy-client regressions; no production account mutation
was performed for acceptance.
