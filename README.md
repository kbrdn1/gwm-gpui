# gwm-gpui

A native desktop app for [gwm](https://github.com/kbrdn1/gwm-cli), the git worktree manager, built on [GPUI](https://www.gpui.rs).

> [!NOTE]
> Early, macOS only. A glass window, after [Monocode Native](https://github.com/hardbeat920/monocode)'s, that shows the worktrees of a repo, kept live through gwm's daemon: a sidebar of worktrees grouped by branch age, and the selected one's details, from where it opens in an editor or a terminal, or is removed. A new worktree is created from the title bar.

## Requirements

- macOS with the Command Line Tools (a full Xcode is not needed).
- [rustup](https://rustup.rs). It installs the Rust version pinned in `rust-toolchain.toml`.
- [`gwm`](https://github.com/kbrdn1/gwm-cli) on your `PATH`.

## Run

```bash
cargo run -- /path/to/a/repo   # defaults to the current directory
```

The first build compiles GPUI and takes several minutes. Without an argument, outside a repo, the app asks for a folder. A folder in no repo still opens, and the window says what git and gwm make of it; File > Open… (⌘O) opens another folder in a new window.

## Install

```bash
scripts/bundle.sh   # builds gwm.app, unsigned, and prints its path
```

It is a release build: GPUI compiles again, and takes several GB under `target/`. Copy `gwm.app` to `/Applications`, or open it where it is (`open target/release/gwm.app`, `--args /path/to/a/repo` to skip the folder picker). Not being signed, it is Gatekeeper's to judge on another Mac.

A Finder launch gets launchd's `PATH` (`/usr/bin:/bin:/usr/sbin:/sbin`), without `gwm` or Homebrew. When `gwm` is not on the `PATH` it was given, the app asks your login shell (`$SHELL -l -i`, so `.zprofile` and `.zshrc`) for its own, and gives up after 5 seconds. The daemon it starts inherits it.

The sidebar's top row names the repo: a click on it lists the repos opened before, most recent first, and "Open folder…". A repo opens in a window of its own, live, so several are followed at once, one per window; a repo already open, from any of its worktrees, brings its window forward instead. The list keeps the last 10 repos, in `~/Library/Application Support/gwm-gpui/recent`, and hides those whose folder is gone.

Up and down move the selection; a click selects a worktree. The title bar's sidebar button hides the sidebar. In the selected worktree's card, "Open in…" lists the editors and terminals installed in `/Applications`, `~/Applications` and the system's Utilities, and "Remove…" removes the worktree through gwm, keeping its branch, once you confirm.

Once the list is shown, the title bar's `+` opens the "New worktree" form in the card: from an existing issue's number (gwm reads its description through the forge's CLI, `gh` or `glab`, and its type from its labels when `.gwm.toml` maps them to types, otherwise you give the type; a worktree that already has the issue is selected), from a type, an issue number and a description, or with a free name, which is the branch. Enter creates it, Escape closes the form, Tab moves to the next field. Once gwm is done, the new worktree is selected as soon as the list shows it. A refusal shows gwm's own words. gwm runs with no terminal to ask on: a `.gwm.toml` whose bootstrap commands you have not approved is refused, and `gwm trust add`, run in a terminal in the repo, approves it.

## How it talks to gwm

gwm stays the source of truth: this app holds no worktree logic. It is one more consumer of [gwm's frozen machine contracts](https://github.com/kbrdn1/gwm-cli/blob/main/docs/schema/README.md), next to the [herdr plugin](https://github.com/kbrdn1/herdr-plugin-gwm), editors and status bars.

| Need | Through |
|---|---|
| Read the worktree list | the daemon's JSON-RPC 2.0, `gwm list --format=json` when no daemon can be had |
| Live updates | the daemon's `subscribe` → `worktrees.changed`, refused unless `params.schema_version` is the one this build reads |
| Remove a worktree | `gwm remove <name>`, a subprocess, once a fresh `gwm list --format=json` shows that name alone on the selected path: gwm removes by pattern and falls back to a substring match, it has no remove by path |
| Create a worktree | `gwm create --issue <N> [--type <type>]`, `gwm create <type> <issue> <desc>` or `gwm create --name <name>`, a subprocess with stdin closed, never `--allow-bootstrap`, and `GWM_ALLOW_BOOTSTRAP` removed from its environment (as for `gwm remove`). The new worktree is found in the next list by its issue number or its branch, on a path that was not listed before, not in gwm's text output |
| Review | a `gwm` subprocess (planned) |

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
