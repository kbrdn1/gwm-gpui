//! Opening a repo in a window, for the app's `main` and the screenshot example:
//! in a window of its own, or in place of what a window shows, a repo or the
//! landing view. And what a launch opens, and what becomes of a folder picked
//! in the open panel.

use std::{
  cell::RefCell,
  collections::HashMap,
  path::{Path, PathBuf},
  rc::Rc,
};

use gpui::{AnyWindowHandle, App, Focusable, Global, Modifiers, PathPromptOptions, PromptLevel, prelude::*};
use gwm_client::{Repo, watch::Spawned};

use crate::{
  Landing, Workspace,
  recent::{self, Entry, Projects},
  window_options,
};

/// The daemons the app started, one holder per view: a window that starts one
/// must not stop another's. A view stops its own when it goes, on a thread of
/// its own. On quit all are stopped from here, before GPUI drops the views:
/// the process may be gone before those threads ran.
// ponytail: a holder per window opened and per switch, never pruned: a few bytes each.
pub type Daemons = Rc<RefCell<Vec<Spawned>>>;

/// Where a repo opens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
  /// A window of its own: ⌘O, File > Open…, a ⌘-click in a project picker.
  NewWindow,
  /// This window, in place of the repo it shows: a plain click in its picker.
  Window(AnyWindowHandle),
}

impl Target {
  /// A click in `window`'s project picker: ⌘ held asks for another window.
  pub fn of_click(modifiers: Modifiers, window: AnyWindowHandle) -> Self {
    if modifiers.platform {
      Self::NewWindow
    } else {
      Self::Window(window)
    }
  }
}

/// What becomes of a repo asked for `target`.
#[derive(Debug, PartialEq)]
enum Plan {
  /// The window that asked shows it already: nothing to do.
  Stay,
  /// Another window shows it: that one comes forward, a repo has one window.
  Raise(AnyWindowHandle),
  /// No window shows it: it opens where asked.
  Open(Target),
}

/// `shown`: the window that shows the repo, if any.
fn plan(target: Target, shown: Option<AnyWindowHandle>) -> Plan {
  match shown {
    Some(shown) if target == Target::Window(shown) => Plan::Stay,
    Some(shown) => Plan::Raise(shown),
    None => Plan::Open(target),
  }
}

/// What a launch shows.
#[derive(Debug, PartialEq)]
pub enum Start {
  /// The window of the repo at this path.
  Repo(PathBuf),
  /// The landing view, and the open panel over it.
  Landing,
}

/// What a launch shows: the repo at `argument`, taken as given, else the one
/// `cwd` is in, else the landing view. An argument is not checked: a bare repo
/// is no work tree for git, but gwm lists it, and the window says why when
/// nothing can be listed. Runs git: before GPUI starts.
pub fn start(argument: Option<PathBuf>, cwd: &Path) -> Start {
  match argument {
    Some(repo) => Start::Repo(repo),
    None if Repo::locate(cwd).is_ok() => Start::Repo(cwd.to_owned()),
    None => Start::Landing,
  }
}

/// The window of a launch with no repo to show, the Finder's: the landing
/// view, with the recent repos read from disk for it to list, and the open
/// panel over it. The panel cancelled, the view stays: the user picks there,
/// or quits.
pub fn landing(daemons: Daemons, cx: &mut App) {
  let window = cx
    .open_window(window_options(cx), |_, cx| cx.new(|_| Landing))
    .expect("the main window opens");
  // In front, or the panel opens behind the app that launched this one.
  cx.activate(true);
  if let Some(file) = recent_file() {
    cx.spawn(async move |cx| {
      let listed = cx.background_spawn(async move { recent::listed(&file) }).await;
      cx.update(|cx| {
        let projects = cx.global_mut::<Projects>();
        // A repo opened meanwhile left its own list, itself first: that one stays.
        if projects.recent.is_empty() {
          projects.recent = listed;
          cx.refresh_windows();
        }
      });
    })
    .detach();
  }
  folder(Target::Window(window.into()), daemons, cx);
}

