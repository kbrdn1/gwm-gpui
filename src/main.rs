use std::path::PathBuf;

use gpui::{App, Application, KeyBinding, Menu, MenuItem, actions};
use gwm_client::{Repo, watch::Spawned};
use gwm_gpui::{
  OpenFolder, OpenRepo,
  assets::Assets,
  login_shell,
  open::{self, Daemons, Target},
  recent::Projects,
};

actions!(gwm_gpui, [Quit]);

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
        KeyBinding::new("cmd-o", OpenFolder(Target::NewWindow), None),
      ]);
      gwm_gpui::bind_keys(cx);
      cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
      // The way out of a window on the wrong folder, and into another repo.
      let to_pick = daemons.clone();
      cx.on_action(move |OpenFolder(target): &OpenFolder, cx: &mut App| open::folder(*target, to_pick.clone(), cx));
      // A repo from the project menu's recent list: in its window, or with ⌘ in another.
      let to_open = daemons.clone();
      cx.on_action(move |OpenRepo(repo, target): &OpenRepo, cx: &mut App| {
        open::repo(repo.clone(), *target, &to_open, cx);
      });
      cx.set_global(Projects::default());
      cx.set_menus([
        Menu::new("gwm").items([MenuItem::action("Quit gwm", Quit)]),
        Menu::new("File").items([MenuItem::action("Open…", OpenFolder(Target::NewWindow))]),
      ]);
      cx.on_window_closed(|cx, _| {
        if cx.windows().is_empty() {
          cx.quit();
        }
      })
      .detach();

      if let Some(repo) = repo {
        open::repo(repo, Target::NewWindow, &daemons, cx);
        return;
      }
      // In front, or the panel opens behind the app that launched this one.
      cx.activate(true);
      open::folder(Target::NewWindow, daemons, cx);
    });
}
