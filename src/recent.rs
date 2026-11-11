//! The repos opened before, most recent first: the app's own state, kept in a
//! small file, not gwm's. One line per repo, its common git directory (which
//! names it whichever worktree was opened) and the worktree last opened, a tab
//! between them.

use std::{
  ffi::OsStr,
  os::unix::ffi::OsStrExt,
  path::{Path, PathBuf},
};

use gpui::{AnyWindowHandle, Global};

/// How many repos the file keeps.
pub const CAP: usize = 10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
  /// The git directory every worktree of the repo shares: the repo's key.
  pub key: PathBuf,
  /// The worktree to open, the one last opened.
  pub path: PathBuf,
}

impl Entry {
  /// The repo's name: the main worktree's folder when the common directory
  /// is its `.git`, else the common directory's own (a bare repo, a submodule).
  pub fn name(&self) -> String {
    let named = match self.key.file_name() {
      Some(name) if name == ".git" => self.key.parent().and_then(Path::file_name),
      name => name,
    };
    named.unwrap_or(self.key.as_os_str()).to_string_lossy().into_owned()
  }
}

/// What the windows share: the recent repos, and which window shows which
/// repo, so a repo opens once and its picker marks it.
#[derive(Default)]
pub struct Projects {
  /// Those whose folder still exists, most recent first.
  pub recent: Vec<Entry>,
  /// A repo's key and its window. A closed window stays until the repo opens
  /// again: its handle no longer updates, and a new window takes its place.
  pub windows: Vec<(PathBuf, AnyWindowHandle)>,
}

impl Global for Projects {}

impl Projects {
  /// The key of the repo `window` shows.
  pub fn key_of(&self, window: AnyWindowHandle) -> Option<&Path> {
    self
      .windows
      .iter()
      .find(|(_, handle)| *handle == window)
      .map(|(key, _)| key.as_path())
  }
}

/// The file in the app's support directory, under `home`.
pub fn file_in(home: &Path) -> PathBuf {
  home.join("Library/Application Support/gwm-gpui/recent")
}

/// A line that does not read as an entry is skipped: a damaged file costs
/// the lines it damaged, never the app.
pub fn parse(bytes: &[u8]) -> Vec<Entry> {
  bytes
    .split(|&byte| byte == b'\n')
    .filter_map(|line| {
      let tab = line.iter().position(|&byte| byte == b'\t')?;
      let (key, path) = (
        Path::new(OsStr::from_bytes(&line[..tab])),
        Path::new(OsStr::from_bytes(&line[tab + 1..])),
      );
      (key.is_absolute() && path.is_absolute()).then(|| Entry {
        key: key.to_owned(),
        path: path.to_owned(),
      })
    })
    .collect()
}

pub fn serialize(entries: &[Entry]) -> Vec<u8> {
  let mut bytes = Vec::new();
  for entry in entries {
    bytes.extend_from_slice(entry.key.as_os_str().as_bytes());
    bytes.push(b'\t');
    bytes.extend_from_slice(entry.path.as_os_str().as_bytes());
    bytes.push(b'\n');
  }
  bytes
}

/// `entry` first, its older line gone, at most [`CAP`] lines. A path with a
/// tab or a newline would not read back: it is not recorded.
pub fn record(mut entries: Vec<Entry>, entry: Entry) -> Vec<Entry> {
  let unsafe_byte = |path: &Path| path.as_os_str().as_bytes().iter().any(|&b| b == b'\t' || b == b'\n');
  if unsafe_byte(&entry.key) || unsafe_byte(&entry.path) {
    return entries;
  }
  entries.retain(|old| old.key != entry.key);
  entries.insert(0, entry);
  entries.truncate(CAP);
  entries
}

/// A missing or unreadable file is an empty list.
pub fn load(file: &Path) -> Vec<Entry> {
  std::fs::read(file).map(|bytes| parse(&bytes)).unwrap_or_default()
}

