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

## Past releases
