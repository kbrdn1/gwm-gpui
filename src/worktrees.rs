//! The worktree list, read through gwm's frozen machine contract
//! (`docs/schema/worktree-list.schema.json` in gwm-cli).
//!
//! Types declare only the fields the app reads and never `deny_unknown_fields`:
//! the contract adds optional fields without a version bump, and asks consumers
//! to ignore the ones they don't know.

use std::{path::Path, process::Command};

use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
pub struct Worktree {
  pub name: String,
  /// `None` on a detached HEAD.
  pub branch: Option<String>,
  pub status: Status,
}

#[derive(Debug, Deserialize, PartialEq)]
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

  fn clean(name: &str, branch: Option<&str>) -> Worktree {
    Worktree {
      name: name.to_owned(),
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
        Some("fix/#57-rate-limit-headers")
      )])
    );
  }

  #[test]
  fn a_detached_head_has_no_branch() {
    let json = br#"[{ "name": "wt", "branch": null, "status": { "is_dirty": false } }]"#;
    assert_eq!(parse(json), Ok(vec![clean("wt", None)]));
  }

  /// The contract's promise to consumers: a field added later must not break us.
  #[test]
  fn ignores_fields_added_after_this_build() {
    let json = br#"[{ "name": "wt", "branch": "main", "status": { "is_dirty": false }, "from_the_future": 1 }]"#;
    assert_eq!(parse(json), Ok(vec![clean("wt", Some("main"))]));
  }

  #[test]
  fn malformed_output_is_an_error_not_an_empty_list() {
    assert!(parse(b"not json").unwrap_err().starts_with("unexpected gwm output"));
  }
}
