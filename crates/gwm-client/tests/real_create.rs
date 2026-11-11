//! [`gwm_client::create`] against a real `gwm`, on throwaway repos. Like
//! `real_remove.rs`, these need `gwm` on the PATH and fail without it.
//!
//! `gwm create` writes to the user's undo journal and reads their trust
//! ledger: every test here calls [`isolate`] first, so both are files of this
//! test run instead. `--issue` needs a forge: it is only run where gwm refuses
//! before reaching one.

use std::{
  fs,
  path::{Path, PathBuf},
  process::{Command, Stdio},
  sync::Once,
};

use gwm_client::Create;

/// Points gwm's undo journal and trust ledger at files under cargo's
/// per-target scratch directory, before anything in this binary runs gwm.
fn isolate() -> PathBuf {
  static ONCE: Once = Once::new();
  let scratch = Path::new(env!("CARGO_TARGET_TMPDIR")).join("gwm-client-real-create");
  ONCE.call_once(|| {
    fs::create_dir_all(&scratch).unwrap();
    // SAFETY: every test calls `isolate` before it spawns a process or reads
    // the environment, and `Once` holds the others until this returns, so no
    // thread reads the environment while it is written. Nothing writes it after.
    unsafe {
      std::env::set_var("GWM_HISTORY_FILE", scratch.join("history.toml"));
      std::env::set_var("GWM_TRUST_LEDGER", scratch.join("trust-ledger.toml"));
      // gwm takes it for `--allow-bootstrap`. Set here, whatever the user's
      // shell exports: the client must strip it, and the trust tests prove it.
      std::env::set_var("GWM_ALLOW_BOOTSTRAP", "1");
      // No `~/.config/gwm/config.toml`: its base, its hooks stay out of the
      // tests, on this machine as on CI.
      std::env::set_var("GWM_NO_GLOBAL_CONFIG", "1");
    }
  });
  scratch.join("trust-ledger.toml")
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

/// Where gwm puts `acme`'s worktrees: beside it, in the test's directory.
/// gwm's default, without the user's config, is `{home}/cc-worktree/{repo}`.
const BASE: &str = "[worktree]\nbase = \"{repo_parent}/{repo}-worktrees\"\n";

/// `acme` with one commit, holding a `.gwm.toml` of [`BASE`] and `config`.
fn acme(dir: &Path, config: &str) -> PathBuf {
  let acme = dir.join("acme");
  fs::create_dir(&acme).unwrap();
  git(&acme, &["init", "-q", "-b", "main"]);
  fs::write(acme.join(".gwm.toml"), format!("{BASE}{config}")).unwrap();
  git(&acme, &["add", ".gwm.toml"]);
  git(&acme, &["commit", "-q", "-m", "init"]);
  acme.canonicalize().unwrap()
}

fn list(repo: &Path) -> Vec<gwm_client::Worktree> {
  gwm_client::list(repo).expect("gwm is on the PATH: these tests drive a real gwm")
}

fn paths(repo: &Path) -> Vec<PathBuf> {
  list(repo).into_iter().map(|worktree| worktree.path).collect()
}

/// Created, then found in the next list by its issue, as the app selects it.
#[test]
fn creates_from_a_type_an_issue_and_a_description() {
  isolate();
  let dir = tempfile::tempdir().unwrap();
  let acme = acme(dir.path(), "");
  let known = paths(&acme);
  let request = Create::triple("feat", "42", "Add login").unwrap();

  assert_eq!(gwm_client::create(&acme, &request), Ok(()));
  let worktrees = list(&acme);
  let created = request.created(&worktrees, &known).expect("the new worktree is listed");
  assert_eq!(created.branch.as_deref(), Some("feat/#42-add-login"));
  assert!(created.path.is_dir());
  assert!(
    created.path.starts_with(dir.path().canonicalize().unwrap()),
    "{}",
    created.path.display()
  );
}

/// A free name with a slash: the branch keeps it, the directory does not.
#[test]
fn creates_with_a_free_name() {
  isolate();
  let dir = tempfile::tempdir().unwrap();
  let acme = acme(dir.path(), "");
  let known = paths(&acme);
  let request = Create::name("team/spike").unwrap();

  assert_eq!(gwm_client::create(&acme, &request), Ok(()));
  let worktrees = list(&acme);
  let created = request.created(&worktrees, &known).expect("the new worktree is listed");
  assert_eq!(created.name, "team-spike");
  assert!(
    created.path.starts_with(dir.path().canonicalize().unwrap()),
    "{}",
    created.path.display()
  );
}

/// A type gwm does not know, and `--issue` with no forge to ask: gwm's words.
#[test]
fn a_refusal_from_gwm_comes_back_as_gwm_wrote_it() {
  isolate();
  let dir = tempfile::tempdir().unwrap();
  let acme = acme(dir.path(), "");
  let before = paths(&acme);

  let refused = gwm_client::create(&acme, &Create::triple("nope", "3", "x").unwrap()).unwrap_err();
  assert!(refused.starts_with("error: invalid branch type 'nope'"), "{refused}");
  let refused = gwm_client::create(&acme, &Create::issue("5", "").unwrap()).unwrap_err();
  assert!(
    refused.starts_with("error: ") && refused.contains("origin"),
    "{refused}"
  );
  // `--type` is taken by gwm's parser: the refusal is the forge's, not the arguments'.
  let refused = gwm_client::create(&acme, &Create::issue("5", "feat").unwrap()).unwrap_err();
  assert!(
    refused.starts_with("error: ") && refused.contains("origin"),
    "{refused}"
  );
  assert_eq!(paths(&acme), before);
}

/// A bootstrap command no one approved: with stdin closed gwm refuses, the
/// app says how to approve it, and nothing is created, run nor approved, with
/// `GWM_ALLOW_BOOTSTRAP=1` in the environment (see [`isolate`]).
#[test]
fn an_unapproved_bootstrap_is_refused_with_how_to_approve_it() {
  let ledger = isolate();
  let dir = tempfile::tempdir().unwrap();
  let config = "[[bootstrap.command]]\nname = \"install\"\nrun = \"touch bootstrapped\"\n";
  let acme = acme(dir.path(), config);
  let before = paths(&acme);

  let refused = gwm_client::create(&acme, &Create::name("spike").unwrap()).unwrap_err();
  assert!(
    refused.starts_with("error: ") && refused.contains("not in the trust ledger"),
    "{refused}"
  );
  assert!(refused.contains("run `gwm trust add` in a terminal in"), "{refused}");
  assert_eq!(paths(&acme), before);
  assert!(!ledger.exists());
  assert!(!dir.path().join("acme-worktrees/spike/bootstrapped").exists());
}
