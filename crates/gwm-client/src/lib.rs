//! The worktree list, read through gwm's frozen machine contract
//! (`docs/schema/worktree-list.schema.json` in gwm-cli): once with
//! `gwm list --format=json`, or live from the daemon ([`watch`]).
//!
//! Types declare only the fields the app reads and never `deny_unknown_fields`:
//! the contract adds optional fields without a version bump, and asks consumers
//! to ignore the ones they don't know.

mod create;
pub mod daemon;
mod status;
pub mod watch;

pub use create::{Create, Number, create};
pub use status::{IssueLink, IssueState, Links, PrLink, PrState, status};

use std::{
  ffi::OsStr,
  os::unix::ffi::OsStrExt,
  path::{Path, PathBuf},
  process::{Command, Stdio},
  sync::{Mutex, PoisonError},
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
      .stdin(Stdio::null())
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

  /// Whether `path` is in a bare repo: [`Repo::locate`] fails there, git
  /// having no work tree to name, while gwm lists it. False when git cannot
  /// tell.
  pub fn is_bare(path: &Path) -> bool {
    Command::new("git")
      .args(["rev-parse", "--is-bare-repository"])
      .current_dir(path)
      .stdin(Stdio::null())
      .output()
      .is_ok_and(|output| output.status.success() && output.stdout.trim_ascii() == b"true")
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
    .stdin(Stdio::null())
    .output()
    .map_err(|error| format!("could not run gwm: {error}"))?;
  if !output.status.success() {
    return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
  }
  parse(&output.stdout)
}

/// Held by the removal under way: one at a time in this process.
// ponytail: one lock for every repo; one per repo if a slow removal ever holds up another repo's.
static REMOVING: Mutex<()> = Mutex::new(());

/// Removes the worktree at `selected` with `gwm remove`, keeping its branch.
/// `repo` is where the app was opened: gwm runs there. The error is a message
/// for the user, gwm's stderr as is when gwm fails.
///
/// gwm removes by pattern, not by path, and a pattern that names no worktree
/// falls back to a substring match: `auth`, removed behind the app's back,
/// would take `auth-v2`. So a fresh list must name the selected worktree alone
/// first ([`removable`]).
///
/// `confirmed_dirty`: the user was told uncommitted changes would be lost. If
/// they were not, and the worktree has some now (or gwm cannot tell), nothing
/// is removed: the confirmation was for another state.
///
/// One removal at a time, from the list to gwm's return. A second one asked
/// for the same worktree then lists it gone and stops there. Run alongside,
/// it could list it, then name it to gwm once the first had removed it.
pub fn remove(repo: &Path, selected: &Path, confirmed_dirty: bool) -> Result<(), String> {
  let _one_at_a_time = REMOVING.lock().unwrap_or_else(PoisonError::into_inner);
  let opened_in = Repo::locate(repo).ok().map(|repo| repo.toplevel);
  let fresh = list(repo)?;
  let name = removable(&fresh, selected, opened_in.as_deref(), confirmed_dirty)?;
  let output = writing_gwm()
    .args(["remove", "--", name])
    .current_dir(repo)
    .output()
    .map_err(|error| format!("could not run gwm: {error}"))?;
  if !output.status.success() {
    return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
  }
  Ok(())
}

/// The name that makes `gwm remove` resolve to the worktree at `selected` and
/// nothing else, per `fresh`, a list taken just now: its name is unique and no
/// other worktree's id. Stricter than gwm, which leaves the main worktree out
/// of its name match. `opened_in`: the worktree the app runs gwm, and its
/// daemon, from, which it does not pull from under itself. `confirmed_dirty`:
/// see [`remove`].
fn removable<'a>(
  fresh: &'a [Worktree],
  selected: &Path,
  opened_in: Option<&Path>,
  confirmed_dirty: bool,
) -> Result<&'a str, String> {
  let Some(target) = fresh.iter().find(|worktree| worktree.path == selected) else {
    return Err(format!("{} is no longer a worktree of this repo", selected.display()));
  };
  let name = target.name.as_str();
  if target.is_main {
    return Err("the main worktree is not removed from here".to_owned());
  }
  if opened_in == Some(target.path.as_path()) {
    return Err(format!(
      "the app was opened in {name}: open it on another worktree to remove this one"
    ));
  }
  if !confirmed_dirty && (target.status.is_dirty || target.status.unknown) {
    return Err(format!(
      "{name} has uncommitted changes since you confirmed, or gwm cannot tell: nothing was removed, confirm again"
    ));
  }
  let namesakes = fresh.iter().filter(|worktree| worktree.name == name).count();
  if namesakes > 1 {
    return Err(format!(
      "{namesakes} worktrees are named {name}: gwm cannot tell them apart, remove it with gwm by its id"
    ));
  }
  if let Some(other) = fresh
    .iter()
    .find(|worktree| worktree.id == name && worktree.path != target.path)
  {
    return Err(format!(
      "{name} is also the id of the worktree at {}: gwm cannot tell them apart",
      other.path.display()
    ));
  }
  Ok(name)
}

