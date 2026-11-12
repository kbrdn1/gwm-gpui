//! Keeps a worktree list live: finds or starts a `gwm daemon` for the repo,
//! follows its stream, and falls back to one `gwm list` when no daemon can be
//! had.
//!
//! gwm documents the daemon as started by the user, one per repo, on a socket
//! that is shared by default. A daemon on the default socket may therefore
//! serve another repo: it is only used once its first snapshot shows this one.
//! Failing that, the app runs a daemon of its own on a socket of its own, one
//! per repo, and stops it when it quits ([`Spawned`]).

use std::{
  io::{BufRead, BufReader},
  os::unix::ffi::OsStrExt,
  path::{Path, PathBuf},
  process::{Child, Command, Stdio},
  sync::{Arc, Mutex, PoisonError, mpsc},
  thread,
  time::Duration,
};

use crate::{Repo, Worktree, daemon};

/// How long a daemon the app starts may take to print that it listens.
const STARTUP: Duration = Duration::from_secs(10);
const READY: &str = "gwm daemon listening on ";
/// The pause before looking for a daemon again once a stream is lost, so a
/// daemon that dies right after starting is not restarted in a tight loop.
const RECONNECT_DELAY: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
  Snapshot(Vec<Worktree>),
  /// Where the next snapshots come from.
  Source(Source),
  /// Live updates stopped for good: a message for the user. Nothing follows.
  Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Source {
  /// Live, through a daemon that was already running.
  Daemon(PathBuf),
  /// Live, through a daemon the app started.
  OwnDaemon(PathBuf),
  /// The stream ended, and a daemon is being looked for again: the list on
  /// screen may be stale.
  Lost(String),
  /// Not live: one `gwm list --format=json`, and why no daemon could be used.
  Once(String),
}

/// The sockets to try, in order.
#[derive(Clone, Debug)]
pub struct Sockets {
  /// Where a daemon the user started listens by default.
  pub default: PathBuf,
  /// The app's own socket for this repo, `None` when there is no private
  /// directory to put it in.
  pub own: Option<PathBuf>,
}

impl Sockets {
  /// One own socket per repo, named after its common git directory, so every
  /// worktree the app is opened from finds the same daemon.
  pub fn for_repo(repo: &Repo) -> Self {
    let base = daemon::base_dir(
      std::env::var_os("XDG_RUNTIME_DIR").as_deref(),
      std::env::var_os("TMPDIR").as_deref(),
    );
    // A short hash, not the repo path: macOS caps a socket path at 104 bytes,
    // and `$TMPDIR` already takes about half.
    let own = daemon::is_private(&base).then(|| {
      base.join(format!(
        "gwm-gpui-{:016x}.sock",
        fnv1a(repo.common_dir.as_os_str().as_bytes())
      ))
    });
    Self {
      default: daemon::default_socket(),
      own,
    }
  }
}

/// FNV-1a: unlike `DefaultHasher`, it gives the same name on every build, so a
/// relaunch finds a daemon an earlier run left behind.
fn fnv1a(bytes: &[u8]) -> u64 {
  bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
    (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
  })
}

/// The `gwm daemon` a [`watch`] started, if any. `watch` stops it when it
/// returns, but a child outlives its parent: the app calls [`Spawned::stop`]
/// when it quits, on the watches still running.
#[derive(Clone, Default)]
pub struct Spawned(Arc<Mutex<Option<Child>>>);

impl Spawned {
  pub fn stop(&self) {
    if let Some(mut child) = self.0.lock().unwrap_or_else(PoisonError::into_inner).take() {
      // An error only means it already exited. `wait` reaps it either way.
      let _ = child.kill();
      let _ = child.wait();
    }
  }

  fn keep(&self, child: Child) {
    *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(child);
  }
}

/// Feeds `on_event` for the repo at `path` until it returns `false`, or until
/// there is nothing more to say: a fallback list, or a failure. `sockets` names
/// where to look once the repo is located. Blocks: run it on a thread of its own.
///
/// It only learns that `on_event` wants no more when it has an event to hand
/// over, and it looks for a daemon again a second after it loses one. So to end
/// it from outside, have `on_event` return `false` first, then stop its daemon
/// ([`Spawned::stop`]): the other way round, it starts another. On a daemon it
/// did not start, it ends when that daemon next speaks.
pub fn watch(
  path: &Path,
  sockets: impl FnOnce(&Repo) -> Sockets,
  spawned: &Spawned,
  on_event: impl FnMut(Event) -> bool,
) {
  follow_repo(path, sockets, spawned, on_event);
  // Nothing follows the daemon it started any more, and one that came up
  // after the caller's own stop is still running.
  spawned.stop();
}

