//! The `subscribe` stream of `gwm daemon` (`docs/5.integrations/4.daemon-consumers.md`
//! in gwm-cli): newline-delimited JSON-RPC 2.0 over a unix socket. The first
//! `worktrees.changed` notification is the current snapshot, then one comes per
//! change.

use std::{
  ffi::OsStr,
  io::{BufRead, BufReader, ErrorKind, Write},
  os::unix::{fs::MetadataExt, net::UnixStream},
  path::{Path, PathBuf},
  time::Duration,
};

use serde_json::Value;

use crate::Worktree;

/// The contract version this build reads: `contract::SCHEMA_VERSION` in gwm-cli,
/// carried by every `worktrees.changed` as `params.schema_version`.
pub const SCHEMA_VERSION: u64 = 1;

/// How long a daemon may stay silent before its first snapshot. It can accept a
/// connection and say nothing when its first scan fails, so the wait is bounded,
/// as in gwm's own client. Once the first snapshot is in, there is no deadline.
pub const HANDSHAKE: Duration = Duration::from_secs(5);

const SUBSCRIBE: &str = r#"{"jsonrpc":"2.0","method":"subscribe","id":1}"#;

#[derive(Debug, PartialEq)]
pub enum Error {
  /// No daemon at the socket, or it went away: worth trying elsewhere.
  Unreachable(String),
  /// The daemon sent something this build cannot read: a contract version it
  /// was not made for, or output that does not parse. Shown, never retried.
  Contract(String),
}

/// Where `gwm daemon` binds without `--socket`, resolved the way gwm documents
/// it: the first non-empty of `$XDG_RUNTIME_DIR`, `$TMPDIR`, then `/tmp`, with
/// the socket nested in `gwm-<uid>/` when that base is not owner-only.
///
/// gwm has no command that prints this path, so it is mirrored from
/// `socket_path` in gwm-cli's `src/daemon.rs`.
pub fn default_socket() -> PathBuf {
  let base = base_dir(
    std::env::var_os("XDG_RUNTIME_DIR").as_deref(),
    std::env::var_os("TMPDIR").as_deref(),
  );
  socket_in(&base, is_private(&base), current_uid())
}

/// The first non-empty of `$XDG_RUNTIME_DIR` and `$TMPDIR`, else `/tmp`.
pub fn base_dir(xdg_runtime_dir: Option<&OsStr>, tmpdir: Option<&OsStr>) -> PathBuf {
  [xdg_runtime_dir, tmpdir]
    .into_iter()
    .flatten()
    .find(|dir| !dir.is_empty())
    .map_or_else(|| PathBuf::from("/tmp"), PathBuf::from)
}

fn socket_in(base: &Path, base_is_private: bool, uid: u32) -> PathBuf {
  if base_is_private {
    base.join("gwm.sock")
  } else {
    base.join(format!("gwm-{uid}")).join("gwm.sock")
  }
}

/// A directory this user owns and nobody else can enter.
pub fn is_private(dir: &Path) -> bool {
  std::fs::metadata(dir).is_ok_and(|meta| meta.is_dir() && meta.uid() == current_uid() && meta.mode() & 0o077 == 0)
}

fn current_uid() -> u32 {
  // SAFETY: getuid has no preconditions and cannot fail.
  unsafe { libc::getuid() }
}

/// Follows the stream at `socket`, handing every snapshot to `on_snapshot`
/// until it returns `false` (then `Ok`). A stream that ends, or never starts,
/// is `Unreachable`.
pub fn subscribe(socket: &Path, on_snapshot: impl FnMut(Vec<Worktree>) -> bool) -> Result<(), Error> {
  subscribe_with(socket, HANDSHAKE, on_snapshot)
}

fn subscribe_with(
  socket: &Path,
  handshake: Duration,
  mut on_snapshot: impl FnMut(Vec<Worktree>) -> bool,
) -> Result<(), Error> {
  let unreachable = |error: std::io::Error| {
    Error::Unreachable(match error.kind() {
      ErrorKind::WouldBlock | ErrorKind::TimedOut => {
        format!(
          "the daemon on {} sent nothing for {} s",
          socket.display(),
          handshake.as_secs_f32()
        )
      }
      _ => format!("{}: {error}", socket.display()),
    })
  };
  let stream = UnixStream::connect(socket).map_err(unreachable)?;
  stream.set_read_timeout(Some(handshake)).map_err(unreachable)?;
  stream.set_write_timeout(Some(handshake)).map_err(unreachable)?;
  writeln!(&stream, "{SUBSCRIBE}").map_err(unreachable)?;

  let mut reader = BufReader::new(&stream);
  let mut line = String::new();
  let mut delivered = false;
  loop {
    line.clear();
    if reader.read_line(&mut line).map_err(unreachable)? == 0 {
      break;
    }
    if line.trim().is_empty() {
      continue;
    }
    let worktrees = parse_notification(&line)?;
    if !delivered {
      // macOS refuses the change (EINVAL) once the daemon has hung up. The
      // next read then reports the end of the stream, so the snapshot already
      // received must not be lost to that error.
      let _ = stream.set_read_timeout(None);
      delivered = true;
    }
    if !on_snapshot(worktrees) {
      return Ok(());
    }
  }
  Err(Error::Unreachable(if delivered {
    format!("the daemon on {} closed the stream", socket.display())
  } else {
    format!("the daemon on {} hung up before its first snapshot", socket.display())
  }))
}

