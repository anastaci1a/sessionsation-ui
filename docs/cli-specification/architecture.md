# Reference adaptation boundary

The reference is a Bash terminal menu. sessionsation-ui is a Rust implementation
with reusable background, widget, navigation, form and directory components; see
[current architecture](../architecture.md). Provider behavior and lifecycle remain
in the public sessionsation CLI, independently of terminal presentation.

Required visual adaptations are a full-PTY star field, no ASCII title, a functional
status bar and a 90%-wide/65%-high HERDR popup. The current management and creation
interactions are specified in [usage](../usage.md), rather than duplicated here.
Directory creation is staged until Start; the popup creates a normal tab and exits.
It does not promote its own process, invent a Projects entity, or manage credentials.

The reference's legacy shell/supervisor integrations are not this UI's contract.
Known reference bugs, timing artifacts and unimplemented actions are not assumed
features. Source provenance and permission review remain recorded in [NOTICE](../../NOTICE).
