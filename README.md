# gwm-gpui

A native desktop app for [gwm](https://github.com/kbrdn1/gwm-cli), the git worktree manager, built on [GPUI](https://www.gpui.rs).

> [!NOTE]
> Early, macOS only. A glass window, after [Monocode Native](https://github.com/hardbeat920/monocode)'s, that shows the worktrees of a repo, kept live through gwm's daemon: a sidebar of worktrees grouped by branch age, and the selected one's details. Read only for now.

## Requirements

- macOS with the Command Line Tools (a full Xcode is not needed).
- [rustup](https://rustup.rs). It installs the Rust version pinned in `rust-toolchain.toml`.
- [`gwm`](https://github.com/kbrdn1/gwm-cli) on your `PATH`.

## Run

```bash
cargo run -- /path/to/a/repo   # defaults to the current directory
```

The first build compiles GPUI and takes several minutes. Without an argument, outside a repo, the app asks for a folder. A folder that is not a repo still opens, and the status bar says what git made of it.

## Install

```bash
scripts/bundle.sh   # builds gwm.app, unsigned, and prints its path
```

It is a release build: GPUI compiles again, and takes several GB under `target/`. Copy `gwm.app` to `/Applications`, or open it where it is (`open target/release/gwm.app`, `--args /path/to/a/repo` to skip the folder picker). Not being signed, it is Gatekeeper's to judge on another Mac.

A Finder launch gets launchd's `PATH` (`/usr/bin:/bin:/usr/sbin:/sbin`), without `gwm` or Homebrew. When `gwm` is not on the `PATH` it was given, the app asks your login shell (`$SHELL -l -i`, so `.zprofile` and `.zshrc`) for its own, and gives up after 5 seconds. The daemon it starts inherits it.

Up and down move the selection; a click selects a worktree. The title bar's sidebar button hides the sidebar.

## How it talks to gwm

gwm stays the source of truth: this app holds no worktree logic. It is one more consumer of [gwm's frozen machine contracts](https://github.com/kbrdn1/gwm-cli/blob/main/docs/schema/README.md), next to the [herdr plugin](https://github.com/kbrdn1/herdr-plugin-gwm), editors and status bars.

| Need | Through |
|---|---|
| Read the worktree list | the daemon's JSON-RPC 2.0, `gwm list --format=json` when no daemon can be had |
| Live updates | the daemon's `subscribe` → `worktrees.changed`, refused unless `params.schema_version` is the one this build reads |
| Change (`create`, `remove`, `review`) | a `gwm` subprocess (planned) |

### Live updates

A `gwm daemon` serves one repo, and its default socket is shared by every repo. So the app:

1. uses a daemon on gwm's default socket, but only once its first snapshot lists the worktree the app was opened in;
2. else uses a daemon on its own socket for this repo, `gwm-gpui-<hash>.sock` in the same directory as gwm's default socket (`$XDG_RUNTIME_DIR`, else `$TMPDIR`). The hash is of the repo's common git directory, so every worktree of the repo shares it;
3. else starts `gwm daemon` on that socket, and stops it when the app quits;
4. else, or when git cannot locate the repo (a bare one), lists the worktrees once with `gwm list --format=json`.

The status bar says which: live, reconnecting after a lost stream, or not live and why. If the app is killed instead of quit, the daemon it started keeps running: the next launch uses it, but does not own it, so it runs until you stop it.

It does not link the `gwm-cli` crate: its `[lib]` target is an internal test seam, [not covered by SemVer](https://github.com/kbrdn1/gwm-cli/blob/main/docs/6.development/3.stability.md).

## Development

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option. See [NOTICE](NOTICE) for third-party attributions.
