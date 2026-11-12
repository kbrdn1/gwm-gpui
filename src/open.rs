//! Opening a repo in a window, for the app's `main` and the screenshot example:
//! in a window of its own, or in place of the repo a window shows.

use std::{
  cell::RefCell,
  path::{Path, PathBuf},
  rc::Rc,
};

use gpui::{AnyWindowHandle, App, Focusable, Modifiers, PathPromptOptions, prelude::*};
use gwm_client::{Repo, watch::Spawned};

use crate::{
  Workspace,
  recent::{self, Entry, Projects},
  window_options,
};

/// The daemons the app started, one holder per view: a window that starts one
/// must not stop another's. A view stops its own when it goes; all are stopped
/// on quit, which drops no view.
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

/// Asks for a folder and opens it where `target` says. One that is in no repo
/// opens too: the window shows what git and gwm make of it, and File > Open…
/// picks another.
pub fn folder(target: Target, daemons: Daemons, cx: &mut App) {
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
    cx.update(|cx| match folder {
      Some(folder) => repo(folder, target, &daemons, cx),
      // Cancelled at launch: there is nothing to show.
      None if cx.windows().is_empty() => cx.quit(),
      None => {}
    });
  })
  .detach();
}

/// Opens the repo at `path` where `target` says, with its own live list: in
/// a window of its own, or in the window that asked, in place of the repo it
/// showed. A repo another window shows already brings that window forward
/// instead, whichever worktree it was opened from, and the asking window's own
/// repo changes nothing. A repo opened is recorded in the recent list. A folder
/// git does not place (no repo, a bare one) opens all the same, unrecorded.
pub fn repo(path: PathBuf, target: Target, daemons: &Daemons, cx: &mut App) {
  let daemons = daemons.clone();
  cx.spawn(async move |cx| {
    let located = cx
      .background_spawn({
        let path = path.clone();
        async move { Repo::locate(&path).ok() }
      })
      .await;
    cx.update(|cx| {
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
        Target::Window(window) if switch(window, &path, &daemons, cx) => window,
        // Also when the window that asked was closed meanwhile.
        _ => open_window(path, &daemons, cx),
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

/// Shows the repo at `path` in `window`, in place of the one it showed: that
/// view is dropped, and its watch and daemon end with it (see
/// [`Workspace::new`]). False when the window was closed.
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
