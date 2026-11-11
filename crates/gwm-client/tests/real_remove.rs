//! [`gwm_client::remove`] against a real `gwm`, on throwaway repos. Like
//! `real_daemon.rs`, these need `gwm` on the PATH and fail without it.
//!
//! `gwm remove` writes to the user's undo journal (capped at 100 entries) and
//! reads their trust ledger: every test here calls [`isolate`] first, so both
//! are files of this test run instead.

use std::{
  fs,
  path::{Path, PathBuf},
  process::{Command, Stdio},
  sync::{Once, mpsc},
  thread,
  time::Duration,
};

use gwm_client::watch::{Event, Sockets, Source, Spawned, watch};

/// Points gwm's undo journal and trust ledger at files under cargo's
/// per-target scratch directory, before anything in this binary runs gwm.
fn isolate() {
  static ONCE: Once = Once::new();
  ONCE.call_once(|| {
    let scratch = Path::new(env!("CARGO_TARGET_TMPDIR")).join("gwm-client-real-remove");
    fs::create_dir_all(&scratch).unwrap();
    // SAFETY: every test calls `isolate` before it spawns a process or reads
    // the environment, and `Once` holds the others until this returns, so no
    // thread reads the environment while it is written. Nothing writes it after.
    unsafe {
      std::env::set_var("GWM_HISTORY_FILE", scratch.join("history.toml"));
      std::env::set_var("GWM_TRUST_LEDGER", scratch.join("trust-ledger.toml"));
    }
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

/// `acme` with one commit, and a linked worktree beside it per name, each on
/// its own `feat/<name>` branch.
fn acme(dir: &Path, linked: &[&str]) -> PathBuf {
  let acme = dir.join("acme");
  fs::create_dir(&acme).unwrap();
  git(&acme, &["init", "-q", "-b", "main"]);
  git(&acme, &["commit", "-q", "--allow-empty", "-m", "init"]);
  for name in linked {
    git(
      &acme,
      &[
        "worktree",
        "add",
        "-q",
        &format!("../{name}"),
        "-b",
        &format!("feat/{name}"),
      ],
    );
  }
  acme.canonicalize().unwrap()
}

/// The path gwm lists for `name`, as the app holds it.
fn path_of(repo: &Path, name: &str) -> PathBuf {
  let worktrees = gwm_client::list(repo).expect("gwm is on the PATH: these tests drive a real gwm");
  worktrees
    .into_iter()
    .find(|worktree| worktree.name == name)
    .unwrap()
    .path
}

fn names(repo: &Path) -> Vec<String> {
  gwm_client::list(repo)
    .unwrap()
    .into_iter()
    .map(|worktree| worktree.name)
    .collect()
}

/// `gwm remove auth` falls back to a substring match once `auth` is gone: the
/// app, still showing `auth`, must not take `auth-v2` with it.
#[test]
fn refuses_a_worktree_removed_behind_its_back_instead_of_its_namesake() {
  isolate();
  let dir = tempfile::tempdir().unwrap();
  let acme = acme(dir.path(), &["auth", "auth-v2"]);
  let auth = path_of(&acme, "auth");
  git(&acme, &["worktree", "remove", "../auth"]);

  let refused = gwm_client::remove(&acme, &auth, false).unwrap_err();
  assert!(refused.contains("no longer"), "{refused}");
  assert_eq!(names(&acme), ["acme", "auth-v2"]);
}

/// gwm 1.11.0 removes a dirty worktree, untracked files and all. The branch stays.
#[test]
fn removes_the_selected_worktree_and_keeps_its_branch() {
  isolate();
  let dir = tempfile::tempdir().unwrap();
  let acme = acme(dir.path(), &["auth", "auth-v2"]);
  let auth = path_of(&acme, "auth");
  fs::write(auth.join("draft.txt"), "uncommitted").unwrap();

  assert_eq!(gwm_client::remove(&acme, &auth, true), Ok(()));
  assert_eq!(names(&acme), ["acme", "auth-v2"]);
  assert!(!auth.exists());
  git(&acme, &["rev-parse", "--verify", "-q", "refs/heads/feat/auth"]);
}

/// Confirmed while clean, then edited before gwm ran: the confirmation did
/// not cover those changes, so nothing is removed.
#[test]
fn refuses_changes_made_after_a_clean_confirmation() {
  isolate();
  let dir = tempfile::tempdir().unwrap();
  let acme = acme(dir.path(), &["auth"]);
  let auth = path_of(&acme, "auth");
  fs::write(auth.join("draft.txt"), "written after the confirmation").unwrap();

  let refused = gwm_client::remove(&acme, &auth, false).unwrap_err();
  assert!(refused.contains("since you confirmed"), "{refused}");
  assert_eq!(names(&acme), ["acme", "auth"]);
  assert!(auth.join("draft.txt").exists());
}

/// The app runs gwm, and its daemon, from where it was opened: removing that
/// worktree would pull the ground from under both.
#[test]
fn refuses_the_worktree_the_app_was_opened_in() {
  isolate();
  let dir = tempfile::tempdir().unwrap();
  let acme = acme(dir.path(), &["auth"]);
  let auth = path_of(&acme, "auth");
  fs::create_dir(auth.join("src")).unwrap();

  let refused = gwm_client::remove(&auth.join("src"), &auth, false).unwrap_err();
  assert!(refused.contains("opened in"), "{refused}");
  assert_eq!(names(&acme), ["acme", "auth"]);
}

#[test]
fn refuses_the_main_worktree() {
  isolate();
  let dir = tempfile::tempdir().unwrap();
  let acme = acme(dir.path(), &["auth"]);
  let main = path_of(&acme, "acme");

  let refused = gwm_client::remove(&path_of(&acme, "auth"), &main, false).unwrap_err();
  assert!(refused.contains("main worktree"), "{refused}");
  assert_eq!(names(&acme), ["acme", "auth"]);
}

/// A `pre_remove` hook from a `.gwm.toml` no one approved: gwm refuses, as it
/// should with no one to ask, and its own words reach the user.
#[test]
fn a_refusal_from_gwm_comes_back_as_gwm_wrote_it() {
  isolate();
  let dir = tempfile::tempdir().unwrap();
  let acme = acme(dir.path(), &[]);
  fs::write(
    acme.join(".gwm.toml"),
    "[[hooks.pre_remove]]\nname = \"keep\"\nrun = \"exit 3\"\n",
  )
  .unwrap();
  git(&acme, &["add", ".gwm.toml"]);
  git(&acme, &["commit", "-q", "-m", "hooks"]);
  git(&acme, &["worktree", "add", "-q", "../auth", "-b", "feat/auth"]);

  let refused = gwm_client::remove(&acme, &path_of(&acme, "auth"), false).unwrap_err();
  assert!(
    refused.starts_with("error: ") && refused.contains("trust ledger"),
    "{refused}"
  );
  assert_eq!(names(&acme), ["acme", "auth"]);
}

/// The app removes through `gwm remove`, and reloads nothing: the daemon's next
/// snapshot carries the change.
#[test]
fn a_removal_through_gwm_shows_in_the_next_snapshot() {
  isolate();
  let dir = tempfile::tempdir().unwrap();
  let acme = acme(dir.path(), &["feat-1"]);
  let sockets = Sockets {
    default: dir.path().join("default.sock"),
    own: Some(dir.path().join("own.sock")),
  };
  let spawned = Spawned::default();
  // Stops the daemon `watch` starts even when an assertion fails first.
  struct StopOnDrop(Spawned);
  impl Drop for StopOnDrop {
    fn drop(&mut self) {
      self.0.stop();
    }
  }
  let _stop = StopOnDrop(spawned.clone());
  let (events, received) = mpsc::channel();
  let watcher = {
    let (acme, spawned) = (acme.clone(), spawned.clone());
    thread::spawn(move || watch(&acme, |_| sockets, &spawned, |event| events.send(event).is_ok()))
  };
  let snapshot = || loop {
    match received
      .recv_timeout(Duration::from_secs(20))
      .expect("an event within 20 s")
    {
      Event::Snapshot(worktrees) => break worktrees.into_iter().map(|worktree| worktree.name).collect::<Vec<_>>(),
      Event::Source(Source::OwnDaemon(_)) => {}
      other => panic!("unexpected {other:?}"),
    }
  };

  assert_eq!(snapshot(), ["acme", "feat-1"]);
  let removed = gwm_client::remove(&acme, &path_of(&acme, "feat-1"), false);
  let after = snapshot();
  drop(received);
  spawned.stop();
  watcher.join().unwrap();
  assert_eq!(removed, Ok(()));
  assert_eq!(after, ["acme"]);
}
