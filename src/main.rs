use std::path::PathBuf;

use gpui::{App, Application, Focusable, KeyBinding, PathPromptOptions, actions, prelude::*};
use gwm_client::{Repo, watch::Spawned};
use gwm_gpui::{Workspace, assets::Assets, login_shell};

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

  // The repo to list: the first argument (an old macOS adds `-psn_…` to a
  // Finder launch, not a path), else the current directory if it is in one.
  // The Finder starts the app in `/`: then the user picks a folder. A given
  // argument is not checked: a bare repo is no work tree for git, but gwm
  // lists it, and the window says why when nothing can be listed.
  let repo = std::env::args_os()
    .skip(1)
    .find(|arg| !arg.to_string_lossy().starts_with("-psn_"))
    .map(PathBuf::from)
    .or_else(|| Repo::locate(".".as_ref()).is_ok().then(|| PathBuf::from(".")));

  Application::with_platform(gpui_platform::current_platform(false))
    .with_assets(Assets)
    .run(move |cx: &mut App| {
      // A daemon this app started would outlive it: stop it on the way out.
      let spawned = Spawned::default();
      let on_quit = spawned.clone();
      cx.on_app_quit(move |_| {
        on_quit.stop();
        async {}
      })
      .detach();
      cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
      gwm_gpui::bind_keys(cx);
      cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
      cx.on_window_closed(|cx, _| {
        if cx.windows().is_empty() {
          cx.quit();
        }
      })
      .detach();

      if let Some(repo) = repo {
        open(repo, spawned, cx);
        return;
      }
      // In front, or the panel opens behind the app that launched this one.
      cx.activate(true);
      // A folder that is no repo still opens: the window says what git and
      // gwm make of it, which an alert could only repeat.
      let picked = cx.prompt_for_paths(PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: Some("Open".into()),
      });
      cx.spawn(async move |cx| {
        let repo = picked
          .await
          .ok()
          .and_then(Result::ok)
          .flatten()
          .and_then(|paths| paths.into_iter().next());
        cx.update(|cx| match repo {
          Some(repo) => open(repo, spawned, cx),
          // Cancelled: there is nothing to show.
          None => cx.quit(),
        });
      })
      .detach();
    });
}

fn open(repo: PathBuf, spawned: Spawned, cx: &mut App) {
  cx.open_window(gwm_gpui::window_options(cx), |window, cx| {
    let workspace = cx.new(|cx| Workspace::new(repo, spawned, cx));
    window.focus(&workspace.focus_handle(cx), cx);
    workspace
  })
  .expect("the main window opens");
  cx.activate(true);
}
