# Verification

Run `npm test` for isolated client/configuration regressions and `npm run check`
for JavaScript/Bash syntax, manifest fields and documentation links. Fourteen tests
pass, including literal argv, stale/version/protocol errors, exact launcher exec,
viewer exit without lifecycle mutation, plain-shell execution, selected-session
switch identity/generation, exact removal confirmation/revision, active-account
refusal, configuration rollback, concurrent-edit refusal and unsafe-config rejection.
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

No additional model message or real enrollment/removal was needed for UI acceptance.
Claude remains preparation-only. Global future-terminal configuration is separate
from isolated acceptance and is recorded only after application.
