# Verification

## Rust popup

The release candidate passes **30 Rust tests** (29 unit tests and one test covering
four complete cell goldens), Clippy with warnings denied, formatting, and **22 Node
regressions**. Syntax, manifest fields and documentation links pass. The goldens
exercise fixed time/seed/geometry, wide graphemes, text masks, disabled strike,
shared palette/shimmer, reference Back/Close placement and fixed field geometry. The rendered
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
cancellation and unchanged origin shell. Twenty actual prefix+uppercase-L openings
passed. The separate backend test uses the real new-launch API with an inert compiled
provider to verify exact account/UUID/socket, deduplication and retained shell.
No acceptance test starts a model or touches production accounts/Agent DM state.

Measured release p95: first responsive PTY paint **6.17 ms**, key feedback **0.89 ms**,
keybind-to-popup paint **172.33 ms**, frame computation/diff **0.099 ms** at 80×24 and
**0.358 ms** at 160×48 (600 frames each). These exclude provider startup/model response.
CLI-triggered plugin invocation additionally pays the external command round trip;
one earlier observation was 284 ms, whereas the configured key path met 250 ms.

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

Creation is Codex-first; Claude remains preparation-only. No cross-provider
conversation conversion, existing-session switching/resume, upstream account
management or model-start acceptance is included in the new popup. Uncertain
creation stays visible through the original request ID; no automatic retry or
resource deletion is implemented. Back after Start detaches, not stops. Existing
Agent DM pins, receipt semantics and sole-root ownership are unchanged.
