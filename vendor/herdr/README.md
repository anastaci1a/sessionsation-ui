# HERDR palette data

`palettes.json` contains the `accent`, `panel_bg`, `text`, `subtext0`, `overlay0` and `red` RGB tokens from
HERDR 0.9.5's [Palette definitions](https://github.com/herdrdev/herdr/blob/v0.9.5/src/app/state.rs).
The source is HERDR contributors' Apache-2.0 work; its license is retained in
[LICENSE](LICENSE). This is a reduced data extraction, not the host renderer.

The read-only adapter resolves configured built-in names and custom RGB overrides.
The installed host exposes no resolved runtime palette API. Automatic appearance,
terminal-derived palettes and unsupported color formats receive an explicit
approximation notice; the adapter never edits host configuration.

HERDR uses accent borders and panel surfaces in its
[worktree dialogs](https://github.com/herdrdev/herdr/blob/v0.9.5/src/client/shell/worktree_overlays.rs),
main text for primary content/key hints, subtext for previews, overlay text for dim
labels, and red for errors. Our shared popup palette follows those roles rather
than animating menu accent colors or deriving grays from brightness.
