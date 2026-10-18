# Verification

Run `npm test` for isolated client/configuration regressions and `npm run check`
for JavaScript/Bash syntax, manifest fields and documentation links. Eleven tests
pass: literal argv, structured stale/version/protocol errors, exact launcher exec,
viewer exit without lifecycle mutation, plain-shell plan execution, control-character
sanitization, exact configuration rollback, concurrent-edit refusal, ambiguous or
symlinked config rejection and quoted paths. No test uses credentials or models.
Backend evidence/provenance remains authoritative in [architecture](architecture.md).

Installed HERDR 0.9.5 accepted the manifest via an isolated plugin registry. An owned
headless test server displayed the default launcher and the management overlay;
exiting management left its underlying terminal alive. The launcher uses an actual
v1 backend against an empty, separate test root for these checks.

Real account enrollment/removal, UI-driven model usage and global default-launcher
configuration are not established by fixture tests. Claude remains preparation-only.
Further retained-shell and rollback acceptance must pass before global configuration.
