# Contributing to sessionsation-ui

## Workflow and boundaries

Read AGENTS.md and this file. Inspect the existing remote/tree before editing and
preserve ongoing work. Use the configured Git identity; push small completed tested
commits to the existing repository. Issues own tasks/acceptance, Git/releases own
history. Preserve live accounts, agents and original installations. Deployment,
credential/access changes, spending and live migration need operator authorization.

Use public sessionsation API v1 only. Account/credential/lifecycle logic belongs
to the CLI; the UI must not call inherited implementation commands or invent
provider behavior. Retain stable IDs, generations, locks and uncertainty. Never
retry a mutation automatically. Viewer detach does not stop agents. Tests and demo
use isolated homes, fake provider data and only their own named host.

## Reproducible checks

Install rustup, the pinned toolchain, Node.js 22.18+ and Python 3. On Debian/Ubuntu
install `build-essential git curl ca-certificates python3`; obtain Node from its
[official downloads](https://nodejs.org/en/download). There are no npm runtime
packages to install in this checkout.

```sh
rustup show
scripts/check-rust.sh
npm test
npm run check
```

Rust checks resolve the checkout, default to `target-ui/`, run two independent
test cases at a time, and use locked Cargo dependencies. Override
`CARGO_TARGET_DIR` or `RUST_TEST_THREADS` explicitly. First use downloads public
crates; subsequent runs may use `CARGO_NET_OFFLINE=true` after `cargo fetch --locked`.
Do not overwrite a binary selected by an installed popup.

Without local toolchains, use the repository's public-image recipe:

```sh
docker build -f scripts/Dockerfile.check -t sessionsation-ui-check .
docker run --rm -v "$PWD:/work" -w /work sessionsation-ui-check \
  sh -c 'scripts/check-rust.sh && npm test && npm run check'
```

This requires no private base image or host Cargo cache. Network is needed for
images/packages/crates initially. No account homes or live sockets are mounted.
Use a dedicated checkout or fix container-created build-file ownership afterward.
Rust changes need the complete checks and meaningful regressions. Docs-only work
needs `npm run check`. Setup/check-script corrections need clean-environment
execution. [Verification](../docs/verification.md) owns the optional PTY/HERDR and
demo commands; goldens are regenerated deliberately and reviewed, never in a loop.

## Documentation and release preparation

README owns purpose, requirements and quick start; docs/ owns usage, configuration,
architecture and verification. agents/ owns contributor workflow. Update the
existing authoritative section and remove stale copies. Do not retain machine
inventories, private rollback paths, session logs or duplicated feature lists.
Keep operational evidence/configuration backups outside public Git.

This repository is public. Before each release, review tracked files AND reachable history. This
pass's bounded token/private-key scan found no credential-like matches, but it is
not a security certification. History still contains private host/deployment paths,
account/session metadata and earlier operational records, including a removed
terminal-verification file and a prior PNG with an operator hostname. Current cleanup does not sanitize those Git objects.
Do not rewrite history or change visibility without an explicit owner decision.

Retain [LICENSE](../LICENSE), [NOTICE](../NOTICE) and the HERDR palette license.
The unlicensed cli-main-menu reference needs owner provenance/permission review
for distribution, including translated visual algorithms. Public visibility does
not resolve that permission. A matching
author name or a public repository is not a license grant. New demo assets must
contain synthetic values only; inspect the contact sheet and loop before promotion.
CI uploads review artifacts with read-only repository permissions and never commits
or publishes assets automatically.

Check scripts also default TOKIO_WORKER_THREADS to 2 for owned subprocesses,
preventing large host CPU counts from creating oversized test runtimes. Override
it explicitly when investigating concurrency; production binaries are unchanged.
