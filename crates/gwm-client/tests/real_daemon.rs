//! [`watch`] against a real `gwm daemon`, on throwaway repos. These tests need
//! `gwm` on the PATH and fail without it, never skip: CI installs a pinned
//! release (`.github/workflows/ci.yml`). Every socket is passed explicitly, so
//! a daemon the user runs on the default socket is never touched.

use std::{
  fs,
  io::{BufRead, BufReader, Write},
  os::unix::net::{UnixListener, UnixStream},
  path::{Path, PathBuf},
  process::{Child, Command, Stdio},
  sync::mpsc::{self, Receiver},
  thread,
  time::Duration,
};

use gwm_client::{
  Repo, Worktree,
  watch::{Event, Sockets, Source, Spawned, watch},
};
use tempfile::TempDir;

const WAIT: Duration = Duration::from_secs(20);

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

/// A repo with one empty commit, by its canonical path.
fn repo(dir: &Path, name: &str) -> PathBuf {
  let path = dir.join(name);
  fs::create_dir(&path).unwrap();
  git(&path, &["init", "-q", "-b", "main"]);
  git(&path, &["commit", "-q", "--allow-empty", "-m", "init"]);
  path.canonicalize().unwrap()
}

/// A daemon started by hand, as a user would, killed with the test.
struct UserDaemon(Child);

impl UserDaemon {
  fn start(repo: &Path, socket: &Path) -> Self {
    let mut child = Command::new("gwm")
      .args(["daemon", "--socket"])
      .arg(socket)
      .current_dir(repo)
      .stdin(Stdio::null())
      .stderr(Stdio::piped())
      .spawn()
      .expect("gwm is on the PATH: these tests drive a real gwm daemon");
    let mut line = String::new();
    BufReader::new(child.stderr.as_mut().unwrap())
      .read_line(&mut line)
      .unwrap();
    assert!(line.starts_with("gwm daemon listening on "), "gwm daemon said: {line}");
    Self(child)
  }
}

impl Drop for UserDaemon {
  fn drop(&mut self) {
    let _ = self.0.kill();
    let _ = self.0.wait();
  }
}

/// Stops a daemon the test's `watch` started, even when an assertion fails
/// first: a test must not leave a `gwm daemon` behind.
struct StopOnDrop(Spawned);

impl Drop for StopOnDrop {
  fn drop(&mut self) {
    self.0.stop();
  }
}

/// Runs [`watch`] on a thread and hands its events over a channel.
fn start_watch(path: &Path, sockets: Sockets, spawned: &Spawned) -> (Receiver<Event>, thread::JoinHandle<()>) {
  let (events, received) = mpsc::channel();
  let (path, spawned) = (path.to_owned(), spawned.clone());
  let handle = thread::spawn(move || watch(&path, |_| sockets, &spawned, |event| events.send(event).is_ok()));
  (received, handle)
}

/// A fake daemon on `socket` that answers one `subscribe` with `line`.
fn fake_daemon(socket: &Path, line: &'static str) {
  let listener = UnixListener::bind(socket).unwrap();
  thread::spawn(move || {
    let (connection, _) = listener.accept().unwrap();
    let mut request = String::new();
    BufReader::new(&connection).read_line(&mut request).unwrap();
    let _ = writeln!(&connection, "{line}");
  });
}

const DRIFTED: &str = r#"{"jsonrpc":"2.0","method":"worktrees.changed","params":{"schema_version":2,"worktrees":{}}}"#;

fn next(events: &Receiver<Event>) -> Event {
  events.recv_timeout(WAIT).expect("an event within 20 s")
}

fn snapshot(events: &Receiver<Event>) -> Vec<String> {
  match next(events) {
    Event::Snapshot(worktrees) => worktrees.iter().map(|Worktree { name, .. }| name.clone()).collect(),
    other => panic!("expected a snapshot, got {other:?}"),
  }
}

fn sockets(dir: &TempDir, own: bool) -> Sockets {
  Sockets {
    default: dir.path().join("default.sock"),
    own: own.then(|| dir.path().join("own.sock")),
  }
}

