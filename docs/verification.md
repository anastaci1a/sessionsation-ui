# Verification

## Rust popup

The release candidate passes **25 Rust tests** (24 unit tests and one test covering
four complete cell goldens), Clippy with warnings denied, formatting, and **22 Node
regressions**. Syntax, manifest fields and documentation links pass. The goldens
exercise fixed time/seed/geometry, wide graphemes, text masks, disabled strike,
shared palette/shimmer and terminal-bottom controls. The rendered
[summary preview](assets/popup-summary.png) was inspected against the reference;
random star coordinates intentionally use a deterministic Rust generator.

`scripts/test-terminal.py --binary PATH` uses isolated fake APIs and real PTYs:
20 process startups, 238 key events, two-second backend delays, summary edits,
bracketed paste, staged directory/request identity, resize/cancel and exact TTY
restoration. ANSI updates are decoded into cells. Simultaneous resize/input exposed
an intermittent key-readiness loss in the default Crossterm Mio reader; the
level-triggered TTY reader passes this regression.

`scripts/test-herdr.py --binary PATH` runs against installed HERDR 0.9.5 inside a
network-disabled container, with separate config/socket/state and a synthetic public
backend. It verifies half-size popup geometry, origin context without a pane ID,
selection of a different Space, normal-tab handoff, focus before exit, second-popup
cancellation and unchanged origin shell. Twenty actual prefix+uppercase-L openings
passed. The separate backend test uses the real new-launch API with an inert compiled
provider to verify exact account/UUID/socket, deduplication and retained shell.
No acceptance test starts a model or touches production accounts/Agent DM state.

Measured release p95: first responsive PTY paint **5.58 ms**, key feedback **0.62 ms**,
keybind-to-popup paint **176.70 ms**, frame computation/diff **0.113 ms** at 80×24 and
**0.467 ms** at 160×48 (600 frames each). These exclude provider startup/model response.
CLI-triggered plugin invocation additionally pays the external command round trip;
one earlier observation was 284 ms, whereas the configured key path met 250 ms.

Run `scripts/check-rust.sh`, `npm test` and `npm run check`. The PTY/host scripts
require Python 3; use the existing offline build container, mount only this checkout
and the HERDR executable at `/usr/local/bin/herdr`, and pass the release executable.
Never mount production homes, sockets or account roots into these fixtures.
`UPDATE_GOLDENS=1 cargo test --test visual` deliberately regenerates cell fixtures;
ordinary checks compare them unchanged. Deployment/provenance is recorded separately
in [deployment.json](deployment.json).

## Preserved installed client

Run `npm test` for isolated client/configuration regressions and `npm run check`
for JavaScript/Bash syntax, manifest fields and documentation links. Twenty tests
pass, including literal argv, stale/version/protocol errors, exact launcher exec,
viewer exit without lifecycle mutation, plain-shell execution, selected-session
switch identity/generation, exact removal confirmation/revision, active-account
refusal, configuration rollback, concurrent-edit refusal and unsafe-config rejection. Host regressions cover the
default socket through a symlink, named/explicit sessions, unknown sockets/missing
panes, non-HERDR terminals and the retained-shell new-account execution path.
The New-page regression refuses global state discovery and accepts only the
account-list request followed by the exact selected-account launch plan.
No fixture test uses credentials or models. Backend evidence/provenance remains
linked from [architecture](architecture.md).

## Installed HERDR acceptance

HERDR 0.9.5 accepted the manifest. Plugin registration is user-wide; use
`XDG_CONFIG_HOME` for isolated registration (`HERDR_DATA_DIR` does not isolate it).
The requested plugin is linked in Ana's user registry. An owned headless
server verified the default launcher and management overlay. Exiting management
left the underlying terminal alive. Tests also verified:

- Plain-shell selection and menu exit retain a host shell.
- The requested cwd survives user shell initialization.
- The backend-created-pane bypass skips the menu and retains the original shell.
- Applying actual configuration shows the menu in a new terminal; rollback restores
  byte-identical original configuration and original plain-shell behavior after reload.
- UI resume of the authorized disposable Codex conversation retains the stable
  backend ID, provider UUID, selected `codex:personal` account, cwd and exact native
  app-server socket. Discovery recognizes its HERDR attachment and lifecycle capabilities.
- Configuration reload/rollback leaves that running disposable agent unchanged.
- Backend restart of the UI-launched agent completes on the same account and UUID
  through the retained shell; no additional input or model usage is sent.

No additional model message or real enrollment/removal was needed for UI acceptance.
Claude remains preparation-only. The applied future-terminal configuration and private rollback location are
recorded in [usage](usage.md#anas-configured-host). All 17 existing Codex/Claude
processes retained their PIDs/start times across default-server reload, and its
original pane retained its terminal identity.


## Default-server environment repair

The original named-server acceptance did not expose HERDR's missing default-server
session variable. The repaired wrapper was checked in a fresh terminal on
Satellite's actual default server, following menu 1 (new), account 1 (codex:gate1).
An isolated fake backend received the exact selected account and exported
`SESSIONSATION_HERDR_SESSION=default`, and its returned execution plan inherited
both that session and the real pane ID. After the fixture exited, the original
retained shell was still present. The owned smoke workspace was closed afterward.

This repair changes launcher environment preparation only. No new model session,
prompt, credential operation or native provider call was made to verify it.
That environment-only repair retained the selected e4d7833 binary. The subsequent
performance repair below selects a separate launcher build. Agent DM's pinned
review artifact and its data remain unchanged. Saved conversations from failed attempts are preserved; no automatic
retry or cleanup is performed. Existing terminals are not restarted or migrated.

## Launcher responsiveness

[Timing evidence](latency.json) records measured input-to-menu and execution-plan
handoff boundaries, before/after sources and individual samples. Profiling found
that live-terminal discovery spent most of its time parsing complete conversation
rollouts. The backend now reads at most 64 KiB for activity hints, retaining unknown
when evidence is absent or damaged. Mutation-time native idle/ownership checks and
locks remain intact. Dead/replaced HERDR bindings are rejected before socket calls;
their files and histories remain recoverable. New requests only the account list.

Regression coverage includes bounded reads, conservative activity hints, stale
binding avoidance, live-host confirmation and alphanumeric HERDR pane IDs. The
[deployed artifact](deployment.json) passed all 213 backend tests and required
checks. The same fixes on backend main passed 219 tests. Two earlier unrestricted
parallel runs hit different worker-timing fixtures; targeted reproduction passed,
and the complete suite passed with two independent cases at a time. Explicit
concurrency tests retain their own workers; no operation safeguards were relaxed.

An owned terminal on Satellite's actual default server passed New, Resume, Back
and plain Shell with no model selected, then was closed. Timings use real read/plan
calls but replace returned execution argv with an inert marker. They measure
launcher overhead, not provider startup, model response or shell-prompt readiness.
No new model session was started. Agent DM retains its isolated native acceptance
test; this repair changes neither Submit/Queue semantics nor legacy receipts.
