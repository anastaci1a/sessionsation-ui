# Maintaining sessionsation-ui

## Workflow

- Verify the working tree and remote. This is the existing, independent
  `anastaci1a/sessionsation-ui` repository, sibling to `sessionsation`.
- Push each small, completed, tested commit immediately. Issues own tasks and
  acceptance criteria; Git and releases own history, not living documentation.
- Preserve existing installations, account data and live agents. Ask Ana before
  new deployment, spending or credential/access changes. The explicitly requested
  future-terminal launcher configuration needs a backup and tested rollback.
- Never commit credentials, account state, private traces or configuration backups.
- Keep complete local contributor context here. Backend rules and behavior have
  their authoritative homes in the linked backend documentation.

## Boundaries and checks

- Consume public CLI v1 only. Never implement provider authentication, lifecycle,
  account storage, message delivery or supervisors here. Never call inherited CLI.
- Pass stable IDs and fresh generations; show stale/uncertain errors and refresh.
  Do not retry mutations automatically or map legacy receipts without coordination.
- Execute opaque backend launcher argv without shell interpolation. Closing the
  management viewer must never stop an agent. Preserve a host shell for restart.
- Rust changes: run `scripts/check-rust.sh` (format, Clippy, tests in the offline
  build container). `target-ui/` is isolated from installed/review backend artifacts.
- Run `npm test` and `npm run check`. Use isolated config and fake backend fixtures;
  live tests target only explicitly owned disposable HERDR sessions.
- Docs-only changes need `npm run check`, not runtime tests. Validate examples.

## Documentation

- README owns purpose, prerequisites and quick start. docs/ owns usage, architecture
  and verification with a small index. Give each fact one authoritative home.
- Consolidate relevant sections with every change; remove superseded text. Split
  files for different reader needs, not features or work sessions. No session logs.
- Clearly distinguish implemented capabilities, tested evidence and limitations.