/// Why a picked folder is not opened: what the sheet says.
#[derive(Debug, PartialEq)]
pub struct Refusal {
  pub message: &'static str,
  /// git's own words, then the folder.
  pub detail: String,
}

/// Whether a folder picked in the open panel opens: only one git places in a
/// repo, unlike a command-line argument (see [`start`]). That is the
/// maintainer's decision (#19): [`Repo::locate`] judges a pick, and gwm what it
/// lists of an argument. So a bare repo is refused too, git placing no work
/// tree there, and told apart, with its ways in: one of its worktrees, which
/// git does place, or the command line. Runs git: off the UI thread.
pub fn judge(folder: &Path) -> Result<Repo, Refusal> {
  Repo::locate(folder).map_err(|gits| {
    let folder_line = folder.display();
    if Repo::is_bare(folder) {
      Refusal {
        message: "A bare git repository",
        detail: format!(
          "{gits}\n\nIt has no work tree here for git to place. Choose one of its worktrees instead, or pass this \
           folder to the app on the command line.\n\n{folder_line}"
        ),
      }
    } else {
      Refusal {
        message: "Not a git repository",
        detail: format!("{gits}\n\n{folder_line}"),
      }
    }
  })
}

/// The window a refusal's sheet goes on: the one that asked while it is open,
/// else the one in front, else any.
fn host(
  asked: Option<AnyWindowHandle>,
  front: Option<AnyWindowHandle>,
  open: &[AnyWindowHandle],
) -> Option<AnyWindowHandle> {
  [asked, front]
    .into_iter()
    .flatten()
    .find(|window| open.contains(window))
    .or_else(|| open.first().copied())
}

/// The windows a sheet is up on, each with what waits for its answer: one
/// sheet at a time on a window, `Window::prompt` is not entered again while
/// its sheet is up.
struct Sheets<T>(HashMap<AnyWindowHandle, Vec<T>>);

impl<T> Default for Sheets<T> {
  fn default() -> Self {
    Self(HashMap::new())
  }
}

impl<T> Sheets<T> {
  /// `sheet` back when `window` has none up: it goes up now. Else it waits.
  fn begin(&mut self, window: AnyWindowHandle, sheet: T) -> Option<T> {
    match self.0.get_mut(&window) {
      Some(waiting) => {
        waiting.push(sheet);
        None
      }
      None => {
        self.0.insert(window, Vec::new());
        Some(sheet)
      }
    }
  }

  /// `window`'s sheet was answered: those that waited for it, the oldest
  /// first, each to [`begin`](Self::begin) again.
  fn end(&mut self, window: AnyWindowHandle) -> Vec<T> {
    self.0.remove(&window).unwrap_or_default()
  }
}

/// A folder refused, and what asked for it.
struct Refused {
  refusal: Refusal,
  /// The window that asked, if one did.
  asked: Option<AnyWindowHandle>,
  target: Target,
  daemons: Daemons,
}

impl Global for Sheets<Refused> {}

/// Asks for a folder and opens it where `target` says. One git places in no
/// repo is not opened ([`judge`]): a sheet says why, and its "Choose another
/// folder…" asks again. The panel cancelled, nothing changes.
pub fn folder(target: Target, daemons: Daemons, cx: &mut App) {
  // Where a refusal is said: ⌘O and File > Open… are asked from the window in front.
  let asked = match target {
    Target::Window(window) => Some(window),
    Target::NewWindow => cx.active_window(),
  };
  let picked = cx.prompt_for_paths(PathPromptOptions {
    files: false,
    directories: true,
    multiple: false,
    prompt: Some("Open".into()),
  });
  cx.spawn(async move |cx| {
    let folder = picked
      .await
      .ok()
      .and_then(Result::ok)
      .flatten()
      .and_then(|paths| paths.into_iter().next());
    let Some(folder) = folder else {
      return;
    };
    let judged = cx
      .background_spawn({
        let folder = folder.clone();
        async move { judge(&folder) }
      })
      .await;
    cx.update(|cx| match judged {
      Ok(located) => show(folder, Some(located), target, &daemons, cx),
      Err(refusal) => refuse(
        Refused {
          refusal,
          asked,
          target,
          daemons,
        },
        cx,
      ),
    });
  })
  .detach();
}

