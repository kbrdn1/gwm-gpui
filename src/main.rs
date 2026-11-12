use std::path::PathBuf;

use gpui::{App, Application, KeyBinding, Menu, MenuItem, actions};
use gwm_client::watch::Spawned;
use gwm_gpui::{
  OpenFolder, OpenRepo,
  assets::Assets,
  login_shell,
  open::{self, Daemons, Start, Target},
  recent::Projects,
  workspace::NewWorktree,
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
  // in one. The Finder starts the app in `/`: then the landing view, where the
  // user picks one.
  let start = open::start(std::env::args_os().nth(1).map(PathBuf::from), ".".as_ref());

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
      // Into another repo, from any window and from the landing view.
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
        // New Worktree is the workspace's own action: greyed out on the landing
        // view, which has no repo to create one in.
        Menu::new("File").items([
          MenuItem::action("New Worktree", NewWorktree),
          MenuItem::action("Open…", OpenFolder(Target::NewWindow)),
        ]),
      ]);
      cx.on_window_closed(|cx, _| {
        if cx.windows().is_empty() {
          cx.quit();
        }
      })
      .detach();

      match start {
        Start::Repo(repo) => open::repo(repo, Target::NewWindow, &daemons, cx),
        Start::Landing => open::landing(daemons, cx),
      }
    });
}
