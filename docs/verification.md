# Verification

## Rust replacement (not deployed)

`scripts/check-rust.sh` checks formatting, Clippy and Rust unit regressions in an
offline container. `scripts/test-terminal.py --binary PATH` adds isolated PTY
acceptance with fake backend/HERDR commands: 20 process startups, 238 key events,
two-second backend delays, editable summary, bracketed paste, staged directories,
exact Start request identity, resize/cancel and byte-equivalent TTY restoration.
It decodes ANSI cell updates rather than searching incremental output as plain text.
The resize/cancel case exercises simultaneous signal/input readiness; the default
Crossterm Mio reader intermittently lost the key, while its level-triggered reader
passes. No test invokes a model or reads production account state.

The release build measured p95 first paint 7.99 ms, key feedback 0.85 ms, and frame
computation/diff 0.102 ms at 80×24 / 0.381 ms at 160×48 (600 frames each).
These are local PTY/renderer measurements, not host invocation or provider startup.
Actual popup handoff, visual fidelity and deployment remain acceptance gates.

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