fn follow_repo(
  path: &Path,
  sockets: impl FnOnce(&Repo) -> Sockets,
  spawned: &Spawned,
  mut on_event: impl FnMut(Event) -> bool,
) {
  let repo = match Repo::locate(path) {
    Ok(repo) => repo,
    // Without git's name for the repo (a bare one, say), no daemon can be told
    // apart. gwm is the judge of what it can list, not git.
    Err(why) => return once(path, format!("git cannot locate the repo: {why}"), &mut on_event),
  };
  let sockets = sockets(&repo);
  loop {
    let reason = match follow(&repo, &sockets, spawned, &mut on_event) {
      Outcome::Stopped => return,
      Outcome::Contract(message) => {
        on_event(Event::Failed(message));
        return;
      }
      Outcome::NoDaemon(reason) => return once(&repo.toplevel, reason, &mut on_event),
      Outcome::Lost(reason) => reason,
    };
    if !on_event(Event::Source(Source::Lost(reason))) {
      return;
    }
    thread::sleep(RECONNECT_DELAY);
  }
}

/// The fallback: one `gwm list --format=json`, and why it is not live.
fn once(path: &Path, reason: String, on_event: &mut impl FnMut(Event) -> bool) {
  if on_event(Event::Source(Source::Once(reason))) {
    on_event(match crate::list(path) {
      Ok(worktrees) => Event::Snapshot(worktrees),
      Err(error) => Event::Failed(error),
    });
  }
}

enum Outcome {
  /// `on_event` asked to stop.
  Stopped,
  Contract(String),
  /// A stream was followed, then ended.
  Lost(String),
  /// No daemon could be followed, and why.
  NoDaemon(String),
}

enum Session {
  /// Nothing came from this socket: try the next one.
  Missed(String),
  Over(Outcome),
}

fn follow(repo: &Repo, sockets: &Sockets, spawned: &Spawned, on_event: &mut impl FnMut(Event) -> bool) -> Outcome {
  let mut missed = String::new();
  // The own socket is named after this repo: whatever answers there is ours.
  let found = [(Some(&sockets.default), false), (sockets.own.as_ref(), true)];
  for (socket, ours) in found.into_iter().filter_map(|(socket, ours)| Some((socket?, ours))) {
    match session(repo, socket, Source::Daemon(socket.clone()), ours, on_event) {
      Session::Missed(why) => missed = why,
      Session::Over(outcome) => return outcome,
    }
  }
  let Some(own) = &sockets.own else {
    return Outcome::NoDaemon(format!("{missed}, and no private directory to start one in"));
  };
  // A daemon of ours that died: reap it before starting the next.
  spawned.stop();
  if let Err(why) = spawn(&repo.toplevel, own, spawned) {
    // A daemon still on the own socket makes the spawn fail: its own miss is the cause.
    return Outcome::NoDaemon(format!("{why} (before that: {missed})"));
  }
  match session(repo, own, Source::OwnDaemon(own.clone()), true, on_event) {
    Session::Over(outcome) => outcome,
    Session::Missed(why) => {
      spawned.stop();
      Outcome::NoDaemon(why)
    }
  }
}

/// `ours`: the socket is the app's own for this repo. A daemon there serves this
/// repo, so output it cannot read is contract drift, and ends the watch. One on
/// the shared socket may serve another repo: what it sends before its first
/// snapshot says nothing about this one, and the next socket gets its chance.
fn session(
  repo: &Repo,
  socket: &Path,
  source: Source,
  ours: bool,
  on_event: &mut impl FnMut(Event) -> bool,
) -> Session {
  let mut snapshots = 0;
  let mut verdict = None;
  let result = daemon::subscribe(socket, |worktrees| {
    if snapshots == 0 {
      if !serves(&worktrees, &repo.toplevel) {
        verdict = Some(Session::Missed(format!(
          "the daemon on {} serves another repo",
          socket.display()
        )));
        return false;
      }
      if !on_event(Event::Source(source.clone())) {
        verdict = Some(Session::Over(Outcome::Stopped));
        return false;
      }
    }
    snapshots += 1;
    let more = on_event(Event::Snapshot(worktrees));
    if !more {
      verdict = Some(Session::Over(Outcome::Stopped));
    }
    more
  });
  if let Some(verdict) = verdict {
    return verdict;
  }
  match result {
    Ok(()) => Session::Over(Outcome::Stopped),
    Err(daemon::Error::Contract(message)) if snapshots == 0 && !ours => Session::Missed(message),
    Err(daemon::Error::Contract(message)) => Session::Over(Outcome::Contract(message)),
    Err(daemon::Error::Unreachable(why)) if snapshots == 0 => Session::Missed(why),
    Err(daemon::Error::Unreachable(why)) => Session::Over(Outcome::Lost(why)),
  }
}