/// Says why a picked folder is not opened, in a sheet on the window that
/// asked, brought forward. "Choose another folder…" asks again, for the same
/// target; "Cancel" leaves the window as it was.
fn refuse(refused: Refused, cx: &mut App) {
  // No window left to say it on: the app is quitting.
  let Some(window) = host(refused.asked, cx.active_window(), &cx.windows()) else {
    return;
  };
  let Some(refused) = cx.default_global::<Sheets<Refused>>().begin(window, refused) else {
    return;
  };
  let Refused {
    refusal,
    target,
    daemons,
    ..
  } = refused;
  let answer = window.update(cx, |_, window, cx| {
    window.activate_window();
    let answers = ["Choose another folder…", "Cancel"];
    window.prompt(PromptLevel::Info, refusal.message, Some(&refusal.detail), &answers, cx)
  });
  cx.spawn(async move |cx| {
    // A window closed under its sheet answers nothing: as "Cancel".
    let again = match answer {
      Ok(answer) => answer.await == Ok(0),
      Err(_) => false,
    };
    cx.update(|cx| {
      let waiting = cx.default_global::<Sheets<Refused>>().end(window);
      if again {
        folder(target, daemons, cx);
      }
      for refused in waiting {
        refuse(refused, cx);
      }
    });
  })
  .detach();
}

/// Opens the repo at `path` where `target` says, with its own live list: in
/// a window of its own, or in the window that asked, in place of what it
/// showed. A repo another window shows already brings that window forward
/// instead, whichever worktree it was opened from, and the asking window's own
/// repo changes nothing. A repo opened is recorded in the recent list. A folder
/// git does not place (no repo, a bare one) opens all the same, unrecorded: a
/// command-line argument, a recent repo since removed. One picked in the open
/// panel is judged first ([`folder`]).
pub fn repo(path: PathBuf, target: Target, daemons: &Daemons, cx: &mut App) {
  let daemons = daemons.clone();
  cx.spawn(async move |cx| {
    let located = cx
      .background_spawn({
        let path = path.clone();
        async move { Repo::locate(&path).ok() }
      })
      .await;
    cx.update(|cx| show(path, located, target, &daemons, cx));
  })
  .detach();
}

/// [`repo`], once git said where `path` belongs.
fn show(path: PathBuf, located: Option<Repo>, target: Target, daemons: &Daemons, cx: &mut App) {
  let key = located.as_ref().map(|located| located.common_dir.clone());
  let shown = key.as_deref().and_then(|key| cx.global::<Projects>().window_of(key));
  let target = match plan(target, shown) {
    Plan::Stay => return,
    Plan::Raise(shown) if raise(shown, cx) => return,
    // A closed window's handle fails to update: the repo opens as asked.
    Plan::Raise(_) => target,
    Plan::Open(target) => target,
  };
  let window = match target {
    Target::Window(window) if switch(window, &path, daemons, cx) => window,
    // Also when the window that asked was closed meanwhile.
    _ => open_window(path, daemons, cx),
  };
  cx.global_mut::<Projects>().show(key.clone(), window);
  let (Some(key), Some(located), Some(file)) = (key, located, recent_file()) else {
    return;
  };
  let entry = Entry {
    key,
    path: located.toplevel,
  };
  cx.spawn(async move |cx| {
    let shown = cx.background_spawn(async move { recent::open_in(&file, entry) }).await;
    cx.update(|cx| {
      cx.global_mut::<Projects>().recent = shown;
      cx.refresh_windows();
    });
  })
  .detach();
}

