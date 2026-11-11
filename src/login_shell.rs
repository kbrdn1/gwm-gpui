//! The login shell's `PATH`, for an app the Finder launched with launchd's
//! (`/usr/bin:/bin:/usr/sbin:/sbin`), where neither `gwm` nor Homebrew's `gh`
//! and `git` are. After Monocode Native's (`monocode-core/src/scm/shell.rs`).

use std::ffi::OsStr;
use std::io;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// Long enough for a heavy `.zshrc`, short enough that a hanging one does not
/// hold the launch for good.
pub const TIMEOUT: Duration = Duration::from_secs(5);

/// Printed on both sides of the value, so that whatever the rc files print
/// around it (a motd, a prompt theme's escape codes) is left out.
const MARKER: &str = "__GWM_GPUI_PATH__";

/// Whether `program` is a file in one of the directories of `path`.
pub fn finds(path: &OsStr, program: &str) -> bool {
  std::env::split_paths(path).any(|dir| !dir.as_os_str().is_empty() && dir.join(program).is_file())
}

/// The `PATH` that `shell` sets up as a login, interactive shell, or `None` if
/// it fails, prints nothing usable, or takes longer than `timeout`.
///
/// Login (`-l`) reads `.zprofile`, where Homebrew puts its `shellenv`;
/// interactive (`-i`) reads `.zshrc`, where version managers and many users
/// put theirs. `printenv`, not `$PATH`, so that fish, which keeps it as a
/// list, prints it the same way.
pub fn path(shell: &Path, timeout: Duration) -> Option<String> {
  let mut command = Command::new(shell);
  command
    .args([
      "-l",
      "-i",
      "-c",
      &format!("echo {MARKER}; printenv PATH; echo {MARKER}"),
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::null());
  // SAFETY: `setsid` is async-signal-safe, the only kind of call allowed
  // between fork and exec. A session of its own leaves the shell without a
  // controlling terminal, so an interactive one launched from a terminal does
  // not stop itself for reading it, and its group can be killed whole.
  unsafe {
    command.pre_exec(|| {
      if libc::setsid() == -1 {
        return Err(io::Error::last_os_error());
      }
      Ok(())
    });
  }
  let child = command.spawn().ok()?;
  let group = child.id() as libc::pid_t;
  let (sender, receiver) = mpsc::channel();
  let waiter = thread::spawn(move || sender.send(child.wait_with_output()));
  match receiver.recv_timeout(timeout) {
    Ok(output) => {
      // Joined, so that the caller is back to a single thread.
      let _ = waiter.join();
      parse(&output.ok()?.stdout)
    }
    Err(_) => {
      // The whole group: an rc file's own children too. The waiter then
      // reads the end of the pipe and returns; it does not touch the
      // environment either way.
      // SAFETY: a plain syscall on the group this function created.
      unsafe { libc::kill(-group, libc::SIGKILL) };
      None
    }
  }
}

fn parse(stdout: &[u8]) -> Option<String> {
  let stdout = String::from_utf8_lossy(stdout);
  let mut parts = stdout.split(MARKER);
  parts.next()?;
  let value = parts.next()?.trim();
  // Without the closing marker, the value may be cut short.
  parts.next()?;
  (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
  use std::os::unix::fs::PermissionsExt;
  use std::time::Instant;

  use super::*;

  #[test]
  fn parse_keeps_only_what_is_between_the_markers() {
    // What a login shell prints around the value: a motd, a prompt theme's
    // escape codes, a logout message.
    let stdout = b"Last login: Tue Nov 11 09:12:01\n\x1b[?1h\x1b=\n__GWM_GPUI_PATH__\n\
      /opt/homebrew/bin:/usr/bin:/bin\n__GWM_GPUI_PATH__\nbye\n";
    assert_eq!(parse(stdout).as_deref(), Some("/opt/homebrew/bin:/usr/bin:/bin"));
  }

  #[test]
  fn parse_refuses_a_missing_or_empty_value() {
    assert_eq!(parse(b""), None);
    assert_eq!(parse(b"zsh: command not found: printenv\n"), None);
    assert_eq!(parse(b"__GWM_GPUI_PATH__\n/usr/bin\n"), None, "cut short");
    assert_eq!(parse(b"__GWM_GPUI_PATH__\n__GWM_GPUI_PATH__\n"), None, "empty PATH");
  }

  #[test]
  fn finds_a_program_only_where_it_is() {
    assert!(finds(OsStr::new("/nowhere:/bin"), "sh"));
    assert!(!finds(OsStr::new("/nowhere:/bin"), "a-program-that-does-not-exist"));
    assert!(!finds(OsStr::new(""), "sh"));
  }

  #[test]
  fn path_reads_a_real_shell() {
    let path = path(Path::new("/bin/sh"), TIMEOUT).expect("/bin/sh prints its PATH");
    assert!(finds(OsStr::new(&path), "sh"), "{path}");
  }

  #[test]
  fn path_gives_up_on_a_shell_that_hangs() {
    let shell = std::env::temp_dir().join(format!("gwm-gpui-hanging-shell-{}", std::process::id()));
    std::fs::write(&shell, "#!/bin/sh\nsleep 30\n").unwrap();
    std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap();
    let started = Instant::now();
    let path = path(&shell, Duration::from_millis(300));
    std::fs::remove_file(&shell).unwrap();
    assert_eq!(path, None);
    assert!(started.elapsed() < Duration::from_secs(5), "{:?}", started.elapsed());
  }
}