/// Whether a snapshot is this repo's: `toplevel`, the root of the worktree the
/// app was opened in, is exactly one of its worktrees. Not a prefix: a repo
/// nested in another (a submodule, a vendored repo) lies inside the outer one's
/// worktree. gwm's daemon has no method that names its repo, so this is how a
/// shared socket is told apart.
fn serves(worktrees: &[Worktree], toplevel: &Path) -> bool {
  worktrees
    .iter()
    .any(|worktree| worktree.path.canonicalize().is_ok_and(|path| path == toplevel))
}

/// Starts `gwm daemon` on `socket` and waits until it listens.
fn spawn(repo: &Path, socket: &Path, spawned: &Spawned) -> Result<(), String> {
  let mut child = Command::new("gwm")
    .arg("daemon")
    .arg("--socket")
    .arg(socket)
    .current_dir(repo)
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::piped())
    .spawn()
    .map_err(|error| format!("could not run gwm daemon: {error}"))?;
  let stderr = child.stderr.take().expect("stderr is piped");
  let (ready_tx, ready_rx) = mpsc::channel();
  // Watch stderr for the readiness line, then keep draining it: a daemon
  // blocked on a full pipe would stop answering.
  thread::spawn(move || {
    let mut ready_tx = Some(ready_tx);
    let mut last = String::new();
    for line in BufReader::new(stderr).lines().map_while(Result::ok) {
      if let Some(ready_tx) = ready_tx.take_if(|_| line.starts_with(READY)) {
        let _ = ready_tx.send(Ok(()));
      } else if ready_tx.is_some() && !line.trim().is_empty() {
        last = line;
      }
    }
    if let Some(ready_tx) = ready_tx {
      let _ = ready_tx.send(Err(last));
    }
  });
  match ready_rx.recv_timeout(STARTUP) {
    Ok(Ok(())) => {
      spawned.keep(child);
      Ok(())
    }
    Ok(Err(last)) => {
      let _ = child.wait();
      Err(if last.is_empty() {
        "gwm daemon exited".to_owned()
      } else {
        format!("gwm daemon exited: {last}")
      })
    }
    Err(_) => {
      let _ = child.kill();
      let _ = child.wait();
      Err(format!("gwm daemon did not listen within {} s", STARTUP.as_secs()))
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn at(path: &str) -> Worktree {
    Worktree {
      name: "wt".to_owned(),
      id: "wt".to_owned(),
      path: PathBuf::from(path),
      branch: None,
      head: None,
      is_main: false,
      is_locked: false,
      is_prunable: false,
      status: crate::Status {
        is_dirty: false,
        has_upstream: false,
        ahead: 0,
        behind: 0,
        unknown: false,
      },
      age_seconds: None,
      issue: None,
      pr: None,
    }
  }

  #[test]
  fn a_snapshot_serves_exactly_its_worktrees() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    for path in ["acme/vendor/lib", "acme-worktrees/feat-1", "acme-api"] {
      std::fs::create_dir_all(root.join(path)).unwrap();
    }
    let acme = root.join("acme");
    // gwm prints the main worktree with a trailing slash.
    let snapshot = [
      at(&format!("{}/", acme.display())),
      at(&root.join("acme-worktrees/feat-1").to_string_lossy()),
    ];
    assert!(serves(&snapshot, &acme));
    assert!(serves(&snapshot, &root.join("acme-worktrees/feat-1")));
    // A repo nested in acme's worktree is another repo.
    assert!(!serves(&snapshot, &root.join("acme/vendor/lib")));
    assert!(!serves(&snapshot, &root.join("acme-api")));
    assert!(!serves(&snapshot, &root));
  }

  #[test]
  fn the_own_socket_name_is_stable_and_short() {
    // Pinned: a different hash would orphan the daemon an earlier run left behind.
    assert_eq!(fnv1a(b"/repos/acme"), fnv1a(b"/repos/acme"));
    assert_ne!(fnv1a(b"/repos/acme"), fnv1a(b"/repos/acme-api"));
    assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
  }
}
