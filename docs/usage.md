# Usage

Set `SESSIONSATION_BIN` to the verified absolute backend executable and
`SESSIONSATION_DATA_DIR` to the operator-selected root. Existing Ana accounts use
`/mnt/user/appdata/codex-switcher` with its existing credential owner. The independent
backend default is not an automatic import. See the backend ownership guidance.

Run `./bin/sessionsation-ui launcher` in an ordinary terminal for new, resume or
plain shell. Run `./bin/sessionsation-ui manage` for account and selected-session
management. Exiting management only closes the viewer. Stop is a separate explicit
backend operation. Capability hints determine available choices.

HERDR integration and safe configuration commands are documented here as they
become verified. No existing shell startup files or restored agents are modified.