/// Brings `window` forward. False when it was closed.
fn raise(window: AnyWindowHandle, cx: &mut App) -> bool {
  let raised = window.update(cx, |_, window, _| window.activate_window()).is_ok();
  if raised {
    cx.activate(true);
  }
  raised
}

/// Shows the repo at `path` in `window`, in place of what it showed: the
/// landing view, or another repo, whose view is dropped, and its watch and
/// daemon end with it (see [`Workspace::new`]). False when the window was
/// closed.
fn switch(window: AnyWindowHandle, path: &Path, daemons: &Daemons, cx: &mut App) -> bool {
  let spawned = Spawned::default();
  let switched = window
    .update(cx, |_, window, cx| {
      let workspace = window.replace_root(cx, |_, cx| Workspace::new(path.to_owned(), spawned.clone(), cx));
      window.focus(&workspace.focus_handle(cx), cx);
    })
    .is_ok();
  if switched {
    daemons.borrow_mut().push(spawned);
  }
  switched
}

/// `GWM_GPUI_RECENT` names another file, for a try without touching yours.
fn recent_file() -> Option<PathBuf> {
  std::env::var_os("GWM_GPUI_RECENT")
    .map(PathBuf::from)
    .or_else(|| std::env::var_os("HOME").map(|home| recent::file_in(home.as_ref())))
}

