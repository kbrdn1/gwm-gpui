//! `gwm create`, a subprocess, in its three modes, and how the app finds the
//! worktree it made in the next list: from the list's fields, never from what
//! gwm prints.

use std::path::{Path, PathBuf};

use crate::Worktree;

/// What to create, checked as far as the app can without asking gwm: the
/// rest (a type gwm does not know, a name git refuses) is gwm's to say.
#[derive(Clone, Debug, PartialEq)]
pub enum Create {
  /// `gwm create --issue <N>`: gwm reads the type and description from the
  /// issue on the forge. A worktree that already carries the number is kept.
  Issue(Number),
  /// `gwm create <type> <issue> <desc>`.
  Triple { kind: String, issue: Number, desc: String },
  /// `gwm create --name <name>`: the name is the branch, verbatim.
  Name(String),
}

/// An issue number: the digits as typed, passed to gwm as they are (`007`
/// makes `feat/#007-…`), and their value, which gwm lists as `issue`.
#[derive(Clone, Debug, PartialEq)]
pub struct Number {
  pub typed: String,
  pub value: u64,
}

impl Number {
  pub fn parse(text: &str) -> Result<Self, String> {
    let typed = text.trim();
    if typed.is_empty() {
      return Err("The issue number is missing.".to_owned());
    }
    // `u64::from_str` takes a leading `+`: gwm wants digits only.
    if !typed.bytes().all(|byte| byte.is_ascii_digit()) {
      return Err(format!("The issue number is digits only, not {typed:?}."));
    }
    let value = typed
      .parse()
      .map_err(|_| format!("{typed} is too large for an issue number."))?;
    Ok(Self {
      typed: typed.to_owned(),
      value,
    })
  }
}

fn required(what: &str, text: &str) -> Result<String, String> {
  let text = text.trim();
  if text.is_empty() {
    return Err(format!("The {what} is missing."));
  }
  Ok(text.to_owned())
}

impl Create {
  pub fn issue(issue: &str) -> Result<Self, String> {
    Ok(Self::Issue(Number::parse(issue)?))
  }

  pub fn triple(kind: &str, issue: &str, desc: &str) -> Result<Self, String> {
    Ok(Self::Triple {
      kind: required("type", kind)?,
      issue: Number::parse(issue)?,
      desc: required("description", desc)?,
    })
  }

  pub fn name(name: &str) -> Result<Self, String> {
    Ok(Self::Name(required("name", name)?))
  }

  /// The arguments after `gwm create`. A value given with `=` or after `--`
  /// is never read as a flag, whatever it starts with.
  pub fn args(&self) -> Vec<String> {
    match self {
      Self::Issue(issue) => vec![format!("--issue={}", issue.typed)],
      Self::Triple { kind, issue, desc } => vec!["--".to_owned(), kind.clone(), issue.typed.clone(), desc.clone()],
      Self::Name(name) => vec![format!("--name={name}")],
    }
  }

  /// The worktree this creation made, in `worktrees`, a list taken after it
  /// started; `known` are the paths listed before. A triple or a name makes a
  /// new worktree (gwm makes a second one for an issue that already has one),
  /// `--issue` may also have kept the one that was there.
  pub fn created<'a>(&self, worktrees: &'a [Worktree], known: &[PathBuf]) -> Option<&'a Worktree> {
    let is_new = |worktree: &&Worktree| !known.contains(&worktree.path);
    let matches = |worktree: &&Worktree| match self {
      Self::Issue(issue) | Self::Triple { issue, .. } => worktree.issue == Some(issue.value),
      Self::Name(name) => worktree.branch.as_deref() == Some(name.as_str()),
    };
    let fresh = worktrees.iter().filter(matches).find(is_new);
    match self {
      Self::Issue(_) => fresh.or_else(|| worktrees.iter().find(matches)),
      Self::Triple { .. } | Self::Name(_) => fresh,
    }
  }
}

/// Runs `gwm create` in `repo`, stdin closed and `GWM_ALLOW_BOOTSTRAP` removed
/// (see `writing_gwm`): gwm then refuses a `.gwm.toml` no one approved. The
/// error is gwm's stderr as is, with how to approve when that is why.
pub fn create(repo: &Path, request: &Create) -> Result<(), String> {
  let output = crate::writing_gwm()
    .arg("create")
    .args(request.args())
    .current_dir(repo)
    .output()
    .map_err(|error| format!("could not run gwm: {error}"))?;
  if output.status.success() {
    return Ok(());
  }
  let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
  if stderr.is_empty() {
    return Err(format!("gwm create failed ({})", output.status));
  }
  Err(with_trust_hint(stderr, repo))
}

