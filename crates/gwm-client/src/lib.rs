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
  /// The directory's name.
  pub name: String,
  /// git's id for the worktree: unlike `name`, it survives a `git worktree move`,
  /// so a selection keys on it.
  pub id: String,
  pub path: PathBuf,
  /// `None` on a detached HEAD.
  pub branch: Option<String>,
  /// The full HEAD oid, `None` when unresolvable.
  pub head: Option<String>,
  pub is_main: bool,
  pub is_locked: bool,
  pub is_prunable: bool,
  pub status: Status,
  /// The branch's age against the trunk baseline. `None` on the trunk.
  pub age_seconds: Option<u64>,
  pub issue: Option<u64>,
  pub pr: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Status {
  pub is_dirty: bool,
  pub has_upstream: bool,
  pub ahead: u32,
  pub behind: u32,
  /// The status could not be computed (detached HEAD, unborn branch).
  pub unknown: bool,
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

  /// The real entry of `parses_the_real_output_of_gwm`, as parsed.
  fn fix_57() -> Worktree {
    Worktree {
      name: "fix-57-rate-limit-headers".to_owned(),
      id: "fix-57-rate-limit-headers".to_owned(),
      path: PathBuf::from("/Users/kbrdn1/gwm-demo/worktrees/acme-api/fix-57-rate-limit-headers"),
      branch: Some("fix/#57-rate-limit-headers".to_owned()),
      head: Some("0aef1336019c0f5faae2a64ea6acf233a8b4ee76".to_owned()),
      is_main: false,
      is_locked: false,
      is_prunable: false,
      status: Status {
        is_dirty: false,
        has_upstream: false,
        ahead: 0,
        behind: 0,
        unknown: false,
      },
      age_seconds: Some(1289704),
      issue: Some(57),
      pr: None,
    }
  }

  const REAL: &str = r#"[{
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

  /// A real `gwm list --format=json` entry from gwm 1.11.0, whole.
  /// A real `gwm list --format=json` entry from gwm 1.11.0, whole.
  #[test]
  fn parses_the_real_output_of_gwm() {
    assert_eq!(parse(REAL.as_bytes()), Ok(vec![fix_57()]));
  }

  #[test]
  fn a_detached_head_has_no_branch() {
    let json = REAL.replace(r#""branch": "fix/#57-rate-limit-headers""#, r#""branch": null"#);
    assert_eq!(parse(json.as_bytes()).unwrap()[0].branch, None);
  }

  /// The contract's promise to consumers: a field added later must not break us.
  #[test]
  fn ignores_fields_added_after_this_build() {
    let json = REAL.replace(r#""pr": null"#, r#""pr": null, "from_the_future": 1"#);
    assert_eq!(parse(json.as_bytes()), Ok(vec![fix_57()]));
  }

  /// A field the contract requires is not optional here either: its absence is
  /// a broken contract, shown, not a default.
  #[test]
  fn a_missing_required_field_is_an_error() {
    let json = REAL.replace(r#""is_main": false,"#, "");
    assert!(parse(json.as_bytes()).unwrap_err().contains("is_main"));
  }

  #[test]
  fn malformed_output_is_an_error_not_an_empty_list() {
    assert!(parse(b"not json").unwrap_err().starts_with("unexpected gwm output"));
  }
}