#[test]
fn starts_its_own_daemon_follows_changes_and_stops_it() {
  let dir = tempfile::tempdir().unwrap();
  let acme = repo(dir.path(), "acme");
  let sockets = sockets(&dir, true);
  let own = sockets.own.clone().unwrap();
  let spawned = Spawned::default();
  let _stop = StopOnDrop(spawned.clone());
  let (events, handle) = start_watch(&acme, sockets, &spawned);

  assert_eq!(next(&events), Event::Source(Source::OwnDaemon(own.clone())));
  assert_eq!(snapshot(&events), ["acme"]);
  git(&acme, &["worktree", "add", "-q", "../feat-1", "-b", "feat/#1-x"]);
  assert_eq!(snapshot(&events), ["acme", "feat-1"]);

  // Its daemon gone, the list says so, then goes live again on a new one.
  spawned.stop();
  assert!(UnixStream::connect(&own).is_err(), "the daemon is gone");
  assert!(matches!(next(&events), Event::Source(Source::Lost(_))));
  assert_eq!(next(&events), Event::Source(Source::OwnDaemon(own.clone())));
  assert_eq!(snapshot(&events), ["acme", "feat-1"]);
  drop(events);
  spawned.stop();
  handle.join().unwrap();
}

#[test]
fn uses_the_daemon_left_on_its_own_socket() {
  let dir = tempfile::tempdir().unwrap();
  let acme = repo(dir.path(), "acme");
  let sockets = sockets(&dir, true);
  let own = sockets.own.clone().unwrap();
  let left = UserDaemon::start(&acme, &own);
  let spawned = Spawned::default();
  let _stop = StopOnDrop(spawned.clone());
  let (events, handle) = start_watch(&acme, sockets, &spawned);

  assert_eq!(next(&events), Event::Source(Source::Daemon(own)));
  assert_eq!(snapshot(&events), ["acme"]);
  drop(left);
  assert!(matches!(next(&events), Event::Source(Source::Lost(_))));
  drop(events);
  handle.join().unwrap();
}

#[test]
fn uses_a_daemon_already_serving_the_repo_from_any_folder_of_it() {
  let dir = tempfile::tempdir().unwrap();
  let acme = repo(dir.path(), "acme");
  fs::create_dir(acme.join("src")).unwrap();
  let sockets = sockets(&dir, true);
  let user = UserDaemon::start(&acme, &sockets.default);
  let (default, own) = (sockets.default.clone(), sockets.own.clone().unwrap());
  let spawned = Spawned::default();
  let _stop = StopOnDrop(spawned.clone());
  let (events, handle) = start_watch(&acme.join("src"), sockets, &spawned);

  assert_eq!(next(&events), Event::Source(Source::Daemon(default)));
  assert_eq!(snapshot(&events), ["acme"]);
  assert!(!own.exists(), "no daemon of its own when one already serves the repo");

  drop(user);
  assert!(matches!(next(&events), Event::Source(Source::Lost(_))));
  drop(events);
  handle.join().unwrap();
}

#[test]
fn skips_a_daemon_serving_another_repo() {
  let dir = tempfile::tempdir().unwrap();
  let (acme, other) = (repo(dir.path(), "acme"), repo(dir.path(), "other"));
  let sockets = sockets(&dir, true);
  let _user = UserDaemon::start(&other, &sockets.default);
  let own = sockets.own.clone().unwrap();
  let spawned = Spawned::default();
  let _stop = StopOnDrop(spawned.clone());
  let (events, handle) = start_watch(&acme, sockets, &spawned);

  assert_eq!(next(&events), Event::Source(Source::OwnDaemon(own)));
  assert_eq!(snapshot(&events), ["acme"]);
  drop(events);
  spawned.stop();
  handle.join().unwrap();
}

