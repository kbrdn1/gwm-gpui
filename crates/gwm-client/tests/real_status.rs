//! [`gwm_client::status`] against a real `gwm`, on a throwaway repo with no
//! remote: gwm has no forge to ask, so nothing leaves the machine. Like the
//! other real tests, these need `gwm` on the PATH and fail without it.

use std::{
  fs,
  path::Path,
  process::{Command, Stdio},
  sync::Once,
};

use gwm_client::{IssueLink, Links};

/// No `~/.config/gwm/config.toml`: its settings stay out of the test.
fn isolate() {
  static ONCE: Once = Once::new();
  ONCE.call_once(|| {
    // SAFETY: every test calls `isolate` before it spawns a process or reads
    // the environment, and `Once` holds the others until this returns, so no
    // thread reads the environment while it is written. Nothing writes it after.
    unsafe { std::env::set_var("GWM_NO_GLOBAL_CONFIG", "1") };
  });
}

fn git(dir: &Path, args: &[&str]) {
  let status = Command::new("git")
    .args(["-c", "user.name=gwm-gpui", "-c", "user.email=gwm-gpui@example.invalid"])
    .args(args)
    .current_dir(dir)
    .stdout(Stdio::null())
    .status()
    .expect("git runs");
  assert!(status.success(), "git {args:?} failed");
}

/// The main worktree and a linked one, both read by path: gwm's name patterns
/// leave the main worktree out.
#[test]
fn reads_the_links_of_the_worktree_at_a_path() {
  isolate();
  let dir = tempfile::tempdir().unwrap();
  let acme = dir.path().join("acme");
  fs::create_dir(&acme).unwrap();
  git(&acme, &["init", "-q", "-b", "main"]);
  git(&acme, &["commit", "-q", "--allow-empty", "-m", "init"]);
  git(&acme, &["worktree", "add", "-q", "../feat-42", "-b", "feat/#42-thing"]);

  let main = gwm_client::status(&acme).expect("gwm is on the PATH: these tests drive a real gwm");
  assert_eq!(main, Links { issue: None, pr: None });
  let linked = gwm_client::status(&dir.path().join("feat-42")).unwrap();
  assert_eq!(
    linked,
    Links {
      // No remote: the link from the branch name, no live state.
      issue: Some(IssueLink {
        number: 42,
        state: None,
        title: None,
        url: None,
      }),
      pr: None,
    }
  );
}

#[test]
fn outside_a_repo_gwms_error_is_kept() {
  isolate();
  let dir = tempfile::tempdir().unwrap();
  let error = gwm_client::status(dir.path()).unwrap_err();
  assert!(error.contains("not inside a git repository"), "{error}");
}
