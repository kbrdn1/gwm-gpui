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

## Past releases