/// Reads one `worktrees.changed` line. The version is checked before the
/// worktrees are read: under another contract version, their fields may not
/// mean what this build expects.
pub fn parse_notification(line: &str) -> Result<Vec<Worktree>, Error> {
  let unreadable = |what: String| Error::Contract(format!("unexpected gwm daemon output: {what}"));
  let mut message: Value = serde_json::from_str(line).map_err(|error| unreadable(error.to_string()))?;
  // A daemon that refuses `subscribe` (one older than the method) answers with
  // an error envelope instead. It is not this repo's stream, so look elsewhere.
  if let Some(error) = message.get("error") {
    let text = error.get("message").and_then(Value::as_str).unwrap_or("no message");
    return Err(Error::Unreachable(format!(
      "the gwm daemon refused to subscribe: {text}"
    )));
  }
  match message.get("method").and_then(Value::as_str) {
    Some("worktrees.changed") => {}
    other => return Err(unreadable(format!("expected worktrees.changed, got {other:?}"))),
  }
  let params = message
    .get_mut("params")
    .ok_or_else(|| unreadable("no params".to_owned()))?;
  match params.get("schema_version").and_then(Value::as_u64) {
    Some(SCHEMA_VERSION) => {}
    Some(version) => {
      return Err(Error::Contract(format!(
        "gwm sends contract version {version}, this build of gwm-gpui reads version {SCHEMA_VERSION}"
      )));
    }
    None => return Err(unreadable("no params.schema_version".to_owned())),
  }
  let worktrees = params.get_mut("worktrees").map(Value::take).unwrap_or(Value::Null);
  serde_json::from_value(worktrees).map_err(|error| unreadable(error.to_string()))
}

#[cfg(test)]
mod tests {
  use std::{os::unix::net::UnixListener, thread};

  use super::*;

  /// A real notification from `gwm daemon` 1.11.0 on a throwaway repo, whole.
  const REAL: &str = r#"{"jsonrpc":"2.0","method":"worktrees.changed","params":{"schema_version":1,"worktrees":[{"age_seconds":null,"branch":"main","head":"a57e9d733da2fdcc699de4ae28e8559bc36cda02","id":"acme","is_locked":false,"is_main":true,"is_prunable":false,"issue":null,"name":"acme","path":"/private/tmp/gwm-fixture/acme/","pr":null,"status":{"ahead":0,"behind":0,"has_upstream":false,"is_dirty":false,"unknown":false}},{"age_seconds":null,"branch":"feat/#12-search","head":"a57e9d733da2fdcc699de4ae28e8559bc36cda02","id":"feat-12-search","is_locked":false,"is_main":false,"is_prunable":false,"issue":12,"name":"feat-12-search","path":"/private/tmp/gwm-fixture/acme-worktrees/feat-12-search","pr":null,"status":{"ahead":0,"behind":0,"has_upstream":false,"is_dirty":true,"unknown":false}}]}}"#;

  fn names(worktrees: &[Worktree]) -> Vec<&str> {
    worktrees.iter().map(|worktree| worktree.name.as_str()).collect()
  }

  #[test]
  fn reads_a_real_notification() {
    let worktrees = parse_notification(REAL).unwrap();
    assert_eq!(names(&worktrees), ["acme", "feat-12-search"]);
    assert_eq!(worktrees[0].path, Path::new("/private/tmp/gwm-fixture/acme/"));
    assert_eq!(worktrees[1].branch.as_deref(), Some("feat/#12-search"));
    assert!(worktrees[1].status.is_dirty);
  }

