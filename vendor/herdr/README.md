# HERDR palette data

`palettes.json` contains the accent, panel background and text RGB tokens from
HERDR 0.9.5's [Palette definitions](https://github.com/herdrdev/herdr/blob/v0.9.5/src/app/state.rs).
The source is HERDR contributors' Apache-2.0 work; its license is retained in
[LICENSE](LICENSE). This is a reduced data extraction, not the host renderer.

The read-only adapter resolves configured built-in names and custom RGB overrides.
The installed host exposes no resolved runtime palette API. Automatic appearance,
terminal-derived palettes and unsupported color formats receive an explicit
approximation notice; the adapter never edits host configuration.
