//! The editors and terminals installed on the Mac, found the way Monocode
//! finds editors (`crates/monocode-core/src/external_editor.rs`, MIT, see
//! NOTICE): by their `.app` bundle, the user's own first. Opened with
//! `/usr/bin/open -a`, which needs no `PATH`: an app launched from the Finder
//! has next to none.

use std::{
  path::{Path, PathBuf},
  process::{Command, Stdio},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Kind {
  Editor,
  Terminal,
}

/// Each app by its bundle's name, without `.app`. Monocode's editors, the
/// Insiders build and Zed Preview apart, then the common terminals.
const KNOWN: &[(&str, Kind)] = &[
  ("Visual Studio Code", Kind::Editor),
  ("Visual Studio Code - Insiders", Kind::Editor),
  ("VSCodium", Kind::Editor),
  ("Cursor", Kind::Editor),
  ("Zed", Kind::Editor),
  ("Zed Preview", Kind::Editor),
  ("Windsurf", Kind::Editor),
  ("Sublime Text", Kind::Editor),
  ("Terminal", Kind::Terminal),
  ("iTerm", Kind::Terminal),
  ("Ghostty", Kind::Terminal),
  ("WezTerm", Kind::Terminal),
  ("Warp", Kind::Terminal),
  ("kitty", Kind::Terminal),
  ("Alacritty", Kind::Terminal),
];

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Installed {
  pub name: &'static str,
  pub kind: Kind,
  pub bundle: PathBuf,
}

/// Where apps live: the user's own, then the system's. Terminal.app sits in
/// the system's Utilities.
pub(crate) fn roots() -> Vec<PathBuf> {
  let home = std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Applications"));
  home
    .into_iter()
    .chain(["/Applications", "/System/Applications/Utilities"].map(PathBuf::from))
    .collect()
}

/// The known apps found in `roots`, in [`KNOWN`]'s order, each from the first
/// root that has it. Reads the disk: off the UI thread.
pub(crate) fn installed(roots: &[PathBuf]) -> Vec<Installed> {
  KNOWN
    .iter()
    .filter_map(|&(name, kind)| {
      let bundle = roots
        .iter()
        .map(|root| root.join(format!("{name}.app")))
        .find(|bundle| bundle.is_dir())?;
      Some(Installed { name, kind, bundle })
    })
    .collect()
}

/// Opens `dir` in the app at `bundle`. The error is `open`'s stderr, as is.
pub(crate) fn open(bundle: &Path, dir: &Path) -> Result<(), String> {
  let output = Command::new("/usr/bin/open")
    .arg("-a")
    .arg(bundle)
    .arg(dir)
    .stdin(Stdio::null())
    .output()
    .map_err(|error| format!("could not run open: {error}"))?;
  if !output.status.success() {
    return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use std::fs;

  use super::*;

  #[test]
  fn finds_the_known_bundles_the_users_own_first() {
    let dir = tempfile::tempdir().unwrap();
    let (user, system, utilities) = (
      dir.path().join("user"),
      dir.path().join("system"),
      dir.path().join("utilities"),
    );
    for bundle in [
      user.join("Zed.app"),
      system.join("Zed.app"),
      system.join("Ghostty.app"),
      system.join("Notes.app"),
      utilities.join("Terminal.app"),
    ] {
      fs::create_dir_all(bundle).unwrap();
    }
    // Not a bundle: a file of that name.
    fs::write(system.join("Cursor.app"), "").unwrap();

    let found = installed(&[user.clone(), system.clone(), utilities.clone()]);
    assert_eq!(
      found,
      [
        Installed {
          name: "Zed",
          kind: Kind::Editor,
          bundle: user.join("Zed.app"),
        },
        Installed {
          name: "Terminal",
          kind: Kind::Terminal,
          bundle: utilities.join("Terminal.app"),
        },
        Installed {
          name: "Ghostty",
          kind: Kind::Terminal,
          bundle: system.join("Ghostty.app"),
        },
      ]
    );
  }

  /// Launches nothing: there is no app at that path.
  #[test]
  fn a_failed_open_says_why_in_opens_words() {
    let dir = tempfile::tempdir().unwrap();
    let error = open(&dir.path().join("Gone.app"), dir.path()).unwrap_err();
    assert!(error.contains("Gone.app"), "{error}");
  }
}