fn open_window(repo: PathBuf, daemons: &Daemons, cx: &mut App) -> AnyWindowHandle {
  let spawned = Spawned::default();
  daemons.borrow_mut().push(spawned.clone());
  let window = cx
    .open_window(window_options(cx), |window, cx| {
      let workspace = cx.new(|cx| Workspace::new(repo, spawned, cx));
      window.focus(&workspace.focus_handle(cx), cx);
      workspace
    })
    .expect("the main window opens");
  cx.activate(true);
  window.into()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::recent::tests::window;

  fn git_init(args: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let status = std::process::Command::new("git")
      .args(["init", "-q"])
      .args(args)
      .current_dir(dir.path())
      .stdin(std::process::Stdio::null())
      .status()
      .unwrap();
    assert!(status.success());
    dir
  }

  /// Against a real git, as the three below.
  #[test]
  fn a_picked_folder_in_a_repo_is_accepted_from_any_depth() {
    let dir = git_init(&[]);
    let root = dir.path().canonicalize().unwrap();
    let inside = root.join("src");
    std::fs::create_dir(&inside).unwrap();
    let repo = Repo {
      common_dir: root.join(".git"),
      toplevel: root,
    };
    assert_eq!(judge(dir.path()), Ok(repo.clone()));
    assert_eq!(judge(&inside), Ok(repo));
  }

  #[test]
  fn a_picked_folder_in_no_repo_is_refused_in_gits_own_words() {
    let dir = tempfile::tempdir().unwrap();
    let refusal = judge(dir.path()).unwrap_err();
    assert_eq!(refusal.message, "Not a git repository");
    // Whatever language git speaks here.
    let gits = Repo::locate(dir.path()).unwrap_err();
    assert!(!gits.is_empty());
    assert_eq!(refusal.detail, format!("{gits}\n\n{}", dir.path().display()));
  }

  #[test]
  fn a_picked_bare_repo_is_refused_as_one_with_its_two_ways_in() {
    let dir = git_init(&["--bare"]);
    let refusal = judge(dir.path()).unwrap_err();
    assert_eq!(refusal.message, "A bare git repository");
    let gits = Repo::locate(dir.path()).unwrap_err();
    assert!(refusal.detail.starts_with(&gits), "{}", refusal.detail);
    assert!(refusal.detail.contains("one of its worktrees"), "{}", refusal.detail);
    assert!(refusal.detail.contains("command line"), "{}", refusal.detail);
    assert!(refusal.detail.ends_with(&dir.path().display().to_string()));

    // What the sheet says to pick instead is accepted, and is this repo.
    let worktree = tempfile::tempdir().unwrap();
    let added = std::process::Command::new("git")
      .args(["worktree", "add", "-q", "--orphan", "-b", "main"])
      .arg(worktree.path().join("main"))
      .current_dir(dir.path())
      .stdin(std::process::Stdio::null())
      .status()
      .unwrap();
    assert!(added.success());
    let picked = judge(&worktree.path().join("main")).unwrap();
    assert_eq!(picked.common_dir, dir.path().canonicalize().unwrap());
  }

  #[test]
  fn a_launch_takes_its_argument_then_the_current_directorys_repo_then_the_landing_view() {
    let (repo, plain) = (git_init(&[]), tempfile::tempdir().unwrap());
    let (repo, plain) = (repo.path(), plain.path());
    // An argument is not checked: a bare repo is no work tree, and gwm lists it.
    assert_eq!(start(Some(plain.to_owned()), repo), Start::Repo(plain.to_owned()));
    assert_eq!(start(Some(plain.to_owned()), plain), Start::Repo(plain.to_owned()));
    assert_eq!(start(None, repo), Start::Repo(repo.to_owned()));
    assert_eq!(start(None, plain), Start::Landing);
  }

  #[test]
  fn the_refusal_goes_on_the_window_that_asked_else_the_one_in_front_else_any() {
    let (asked, front, other) = (window(1), window(2), window(3));
    let open = [other, front, asked];
    assert_eq!(host(Some(asked), Some(front), &open), Some(asked));
    // ⌘O with no window in front, and a window closed while the panel was up.
    assert_eq!(host(None, Some(front), &open), Some(front));
    assert_eq!(host(Some(asked), Some(front), &[other, front]), Some(front));
    assert_eq!(host(Some(asked), None, &[other]), Some(other));
    assert_eq!(host(Some(asked), Some(front), &[]), None);
  }

  #[test]
  fn a_window_has_one_sheet_at_a_time_and_the_others_wait_their_turn() {
    let (here, other) = (window(1), window(2));
    let mut sheets = Sheets::default();
    assert_eq!(sheets.begin(here, "first"), Some("first"));
    assert_eq!(sheets.begin(here, "second"), None, "the first is still up");
    assert_eq!(sheets.begin(here, "third"), None);
    assert_eq!(sheets.begin(other, "elsewhere"), Some("elsewhere"), "another window's");
    // Answered: those that waited begin again, in their order.
    assert_eq!(sheets.end(here), ["second", "third"]);
    assert_eq!(sheets.begin(here, "second"), Some("second"));
    assert_eq!(sheets.begin(here, "third"), None);
    assert!(sheets.end(other).is_empty());
    assert_eq!(sheets.begin(other, "again"), Some("again"));
  }

  #[test]
  fn a_plain_click_asks_for_its_window_and_a_cmd_click_for_another() {
    let here = window(1);
    let cmd = Modifiers {
      platform: true,
      ..Modifiers::default()
    };
    assert_eq!(Target::of_click(Modifiers::default(), here), Target::Window(here));
    assert_eq!(Target::of_click(cmd, here), Target::NewWindow);
    // ⌘ alone decides.
    let others = Modifiers {
      control: true,
      alt: true,
      shift: true,
      ..Modifiers::default()
    };
    assert_eq!(Target::of_click(others, here), Target::Window(here));
  }

  #[test]
  fn a_repo_no_window_shows_opens_where_asked() {
    let here = window(1);
    assert_eq!(plan(Target::Window(here), None), Plan::Open(Target::Window(here)));
    assert_eq!(plan(Target::NewWindow, None), Plan::Open(Target::NewWindow));
  }

  #[test]
  fn the_asking_windows_own_repo_changes_nothing() {
    let here = window(1);
    assert_eq!(plan(Target::Window(here), Some(here)), Plan::Stay);
  }

  #[test]
  fn a_repo_another_window_shows_brings_it_forward_whatever_the_click() {
    let (here, other) = (window(1), window(2));
    assert_eq!(plan(Target::Window(here), Some(other)), Plan::Raise(other));
    assert_eq!(plan(Target::NewWindow, Some(other)), Plan::Raise(other));
  }
}
