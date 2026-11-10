//! The worktree list, read through gwm's frozen machine contract
//! (`docs/schema/worktree-list.schema.json` in gwm-cli): once with
//! `gwm list --format=json`, or live from the daemon ([`watch`]).
//!
//! Types declare only the fields the app reads and never `deny_unknown_fields`:
//! the contract adds optional fields without a version bump, and asks consumers
//! to ignore the ones they don't know.

pub mod daemon;
pub mod watch;

use std::{
  ffi::OsStr,
  os::unix::ffi::OsStrExt,
  path::{Path, PathBuf},
  process::Command,
};

use serde::Deserialize;

/// The repo a window shows, as git names it.
#[derive(Clone, Debug, PartialEq)]
pub struct Repo {
  /// The root of the worktree the app was opened in: a daemon serves this
  /// repo only if this exact path is one of its worktrees.
  pub toplevel: PathBuf,
  /// The git directory every worktree of the repo shares, so it names the
  /// repo whichever worktree the app was opened from.
  pub common_dir: PathBuf,
}

impl Repo {
  /// Asks git, once, where `path` belongs. Both paths come back canonical.
  pub fn locate(path: &Path) -> Result<Self, String> {
    let output = Command::new("git")
      .args([
        "rev-parse",
        "--path-format=absolute",
        "--show-toplevel",
        "--git-common-dir",
      ])
      .current_dir(path)
      .output()
      .map_err(|error| format!("could not run git in {}: {error}", path.display()))?;
    if !output.status.success() {
      return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    let mut lines = output.stdout.split(|&byte| byte == b'\n');
    let mut next = || {
      let line = lines.next().filter(|line| !line.is_empty());
      let path = PathBuf::from(OsStr::from_bytes(line.ok_or("unexpected git rev-parse output")?));
      path
        .canonicalize()
        .map_err(|error| format!("{}: {error}", path.display()))
    };
    Ok(Self {
      toplevel: next()?,
      common_dir: next()?,
    })
  }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Worktree {
  pub name: String,
  pub path: PathBuf,
  /// `None` on a detached HEAD.
  pub branch: Option<String>,
  pub status: Status,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Status {
  pub is_dirty: bool,
}

/// Runs `gwm list --format=json` in `repo`. The error is a message for the user.
pub fn list(repo: &Path) -> Result<Vec<Worktree>, String> {
  let output = Command::new("gwm")
    .args(["list", "--format=json"])
    .current_dir(repo)
    .output()
    .map_err(|error| format!("could not run gwm: {error}"))?;
  if !output.status.success() {
    return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
  }
  parse(&output.stdout)
}

fn parse(json: &[u8]) -> Result<Vec<Worktree>, String> {
  serde_json::from_slice(json).map_err(|error| format!("unexpected gwm output: {error}"))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn clean(name: &str, path: &str, branch: Option<&str>) -> Worktree {
    Worktree {
      name: name.to_owned(),
      path: PathBuf::from(path),
      branch: branch.map(str::to_owned),
      status: Status { is_dirty: false },
    }
  }

  /// A real `gwm list --format=json` entry from gwm 1.11.0, whole.
  #[test]
  fn parses_the_real_output_of_gwm() {
    let json = br#"[{
      "name": "fix-57-rate-limit-headers",
      "id": "fix-57-rate-limit-headers",
      "path": "/Users/kbrdn1/gwm-demo/worktrees/acme-api/fix-57-rate-limit-headers",
      "branch": "fix/#57-rate-limit-headers",
      "head": "0aef1336019c0f5faae2a64ea6acf233a8b4ee76",
      "is_main": false,
      "is_locked": false,
      "is_prunable": false,
      "status": { "is_dirty": false, "has_upstream": false, "ahead": 0, "behind": 0, "unknown": false },
      "age_seconds": 1289704,
      "issue": 57,
      "pr": null
    }]"#;
    assert_eq!(
      parse(json),
      Ok(vec![clean(
        "fix-57-rate-limit-headers",
        "/Users/kbrdn1/gwm-demo/worktrees/acme-api/fix-57-rate-limit-headers",
        Some("fix/#57-rate-limit-headers")
      )])
    );
  }

  #[test]
  fn a_detached_head_has_no_branch() {
    let json = br#"[{ "name": "wt", "path": "/wt", "branch": null, "status": { "is_dirty": false } }]"#;
    assert_eq!(parse(json), Ok(vec![clean("wt", "/wt", None)]));
  }

  /// The contract's promise to consumers: a field added later must not break us.
  #[test]
  fn ignores_fields_added_after_this_build() {
    let json =
      br#"[{ "name": "wt", "path": "/wt", "branch": "main", "status": { "is_dirty": false }, "from_the_future": 1 }]"#;
    assert_eq!(parse(json), Ok(vec![clean("wt", "/wt", Some("main"))]));
  }

  #[test]
  fn malformed_output_is_an_error_not_an_empty_list() {
    assert!(parse(b"not json").unwrap_err().starts_with("unexpected gwm output"));
  }
}
