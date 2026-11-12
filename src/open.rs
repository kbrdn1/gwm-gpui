//! Opening a repo in a window, for the app's `main` and the screenshot example.

use std::{cell::RefCell, path::PathBuf, rc::Rc};

use gpui::{AnyWindowHandle, App, Focusable, PathPromptOptions, prelude::*};
use gwm_client::{Repo, watch::Spawned};

use crate::{
  Workspace,
  recent::{self, Entry, Projects},
  window_options,
};

/// The daemons the app started, one holder per window: a window that starts
/// one must not stop another's. All are stopped on quit.
pub type Daemons = Rc<RefCell<Vec<Spawned>>>;

/// Asks for a folder and opens it. One that is in no repo opens too: the
/// window shows what git and gwm make of it, and File > Open… picks another.
pub fn folder(daemons: Daemons, cx: &mut App) {
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
      Some(folder) => repo(folder, &daemons, cx),
      // Cancelled at launch: there is nothing to show.
      None if cx.windows().is_empty() => cx.quit(),
      None => {}
    });
  })
  .detach();
}

/// Opens the repo at `path` in a window of its own, with its own live list, or
/// brings forward the window already showing it, whichever worktree that one
/// was opened from. A repo opened is recorded in the recent list. A folder git
/// does not place (no repo, a bare one) opens all the same, unrecorded.
pub fn repo(path: PathBuf, daemons: &Daemons, cx: &mut App) {
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
      if let Some(key) = &key {
        let shown = cx
          .global::<Projects>()
          .windows
          .iter()
          .find(|(open, _)| open == key)
          .map(|(_, window)| *window);
        // A closed window's handle fails to update: the repo opens anew.
        if shown.is_some_and(|window| window.update(cx, |_, window, _| window.activate_window()).is_ok()) {
          cx.activate(true);
          return;
        }
      }
      let window = open_window(path, &daemons, cx);
      let (Some(key), Some(located)) = (key, located) else {
        return;
      };
      let projects = cx.global_mut::<Projects>();
      projects.windows.retain(|(open, _)| *open != key);
      projects.windows.push((key.clone(), window));
      let Some(file) = recent_file() else {
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