/// gwm's refusal names `--allow-bootstrap`, which the app never passes, and
/// an interactive run, which it has none of: the way that is left.
fn with_trust_hint(stderr: String, repo: &Path) -> String {
  if !stderr.contains("not in the trust ledger") {
    return stderr;
  }
  // The app runs gwm in "." when opened from a repo with no argument.
  let dir = crate::Repo::locate(repo).map_or_else(|_| repo.to_owned(), |repo| repo.toplevel);
  format!(
    "{stderr}\n\nTo approve it, read its bootstrap commands, then run `gwm trust add` in a terminal in {}.",
    dir.display()
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  /// `gwm list --format=json` from gwm 1.11.0 on a throwaway repo after
  /// `gwm create feat 42 add-login`, `gwm create feat 42 again` and
  /// `gwm create --name team/spike`, whole but for the main worktree.
  const AFTER_CREATES: &str = r#"[
  {
    "name": "team-spike",
    "id": "team-spike",
    "path": "/private/var/folders/q1/7spjwgsd7dn1cw9rgxc_tphh0000gn/T/gwm9-probe.ObYU/acme-worktrees/team-spike",
    "branch": "team/spike",
    "head": "f9daf7d7409d6b6ca2bd0d8dde210475f816a0fb",
    "is_main": false,
    "is_locked": false,
    "is_prunable": false,
    "status": {
      "is_dirty": false,
      "has_upstream": false,
      "ahead": 0,
      "behind": 0,
      "unknown": false
    },
    "age_seconds": 0,
    "issue": null,
    "pr": null
  },
  {
    "name": "feat-42-again",
    "id": "feat-42-again",
    "path": "/private/var/folders/q1/7spjwgsd7dn1cw9rgxc_tphh0000gn/T/gwm9-probe.ObYU/acme-worktrees/feat-42-again",
    "branch": "feat/#42-again",
    "head": "f9daf7d7409d6b6ca2bd0d8dde210475f816a0fb",
    "is_main": false,
    "is_locked": false,
    "is_prunable": false,
    "status": {
      "is_dirty": false,
      "has_upstream": false,
      "ahead": 0,
      "behind": 0,
      "unknown": false
    },
    "age_seconds": 0,
    "issue": 42,
    "pr": null
  },
  {
    "name": "feat-42-add-login",
    "id": "feat-42-add-login",
    "path": "/private/var/folders/q1/7spjwgsd7dn1cw9rgxc_tphh0000gn/T/gwm9-probe.ObYU/acme-worktrees/feat-42-add-login",
    "branch": "feat/#42-add-login",
    "head": "f9daf7d7409d6b6ca2bd0d8dde210475f816a0fb",
    "is_main": false,
    "is_locked": false,
    "is_prunable": false,
    "status": {
      "is_dirty": false,
      "has_upstream": false,
      "ahead": 0,
      "behind": 0,
      "unknown": false
    },
    "age_seconds": 0,
    "issue": 42,
    "pr": null
  }
]"#;

  fn after_creates() -> Vec<Worktree> {
    crate::parse(AFTER_CREATES.as_bytes()).unwrap()
  }

  fn named(worktree: Option<&Worktree>) -> Option<&str> {
    worktree.map(|worktree| worktree.name.as_str())
  }

  fn path_of(worktrees: &[Worktree], name: &str) -> PathBuf {
    worktrees
      .iter()
      .find(|worktree| worktree.name == name)
      .unwrap()
      .path
      .clone()
  }

  #[test]
  fn each_mode_builds_its_own_arguments() {
    assert_eq!(Create::issue(" 594 ").unwrap().args(), ["--issue=594"]);
    assert_eq!(
      Create::triple("feat", "42", "Add login").unwrap().args(),
      ["--", "feat", "42", "Add login"]
    );
    assert_eq!(Create::name("team/spike").unwrap().args(), ["--name=team/spike"]);
  }

  /// gwm makes `feat/#007-lead` of `007`, and lists it as issue 7.
  #[test]
  fn the_issue_number_reaches_gwm_as_typed() {
    let triple = Create::triple("feat", "007", "lead").unwrap();
    assert_eq!(triple.args()[2], "007");
    assert_eq!(Number::parse("007").unwrap().value, 7);
  }

  #[test]
  fn an_issue_is_digits_only() {
    for typed in [
      "",
      "  ",
      "#42",
      "+42",
      "-1",
      "4 2",
      "42a",
      "٤٢",
      "99999999999999999999999",
    ] {
      assert!(Create::issue(typed).is_err(), "{typed:?} was taken");
    }
  }

  /// gwm checks the type and the name: the app only refuses what is empty.
  #[test]
  fn an_empty_field_is_refused_the_rest_is_gwms_to_judge() {
    assert!(Create::triple(" ", "42", "x").unwrap_err().contains("type"));
    assert!(Create::triple("feat", "42", "").unwrap_err().contains("description"));
    assert!(Create::name("").unwrap_err().contains("name"));
    assert!(Create::triple("nope", "42", "x").is_ok());
    assert!(Create::name("-dash").is_ok());
  }

  /// A second `feat 42` makes a second worktree for #42: the new one is
  /// selected, never the one that was there.
  #[test]
  fn a_triple_selects_the_new_worktree_for_its_issue() {
    let worktrees = after_creates();
    let triple = Create::triple("feat", "42", "again").unwrap();
    let known = [path_of(&worktrees, "feat-42-add-login")];
    assert_eq!(named(triple.created(&worktrees, &known)), Some("feat-42-again"));
    // Before the list shows it: nothing, not the older one.
    let known = [
      path_of(&worktrees, "feat-42-add-login"),
      path_of(&worktrees, "feat-42-again"),
    ];
    assert_eq!(named(triple.created(&worktrees, &known)), None);
  }

  /// `gwm create --issue 42` with a worktree for #42 prints it and exits 0.
  #[test]
  fn from_an_issue_an_existing_worktree_is_selected() {
    let worktrees = after_creates();
    let known: Vec<PathBuf> = worktrees.iter().map(|worktree| worktree.path.clone()).collect();
    let issue = Create::issue("42").unwrap();
    assert!(issue.created(&worktrees, &known).is_some());
    assert_eq!(named(issue.created(&worktrees, &known[..2])), Some("feat-42-add-login"));
    assert_eq!(named(Create::issue("43").unwrap().created(&worktrees, &known)), None);
  }

  /// The name is the branch: the directory, and gwm's `name`, lose the slash.
  #[test]
  fn a_name_is_matched_on_the_branch() {
    let worktrees = after_creates();
    let request = Create::name("team/spike").unwrap();
    assert_eq!(named(request.created(&worktrees, &[])), Some("team-spike"));
    assert_eq!(
      named(Create::name("team-spike").unwrap().created(&worktrees, &[])),
      None
    );
  }

  /// gwm 1.11.0's refusal, whole.
  #[test]
  fn an_unapproved_gwm_toml_says_how_to_approve_it() {
    let refusal = "error: .gwm.toml at /tmp/acme/.gwm.toml is not in the trust ledger and stdin is not \
                   interactive: pass --allow-bootstrap (or set GWM_ALLOW_BOOTSTRAP=1) to bypass, or run \
                   interactively to approve";
    let shown = with_trust_hint(refusal.to_owned(), Path::new("/tmp/acme"));
    assert!(shown.starts_with(refusal), "{shown}");
    assert!(
      shown.ends_with("run `gwm trust add` in a terminal in /tmp/acme."),
      "{shown}"
    );
    let other = "error: invalid branch type 'nope'";
    assert_eq!(with_trust_hint(other.to_owned(), Path::new("/tmp/acme")), other);
  }

  /// The app opened with no argument runs gwm in ".": the hint names the
  /// repo's directory, not ".". Unit tests run in this crate, inside a repo.
  #[test]
  fn the_trust_hint_names_the_repo_not_a_relative_path() {
    let shown = with_trust_hint("error: x is not in the trust ledger".to_owned(), Path::new("."));
    let toplevel = crate::Repo::locate(Path::new(".")).unwrap().toplevel;
    assert!(
      shown.ends_with(&format!("in a terminal in {}.", toplevel.display())),
      "{shown}"
    );
  }
}
