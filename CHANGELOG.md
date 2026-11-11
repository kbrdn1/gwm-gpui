# Changelog

All notable changes to this project will be documented here.

This file tracks the **in-progress** release only. Past releases live under
[`changelogs/`](changelogs/), one Markdown file per SemVer version.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- A GPUI window that lists the worktrees of a repo through `gwm list --format=json`.
- The list stays live through gwm's daemon (`subscribe` → `worktrees.changed`). The app uses a daemon already serving the repo, or starts one and stops it on quit, and falls back to a single `gwm list`. A line above the list says whether it is live (#3).
- A look after Monocode Native: a glass window with its own title bar and the selected worktree's tile, a sidebar of worktrees grouped by branch age (name, branch, age, issue, PR, dirty, ahead/behind), the selected worktree's details in a recessed card, and a status bar that says whether the list is live. Keyboard and mouse selection (#5).
- `scripts/bundle.sh` builds an unsigned `gwm.app` with its icon. Launched from the Finder, the app takes the login shell's `PATH` to find `gwm`, and asks for a folder when it is given none (#8).
- Actions on the selected worktree, in its card: open it in an installed editor or terminal, found by its `.app` bundle and launched with `open -a`, and remove it through `gwm remove`, behind a native confirmation that says when uncommitted changes will be lost. The branch is kept. Since `gwm remove` falls back to a substring match, a fresh `gwm list` must first show the worktree's name alone on the selected path, or the app refuses and says why (#7).
- Create a worktree from the title bar's `+`, through `gwm create`: from an existing issue's number (with a type when its labels do not give one), from a type, an issue number and a description, or with a free name. The new worktree is selected once the list shows it. A refusal shows gwm's words; an unapproved `.gwm.toml` is refused, with how to approve it (`gwm trust add`), never bypassed, not even by a `GWM_ALLOW_BOOTSTRAP` exported in the shell the app was launched from, for `gwm create` as for `gwm remove` (#9).
- The selected worktree's card shows its pull request (open, draft, merged or closed, its title, how many of its checks passed) and its issue (open or closed, its title), read through `gwm status --json` off the UI thread, each time the worktree is selected and when the list shows another branch, issue or PR for it, never per snapshot; the last answer stays shown while it runs again. A click opens either in the browser, an `https://` link only. Without what gwm resolves through `gh`, the card says gwm returned no live state; a failure, or no answer within 30 s, shows gwm's words, never "none" (#10).
- A project picker on the sidebar's repo row, after Monocode's: the repos opened before, most recent first, the window's own checked, and "Open folder…". A repo opens in a window of its own, with its own live list and daemon, so several repos are followed at once; one already open in a window, from any of its worktrees, brings that window forward, from the picker as from File > Open…. The app keeps the last 10 repos in `~/Library/Application Support/gwm-gpui/recent`, one per repo whichever worktree was opened, and hides those whose folder is gone (#11).

## Past releases
