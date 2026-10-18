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

The installed frontend remains the numbered launcher. The animated redesign was
reverted; the [reference specification](cli-specification/README.md) owns the
intended fidelity requirements and [language discussion](cli-specification/architecture.md)
before a replacement is built. Keep this document about implemented architecture.