/// Records `entry` in `file` and returns the entries whose folder exists. A
/// missing one is hidden, not dropped: an unmounted volume comes back.
/// Blocks on the disk: off the UI thread.
pub fn open_in(file: &Path, entry: Entry) -> Vec<Entry> {
  let entries = record(load(file), entry);
  // Written beside, then renamed: a crash mid-write leaves the old file whole.
  // ponytail: two apps recording at once can lose one's line; a lock if it ever matters.
  let saved = file.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| {
    let partial = file.with_extension("partial");
    std::fs::write(&partial, serialize(&entries))?;
    std::fs::rename(&partial, file)
  });
  if let Err(error) = saved {
    eprintln!(
      "gwm-gpui: could not save the recent repos to {}: {error}",
      file.display()
    );
  }
  entries.into_iter().filter_map(shown).collect()
}

/// The entry as the menu shows it: a worktree since removed gives way to the
/// main one, beside the repo's `.git`. Hidden once both are gone. Only the
/// list shown changes, the file keeps what was opened. Blocks on the disk.
pub fn shown(entry: Entry) -> Option<Entry> {
  if entry.path.is_dir() {
    return Some(entry);
  }
  let main = entry
    .key
    .parent()
    .filter(|_| entry.key.file_name() == Some(OsStr::new(".git")))?;
  main.is_dir().then(|| Entry {
    path: main.to_owned(),
    key: entry.key.clone(),
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  fn entry(key: &str, path: &str) -> Entry {
    Entry {
      key: key.into(),
      path: path.into(),
    }
  }

  #[test]
  fn a_repo_reopened_moves_to_the_front_once_whichever_worktree() {
    let entries = vec![entry("/a/.git", "/a"), entry("/b/.git", "/b")];
    let entries = record(entries, entry("/b/.git", "/b-worktrees/feat"));
    assert_eq!(entries, [entry("/b/.git", "/b-worktrees/feat"), entry("/a/.git", "/a")]);
  }

  #[test]
  fn the_list_is_capped_and_drops_the_oldest() {
    let mut entries = Vec::new();
    for n in 0..=CAP {
      entries = record(entries, entry(&format!("/{n}/.git"), &format!("/{n}")));
    }
    assert_eq!(entries.len(), CAP);
    assert_eq!(entries[0], entry(&format!("/{CAP}/.git"), &format!("/{CAP}")));
    assert!(!entries.contains(&entry("/0/.git", "/0")));
  }

  #[test]
  fn a_path_that_would_not_read_back_is_not_recorded() {
    let entries = record(vec![], entry("/a\tb/.git", "/a\tb"));
    assert!(entries.is_empty());
  }

  #[test]
  fn damaged_lines_are_skipped_and_the_rest_read() {
    let bytes = b"/a/.git\t/a\nno tab here\nrelative/.git\t/r\n\xff\xfe\n/b/.git\t/b";
    assert_eq!(parse(bytes), [entry("/a/.git", "/a"), entry("/b/.git", "/b")]);
    assert_eq!(parse(&serialize(&parse(bytes))), parse(bytes));
  }

  #[test]
  fn names_come_from_the_common_dir() {
    assert_eq!(entry("/src/acme-api/.git", "/wt/feat").name(), "acme-api");
    assert_eq!(entry("/src/bare.git", "/wt/main").name(), "bare.git");
  }

  #[test]
  fn the_file_round_trips_and_hides_folders_that_are_gone() {
    let home = tempfile::tempdir().unwrap();
    let file = file_in(home.path());
    // Missing, then not a list at all: empty, never a crash.
    assert!(load(&file).is_empty());
    let repo = home.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    let gone = home.path().join("gone");
    let shown = open_in(&file, entry("/gone/.git", gone.to_str().unwrap()));
    assert!(shown.is_empty(), "a folder that does not exist is hidden");
    let shown = open_in(&file, entry("/repo/.git", repo.to_str().unwrap()));
    assert_eq!(shown, [entry("/repo/.git", repo.to_str().unwrap())]);
    // Hidden, not dropped: both lines are in the file.
    assert_eq!(load(&file).len(), 2);
    std::fs::write(&file, [0xff, 0x00, 0x13]).unwrap();
    assert!(load(&file).is_empty());
  }

  #[test]
  fn a_repo_whose_last_worktree_was_removed_opens_its_main_one() {
    let home = tempfile::tempdir().unwrap();
    let repo = home.path().join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    let removed = home.path().join("removed-worktree");
    let key = repo.join(".git");
    let shown = open_in(
      &file_in(home.path()),
      Entry {
        key: key.clone(),
        path: removed,
      },
    );
    assert_eq!(shown, [Entry { key, path: repo }]);
  }
}
