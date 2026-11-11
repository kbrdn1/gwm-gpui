use std::{cell::RefCell, path::PathBuf, rc::Rc};

use gpui::{App, Application, Focusable, KeyBinding, Menu, MenuItem, PathPromptOptions, actions, prelude::*};
use gwm_client::{Repo, watch::Spawned};
use gwm_gpui::{
  OpenFolder, OpenRepo, Workspace,
  assets::Assets,
  login_shell,
  recent::{self, Entry, Projects},
};

actions!(gwm_gpui, [Quit]);

/// The daemons the app started, one holder per window: a window that starts
/// one must not stop another's. All are stopped on quit.
type Daemons = Rc<RefCell<Vec<Spawned>>>;

fn main() {
  // The Finder launches the app with launchd's PATH, where `gwm` is not: take
  // the login shell's. Only then, so a launch from a terminal stays as it is.
  // `watch` and the daemon it starts inherit it.
  let inherited = std::env::var_os("PATH").unwrap_or_default();
  if !login_shell::finds(&inherited, "gwm") {
    let shell = std::env::var_os("SHELL").map_or_else(|| PathBuf::from("/bin/zsh"), PathBuf::from);
    if let Some(path) = login_shell::path(&shell, login_shell::TIMEOUT) {
      // SAFETY: no other thread runs yet: GPUI has not started, and the probe
      // joined its own before returning a value.
      unsafe { std::env::set_var("PATH", path) };
    }
  }

  // The repo to list: the first argument, else the current directory if it is
  // in one. The Finder starts the app in `/`: then the user picks a folder. A
  // given argument is not checked: a bare repo is no work tree for git, but gwm
  // lists it, and the window says why when nothing can be listed.
  let repo = std::env::args_os()
    .nth(1)
    .map(PathBuf::from)
    .or_else(|| Repo::locate(".".as_ref()).is_ok().then(|| PathBuf::from(".")));

  Application::with_platform(gpui_platform::current_platform(false))
    .with_assets(Assets)
    .run(move |cx: &mut App| {
      // A daemon this app started would outlive it: stop them on the way out.
      let daemons = Daemons::default();
      let on_quit = daemons.clone();
      cx.on_app_quit(move |_| {
        on_quit.borrow().iter().for_each(Spawned::stop);
        async {}
      })
      .detach();
      cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-o", OpenFolder, None),
      ]);
      gwm_gpui::bind_keys(cx);
      cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
      // The way out of a window on the wrong folder, and into another repo.
      let to_pick = daemons.clone();
      cx.on_action(move |_: &OpenFolder, cx: &mut App| pick(to_pick.clone(), cx));
      // A repo from the project menu's recent list.
      let to_open = daemons.clone();
      cx.on_action(move |OpenRepo(repo): &OpenRepo, cx: &mut App| open(repo.clone(), &to_open, cx));
      cx.set_global(Projects::default());
      cx.set_menus([
        Menu::new("gwm").items([MenuItem::action("Quit gwm", Quit)]),
        Menu::new("File").items([MenuItem::action("Open…", OpenFolder)]),
      ]);
      cx.on_window_closed(|cx, _| {
        if cx.windows().is_empty() {
          cx.quit();
        }
      })
      .detach();

      if let Some(repo) = repo {
        open(repo, &daemons, cx);
        return;
      }
      // In front, or the panel opens behind the app that launched this one.
      cx.activate(true);
      pick(daemons, cx);
    });
}

/// Asks for a folder and opens it. One that is in no repo opens too: the
/// window shows what git and gwm make of it, and File > Open… picks another.
fn pick(daemons: Daemons, cx: &mut App) {
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
      Some(folder) => open(folder, &daemons, cx),
      // Cancelled at launch: there is nothing to show.
      None if cx.windows().is_empty() => cx.quit(),
      None => {}
    });
  })
  .detach();
}

/// Opens `repo` in a window of its own, with its own live list, or brings
/// forward the window already showing it, whichever worktree that one was
/// opened from. A repo opened is recorded in the recent list. A folder git
/// does not place (no repo, a bare one) opens all the same, unrecorded.
fn open(repo: PathBuf, daemons: &Daemons, cx: &mut App) {
  let daemons = daemons.clone();
  cx.spawn(async move |cx| {
    let located = cx
      .background_spawn({
        let repo = repo.clone();
        async move { Repo::locate(&repo).ok() }
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
      let window = open_window(repo, &daemons, cx);
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

fn open_window(repo: PathBuf, daemons: &Daemons, cx: &mut App) -> gpui::AnyWindowHandle {
  let spawned = Spawned::default();
  daemons.borrow_mut().push(spawned.clone());
  let window = cx
    .open_window(gwm_gpui::window_options(cx), |window, cx| {
      let workspace = cx.new(|cx| Workspace::new(repo, spawned, cx));
      window.focus(&workspace.focus_handle(cx), cx);
      workspace
    })
    .expect("the main window opens");
  cx.activate(true);
  window.into()
}
