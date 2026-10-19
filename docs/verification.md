# Verification and demo

## Portable checks

[Contributor instructions](../agents/README.md) own tool installation and default
Rust/Node checks. These use fake public APIs, temporary state and deterministic
cell goldens; no provider credentials or model calls are needed. The suite covers
input/resize/cancel, Unicode, star masks/fades, usage limits, responsive lists,
stale targets, request identity, retained focus and uncertain delivery.

After a release build, optional real-PTY tests use only synthetic backends:

```sh
python3 scripts/test-terminal.py --binary target-ui/release/sessionsation-ui
python3 scripts/test-management.py --binary target-ui/release/sessionsation-ui
python3 scripts/test-session-activity.py --binary target-ui/release/sessionsation-ui
# Install verified HERDR 0.9.5 on PATH first:
python3 scripts/test-herdr.py --binary target-ui/release/sessionsation-ui
```

The HERDR script creates its own config/socket/state and named server, opens the
real popup through a PTY, and tests geometry, key delivery, Space selection,
normal-tab handoff and unchanged origin shell. It never uses the user's default
host. Run it in an isolated container without account homes, live sockets or host
credentials. Native provider acceptance belongs to the backend's
[verification guidance](https://github.com/anastaci1a/sessionsation/blob/main/docs/operations.md).
A GIF/PTY pass is not native account-transfer or model compatibility evidence.

## Animated demo

The demo records a real HERDR TUI containing the real Rust popup. The backend
fixture supplies only synthetic personal/work accounts, sessions, paths and usage;
its launch action opens an inert shell tab, never a provider. This is a visual and
interaction demonstration, not evidence that a fake agent ran successfully.

Regeneration uses a dedicated container, separate HOME/XDG/config/socket/state,
no real config/accounts/agents, and no network at capture time. HERDR and agg are
version/checksum-pinned public downloads. agg renders the actual terminal stream;
no substitute UI is drawn. No desktop, display server or GPU is needed.

From a clean checkout with Docker:

```sh
docker build -f scripts/Dockerfile.check -t sessionsation-ui-check .
docker build -f scripts/Dockerfile.demo -t sessionsation-ui-demo .
mkdir -p .demo/output
docker run --rm --network none --hostname demo-linux \
  -v "$PWD/.demo/output:/output" sessionsation-ui-demo
```

Image builds download the declared public toolchains/dependencies and checksum-verified
HERDR 0.9.5/agg 1.9.0; capture has no network and mounts only its output directory.
No preexisting host cache or installed HERDR is used. The pinned demo tool binaries
currently target Linux x86-64. On a local Linux x86-64 machine, the same driver can
run with Python 3.11+, Pillow, libfaketime, DejaVu fonts, the tools installed through
`python3 scripts/download-demo-tools.py --output .demo/tools`, and a release UI:

```sh
PATH="$PWD/.demo/tools:$PATH" python3 scripts/demo.py \
  --binary target-ui/release/sessionsation-ui --output .demo/output
```

Prefer the container boundary. The driver constructs HOME/XDG/config/environment
from scratch and cleans up only its temporary named host. libfaketime fixes the
synthetic wall clock while preserving monotonic timers. Geometry and fixture data
are fixed; process scheduling/encoder timing means byte-identical GIFs are not promised.

The manual [Review UI demo workflow](../.github/workflows/demo.yml) runs these same
container commands on an Ubuntu runner and uploads review artifacts for seven days.
Its repository permissions are read-only; there is no auto-commit, auto-publish,
push trigger, or repeated-generation loop. Hosted workflow execution is separate
from the locally verified CI-equivalent command.

Review `.demo/output/contact-sheet.png`, watch `demo.gif` through the complete
loop, and inspect `demo-evidence.json` before copying anything into docs. The driver
asserts a single synthetic launch, staged directories, selected-Space focus and
popup exit. It caps the GIF at 2 MB/30 seconds and repeats the genuine initial
terminal stream for the loop seam. This is terminal-stream editing, not a mock UI.
Then deliberately promote the reviewed asset:

```sh
cp .demo/output/demo.gif docs/assets/demo.gif
npm run check
```

The checked-in demo is about 640 KiB, 17 seconds and 143 frames at a 12 FPS cap;
its 128×40 terminal renders to 1096×718 pixels. It shows bars/limits, session
activity/details, the staged directory picker and an inert normal-tab handoff.
The contact sheet and loop seam were inspected before README promotion. No real
accounts, provider calls, credentials or live agents were involved.

HERDR is Apache-2.0 (retained palette license); agg is GPL-3.0-or-later, libfaketime
GPL-2.0 and DejaVu fonts have their own font license. These are separate development
and rendering tools, not linked into the MIT UI. No tool executable/font file is
shipped as a README asset. See [agg](https://github.com/asciinema/agg),
[libfaketime](https://github.com/wolfcw/libfaketime) and
[DejaVu](https://dejavu-fonts.github.io/License.html) for their upstream notices.

## Verification record and limits

A fresh public-image Debian bookworm environment, Rust 1.95.0 and Node.js 22.18.0
passed formatting, Clippy, **132 Rust tests** (130 unit and two visual tests covering
21 goldens), **22 Node tests**, syntax/manifest/docs checks and Python compilation.
The first build fetched locked public dependencies without the operator's cache;
subsequent corrective runs reused only that newly built test tree. The isolated
demo also passed selected-Space focus/popup-exit and one-launch assertions with
network disabled. Hosted GitHub Actions execution is not claimed by these local
results. No account enrollment, model compatibility or credential migration was tested.
Private deployment inventories and raw native traces are intentionally kept outside Git.

The tested HERDR version is 0.9.5/protocol 22. The Rust package's declared minimum
1.88 has not been tested independently of pinned 1.95.0. Real provider setup is not
part of synthetic demo/checks. Supported behavior and unfinished actions have one
authoritative home in [usage](usage.md#capabilities-and-limits). Release history
and upstream provenance require the review described in [contributing](../agents/README.md).