  #[test]
  fn another_contract_version_is_refused_before_reading_worktrees() {
    let line = REAL.replace(r#""schema_version":1"#, r#""schema_version":2"#);
    assert_eq!(
      parse_notification(&line),
      Err(Error::Contract(
        "gwm sends contract version 2, this build of gwm-gpui reads version 1".to_owned()
      ))
    );
  }

  #[test]
  fn a_notification_without_a_version_is_refused() {
    let line = REAL.replace(r#""schema_version":1,"#, "");
    assert!(matches!(parse_notification(&line), Err(Error::Contract(message)) if message.contains("schema_version")));
  }

  #[test]
  fn unreadable_output_is_an_error_not_an_empty_list() {
    assert!(matches!(parse_notification("not json"), Err(Error::Contract(_))));
    let line = REAL.replace(r#""name":"acme","#, "");
    assert!(matches!(parse_notification(&line), Err(Error::Contract(message)) if message.contains("name")));
  }

  #[test]
  fn a_daemon_that_refuses_to_subscribe_is_unreachable() {
    let line = r#"{"error":{"code":-32601,"message":"unknown method 'subscribe'"},"id":1,"jsonrpc":"2.0"}"#;
    assert!(matches!(parse_notification(line), Err(Error::Unreachable(_))));
  }

  #[test]
  fn the_base_dir_is_the_first_non_empty_of_xdg_then_tmpdir() {
    let (xdg, tmp) = (OsStr::new("/run/user/501"), OsStr::new("/var/folders/t/"));
    assert_eq!(base_dir(Some(xdg), Some(tmp)), Path::new("/run/user/501"));
    assert_eq!(base_dir(Some(OsStr::new("")), Some(tmp)), Path::new("/var/folders/t/"));
    assert_eq!(base_dir(None, None), Path::new("/tmp"));
  }

  #[test]
  fn a_shared_base_gets_a_per_user_sub_directory() {
    assert_eq!(
      socket_in(Path::new("/tmp"), false, 501),
      Path::new("/tmp/gwm-501/gwm.sock")
    );
    assert_eq!(
      socket_in(Path::new("/var/folders/t"), true, 501),
      Path::new("/var/folders/t/gwm.sock")
    );
  }

  #[test]
  fn only_an_owner_only_directory_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(is_private(dir.path()));
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(!is_private(dir.path()));
  }

  /// A fake daemon: answers one `subscribe` with `lines`, then hangs up, or
  /// stays silent for a while when `lines` is empty and `silent` is set.
  fn fake_daemon(lines: Vec<String>, silent: bool) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("d.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    thread::spawn(move || {
      let (connection, _) = listener.accept().unwrap();
      let mut request = String::new();
      BufReader::new(&connection).read_line(&mut request).unwrap();
      // The client may hang up first, on purpose: a failed write is not an error here.
      for line in lines {
        let _ = writeln!(&connection, "{line}");
      }
      if silent {
        thread::sleep(Duration::from_secs(2));
      }
    });
    (dir, socket)
  }

  #[test]
  fn follows_snapshots_until_the_stream_ends() {
    let (_dir, socket) = fake_daemon(vec![REAL.to_owned(), REAL.to_owned()], false);
    let mut seen = 0;
    let result = subscribe(&socket, |_| {
      seen += 1;
      true
    });
    assert_eq!(seen, 2);
    assert!(matches!(result, Err(Error::Unreachable(message)) if message.ends_with("closed the stream")));
  }

  #[test]
  fn stops_when_the_caller_says_so() {
    let (_dir, socket) = fake_daemon(vec![REAL.to_owned(), REAL.to_owned()], true);
    let mut seen = 0;
    assert_eq!(
      subscribe(&socket, |_| {
        seen += 1;
        false
      }),
      Ok(())
    );
    assert_eq!(seen, 1);
  }

  #[test]
  fn a_silent_daemon_times_out_before_its_first_snapshot() {
    let (_dir, socket) = fake_daemon(Vec::new(), true);
    let result = subscribe_with(&socket, Duration::from_millis(200), |_| true);
    assert!(matches!(result, Err(Error::Unreachable(message)) if message.contains("sent nothing")));
  }

  #[test]
  fn a_daemon_that_hangs_up_first_is_unreachable() {
    let (_dir, socket) = fake_daemon(Vec::new(), false);
    let result = subscribe(&socket, |_| true);
    assert!(matches!(result, Err(Error::Unreachable(message)) if message.ends_with("before its first snapshot")));
  }

  #[test]
  fn a_version_change_mid_stream_stops_the_stream() {
    let drifted = REAL.replace(r#""schema_version":1"#, r#""schema_version":2"#);
    let (_dir, socket) = fake_daemon(vec![REAL.to_owned(), drifted], true);
    let mut seen = 0;
    let result = subscribe(&socket, |_| {
      seen += 1;
      true
    });
    assert_eq!(seen, 1);
    assert!(matches!(result, Err(Error::Contract(_))));
  }

  #[test]
  fn no_socket_is_unreachable() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
      subscribe(&dir.path().join("none.sock"), |_| true),
      Err(Error::Unreachable(_))
    ));
  }
}
