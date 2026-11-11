# Contributing to gwm-gpui

## Development

```bash
cargo run -- /path/to/a/repo      # launch the app
cargo test                        # tests, the app and gwm-client
cargo test -p gwm-client          # the gwm layer alone, without building GPUI
cargo clippy --workspace --all-targets --all-features -- -D warnings   # --all-features: the screenshot example too
cargo fmt --all
cargo run --example screenshot --features screenshot -- <repo> <out.png>   # render the window off screen
scripts/bundle.sh                 # build gwm.app (release build), print its path
```

`gwm-client`'s tests drive a real `gwm daemon` on throwaway repos: they need `gwm` on your `PATH`, and fail without it.

If `rustc` resolves to something older than your rustup stable, for example through a nix profile, put `~/.cargo/bin` first on your `PATH`.

Logic that doesn't need a window is tested under `#[cfg(test)]`, driven by a real sample of what gwm prints. A change of behaviour comes with a test that failed first.

## Branches

`dev` is the integration branch, `main` is what's released. Work on `<type>/#<issue>-<description>`, from `dev`, through a pull request back into `dev`. `gwm create` sets this up from `.gwm.toml`.

## Commits

[Gitmoji](https://gitmoji.dev) + [Conventional Commits](https://www.conventionalcommits.org), referencing the issue:

```text
✨ feat(worktrees): open a worktree in a new tab (#12)
```

Atomic commits: each one builds and does one thing. Merge pull requests with a merge commit, never a squash.

## Changelog

Every user-visible change adds a line under `## [Unreleased]` in `CHANGELOG.md`.
