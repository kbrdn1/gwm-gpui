use std::path::PathBuf;

use gpui::{App, Application, Focusable, KeyBinding, actions, prelude::*};
use gwm_client::watch::Spawned;
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

  // The repo to list: the first argument, else wherever the app was launched.
  let repo = std::env::args_os()
    .nth(1)
    .map_or_else(|| PathBuf::from("."), PathBuf::from);

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

      cx.open_window(gwm_gpui::window_options(cx), |window, cx| {
        let workspace = cx.new(|cx| Workspace::new(repo, spawned, cx));
        window.focus(&workspace.focus_handle(cx), cx);
        workspace
      })
      .expect("the main window opens");
      cx.activate(true);
    });
}
