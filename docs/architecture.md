# Architecture

The Node.js client invokes `sessionsation api --api-version 1` without a shell and
consumes structured JSON envelopes. The backend owns account state, credential
writers, catalogs, runtime generations, operation locks, message receipts and all
provider behavior. UI state contains no account credentials.

Authoritative backend references:

- [Public contract and schema](https://github.com/anastaci1a/sessionsation/blob/main/docs/contract.md)
- [Architecture](https://github.com/anastaci1a/sessionsation/blob/main/docs/architecture.md)
- [Setup and data roots](https://github.com/anastaci1a/sessionsation/blob/main/docs/cli.md)
- [Acceptance, ownership and adapter coordination](https://github.com/anastaci1a/sessionsation/blob/main/docs/operations.md)
- [Exact CLI artifact provenance](https://github.com/anastaci1a/sessionsation/blob/main/docs/verification.json)

The UI checks API version and consumes capability hints. Backend mutation-time
validation remains authoritative. Selections use backend IDs and captured
generations; stale errors require refresh. No automatic mutation retry, legacy
receipt adapter or Submit/Queue semantic change belongs here.

The management plugin is a viewer. It never launches an agent in an overlay.
The separate terminal launcher executes backend-produced argv with process
replacement. A Bash parent stays alive to retain the host prompt when an agent
exits; an explicit environment bypass prevents recursive menus.

The interactive launcher has an independent terminal-cell renderer and raw-key
controller. Its visual reference is Ana's existing `cli-main-menu` (ae786af);
the Node implementation does not import its Bash backend or copy its source.
Animation geometry uses terminal dimensions, with sparse cell repairs, bounded
typeahead and output-backpressure handling. Navigation does no backend polling.
Directory reads are asynchronous; only explicit folder creation writes local data.
Selected directories affect New's inherited cwd; backend-owned Resume cwd and opaque
argv take priority. The numbered non-TTY path and management CLI remain separate.