/// A repo inside another's worktree, a submodule or a vendored repo, is not
/// the outer repo, though its path starts with the outer one's.
#[test]
fn skips_the_daemon_of_the_repo_it_is_nested_in() {
  let dir = tempfile::tempdir().unwrap();
  let acme = repo(dir.path(), "acme");
  fs::create_dir(acme.join("vendor")).unwrap();
  let lib = repo(&acme.join("vendor"), "lib");
  let sockets = sockets(&dir, true);
  let _user = UserDaemon::start(&acme, &sockets.default);
  let own = sockets.own.clone().unwrap();
  let spawned = Spawned::default();
  let _stop = StopOnDrop(spawned.clone());
  let (events, handle) = start_watch(&lib, sockets, &spawned);

  assert_eq!(next(&events), Event::Source(Source::OwnDaemon(own)));
  assert_eq!(snapshot(&events), ["lib"]);
  drop(events);
  spawned.stop();
  handle.join().unwrap();
}

/// What a daemon on a shared socket sends before its first snapshot says
/// nothing about this repo: a contract it cannot read sends `watch` on to the
/// next socket, it does not stop it.
#[test]
fn moves_past_a_daemon_it_cannot_read_on_the_shared_socket() {
  let dir = tempfile::tempdir().unwrap();
  let acme = repo(dir.path(), "acme");
  let sockets = sockets(&dir, true);
  fake_daemon(&sockets.default, DRIFTED);
  let own = sockets.own.clone().unwrap();
  let spawned = Spawned::default();
  let _stop = StopOnDrop(spawned.clone());
  let (events, handle) = start_watch(&acme, sockets, &spawned);

  assert_eq!(next(&events), Event::Source(Source::OwnDaemon(own)));
  assert_eq!(snapshot(&events), ["acme"]);
  drop(events);
  spawned.stop();
  handle.join().unwrap();
}

/// The own socket is named after this repo: a contract it cannot read there is
/// drift, shown to the user, never a list parsed in silence.
#[test]
fn a_contract_it_cannot_read_on_its_own_socket_ends_the_watch() {
  let dir = tempfile::tempdir().unwrap();
  let acme = repo(dir.path(), "acme");
  let sockets = sockets(&dir, true);
  fake_daemon(sockets.own.as_ref().unwrap(), DRIFTED);
  let spawned = Spawned::default();
  let _stop = StopOnDrop(spawned.clone());
  let (events, handle) = start_watch(&acme, sockets, &spawned);

  assert!(matches!(next(&events), Event::Failed(message) if message.contains("contract version 2")));
  handle.join().unwrap();
}

/// git names no toplevel in a bare repo, but gwm can still list it.
#[test]
fn a_repo_git_cannot_locate_still_gets_one_gwm_list() {
  let dir = tempfile::tempdir().unwrap();
  let status = Command::new("git")
    .args(["init", "-q", "--bare", "acme.git"])
    .current_dir(dir.path())
    .status()
    .unwrap();
  assert!(status.success());
  let spawned = Spawned::default();
  let (events, handle) = start_watch(&dir.path().join("acme.git"), sockets(&dir, true), &spawned);

  assert!(matches!(next(&events), Event::Source(Source::Once(reason)) if reason.starts_with("git cannot locate")));
  assert!(matches!(next(&events), Event::Snapshot(_)), "gwm lists a bare repo");
  handle.join().unwrap();
}

#[test]
fn one_own_socket_per_repo_whichever_worktree_it_is_opened_from() {
  let dir = tempfile::tempdir().unwrap();
  let acme = repo(dir.path(), "acme");
  git(&acme, &["worktree", "add", "-q", "../feat-1", "-b", "feat/#1-x"]);
  let other = repo(dir.path(), "other");
  let own = |path: &Path| Sockets::for_repo(&Repo::locate(path).unwrap()).own;
  assert!(own(&acme).is_some(), "$TMPDIR is owner-only on macOS");
  assert_eq!(own(&acme), own(&dir.path().join("feat-1")));
  assert_ne!(own(&acme), own(&other));
}

#[test]
fn falls_back_to_one_gwm_list_when_no_daemon_can_be_had() {
  let dir = tempfile::tempdir().unwrap();
  let acme = repo(dir.path(), "acme");
  let spawned = Spawned::default();
  let (events, handle) = start_watch(&acme, sockets(&dir, false), &spawned);

  assert!(matches!(next(&events), Event::Source(Source::Once(reason)) if reason.contains("no private directory")));
  assert_eq!(snapshot(&events), ["acme"]);
  handle.join().unwrap();
}