/// `gwm` for a command that writes and may run `.gwm.toml`'s hooks or
/// bootstrap. Stdin closed: gwm refuses what no one approved instead of
/// asking on the terminal the app was launched from. `GWM_ALLOW_BOOTSTRAP`
/// removed: gwm takes it for `--allow-bootstrap`, which the app never passes,
/// and an export in the user's shell would otherwise reach gwm through the app.
/// The daemon is not spawned through this: its methods (`list`, `doctor`,
/// `path`, `subscribe`) read, and run neither hooks nor bootstrap.
fn writing_gwm() -> Command {
  let mut gwm = Command::new("gwm");
  gwm.stdin(Stdio::null()).env_remove("GWM_ALLOW_BOOTSTRAP");
  gwm
}

fn parse(json: &[u8]) -> Result<Vec<Worktree>, String> {
  serde_json::from_slice(json).map_err(|error| format!("unexpected gwm output: {error}"))
}

#[cfg(test)]
mod tests {
  use std::{sync::mpsc, thread, time::Duration};

  use super::*;

  fn git_init(args: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let status = Command::new("git")
      .args(["init", "-q"])
      .args(args)
      .current_dir(dir.path())
      .stdin(Stdio::null())
      .status()
      .unwrap();
    assert!(status.success());
    dir
  }

  /// Against a real git: `locate` places the work tree alone, and `is_bare`
  /// names the bare repo alone.
  #[test]
  fn a_bare_repo_is_told_from_a_work_tree_and_from_no_repo() {
    let bare = git_init(&["--bare"]);
    assert!(Repo::locate(bare.path()).is_err());
    assert!(Repo::is_bare(bare.path()));
    let work_tree = git_init(&[]);
    assert!(Repo::locate(work_tree.path()).is_ok());
    assert!(!Repo::is_bare(work_tree.path()));
    let plain = tempfile::tempdir().unwrap();
    assert!(Repo::locate(plain.path()).is_err());
    assert!(!Repo::is_bare(plain.path()));
  }

  #[test]
  fn a_second_removal_waits_for_the_one_under_way() {
    let first = REMOVING.lock().unwrap_or_else(PoisonError::into_inner);
    let (done, finished) = mpsc::channel();
    let second = thread::spawn(move || {
      // No repo there: it fails as soon as it runs.
      let result = remove(Path::new("/no/such/repo"), Path::new("/no/such/repo/feat-1"), false);
      let _ = done.send(());
      result
    });
    assert!(
      finished.recv_timeout(Duration::from_millis(300)).is_err(),
      "it ran while the first was under way"
    );
    drop(first);
    assert!(second.join().unwrap().is_err());
  }

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

