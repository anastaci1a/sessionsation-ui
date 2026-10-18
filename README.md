# sessionsation-ui

A Rust **account and session popup** for HERDR and
[sessionsation](https://github.com/anastaci1a/sessionsation). Open a terminal popup,
choose an account, destination Space, name and directory, then start in a normal tab.
Manage accounts and existing sessions through the same popup; all account and
lifecycle operations remain owned by the sessionsation CLI.

The source now includes responsive account usage, account actions and lifecycle
menus. Expanded saved-session resume is still under implementation in
[issue 7](https://github.com/anastaci1a/sessionsation-ui/issues/7).
The [deployment record](docs/deployment.json) identifies the older installed launcher;
source changes do not automatically replace that artifact.

The directory follows `projects-root/session-name` until you choose a different path.
Nothing creates directories or Spaces before **Start Session**, including the directory
picker's Create action. Closing the popup leaves running agents alone.

Requires Linux, HERDR 0.9.5+, and a verified sessionsation backend with public launch
API v1. Build from source with Rust 1.88+; Node.js 22.18+ runs the optional configuration
installer and preserved legacy tools. Claude is preparation-only.

```sh
CARGO_TARGET_DIR=target-ui cargo build --release --locked
mkdir -p .runtime
install -m 755 target-ui/release/sessionsation-ui .runtime/sessionsation-ui
herdr plugin link "$PWD" --enabled
```

Configure the explicit backend path/data root and your own binding using
[setup instructions](docs/usage.md). Ana's binding is **prefix + uppercase L**.
Configuration in the popup selects Global or Per-Space projects roots; inactive
settings are retained. Start stays disabled until every choice is valid.

Useful commands after setup:

```sh
./bin/sessionsation-popup open
./bin/sessionsation-popup --help
sessionsation api --api-version 1 account list
```

The old numbered launcher and management viewer remain available for rollback;
they are separate from the new popup. See [architecture](docs/architecture.md),
[verification and limitations](docs/verification.md), the
[reference specification](docs/cli-specification/README.md), and
[contributor instructions](agents/README.md).
