# gwm-gpui

A native desktop app for [gwm](https://github.com/kbrdn1/gwm-cli), the git worktree manager, built on [GPUI](https://www.gpui.rs).

> [!NOTE]
> Early scaffold, macOS only. Today it opens a window and lists the worktrees of a repo, nothing more.

## Requirements

- macOS with the Command Line Tools (a full Xcode is not needed).
- [rustup](https://rustup.rs). It installs the Rust version pinned in `rust-toolchain.toml`.
- [`gwm`](https://github.com/kbrdn1/gwm-cli) on your `PATH`.

## Run

```bash
cargo run -- /path/to/a/repo   # defaults to the current directory
```

The first build compiles GPUI and takes several minutes.

## How it talks to gwm

gwm stays the source of truth: this app holds no worktree logic. It is one more consumer of [gwm's frozen machine contracts](https://github.com/kbrdn1/gwm-cli/blob/main/docs/schema/README.md), next to the [herdr plugin](https://github.com/kbrdn1/herdr-plugin-gwm), editors and status bars.

| Need | Through |
|---|---|
| Read (`list`, `doctor`, `path`) | `gwm <cmd> --format=json` today, the daemon's JSON-RPC 2.0 next |
| Live updates | the daemon's `subscribe` → `worktrees.changed` (planned) |
| Change (`create`, `remove`, `review`) | a `gwm` subprocess (planned) |

It does not link the `gwm-cli` crate: its `[lib]` target is an internal test seam, [not covered by SemVer](https://github.com/kbrdn1/gwm-cli/blob/main/docs/6.development/3.stability.md).

## Development

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option. See [NOTICE](NOTICE) for third-party attributions.