  /// `gwm list --format=json` from gwm 1.11.0, whole, once `auth` was removed
  /// behind the app's back: `auth-v2` remains.
  const AFTER_AUTH: &str = r#"[
  {
    "name": "acme",
    "id": "acme",
    "path": "/private/var/folders/q1/7spjwgsd7dn1cw9rgxc_tphh0000gn/T/gwm7.DhBV/acme/",
    "branch": "main",
    "head": "b146ef6734df72505623c2794e34b2bbded1b567",
    "is_main": true,
    "is_locked": false,
    "is_prunable": false,
    "status": {
      "is_dirty": false,
      "has_upstream": false,
      "ahead": 0,
      "behind": 0,
      "unknown": false
    },
    "age_seconds": null,
    "issue": null,
    "pr": null
  },
  {
    "name": "auth-v2",
    "id": "auth-v2",
    "path": "/private/var/folders/q1/7spjwgsd7dn1cw9rgxc_tphh0000gn/T/gwm7.DhBV/auth-v2",
    "branch": "feat/auth-v2",
    "head": "b146ef6734df72505623c2794e34b2bbded1b567",
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
    "age_seconds": null,
    "issue": null,
    "pr": null
  }
]"#;

  const DIR: &str = "/private/var/folders/q1/7spjwgsd7dn1cw9rgxc_tphh0000gn/T/gwm7.DhBV";

  fn after_auth() -> Vec<Worktree> {
    parse(AFTER_AUTH.as_bytes()).unwrap()
  }

  #[test]
  fn a_worktree_gone_from_the_fresh_list_is_refused_not_matched_by_substring() {
    let auth = PathBuf::from(format!("{DIR}/auth"));
    let refused = removable(&after_auth(), &auth, None, false).unwrap_err();
    assert!(refused.contains("no longer a worktree"), "{refused}");
  }

  #[test]
  fn a_worktree_named_alone_is_removed_by_its_name() {
    let auth_v2 = PathBuf::from(format!("{DIR}/auth-v2"));
    assert_eq!(removable(&after_auth(), &auth_v2, None, false), Ok("auth-v2"));
  }

  #[test]
  fn the_main_worktree_and_the_one_the_app_runs_in_are_refused() {
    let worktrees = after_auth();
    // gwm prints main with a trailing slash, the selection may not have it.
    let main = PathBuf::from(format!("{DIR}/acme"));
    assert!(
      removable(&worktrees, &main, None, false)
        .unwrap_err()
        .contains("main worktree")
    );
    let auth_v2 = PathBuf::from(format!("{DIR}/auth-v2"));
    assert!(
      removable(&worktrees, &auth_v2, Some(&auth_v2), false)
        .unwrap_err()
        .contains("opened in")
    );
  }

  /// Confirmed clean, found dirty or unknown: refused. Confirmed dirty: removed.
  #[test]
  fn changes_made_since_a_clean_confirmation_are_not_dropped() {
    let auth_v2 = PathBuf::from(format!("{DIR}/auth-v2"));
    for (dirty, unknown) in [(true, false), (false, true)] {
      let mut worktrees = after_auth();
      worktrees[1].status.is_dirty = dirty;
      worktrees[1].status.unknown = unknown;
      let refused = removable(&worktrees, &auth_v2, None, false).unwrap_err();
      assert!(refused.contains("since you confirmed"), "{refused}");
      assert_eq!(removable(&worktrees, &auth_v2, None, true), Ok("auth-v2"));
    }
  }

  /// Two directories of the same name in different places: gwm calls the name
  /// ambiguous, and another worktree's id equal to the name shadows it.
  #[test]
  fn a_name_another_worktree_shares_or_bears_as_id_is_refused() {
    let mut worktrees = after_auth();
    let mut namesake = worktrees[1].clone();
    namesake.id = "auth-v2-1".to_owned();
    namesake.path = PathBuf::from("/elsewhere/auth-v2");
    worktrees.push(namesake.clone());
    let auth_v2 = PathBuf::from(format!("{DIR}/auth-v2"));
    assert!(
      removable(&worktrees, &auth_v2, None, false)
        .unwrap_err()
        .contains("2 worktrees are named")
    );

    // Moved by `git worktree move`: the old slug stays as its id.
    worktrees.pop();
    namesake.name = "auth-v3".to_owned();
    namesake.id = "auth-v2".to_owned();
    worktrees.push(namesake);
    assert!(
      removable(&worktrees, &auth_v2, None, false)
        .unwrap_err()
        .contains("also the id")
    );
  }
}
